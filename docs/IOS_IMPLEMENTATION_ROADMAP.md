# iOS implementation roadmap

Companion to `docs/IOS_FEASIBILITY_REPORT.md` (read its section 0 for the evidence tags). Nothing here has been implemented. Every milestone is small, independently testable, and ends with a pass/fail check. **Do not claim iOS support until a physical-device build and test succeeds** (M2 is the first such point).

Ground rules, from the brief:
- Preserve upstream behaviour on Windows, macOS and Linux. Platform code goes behind cfgs or in new crates. Prefer pull requests under ~500 lines that upstream could accept.
- Never weaken a parity harness to make iOS pass.
- No optimisation without a reproducible profile.
- Keep a **porting ledger** (`docs/ios/ledger.md`: crate, dependency, change, status, evidence, test, upstreamable?) and **architecture decision records** (`docs/ios/adr/NNNN-*.md`) for: app shell, renderer integration, resource transfer, memory budgets, supported Minecraft version (26.3, pinned).

## Order at a glance

```
M0 baseline+CI ─► M1 compile-clean ─► M2 hello-Metal on device ─► M3 telemetry ─► M4 Horizon UI on device
                                                                      │                   │
                                                                      ▼                   ▼
                                                              M5 lifecycle ─► M6 resource transfer ─► M7 first real chunk
                                                                                                          │
              M8 world traversal + first budgets ◄────────────────────────────────────────────────────────┘
                   │
                   ├─► M9 saves under suspension ─► M10 audio session
                   ├─► M11 touch controls + minimal HUD
                   ├─► M12 account/ownership/download (Swift)   ─┐  legal/policy gate before shipping
                   └─► M13 Java-free preparation decision (#1750) ─┘
                              M14 optimisation passes ─► M15 sustained play, signing, distribution
```

Go/no-go gates: **G1** after M1 (iOS graph builds on a macOS host). **G2** after M4 (Horizon's own renderer and UI run on the A14 through Metal). **G3** after M7 (a real chunk from user-provided resources renders). **G4** after M8 (a measured, stable footprint at a playable distance on an iPhone 12). Reconsider the approach if G2 or G4 fails.

---

## M0. Baseline, CI gate and ledger
- **Objective:** a reproducible starting point and a regression gate.
- **Existing components:** `.github/workflows/ci.yml`, `tools/pr_ready.py`, `docs/benchmark.md`, `tools/perf_bench.py`.
- **Modifications:** new CI job on a macOS runner: `cargo check --locked --target aarch64-apple-ios` per crate (start with the 17 crates that already pass). Record the upstream commit, Rust version and Xcode version in the ledger. Capture one desktop baseline (Mac) of startup, chunk load, frame times and RSS with the existing tools, for later comparison only (the Mac is not an iPhone).
- **Risks:** C build scripts (`ring`, mimalloc) behave differently on the runner than on a dev machine; a fork lags upstream.
- **Success test:** job green on current `main`; baseline report stored under `benchmarks/ios/`.
- **Depends on:** none.

## M1. Whole game graph builds for `aarch64-apple-ios`
- **Objective:** the library graph of `horizon-client` compiles and links (static library) on a macOS host, with iOS-only code fixed rather than stubbed.
- **Existing components:** `horizon-client-input` (`clipboard.rs`, `keybinds.rs:303-312`), `horizon-hdr` (`lib.rs:12-100`), `horizon-threads` (`process_memory.rs:66-90`), `horizon-client-render` (`gfx.rs:1921`, `gfx.rs:2327-2332`), Cargo features in `horizon-client` and `horizon-client-render` (`vulkan`, `gles`, `controllers`, `discord`, `friends`, `heartbeat`, `audio`).
- **Modifications:**
  1. Clipboard: do not depend on `arboard` on iOS (`cfg(not(target_os = "ios"))` dependency); use an in-memory clipboard (`MemoryClipboard` already exists) first, native `UIPasteboard` later.
  2. `horizon-hdr`: cfg `not(any(macos, ios))` (Vulkan-only code); gate the matching branches in `gfx.rs`.
  3. `keybinds.rs`: on iOS take the layout character from `logical_key` instead of `key_without_modifiers`.
  4. `process_memory.rs`: keep `proc_pidinfo` for macOS only; add an iOS branch using `task_info(TASK_VM_INFO)` `phys_footprint` (this is the figure the OS acts on).
  5. `backend_order`: treat `"ios"` like `"macos"` (Metal only).
  6. Platform feature set: an `ios` build profile that turns off `controllers`, `discord`, `friends`, `heartbeat`, and the Vulkan/GL backends; keep `audio`.
  7. Gate process-spawning helpers (`xdg-open`, `ps`, `sysctl`, current-exe re-spawn) so they are not compiled or not reachable on iOS.
- **Risks:** `ring`/`rustls` and mimalloc may need extra flags on iOS (unverified); hidden runtime-only assumptions behind `cfg(unix)` (90 uses) such as `/proc`.
- **Success test:** CI job builds `--target aarch64-apple-ios` for `horizon-client` lib, plus `aarch64-apple-ios-sim`; desktop CI unchanged and green; the existing unit tests for the touched crates still pass.
- **Depends on:** M0.

## M2. Hello Metal on a physical iPhone
- **Objective:** the minimum "success" from the brief: a signed app launches on a real iPhone, Rust runs natively, `wgpu` presents through Metal, a textured 3D scene renders.
- **Existing components:** `wgpu 30.0.1` (+ `third_party/wgpu-hal`), `winit 0.30.13`, shaders/WGSL from `horizon-client-render/src/*.wgsl`.
- **Modifications:** new crate `crates/horizon-ios` (staticlib, unsafe allowed, one `extern "C"` entry) and `platform/ios/` (Xcode project, `Info.plist`, entitlements). The first scene is a rotating textured cube using a tiny wgpu example, **not** Horizon yet. Log the adapter: features, limits, `max_buffer_size`, `MTLGPUFamily`. Decide ADR 0001: winit-owned lifecycle (Option A) vs Swift-owned view (Option B); start with A.
- **Risks:** signing and provisioning; wgpu surface creation under winit on iOS; Metal feature differences on A14.
- **Success test:** on an iPhone 12, steady 60 fps cube for 5 minutes; adapter log captured; no validation errors.
- **Depends on:** M1 (for the build setup; the cube itself needs only `wgpu`/`winit`).

## M3. Telemetry first
- **Objective:** measure before optimising.
- **Existing components:** `horizon-threads` (`allocator_commit`, `heap_census` with feature `alloc-census`, `process_memory`), `horizon-bench` (report format), `horizon-client-render/src/telemetry.rs`, `spans.rs`.
- **Modifications:** an in-process sampler (4 Hz) writing a `horizon-bench` JSON report and an overlay: `phys_footprint`, `os_proc_available_memory()`, mimalloc committed/peak, wgpu allocator report, frame-time percentiles, thermal state, battery level, thread count. Replace the `ps` sampler path on iOS.
- **Risks:** `os_proc_available_memory` and footprint calls need small FFI shims (unsafe, in `horizon-ios`).
- **Success test:** a 10-minute M2 run produces a report; numbers agree with Xcode's memory gauge within a few percent (record the difference, do not assume it).
- **Depends on:** M2.

## M4. Horizon's own renderer and UI on the device
- **Objective:** prove Horizon's real renderer, text and UI run on the A14 with no Mojang assets present.
- **Existing components:** `game/no_resources.rs` (the "resources missing" screen), `horizon-client-render` UI/text (`ab_glyph`), `gfx.rs` `Renderer::new`.
- **Modifications:** call `horizon_client::entry::main_with(...)` from the iOS entry with explicit `--home`/`--resources` paths pointing into the app container. Only the "missing resources" screen is expected. Fix whatever fails at renderer start: feature masks, limits, arena page size, shader compile.
- **Risks:** hidden desktop assumptions in `Renderer::new` (14k-line `gfx.rs`); first-launch shader compile time; fonts.
- **Success test:** the screen renders at native resolution on the iPhone 12, stable for 10 minutes, shader/pipeline start time recorded, adapter features logged. **Gate G2.**
- **Depends on:** M1, M2, M3.

## M5. App lifecycle
- **Objective:** survive backgrounding, interruptions and memory warnings.
- **Existing components:** `window_handler.rs` (`resumed`, `exiting`, no `suspended`), `gfx.rs` surface recreation, `horizon_threads::release_free_memory`, the job pool.
- **Modifications:** implement `suspended` (drop surface, stop GPU submission, pause workers), `resumed` (recreate surface, resume), `memory_warning` (drop caches, `release_free_memory`), resign-active save hook. Add a cfg'd `ios_lifecycle` module; keep desktop paths untouched.
- **Risks:** device-lost/surface-lost races; worker threads holding GPU resources; iOS killing the app during a background save.
- **Success test:** scripted 100 background/foreground cycles plus an incoming call and a lock/unlock; no crash, no growth in footprint across cycles beyond a measured tolerance, frame rate recovers.
- **Depends on:** M4.

## M6. Resource transfer (Option A) and validation
- **Objective:** get a player's prepared resources onto the phone without any Java on the phone.
- **Existing components:** `horizon-prepare` (`--check`, `--output`), `research-lock.json`, `docs/api/launch.md` (`--resources`), `crates/horizon-prepare/src/check.rs`.
- **Modifications:** document the host-side procedure (run `horizon-prepare` on the player's own machine). iOS side: a Files/`UIDocumentPicker` or `devicectl`/Finder import into `Application Support/horizon/resources/26.3`, a Rust-side completeness/version check equivalent to `--check` (no Java), and clear errors. Measure the prepared size and transfer time.
- **Risks:** size (sounds alone ~380 MB); Files import of a large folder; whether `horizon-prepare` itself compiles for iOS (not required: only its check logic is wanted, possibly via a small shared crate).
- **Success test:** the import of a real prepared set validates; the game proceeds past the "missing resources" screen to the title screen.
- **Depends on:** M4. **Legal/policy review of moving derived data is required before any public build** (feasibility report, section 8.3).

## M7. First real chunk
- **Objective:** render one real chunk from user-provided resources, fixed camera.
- **Existing components:** `horizon-assets` (models/atlases), `chunk.rs`, `mesher.rs`, `streaming.rs`, arena in `gfx/arena.rs`.
- **Modifications:** start a flat/creative world at render distance minimal, no mobs; a debug flag to freeze the camera. Fix atlas/format/limit issues surfaced on Metal.
- **Risks:** atlas sizes, texture format support, arena page limits.
- **Success test:** screenshot of terrain on the device; frame time and footprint logged. **Gate G3.**
- **Depends on:** M6, M3.

## M8. World traversal and first budgets
- **Objective:** walk through a fixed seed world at several render distances; set first measured budgets.
- **Existing components:** the job pool (`jobs.rs` classes), `worker_count`/`default_workers`, `lod.rs`, `graphics.rs` presets, `streaming.rs`.
- **Modifications:** a **mobile preset** (small render and simulation distance, worker count from measurement, frame cap, no shader packs, no LOD at first); scripted camera flight for repeatability.
- **Risks:** jetsam; thermal throttling; worker/queue behaviour at 2 performance + 4 efficiency cores.
- **Success test:** at the chosen distance, a 20-minute flight on an iPhone 12 with no jetsam, p99 frame time and footprint recorded, thermal state logged. Budgets are set **from these numbers**, not guessed. **Gate G4.**
- **Depends on:** M7, M5.

## M9. Persistence under suspension
- **Objective:** worlds survive suspension and termination.
- **Existing components:** `save.rs`, `game/saving.rs`, `horizon-world/src/region.rs`, `pending_save`.
- **Modifications:** verify/atomic-write paths, save on resign-active, recovery test after a forced kill; exclude caches from iCloud backup, keep saves backed up deliberately.
- **Risks:** large saves exceeding the short background window; partial region writes.
- **Success test:** kill the app mid-save 50 times; the world reopens; the existing `save_roundtrip`/`world_reopen` harnesses still pass on desktop.
- **Depends on:** M8.

## M10. Audio session
- **Objective:** correct sound behaviour on iOS.
- **Existing components:** `audio.rs` (cpal 0.16, 64 MiB sound cache), `sound_engine.rs`.
- **Modifications:** configure `AVAudioSession` (ambient/playback category), handle interruptions and route changes by pausing/resuming the output stream; respect the silent switch as chosen.
- **Risks:** cpal stream restarts after interruption.
- **Success test:** phone call, headphone unplug, Bluetooth connect, silent switch: audio recovers, no crash, no leak.
- **Depends on:** M5.

## M11. Touch controls and a minimal mobile HUD
- **Objective:** playable on a phone.
- **Existing components:** `horizon-client-input` (`keybinds`, `mouse`, `gamepad`), `pointer_capture.rs`, HUD/menus in `horizon-client-render`.
- **Modifications:** map touches into the existing input model (virtual stick, look drag, tap/hold for break/place, jump/sneak buttons, hotbar), GUI scale for phone screens, safe-area insets, on-screen keyboard/IME for chat and text fields. No `WindowEvent::Touch` handling exists today.
- **Risks:** large UI surface; input latency; menus designed for a mouse.
- **Success test:** 10 scripted gameplay tasks (move, break, place, open inventory, craft, chat) completed by a tester on the device; input latency measured.
- **Depends on:** M8.

## M12. Account, ownership and game-file download (Swift launcher)
- **Objective:** the same ownership model as Horizon's launcher, natively.
- **Existing components (reference, not reuse as UI):** `docs/microsoft-account.md`, `launcher/src/account/` (the `account::session` API has no GPUI dependency), `launcher/src/mojang.rs`, `horizon-protocol` auth code (`crypto::server_hash`, RSA, `auth.rs` in `horizon-client`).
- **Modifications:** device-code sign-in opened in Safari, Xbox/XSTS/Minecraft services chain, entitlement check, Keychain storage (never in logs/argv), owner gate before resource acquisition; download only from official endpoints with size/SHA-1 checks; hand the session to the game via the existing session-file/stdin mechanism. Own Azure registration and Mojang App ID review are prerequisites.
- **Risks:** approval timeline; token handling bugs; policy review.
- **Success test:** against a mock of the Microsoft/Xbox/Minecraft endpoints (as the launcher's tests do) plus one real owning account in a private build; non-owning account is refused; offline start behaves like Horizon's documented rule.
- **Depends on:** M5; can run in parallel with M9 to M11. **Human legal/policy review is a hard gate before distribution.**

## M13. Java-free preparation decision
- **Objective:** choose how phones get prepared resources without a JVM.
- **Existing components:** `horizon-prepare`, the 371 Java exporter files in `mods/horizon/src/test/java/dev/horizonlab/parity/`, upstream issue #1750 (published tables + local assembly), `docs/server.md`.
- **Modifications:** talk to the maintainers; size the exporter output by category (which tables the engine actually reads, how big, whether any contain Mojang content); prototype assembly on a host from "published tables + the player's own jar/assets". Do not start a Rust port of the exporters without a size estimate and agreement.
- **Risks:** legal acceptability of published tables; parity drift.
- **Success test:** a documented decision (ADR), and on a host a preparation that produces a byte-identical resource set to `horizon-prepare` for the same inputs, proven with the existing `check_prepare_exporters.py` comparison idea.
- **Depends on:** M6 (to know what is consumed).

## M14. Optimisation passes (profile-driven only)
- **Objective:** reach the budgets from M8 with margin.
- **Existing components:** mesher, arena, job pool, sound cache, atlases (see `docs/IOS_MEMORY_STRATEGY.md`).
- **Modifications:** one change per experiment: lower worker count, smaller arena pages, release mesh CPU copies after upload, ASTC atlas (only if visual parity is acceptable), shorter queues. Each change needs a before/after capture.
- **Risks:** regressions in desktop parity; premature unsafe code.
- **Success test:** each experiment moves a measured metric (footprint, p99, thermal) without breaking desktop harnesses.
- **Depends on:** M8.

## M15. Sustained play, signing and distribution
- **Objective:** a build a tester can install and play for 30 minutes.
- **Existing components:** everything above.
- **Modifications:** release signing, privacy manifest and permission strings, App Review/TestFlight or other distribution per the legal review, crash reporting that omits tokens, a device matrix report.
- **Risks:** App Review outcome; local-network permission prompts if LAN features return.
- **Success test:** 30-minute sustained session on iPhone 12 and a newer device with thermal and battery logs; zero jetsam events in the soak run.
- **Depends on:** M9 to M12.

## Cross-cutting dependencies and open questions to settle early
- **M2:** minimum iOS version and Metal family from the adapter log.
- **M3/M8:** whether `hw.perflevel0.physicalcpu` exists on A14; otherwise set worker count explicitly (`HORIZON_WORKER_THREADS`/Graphics > workers).
- **M6/M13:** total prepared size on disk (host measurement).
- **M12/M15:** legal and platform-policy review runs in parallel from now; it is not a late task.

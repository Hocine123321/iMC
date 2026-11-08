# iOS feasibility report: a native Horizon client for iPhone

Status: research, Phase 1 + 2 of the iOS port brief. No source file was changed to produce this report.
Base: `horizon-rust/horizon` `main` at `35f3dad2072c478e6482d776341f6f3eaf653e7b` (2026-11-07), Minecraft Java 26.3, protocol 777.
Target: iPhone 12 (A14, 4 GB) and later, `aarch64-apple-ios`.

## 0. How to read this report

Every finding carries one evidence tag.

| Tag | Meaning |
|---|---|
| **[COMPILED]** | Reproduced with `cargo check --target aarch64-apple-ios` (rustc 1.99.1, Linux host). Type-check only: nothing was linked, run, or tested on a device. |
| **[SOURCE]** | Read in the repository at the file/line named. Not executed. |
| **[DOC]** | Stated by the project's own documentation. Not independently confirmed. |
| **[ASSUMPTION]** | Reasoned, not verified. Needs a measurement or an Apple/Mojang source. |
| **[OPEN]** | Unresolved question. |

Limits of the compile evidence (Appendix A has the commands):

- The host is Linux without Xcode, so any crate whose build script compiles C (`ring`, `libmimalloc-sys`) could not be built. That is a limit of the sandbox, not a defect of the repo. TLS and the allocator are therefore **unverified** on iOS.
- Where a crate failed, I patched a **throwaway copy in `/tmp`** (never this repository) to see what failed next. Those patches are listed in Appendix B and are not a proposed fix.
- A passing `cargo check` says nothing about runtime behaviour, Metal correctness, memory, or App Store acceptability.

## 1. Executive summary

**Recommendation: proceed, with major architectural changes.** Not "reconsider".

Why proceed: the simulation, world generation, protocol, asset decoding and most of the renderer already type-check for iOS, and the renderer is `wgpu` 30.0.1 on Metal, the same path Horizon already ships on macOS. There is no JVM at runtime. The upstream project lists iOS as a long-term goal and has started on it (`ROADMAP.md`, item `phones-and-browser`; `horizon-dualsense` already carries an iOS cfg, #1813).

Why "major changes": three problems are structural, not a matter of adding cfgs.

1. **Resource preparation needs a JVM, which iOS cannot run.** `horizon-prepare` runs Mojang's own classes through JDK 25 using 371 Java exporter files. The plan's idea of reusing "the existing preparation pipeline" on the phone does not work as written (section 9).
2. **The app lifecycle is desktop-shaped.** The game creates its window once in `resumed`, has no `suspended` or memory-warning handler, and the renderer was never run under an OS that revokes the GPU surface (section 6).
3. **Memory is unmeasured on iOS.** The project's own memory numbers come from a desktop and from an external `ps` sampler. Nothing in the repo tells us what an iPhone 12 will tolerate (section 10, and `IOS_MEMORY_STRATEGY.md`).

Biggest risks, ranked:

1. Preparing resources without Java (blocks every real-world milestone), plus the legal/policy review it needs.
2. Jetsam termination from memory use on a 4 GB device.
3. Account/ownership flow: needs an app registration the project does not have, and Mojang approval.
4. Touch input and UI: none exists (no `WindowEvent::Touch` handling found).
5. Apple review and Mojang usage terms for this kind of app. Not analysed here; see section 8.

## 2. Repository map [SOURCE]

- Workspace: `Cargo.toml`, edition 2024, `rust-version = 1.95` (README and `docs/native-client.md` still say 1.90+). 40 crates under `crates/`; `launcher/`, `crates/horizon-telemetry-server`, `crates/horizon-mod-registry` and three `third_party/` forks are separate workspaces.
- Size: about 1.18M lines of Rust in the main crates. Largest: `horizon-client` 630k (gameplay, integrated server, LAN, mobs, commands), `horizon-client-render` 218k (chunks, meshing, light, streaming, renderer `gfx.rs` ~14k lines, HUD, menus), `horizon-world` 190k (NBT, regions, worldgen, level ticks, datafix), `horizon-protocol` 36k.
- Entry: `crates/horizon-client/src/main.rs` is 10 lines: global allocator, then `horizon_client::entry::main()`. It is `#![forbid(unsafe_code)]`. The project's convention for unsafe code is a small separate crate (see `horizon-hdr`, whose header says the game crate "forbids unsafe code, can't make itself").
- Server: the dedicated server is the same binary (`--dedicated`); single-player runs an integrated server on a thread (`docs/architecture.md` row 37). Both are pulled in whole.
- Launcher: `launcher/` is a separate GPUI (Zed's UI framework, `gpui-fast`) desktop app with `wayland`/`x11` features. It does sign-in, install, update and preparation. It cannot be reused on iOS as a UI.
- Tests: hundreds of differential harnesses (`tools/test_*_parity.py`, drive Java oracles in `mods/horizon/src/test/java/...`) plus Rust unit tests. The README states the harnesses pass on Linux and macOS [DOC]. None run on iOS. CI (`.github/workflows/ci.yml`) builds Windows, Linux and macOS; no iOS target appears.
- Benchmarks: `docs/benchmark.md`, `tools/perf_bench.py`, `crates/horizon-bench` (report format), `horizon-client --benchmark`. The memory figure is RSS sampled from outside the process with `ps` (macOS/Linux) [DOC]; on Apple Silicon "no per-process [GPU] figure is available".
- Known limitations: `docs/known-issues.md` is an empty template. `ROADMAP.md` is the real list.

## 3. Rendering pipeline and graphics APIs [SOURCE unless noted]

- `wgpu = 30.0.1` with features `std, parking_lot, vulkan, metal, wgsl, naga-ir`, plus `dx12` on Windows and `gles` on everything that is not macOS (`crates/horizon-client-render/Cargo.toml`, `crates/horizon-client/Cargo.toml`). Note that `vulkan` is enabled on every target, and `gles` is enabled on iOS because the cfg is `not(target_os = "macos")`.
- Patched forks in `third_party/`: `wgpu-hal` 30.0.1 (Vulkan descriptor-pool fix for Intel, the Metal HDR colour-space lookup fix, a Windows fence fix), `naga` 30.0.1 (GLSL front-end fixes for shader packs), `libmimalloc-sys` 0.1.49 (POPCNT fix). The root `Cargo.toml` says to drop them when upstream releases the fixes. On iOS, the Metal HDR fix is irrelevant but the fork must keep building.
- Shaders are WGSL, translated by naga at runtime. Tests translate every shader to HLSL and (dev-dependency) MSL, so Metal translation is already exercised in tests [SOURCE: `crates/horizon-client/Cargo.toml` dev-dependencies].
- Device setup (`gfx.rs:2230-2345`): `PowerPreference::HighPerformance`, requests **the adapter's own limits** and a feature mask intersected with what the adapter offers (`MAPPABLE_PRIMARY_BUFFERS`, `SUBGROUP`, `TEXTURE_ATOMIC`, `FLOAT32_FILTERABLE`, `MULTI_DRAW_INDIRECT_COUNT`, ...), `MemoryHints::MemoryUsage`, a device-lost callback. Because each feature is masked by `adapter.features()`, an absent feature is not requested; whether every dependent code path has a fallback is **[OPEN]**.
- Terrain: all sections share one vertex arena (`gfx/arena.rs`), pages up to 256 MiB, 256-vertex grain, drawn with `multi_draw_indexed_indirect` (`gfx.rs:5763`). The optional GPU-culling path needs `MULTI_DRAW_INDIRECT_COUNT` (`gfx/gpu_culling.rs:517`), which Metal does not expose; it is off by default (`docs/graphics.md`). The occlusion/depth-test helper already has a Metal branch (`gfx/cull.rs:230`, `DepthTest::InShader`).
- Metal already works on macOS: `backend_order` returns the default backends when `os == "macos"` (`gfx.rs:1921-1936`). **iOS is not in that test**, so an iOS build would try Vulkan, then OpenGL, and never Metal. This is a confirmed one-line functional blocker.
- Formats: `Rgba8Unorm` colour, `Depth32Float` depth (9 call sites), `sample_count: 1` at the sites inspected (no MSAA by default). No compressed texture formats were seen [SOURCE, partial grep].

## 4. Subsystem trace [SOURCE unless noted]

| Subsystem | Where | iOS relevance |
|---|---|---|
| Resource decoding | `crates/horizon-assets` (models, baking, atlases), `horizon-client-render/src/packs*`, `sprites.rs`, `atlas_sources.rs` | Pure Rust. Compiles for iOS [COMPILED]. |
| Resource root | "`resources` beside the executable, else `.local/horizon-client/26.3`" (`game/launch_api.rs:31`); `--resources`, `--home`/`--game-dir`, `--session-file`, `--session-stdin` (`docs/api/launch.md`) | A clean seam for an iOS shell. Needs explicit paths because `.local` and "beside the executable" are meaningless in an app bundle. |
| World / chunks | `horizon-world` (regions, NBT, worldgen, datafix), `horizon-client-render/src/{chunk,chunk_map,world,streaming,light,mesher}.rs` | World crate compiles [COMPILED]. Residency is server-driven: `streaming.rs:1226 forget_columns` is called when the server unloads columns. |
| Meshing/streaming | One shared worker pool (`horizon-threads/src/jobs.rs`): classes Urgent/Near/Far, jobs ranked by distance, stale results dropped by sequence number | Good design for backpressure. Thread counts need re-tuning for 2+4 cores. |
| Entities / mobs | `horizon-client/src/mobs/*`, `horizon-client/src/game/*` | Large; not iOS-specific. |
| Networking | `horizon-protocol` (own codecs, RSA/AES-CFB8), `ureq 2` (rustls), `mio 1.2.4` (LAN host) | Protocol crate compiles [COMPILED]. `ureq`'s TLS needs `ring` (C build, unverified). |
| Audio | `cpal 0.16` (in the iOS graph with `coreaudio-rs 0.13`), `symphonia 0.5.5` (Ogg/Vorbis); static sounds cached LRU up to 64 MiB (`audio.rs:151`) | cpal does not configure `AVAudioSession`; interruptions/route changes **[OPEN]**. |
| Input | `winit 0.30.13`, `horizon-client-input` (keyboard, mouse, gamepad), cursor grab in `game/pointer_capture.rs` | **No touch handling found** (`grep WindowEvent::Touch` empty). Pointer lock does not exist on iOS. |
| Persistence | `horizon-client/src/save.rs`, `game/saving.rs`, region files in `horizon-world/src/region.rs` | Atomicity under suspension **[OPEN]**. |
| Allocator | `mimalloc` v3 (vendored), `purge_delay = 0`, `release_free_memory()` calls `mi_collect` plus `malloc_zone_pressure_relief` on Apple (`horizon-threads/src/lib.rs`) | C build, unverified on iOS. |
| Mods | `.hmod` content mods (data only), `.hnmod` native mods compiled into the game by the launcher, wasm runtime behind the `mods` feature (off) | See section 12. |

## 5. iOS dependency and compile matrix

### 5.1 Dependency graph [COMPILED]

`cargo tree --target aarch64-apple-ios -p horizon-client -e normal` resolves (690 unique entries). Relevant entries:

| In the iOS graph | Comment |
|---|---|
| `wgpu 30.0.1`, `wgpu-hal` (fork), `wgpu-core-deps-apple`, `raw-window-metal`, `objc2-metal`, `objc2-ui-kit 0.2.2`, `winit 0.30.13` | The Metal/UIKit path is present. |
| `ash 0.38` | Pulled by `horizon-hdr` (cfg `not(macos)`) and by the always-on `vulkan` feature. Dead weight on iOS. |
| `arboard 3.7.0` → `wl-clipboard-rs`, `wayland-client 0.31.15`, `wayland-backend 0.3.17`, `x11rb 0.13.2` | arboard treats iOS as generic Unix. **Does not compile** (below). |
| `gilrs 0.11.2` / `gilrs-core 0.6.8` | Optional (`controllers`, on by default). iOS support not verified. |
| `cpal 0.16.0`, `coreaudio-rs 0.13.0` | Present. |
| `ring 0.17.14`, `rustls 0.23.46` | Via `ureq`. C build. |
| No `glow`, `khronos-egl` | The `gles` feature did not pull a GL stack on iOS. Good. |

### 5.2 Per-crate `cargo check --target aarch64-apple-ios`

| Crate(s) | Result | Evidence |
|---|---|---|
| `horizon-core`, `-world`, `-protocol`, `-assets`, `-client-core`, `-items`, `-command`, `-optifine`, `-shaders`, `-shader-api`, `-theme`, `-mods`, `-narrator`, `-friends`, `-dualsense`, `-anticheat`, `-chat-filter` | **Pass**, unmodified repo | [COMPILED] |
| `horizon-client-input` | **Fail** (unmodified): `arboard` → `wayland-backend 0.3.17` does not compile on iOS (`CMSG_CLOEXEC`, `NOSIGNAL`, 5 errors). With arboard stubbed, **4 more errors** at `keybinds.rs:303-312`: `winit::platform::modifier_supplement` / `key_without_modifiers` do not exist on iOS | [COMPILED] |
| `horizon-hdr` | **Fail**: `wgpu::hal::api::Vulkan` and `hal::vulkan` do not exist on Apple targets (`src/lib.rs:17, 38, 62`). The crate is cfg'd `not(macos)`, which includes iOS | [COMPILED] |
| `horizon-threads` | **Fail** (seen once the C build was bypassed): `libc::proc_taskinfo`, `proc_pidinfo`, `PROC_PIDTASKINFO` do not exist for iOS (`process_memory.rs:68-75`). The cfg is `target_vendor = "apple"`, which includes iOS | [COMPILED] |
| `ring`, `libmimalloc-sys` | Not buildable on this host (need `xcrun`/iOS SDK) | environment limit |
| `horizon-cosmetics`, `-telemetry`, `-bench`, `-server-files` | Not conclusively checked (no result recorded) | [OPEN] |
| `horizon-client-render` | **Type-checks** only in the throwaway copy after the four fixes in Appendix B | [COMPILED], modified copy |
| `horizon-client-lib`, `horizon-client` (library and binary) | **Type-check** in the throwaway copy with the same patches (the 2m43s run reported `Finished`, no errors; the 630k-line gameplay library produced its metadata) | [COMPILED], modified copy |

Reading of the matrix: after four small source fixes (clipboard, HDR, key-modifier lookup, Apple memory counters) and with TLS and the allocator's C build set aside, **the whole game graph type-checks for `aarch64-apple-ios`**. The compile-level distance to an iOS build is therefore short. The distance to a working app is not (sections 6 to 10), and C-dependent crates (`ring`, mimalloc) still need a real macOS/Xcode build to confirm.

Confirmed concrete blockers (each needs a code change, none needs a redesign): arboard/clipboard, `horizon-hdr`, `keybinds.rs` modifier lookup, Apple process-memory counters, and the `os == "macos"` backend test.

## 6. iOS lifecycle and native integration

Facts [SOURCE]:

- The game is a `winit::ApplicationHandler` (`game/window_handler.rs:201`). `resumed` returns early if a window exists, else creates the window, then the renderer. There is **no `suspended` and no `memory_warning`** implementation anywhere in the game crates (grep over `horizon-client`, `-render`, `-input`).
- Rendering state lives in `Renderer`; `gfx.rs` references `recreate_surface`, so surface re-creation exists in some form (written for desktop display changes). Whether it survives iOS surface loss is **[OPEN]**.
- Several loops use `std::thread::available_parallelism()`/`worker_count()`; Apple builds already count only performance cores via `sysctl hw.perflevel0.physicalcpu` (`horizon-threads/src/lib.rs`). Whether that sysctl exists on A14 iOS is **[OPEN]** (it falls back to all 6 cores).
- `data_dir()` (`horizon-client-render/src/logging.rs:31`) has no iOS branch. It would use `$XDG_DATA_HOME` or `$HOME/.local/share` inside the app container. Workable, but it should be Application Support, flagged excluded from backup where appropriate.
- The code spawns processes in several places: `xdg-open` (`pad_menu.rs:2230`), `Command::new(program)` (`packs/folder.rs:18`, `title/world_management.rs:129`, `crash.rs:234`), `ps` (`telemetry.rs:334`), `sysctl` (`crash.rs:289`, `horizon-bench/src/hardware.rs`), current-exe re-spawn for the dedicated server and splitscreen. iOS apps cannot spawn processes; each must be cfg'd out or replaced. **[ASSUMPTION]** that none is on the single-player startup path; not traced.

Integration options:

| Option | Description | Assessment |
|---|---|---|
| **A. winit-owned** | A Rust staticlib exposes one `extern "C"` entry that runs `entry::main_with(...)`; winit creates the `UIApplication`/`UIWindow`; Swift only provides the launcher UI before handing over | **Recommended first.** The game is already a winit `ApplicationHandler`, so the least code changes. Needs a new small crate with unsafe allowed (the main crate forbids it). |
| B. Swift-owned view | Swift/UIKit owns a `UIView` + `CAMetalLayer`, Rust gets a raw window handle and is driven by `CADisplayLink` | More native control (lifecycle, safe areas, keyboard) but Horizon's loop is built on winit's event model, so this is a large rework. Revisit only if A cannot meet lifecycle or input needs. |
| C. SwiftUI-only game view | Not suitable for a high-rate Metal surface | Rejected for the game view; fine for the launcher screens. |

Required lifecycle work: drop the surface on `suspended` and rebuild on `resumed`; stop issuing GPU work in the background (iOS terminates apps that submit Metal work from the background); quiesce the worker pool; save on resign-active; respond to `memory_warning` by freeing caches (`release_free_memory`, mesh/scratch pools, sound cache). Audio: configure `AVAudioSession` category, handle interruptions and route changes (cpal does not) [ASSUMPTION].

## 7. Rendering compatibility with Metal

What is solid: Metal is already the macOS backend, shaders are translated in tests, feature use is masked by what the adapter reports, and unified memory removes the "pages that no longer fit in VRAM spill to system memory" concern (`MAPPABLE_PRIMARY_BUFFERS`).

What is unverified on an A14 [ASSUMPTION/OPEN]:

1. Which of the requested features the A14 reports (`SUBGROUP`, `TEXTURE_ATOMIC`, `FLOAT32_FILTERABLE`, `RG11B10UFLOAT_RENDERABLE`, `VERTEX_WRITABLE_STORAGE`). Run the existing adapter log (`describe_adapter`) on the device first.
2. `max_buffer_size` against the 256 MiB arena pages. Request a smaller `PAGE_BYTES` on iOS if the limit is lower, and watch page waste.
3. `multi_draw_indexed_indirect` without native multi-draw: expect wgpu to emit one indirect draw per entry. Cost on iPhone CPU is unknown; capture in Xcode's Metal System Trace.
4. Runtime WGSL→MSL compilation and pipeline creation time at startup. The project caches DX12 shaders on disk (`dx12_shader_cache`); no equivalent exists for Metal. Measure cold start before designing one.
5. Depth32Float at phone resolution times the number of depth targets in `gfx.rs`, with no MSAA. Count and size them; consider a lower internal resolution.
6. `ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER`/software-adapter logic and `HighPerformance` power preference are harmless on iOS (one GPU).
7. Shader packs (Iris/OptiFine GLSL → naga) and HDR are desktop features; leave them off on iOS.

## 8. Authentication and ownership

### 8.1 What Horizon actually does [DOC: `docs/microsoft-account.md`, verified in text only]

- The **launcher**, not the game, signs in: Microsoft device-code flow (`login.microsoftonline.com/consumers`), then Xbox Live, XSTS, `api.minecraftservices.com/authentication/login_with_xbox`, `entitlements/mcstore` (must own `game_minecraft` or `product_minecraft`), `minecraft/profile`.
- **Owner gate**: the launcher starts the game only if an account owns Java Edition; the last answer is cached with the tokens so being offline does not block a player who passed before. The game binary itself, `horizon-ctl`, and the server are not gated; there is no bypass flag in the launcher.
- **Download**: only on the player's machine, only after the gate, straight from Mojang (`launcher/src/mojang.rs`): version JSON, client jar, libraries, asset index, assets, each checked against Mojang's size and SHA-1; resumable. "CI, agents, harnesses and the repo never download or keep Mojang's files." The repo ships no Mojang code or assets (`tools/check_public_tree.py` enforces it).
- **Tokens**: OS keychain per account; never in argv (a short-lived session file or stdin handoff, mode 0600, deleted at read); never in logs.
- **Registration**: an Azure public-client app (personal accounts only, no secret), and Mojang's "App ID review" form (new apps get HTTP 403 until approved, days to weeks). "Never use another application's client ID."

### 8.2 Comparison with your intended approach

| Your stated approach | Horizon | Gap for iOS |
|---|---|---|
| User must own Java Edition | Same (launcher owner gate) | Rebuild the gate natively; the reusable part is only `account::session` (no GPUI). |
| Verify ownership before resource acquisition | Same | Same flow, new client ID, new Mojang review. |
| Assets come from the user's own legitimate download, not our servers | Same (Mojang endpoints) | Same. |
| Don't bundle or host Mojang assets | Same | Same. Applies to **derived data** too: see section 9. |
| Engine is independent Rust code | Same; but the **preparation** step runs Mojang's classes | This is the one place Horizon's pipeline differs from a pure "download and decode" model. |
| Prepare locally | Yes, on the desktop with a JVM | Cannot be done on the phone as it works today. |

### 8.3 Questions for human/legal review (no conclusions made here)

- Does Apple's App Review accept an app that downloads a third-party game's files and runs an unofficial client for it? Check the current App Review Guidelines, the Developer Program License Agreement, and distribution channel (App Store, TestFlight, alternative distribution where available).
- Minecraft Usage Guidelines, the Minecraft EULA, and Mojang's API terms for a mobile client that signs players in.
- Whether "computed version tables published from CI" (upstream #1750) contain Mojang content; upstream itself conditions the idea on this.
- Whether storing prepared (derived) data on the device, or transferring it from a desktop, changes any of the above.
- Whether an own Azure app registration and Mojang approval are obtainable for this app.

## 9. Resource import and preparation pipeline

### 9.1 What it does today [DOC + SOURCE]

`horizon-prepare` (`crates/horizon-prepare`, ~8k lines) imports the player's client jar, libraries and asset store, verifies them against `research-lock.json` (pins SHA-1s of the manifest, `client.jar`, `server.jar`; `java_major: 25`), then:

- compiles Horizon's Java exporters with `javac --release 25` and runs them in 2 to 5 JVMs against Mojang's own classes (registries, block states, collision shapes, behaviour tables, gameplay facts, command trees, creative tabs) and vanilla's data generator [DOC: `docs/launcher-preparation.md`];
- extracts the jar's assets and data into `.local/horizon-client/26.3/` (23,204 resource files in the documented comparison runs);
- copies sounds (about 380 MB) from the launcher's asset store, hash-checked.

Measured by the project on desktop Linux: 45 to 120 s per fresh preparation; about 1.06 GB peak across all processes (JVMs run with `-XX:TieredStopAtLevel=1 -XX:+UseSerialGC`). The total prepared size on disk was not found in the docs **[OPEN: measure on a host first]**.

### 9.2 Why it cannot run on an iPhone

- There is no JVM on iOS; executable memory/JIT is not available to ordinary apps, and apps cannot spawn processes. [ASSUMPTION: confirm against current Apple documentation.]
- Even an embedded interpreter-only JVM would contradict the project goal (no JVM, small footprint) and would need the 26.3 jars loaded in-process: rejected.
- The exporters need Mojang's server jar, which the launcher downloads only for an owning account (`docs/native-client.md`, "Run from a checkout").

### 9.3 Options

| Option | What | Pros | Cons / open |
|---|---|---|---|
| **A. Host-side prepare, transfer to the phone** | The player runs the existing `horizon-prepare` on a Mac/PC with their own game, then moves the output (AirDrop, Files, local network, Xcode) into the app container | Unblocks every engine milestone immediately; zero new exporter code; output verified by the existing `--check` | Not a standalone phone experience; large transfer; legal/policy view of moving derived data must be reviewed |
| **B. Upstream #1750** | Publish the computed tables once from CI; the phone assembles the rest from the player's own jar/assets | Best end state; helps servers and phones; maintainer-endorsed idea | Contingent on tables holding no Mojang content (not yet established); needs coordination with maintainers; not available today |
| C. Port the exporters to Rust | Read class files / data directly in Rust | Fully on-device, no Java | Very large and risky: the exporters execute Mojang code (registries, collision, behaviour). Parity with the 371-file Java side is what the differential harness exists to protect. Last resort |
| D. Embedded JVM | Run the exporters in an interpreter-only JVM on iOS | Reuses code | Rejected (above) |

**Recommendation:** use **A** to unblock engineering now, track **B** upstream, and make the legal review a gate before any on-device download/prepare feature ships. Do not start **C** without a size estimate and the maintainers' view.

## 10. Memory-management risks (summary)

Full strategy: `docs/IOS_MEMORY_STRATEGY.md`. Headlines:

- The project reports a steady 1.1 to 1.7 GB working set at render distance 8 to 32 on a desktop Windows machine (README, `benchmarks/perf/windows-9800x3d-5080-2026-10-29.md`) [DOC]. That is **not** evidence for an iPhone: different GPU memory model, no per-process GPU figure on Apple, and the sampler is external.
- What helps: mimalloc with immediate purge, a single bounded-by-class worker pool, mesh scratch of about 150 KiB per worker, arena allocation for terrain, an LRU-capped 64 MiB sound cache, LZ4-compressed GUI atlas pixels, server-driven chunk forgetting with batched frees.
- What is unbounded or unknown: resident chunk count and CPU-side per-chunk data at a given distance, CPU copies of meshes after upload (`SectionMesh` keeps several vectors), texture atlas/mip sizes, distant-terrain (`LOD`) caches, resource-pack and entity-model caches, save buffers. These are inspection or measurement items, not conclusions.
- iOS counts a process by its **physical footprint** (including GPU allocations on unified memory) and terminates it under pressure. The repo currently reports resident size via `proc_pidinfo`, which neither compiles for iOS nor measures the right thing.

## 11. Performance and thermals [ASSUMPTION]

Horizon's published speed-ups are desktop numbers. The default worker policy (`cores - 1`, performance cores only on Apple) was tuned on an M4. An iPhone 12 has 2 performance and 4 efficiency cores (to be confirmed on the device). Expect to need a mobile preset: low render distance and simulation distance, fewer workers, a frame cap tied to thermal state, and no shader packs. All numbers must come from device measurements.

## 12. Compatibility limitations

- **Java mods (Fabric, Forge, NeoForge): will not run.** Horizon has no Java (README). The launcher carries over data and assets from their jars (`.mrpack`, CurseForge, Prism imports); that tooling is desktop-side.
- **Native mods (`.hnmod`)**: built by the launcher compiling Rust into the game. A signed iOS app cannot compile or load new native code, so these are unsupported.
- **Content mods (`.hmod`)**: data only, no code; plausible later. Wasm mods are behind the `mods` feature (off).
- **Shader packs**: unsupported at first (cost, naga GLSL path, memory).
- **Multiplayer**: joining online-mode servers is supported by the protocol crate. LAN hosting and Play with Friends (`socket2`, `if-addrs`, tunnel) touch the local network; iOS needs a usage description and a permission prompt. Disable at first.
- **Controllers**: `gilrs` unverified on iOS; use Apple's GameController framework later. DualSense over HID has no iOS backend (already handled upstream).
- **Input/UI**: no touch controls, no mobile interface, no on-screen keyboard/IME path verified. Biggest product gap after resources.
- **Discord presence, telemetry heartbeat, splitscreen, narrator**: compile out or ignore on iOS.
- **Version**: only 26.3 is supported; the game is pinned by `research-lock.json`.

## 13. Testing and benchmarking strategy

1. **CI gate (cheap, early):** a macOS runner job running `cargo check --target aarch64-apple-ios` per crate, growing the list until the whole game graph builds. Fail on regressions.
2. **Keep upstream parity intact:** every change cfg'd or additive; run the existing harness set a change touches (`tools/pr_ready.py`); never weaken parity checks.
3. **Host tests:** logic tests continue to run on the host. iOS-only code gets small unit tests plus on-device smoke tests (XCTest/UI test launching the app, asserting frame count and no crash).
4. **On-device benchmark mode:** an in-app run that writes a `horizon-bench` report (frame-time percentiles, startup, chunk counts, queue depths) to the app container; extract with `devicectl`/Finder. The existing `perf_bench.py` drives desktop processes from outside and cannot be reused as is.
5. **Memory:** Instruments (Allocations, VM Tracker, Metal System Trace, Game Performance), in-process `task_info(TASK_VM_INFO).phys_footprint`, `os_proc_available_memory`, and wgpu's allocator report. Detail in the memory strategy.
6. **Soak and lifecycle:** 20 to 30 minute sustained flights; 100 background/foreground cycles; memory-pressure injection; battery and thermal state logging.
7. **Device matrix:** at least iPhone 12 (floor) plus one newer device; always physical devices. The simulator is useful for build and UI only.

## 14. Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | No on-device preparation without Java | Certain | Blocks standalone product | Option A now, #1750 upstream, legal gate |
| 2 | Jetsam on 4 GB | Likely at high settings | Crashes | Measure first, mobile preset, memory-warning handling |
| 3 | Lifecycle/GPU-surface loss bugs | Likely | Crashes after backgrounding | M5 lifecycle milestone, soak tests |
| 4 | Account registration / Mojang approval / App Review | Unknown | Blocks distribution | Start legal review and app registration early |
| 5 | Metal feature or limit gaps on A14 | Possible | Rework of culling/arena | Log adapter on device in M2 |
| 6 | Startup shader/pipeline compile time | Possible | Slow first launch | Measure; consider caching |
| 7 | Touch UI scope | Certain | Large product work | Minimal controls first |
| 8 | Maintaining a fork of a very active upstream | Certain | Merge cost | Small upstreamable PRs, platform code in new crates |

## 15. Unresolved questions

- Does `hw.perflevel0.physicalcpu` exist on A14 iOS? (affects worker count)
- Total prepared-resources size on disk, and sizes of sounds/atlases once loaded.
- Which threads and queues are bounded in practice at a given render distance.
- Do the unchecked crates (`cosmetics`, `telemetry`, `bench`, `server-files`) compile for iOS?
- Does the audio path work through cpal on iOS without extra session handling?
- Is the `ring`/`rustls` build for iOS acceptable on a macOS CI host?
- Can `horizon-prepare`'s `--check` logic (no Java) be reused on the phone to validate transferred resources?
- Minimum iOS version: `wgpu-hal` metal probes up to `ios = 14.0` features; pick the deployment target from the lowest version the chosen device supports and the Metal feature set required.

## 16. Verdict

**Proceed with major architectural changes.** The core engine is portable; the platform layer (lifecycle, input, paths, memory measurement, backend selection) and the resource-preparation story are the work. Gate the investment with measurable milestones (`docs/IOS_IMPLEMENTATION_ROADMAP.md`). Reconsider if either of these fails: Metal renders correctly on the A14 with Horizon's renderer (M3 to M4), or a real world renders at a playable distance inside a footprint the device tolerates (M8).

## Appendix A: how the compile evidence was produced

Host: Linux x86_64, 1 core, 4 GB. Toolchain from conda-forge: `rust 1.99.1` plus `rust-std-aarch64-apple-ios` (installed with micromamba). Commands:

```sh
export CARGO_HOME=/tmp/cargo CARGO_TARGET_DIR=/tmp/target
cargo tree  --target aarch64-apple-ios -p horizon-client -e normal --prefix none
cargo check --target aarch64-apple-ios --keep-going --message-format short \
  -p horizon-core -p horizon-protocol -p horizon-assets -p horizon-client-core -p horizon-world
cargo check --target aarch64-apple-ios --keep-going --message-format short \
  -p horizon-items -p horizon-command -p horizon-optifine -p horizon-shaders -p horizon-shader-api \
  -p horizon-cosmetics -p horizon-theme -p horizon-telemetry -p horizon-mods -p horizon-bench \
  -p horizon-hdr -p horizon-narrator -p horizon-friends -p horizon-dualsense -p horizon-server-files \
  -p horizon-anticheat -p horizon-chat-filter
```

## Appendix B: throwaway modifications used only to get past blockers (in `/tmp/scratch`, not in this repo)

1. `arboard` replaced by a stub crate (`Clipboard::new/get_text/set_text`, `Error::ContentNotAvailable`).
2. `ureq` built without the `tls` feature (avoids the `ring` C build).
3. `third_party/libmimalloc-sys/build.rs` replaced by an empty `main` (no C compile; type-check only).
4. `horizon-hdr` and `gfx.rs:2327/2332` treat iOS like macOS (no Vulkan).
5. `keybinds.rs`: `key_without_modifiers()` replaced by `logical_key.clone()`; the `modifier_supplement` import removed.
6. `process_memory.rs`: the `target_vendor = "apple"` branch narrowed to `target_os = "macos"`.
These show *where* the next errors are. They are not the recommended fixes (see the roadmap, M1).

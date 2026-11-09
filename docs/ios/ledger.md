# iOS porting ledger

One row per change or finding that affects the iOS port. Status values: `done` (merged here), `branch` (on a branch/PR), `open`. Evidence tags as in `docs/IOS_FEASIBILITY_REPORT.md`.

## M1: whole game graph compiles for `aarch64-apple-ios` (branch `ios/m1-compile-clean`)

| ID | Crate / file | Change | Evidence | Upstreamable | Status |
|---|---|---|---|---|---|
| L-001 | `horizon-client-input` `Cargo.toml`, `clipboard.rs` | `arboard` becomes a `cfg(not(target_os = "ios"))` dependency (it treats iOS as X11/Wayland and pulls `wayland-backend`, which does not compile). On iOS `SystemClipboard` keeps text copied in the game and pastes it back; a native `UIPasteboard` bridge is later work | [COMPILED] iOS; [SOURCE] desktop paths unchanged (same code under `not(ios)`) | Yes | branch |
| L-002 | `horizon-client-input` `keybinds.rs` (`learn_layout`) | iOS: take the layout character from `event.logical_key` because winit has no `modifier_supplement` there. Other targets unchanged | [COMPILED] iOS | Yes | branch |
| L-003 | `horizon-hdr` `lib.rs`, `Cargo.toml`; `horizon-client-render` `gfx.rs` (device open) | Vulkan-only HDR code and the `ash` dependency are for `not(any(macos, ios))`; iOS takes the same no-op branch as macOS | [COMPILED] iOS and macOS | Yes | branch |
| L-004 | `horizon-client-render` `gfx.rs` `backend_order` + test | `os == "ios"` returns the default (Metal) backends like `"macos"`. Without it an iOS build would try Vulkan then OpenGL and never Metal. Test extended | [COMPILED]; unit test runs on the host (see PR) | Yes | branch |
| L-005 | `horizon-threads` `process_memory.rs` | Apple split: macOS keeps `proc_pidinfo` unchanged; iOS reads the Mach `task_info(TASK_VM_INFO)`. New `footprint_bytes()` (Apple) returns `phys_footprint`, the figure memory pressure is judged by. libc has no `task_vm_info` and no `mach_task_self_` on iOS, so the struct (through `phys_footprint`, 38 words) and the extern are declared locally | [COMPILED] iOS and macOS (lib and tests). **Layout and values not verified on a device**: the struct follows `<mach/task_info.h>` from memory of the header; confirm against Xcode's memory gauge in M3 | Yes, after M3 validation | branch |

### Verification performed for M1 (Linux host, rustc 1.99.1)

- `cargo check --target aarch64-apple-ios -p horizon-client` (whole game graph): passes in a scratch copy that carries **only** environment workarounds, which are not in the branch: `ureq` built without `tls` (its `ring` C build needs the Xcode SDK) and `libmimalloc-sys/build.rs` emptied (its C build needs the SDK).
- `cargo check --target aarch64-apple-darwin -p horizon-threads -p horizon-hdr -p horizon-client-input`, and `--tests` for `horizon-threads`: pass.
- `cargo test -p horizon-threads --lib` on Linux: 19 passed.
- Linux/Windows behaviour: every change is under a `cfg` that is identical to the previous behaviour on those targets.

### Not done in M1 (still open)

| ID | Item | Why it matters |
|---|---|---|
| L-006 | Real link of `ring` (TLS) and `libmimalloc-sys` for iOS | **Closed at the `cargo check` level by CI** (job `rust-ios-check`, whole `horizon-client` graph, real iOS SDK, about 2.5 min). A real *link* of the game binary is still open |
| L-007 | CI job `cargo check --target aarch64-apple-ios` (M0) | Keeps the graph compiling |
| L-008 | An iOS build profile that turns off `vulkan`, `gles`, `controllers`, `discord`, `friends`, `heartbeat` | They compile for iOS today but are dead weight or unverified at runtime |
| L-009 | Gate process spawns (`xdg-open`, `ps`, `sysctl`, current-exe re-spawn) | iOS cannot spawn processes; none is known to be on the single-player start-up path |
| L-010 | `horizon-cosmetics`, `-telemetry`, `-bench`, `-server-files`, `-prepare` not conclusively checked for iOS | Unknown |
| L-011 | `hw.perflevel0.physicalcpu` on A14 | Decides the default worker count |
| L-012 | Metal HDR and `max_buffer_size` vs 256 MiB arena pages | Needs the adapter log from a device (M2) |

## M2: hello Metal on a device (branch `ios/m2-xcode-ci`)

| ID | Item | Evidence | Status |
|---|---|---|---|
| L-020 | New crate `crates/horizon-ios` (winit + wgpu cube, `horizon_ios_main`), `platform/ios/` (XcodeGen spec, C `main`), ADR 0001 | [COMPILED] `cargo check --target aarch64-apple-ios -p horizon-ios` on the Linux host; 6 unit tests in the crate (math, vertex and texture sizes) | branch |
| L-021 | Link set for the Rust static library (UIKit, Foundation, CoreFoundation, CoreGraphics, QuartzCore, Metal, `-lobjc -liconv -lc -lm`) | `rustc --print native-static-libs` on the Linux host | branch; confirmed only when the Xcode build in CI links |
| L-022 | macOS CI: `.github/workflows/ios.yml` (whole-graph check with real C builds = L-006, Rust libs, Xcode build, unsigned IPA, simulator smoke) | **Ran green** on GitHub's macOS arm64 runners: both jobs pass, unsigned IPA of about 1.8 MB produced. The simulator launches the app and it renders at 49 to 60 fps for 25 s with no crash report | branch |
| L-023 | Upstream workflows parked under `.github/upstream-parked/` | the repo is public; upstream workflows use self-hosted runners, secrets and schedules | done on `main` |
| L-024 | Bug found by the simulator smoke test: `request_device` with `Limits::default()` fails on iOS Metal (default asks 16 inter-stage variables, Metal offers 15). `horizon-ios` now requests `adapter.limits()`, as Horizon's `gfx.rs` already does | [CI, simulator] first run failed with that exact message; later runs pass | branch |
| L-025 | Adapter data from the **iOS simulator** ("Apple iOS simulator GPU", via the CI VM; **not an A14**): `max_buffer_size` 268435456 (256 MiB, equal to Horizon's largest arena page), `max_texture_dimension_2d` 8192, `max_inter_stage_shader_variables` 15, `max_storage_buffer_binding_size` 256 MiB; features include `TEXTURE_COMPRESSION_ASTC`, `TEXTURE_COMPRESSION_ETC2`, `SHADER_F16`, `FLOAT32_BLENDABLE`, `DUAL_SOURCE_BLENDING`, `CLIP_DISTANCES`, `TEXTURE_ATOMIC`, `MAPPABLE_PRIMARY_BUFFERS`; **absent**: `SUBGROUP`, `FLOAT32_FILTERABLE`, `MULTI_DRAW_INDIRECT_COUNT`. Needs the same log from a real iPhone 12 (L-011, L-012) | [CI, simulator] annotation of run on `9a13a7d`/`40a1c3c` | open: confirm on device |
| L-026 | Simulator screenshot rendered as text by CI (run on `778722a`): clear colour (63, 75, 97) = the sRGB form of the shader clear colour covers 86.5% of the 1206x2622 screenshot, a checkered cube is visible, 59 to 60 fps, process alive, no crash report. **Simulator only.** Open: orientation (the app declares landscape; the simulator screenshot was portrait), behaviour on an A14 | [CI, simulator] | observed |

## M3 and M4 (on `main`)

| ID | Item | Evidence | Status |
|---|---|---|---|
| L-030 | M3 telemetry: `horizon-ios` sampler thread (4 Hz) writes `Documents/telemetry/latest.json` and `samples.csv`: `phys_footprint` (Mach `task_info`), resident size, `os_proc_available_memory()`, thermal state and low-power mode (`NSProcessInfo` through objc2), frame p50/p99 and fps. Cube app draws it as an overlay (5x7 pixel font, no dependency). App declares file sharing so the Files app shows the reports | [COMPILED] iOS; 14 unit tests on the Linux host (math, glyphs, rasteriser, panel layout, JSON, percentiles) | on `main`; device values not yet seen |
| L-031 | `entry::set_process_args` hook in `horizon-client` (ADR 0002) so a host without a command line can start the game | [COMPILED] iOS; desktop path unchanged | on `main` |
| L-032 | M4: second app target running `horizon_client::entry::main()` with `--home/--game-dir/--resources` in the container; Rust library built with `--features game` | [COMPILED] `cargo check --target aarch64-apple-ios -p horizon-ios --features game` on the Linux host (scratch copy with the two C-build workarounds) | on `main`; run results pending (CI simulator, then device) |
| L-033 | CI builds the library with Horizon's client in it for device and simulator in parallel, derives `OTHER_LDFLAGS` from `rustc --print native-static-libs`, builds both apps and two unsigned IPAs | not yet run when written | on `main` |

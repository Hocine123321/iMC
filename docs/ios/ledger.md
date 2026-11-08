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
| L-006 | Real link of `ring` (TLS) and `libmimalloc-sys` for iOS | Needs a macOS host with Xcode; first job for the M0 CI runner |
| L-007 | CI job `cargo check --target aarch64-apple-ios` (M0) | Keeps the graph compiling |
| L-008 | An iOS build profile that turns off `vulkan`, `gles`, `controllers`, `discord`, `friends`, `heartbeat` | They compile for iOS today but are dead weight or unverified at runtime |
| L-009 | Gate process spawns (`xdg-open`, `ps`, `sysctl`, current-exe re-spawn) | iOS cannot spawn processes; none is known to be on the single-player start-up path |
| L-010 | `horizon-cosmetics`, `-telemetry`, `-bench`, `-server-files`, `-prepare` not conclusively checked for iOS | Unknown |
| L-011 | `hw.perflevel0.physicalcpu` on A14 | Decides the default worker count |
| L-012 | Metal HDR and `max_buffer_size` vs 256 MiB arena pages | Needs the adapter log from a device (M2) |

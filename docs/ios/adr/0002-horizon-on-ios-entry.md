# ADR 0002: how Horizon's own client starts on iOS (M4)

Status: accepted for M4.

## Decision

- A second app target (`iMC`, bundle `...imc.game`) calls `horizon_ios_game_main()`, which sets Horizon's command line and calls `horizon_client::entry::main()`. Horizon keeps creating its own winit event loop, window and renderer, as on desktop. The cube app stays as the known-good fallback and the telemetry reference.
- **Command line:** an iOS app starts with no arguments, so `horizon-client` gets a small, upstreamable hook: `entry::set_process_args` (a `OnceLock`), read by `entry::process_args()` instead of `std::env::args()` in `entry.rs` and `game.rs`. Desktop behaviour is unchanged.
- **Folders:** `--home` and `--game-dir` are `Library/Application Support/horizon` in the app container; `--resources` is `.../horizon/resources`.
- **Allocator:** the system allocator for now. Horizon's executable installs mimalloc as the global allocator; the iOS library does not. Whether mimalloc helps or hurts on iOS is a measurement for M14 (memory strategy, hypothesis 7), not a default.
- **Features off:** `audio`, `controllers`, `discord`, `friends`, `heartbeat` (ledger L-008). Audio waits for M10 (no `AVAudioSession` handling yet).
- **Link flags** come from `rustc --print native-static-libs` for the exact library being linked, passed to `xcodebuild` as `OTHER_LDFLAGS`, so they cannot drift from the Rust dependencies.

## Consequences

- Horizon's start-up code runs on iOS for the first time here: expect desktop assumptions to break at run time (process spawns, `/proc`, paths). They are found by running it (CI simulator, then the device) and recorded in the ledger.
- The telemetry sampler runs in both apps; the on-screen overlay exists only in the cube app until Horizon's own HUD can host it.

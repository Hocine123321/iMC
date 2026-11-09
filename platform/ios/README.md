# iOS apps (M2 hello Metal, M3 telemetry, M4 Horizon's client)

Two apps built from one Rust static library:

- **iMC Cube** (`Cube/`, bundle `io.github.hocine123321.imc`): starts Rust, opens a winit window and draws a spinning textured cube through wgpu on Metal, with an on-screen **telemetry overlay** (fps and frame times, the process's physical footprint, `os_proc_available_memory`, thermal state, low-power mode). It is not Horizon's renderer.
- **iMC** (`Game/`, bundle `io.github.hocine123321.imc.game`): Horizon's own client (`horizon_client::entry::main`), pointed at folders in the app container. With no prepared resources it shows Horizon's own "resources missing" screen, which proves its renderer, text and UI run on the phone (M4). Resources arrive in M6.

Both write `Documents/telemetry/latest.json` (every 5 s) and `samples.csv` (every second). The app declares file sharing, so they show up in the **Files app > On My iPhone > the app**. Please send those back: they are the first real device measurements. Plan: `docs/IOS_IMPLEMENTATION_ROADMAP.md`; decisions: `docs/ios/adr/`.

```
crates/horizon-ios/        Rust staticlib: horizon_ios_main() (cube, telemetry, overlay), horizon_ios_game_main() (feature `game`)
platform/ios/Cube/ Game/    main.c of each app; App/horizon_ios.h is the shared header
platform/ios/project.yml   XcodeGen spec; HorizonIOS.xcodeproj is generated, not committed
.github/workflows/ios.yml  macOS CI: whole-graph iOS check, Rust libs, Xcode build, unsigned IPA
```

## Build in CI (no Mac needed)

Every push to `main` or `ios/**`, and every pull request, runs `.github/workflows/ios.yml` on GitHub's macOS runners:

1. `rust-ios-check`: `cargo check --locked --target aarch64-apple-ios -p horizon-client`, with the real `ring` and mimalloc C builds against the iOS SDK (ledger item L-006).
2. `crate-tests`: the crate's unit tests on the macOS host.
3. `libs` (device and simulator in parallel): the release static library with Horizon's client in it (`--features game`, no LTO because this is a test build) and the link flags rustc reports.
4. `xcode`: `xcodegen generate`, both apps built unsigned for device and simulator with those link flags, two unsigned IPA artifacts (`iMC-cube-unsigned-ipa`, `iMC-game-unsigned-ipa`), and an informational simulator run of both apps whose results (stderr, crash reports, the telemetry report, Horizon's log, a text rendering of the screenshot) are posted as check annotations.

CI holds **no signing credentials**. The IPA is unsigned, so it will not install on an iPhone as is: it has to be signed with your own Apple developer identity (Xcode, or a re-signing tool of your choice) first. Apple's rules for device signing, provisioning and any account limits should be checked at https://developer.apple.com before relying on a path.

## Build on a Mac

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
export IPHONEOS_DEPLOYMENT_TARGET=15.0
cargo build --release --target aarch64-apple-ios     -p horizon-ios --features game
cargo build --release --target aarch64-apple-ios-sim -p horizon-ios --features game
mkdir -p platform/ios/build/rust/device platform/ios/build/rust/simulator
cp target/aarch64-apple-ios/release/libhorizon_ios.a     platform/ios/build/rust/device/
cp target/aarch64-apple-ios-sim/release/libhorizon_ios.a platform/ios/build/rust/simulator/
cd platform/ios && brew install xcodegen && xcodegen generate && open HorizonIOS.xcodeproj
```

Set your team under Signing & Capabilities, choose your iPhone, Run. The console shows the adapter, its features and limits (send these back: they answer ledger items L-011 and L-012) and the frame rate every five seconds.

## Try the Rust side on a desktop

`cargo run -p horizon-ios --example desktop` (Metal on a Mac, Vulkan elsewhere).

## What this does and does not prove

Proves, when it runs on a device: the toolchain, signing, winit lifecycle, wgpu surface and Metal pipeline work together. Does not prove anything about Horizon's renderer, memory, resources or performance.

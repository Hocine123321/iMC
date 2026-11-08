# iOS app (milestone M2: hello Metal)

A signed-able iOS app that starts Rust, opens a winit window and draws a spinning textured cube through wgpu on Metal. It is **not** Horizon's renderer yet (that is M4). Plan: `docs/IOS_IMPLEMENTATION_ROADMAP.md`; decisions: `docs/ios/adr/`.

```
crates/horizon-ios/        Rust staticlib: horizon_ios_main(), winit + wgpu app
platform/ios/App/          main.c (calls horizon_ios_main), horizon_ios.h
platform/ios/project.yml   XcodeGen spec; HorizonIOS.xcodeproj is generated, not committed
.github/workflows/ios.yml  macOS CI: whole-graph iOS check, Rust libs, Xcode build, unsigned IPA
```

## Build in CI (no Mac needed)

Every push to `main` or `ios/**`, and every pull request, runs `.github/workflows/ios.yml` on GitHub's macOS runners:

1. `rust-ios-check`: `cargo check --locked --target aarch64-apple-ios -p horizon-client`, with the real `ring` and mimalloc C builds against the iOS SDK (ledger item L-006).
2. `ios-app`: host unit tests for `horizon-ios`, release static libraries for device and simulator, `xcodegen generate`, an unsigned device build, an unsigned IPA artifact (`iMC-unsigned-ipa`), a simulator build, and an informational simulator launch with a screenshot.

CI holds **no signing credentials**. The IPA is unsigned, so it will not install on an iPhone as is: it has to be signed with your own Apple developer identity (Xcode, or a re-signing tool of your choice) first. Apple's rules for device signing, provisioning and any account limits should be checked at https://developer.apple.com before relying on a path.

## Build on a Mac

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
export IPHONEOS_DEPLOYMENT_TARGET=15.0
cargo build --release --target aarch64-apple-ios     -p horizon-ios
cargo build --release --target aarch64-apple-ios-sim -p horizon-ios
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

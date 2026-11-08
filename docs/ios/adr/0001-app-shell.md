# ADR 0001: app shell for the iOS build

Status: accepted for M2, to be revisited at M5 (lifecycle) and M11 (touch UI).

## Decision

- **winit owns the `UIApplication`** (feasibility report, section 6, option A). The Xcode app has a tiny C `main` that calls `horizon_ios_main()` in the Rust static library; winit starts `UIApplicationMain` and creates the window and the Metal-backed view.
- The Xcode project is described by `platform/ios/project.yml` (XcodeGen) and generated on demand, so no hand-edited `.pbxproj` is merged.
- Rust is built outside Xcode (`cargo build` per target) and linked as `libhorizon_ios.a`; the frameworks Rust needs come from `rustc --print native-static-libs` (UIKit, Foundation, CoreFoundation, CoreGraphics, QuartzCore, Metal; `-lobjc -liconv -lc -lm`).
- CI builds on GitHub macOS runners and produces an **unsigned** IPA. No signing material is stored in the repository or in CI.
- Deployment target **iOS 15.0** (placeholder). Not derived from a measurement: set it from the lowest iOS the supported devices run and the Metal features Horizon needs (feasibility report, section 15), then update this record.
- Bundle identifier `io.github.hocine123321.imc` is a placeholder.

## Why

The game is already a `winit::ApplicationHandler`, so this needs the least change. The main crate forbids unsafe code; the one FFI entry point lives in the new `horizon-ios` crate instead.

## Consequences and what would change this

- Lifecycle (`suspended`, `resumed`, memory warnings) is whatever winit delivers; if it cannot meet iOS needs (safe areas, keyboard, interruptions), move to a Swift-owned `UIView` with a `CAMetalLayer` (option B), which is a larger rework.
- Launcher screens (sign-in, downloads) will be native Swift later (M12) and hand over to this entry point.

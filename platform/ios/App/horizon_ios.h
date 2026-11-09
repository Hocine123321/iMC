#ifndef HORIZON_IOS_H
#define HORIZON_IOS_H

/// The hello-Metal cube with the telemetry overlay. Defined in Rust (`crates/horizon-ios`); winit
/// starts UIApplicationMain from it and does not return.
void horizon_ios_main(void);

/// Horizon's own client. Only in the library when it is built with the `game` feature.
void horizon_ios_game_main(void);

#endif

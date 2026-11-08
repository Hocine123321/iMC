#ifndef HORIZON_IOS_H
#define HORIZON_IOS_H

/// Runs the app. Defined in Rust (`crates/horizon-ios`); winit starts UIApplicationMain from it
/// and does not return.
void horizon_ios_main(void);

#endif

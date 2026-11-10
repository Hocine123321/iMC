//! Horizon's own client on iOS (milestone M4): `horizon_ios_game_main` points Horizon at folders
//! inside the app's Documents folder and starts it. Horizon creates its own winit event loop, window and
//! renderer, so on a phone with no prepared resources the first thing it shows is its own
//! "resources missing" screen: that proves its renderer, text and UI run here before any of the
//! game's files are involved. Resources arrive in M6.

use std::path::PathBuf;

/// The folder Horizon keeps its settings, logs and worlds in: inside Documents, so that the Files
/// app shows `horizon/logs/latest.log` (the app declares file sharing) and a tester can send it back.
/// Documents is also backed up by iOS; revisit when worlds are big (memory strategy, persistence).
fn documents_dir() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join("Documents"),
        None => std::env::temp_dir(),
    }
}

/// The command line Horizon is started with. `--home` is where it keeps its settings, logs and
/// worlds; `--resources` is where the prepared resources are expected.
pub fn launch_args(home: &std::path::Path) -> Vec<String> {
    vec![
        "--home".to_string(),
        home.display().to_string(),
        "--game-dir".to_string(),
        home.display().to_string(),
        "--resources".to_string(),
        home.join("resources").display().to_string(),
    ]
}

/// The function the "game" Xcode target's `main` calls (`platform/ios/App/horizon_ios.h`).
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_game_main() {
    crate::telemetry::start("game");
    let home = documents_dir().join("horizon");
    if let Err(e) = std::fs::create_dir_all(&home) {
        eprintln!("horizon-ios: cannot create {}: {e}", home.display());
    }
    eprintln!("horizon-ios: starting Horizon, home {}", home.display());
    horizon_client::entry::set_process_args(launch_args(&home));
    horizon_client::entry::main();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launch_arguments_name_the_home_and_resources() {
        let args = launch_args(std::path::Path::new("/data/horizon"));
        assert_eq!(args[0], "--home");
        assert_eq!(args[1], "/data/horizon");
        let at = args.iter().position(|a| a == "--resources").unwrap();
        assert_eq!(args[at + 1], "/data/horizon/resources");
    }
}

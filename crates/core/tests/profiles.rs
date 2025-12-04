use std::path::PathBuf;

use ttcore::{command::build_command, config::AppPaths, profile::ProfileSet};

#[test]
fn load_default_profiles() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    path.push("config/default_profiles.toml");
    let set = ProfileSet::load(&path).expect("profiles should load");
    assert!(!set.profiles.is_empty());
}

#[test]
fn build_command_includes_user_and_macro() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    path.push("config/default_profiles.toml");
    let set = ProfileSet::load(&path).expect("profiles should load");
    let profile = set.find("stg-batch").expect("profile exists");

    let paths = AppPaths::discover().unwrap();
    let config = ttcore::config::AppConfig::load_or_default(&paths).unwrap();
    let cmd = build_command(&profile, &config);

    assert!(cmd
        .args
        .iter()
        .any(|a| a.to_string_lossy().contains("user=\"deploy\"")));
    assert!(cmd
        .args
        .iter()
        .any(|a| a.to_string_lossy().contains("MACRO")));
}

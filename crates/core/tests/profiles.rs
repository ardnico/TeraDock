use std::ffi::OsString;
use std::path::PathBuf;

use ttcore::{
    command::build_command,
    config::{AppConfig, AppPaths},
    profile::{ClientKind, Profile, ProfileSet, Protocol},
};

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
fn build_windows_terminal_command_targets_ssh() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    path.push("config/default_profiles.toml");
    let set = ProfileSet::load(&path).expect("profiles should load");
    let profile = set.find("stg-batch").expect("profile exists");

    let paths = AppPaths::discover().unwrap();
    let mut config = AppConfig::load_or_default(&paths).unwrap();
    config.ssh_path = PathBuf::from("C:/Windows/System32/OpenSSH/ssh.exe");
    config.windows_terminal_path = Some(PathBuf::from("wt.exe"));

    let cmd = build_command(&profile, &config, None);

    assert_eq!(cmd.program.file_name().unwrap(), "wt.exe");
    assert_eq!(cmd.args.get(0), Some(&OsString::from("new-tab")));
    assert!(cmd
        .args
        .iter()
        .any(|a| a.to_string_lossy().contains("ssh.exe")));
}

#[test]
fn falls_back_to_plain_ssh_without_windows_terminal() {
    let profile = Profile {
        id: "plain-test".into(),
        name: "Plain SSH".into(),
        host: "example.com".into(),
        port: Some(2222),
        protocol: Protocol::Ssh,
        client_kind: ClientKind::WindowsTerminalSsh,
        user: Some("tester".into()),
        group: None,
        tags: vec![],
        danger_level: Default::default(),
        pinned: false,
        macro_path: None,
        color: None,
        description: None,
        extra_args: None,
        password: None,
        ssh_forwardings: vec![],
    };

    let paths = AppPaths::discover().unwrap();
    let mut config = AppConfig::load_or_default(&paths).unwrap();
    config.windows_terminal_path = None;
    config.ssh_path = PathBuf::from("ssh");

    let cmd = build_command(&profile, &config, None);

    assert_eq!(cmd.program.file_name().unwrap(), "cmd.exe");
    assert!(cmd.args.iter().any(|a| a == "/c"));
    assert!(cmd.args.iter().any(|a| a == "start"));
    assert!(cmd
        .args
        .iter()
        .any(|a| a.to_string_lossy().contains("tester@example.com")));
}

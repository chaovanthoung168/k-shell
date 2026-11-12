#[cfg(windows)]
use std::process::Command;

use kshell::{interactive::completion::complete, runtime::ShellState};

#[test]
fn specs_usage_and_completion_are_available_without_reading_hardware() {
    let cwd = std::env::current_dir().unwrap();
    let mut state = ShellState::new(cwd.clone());
    for name in ["sysinfo", "specs"] {
        let mut output = Vec::new();
        assert_eq!(
            state
                .execute(&format!("{name} --help"), &mut output)
                .unwrap()
                .status,
            0
        );
        assert!(String::from_utf8(output).unwrap().contains("CPU"));
        for arg in ["--unknown", "extra", "--help extra"] {
            let mut output = Vec::new();
            assert_eq!(
                state
                    .execute(&format!("{name} {arg}"), &mut output)
                    .unwrap_err()
                    .status,
                2
            );
            assert!(output.is_empty());
        }
        let (_, candidates) = complete(&name[..3], 3, &cwd, &[]);
        assert!(
            candidates
                .iter()
                .any(|c| c.replacement == format!("{name} "))
        );
        assert!(
            complete(&format!("{name} x"), name.len() + 2, &cwd, &[])
                .1
                .is_empty()
        );
    }
}

#[test]
#[cfg(windows)]
fn windows_snapshot_reports_physical_ram_and_local_capacity_without_ansi() {
    let specs = kshell::platform::system::collect().unwrap();
    let os = specs.os.as_ref().unwrap();
    assert!(!os.name.is_empty());
    assert!(os.build > 0);
    let user = specs.user.as_ref().unwrap();
    assert!(!user.is_empty());
    assert!(specs.logical_processors.is_some_and(|count| count > 0));
    assert!(!specs.cpu.unwrap().is_empty());
    let memory = specs.memory.unwrap();
    assert!(memory.total > 0);
    assert!(memory.available <= memory.total);
    let volumes = specs.volumes.unwrap();
    assert!(!volumes.is_empty());
    for volume in volumes {
        // Inaccessible/locked volumes remain visible with individual errors.
        if let Ok(capacity) = volume.capacity {
            assert!(capacity.total > 0);
            assert!(capacity.available <= capacity.total);
        }
    }
    for name in ["sysinfo", "specs"] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .args(["-c", name])
            .env("USERNAME", "kshell-environment-user-must-not-be-used")
            .env("OS", "kshell-environment-os-must-not-be-used")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        for category in ["OS", "User", "CPU", "GPU", "RAM", "Storage"] {
            assert!(text.contains(category), "{text}");
        }
        assert!(text.contains(&format!("  User     {user}\n")), "{text}");
        assert!(text.contains(&os.name), "{text}");
        assert!(!text.contains("kshell-environment-"));
        assert!(!text.contains('\x1b'));
        assert!(!text.contains("›"));
    }
}

#[test]
#[cfg(windows)]
fn native_ssh_fallback_preserves_arguments_and_status_without_network_access() {
    if kshell::platform::ssh::default_client().is_none() {
        eprintln!("OpenSSH Client is not installed; skipping native offline smoke test");
        return;
    }
    let shell = |input: &str| {
        Command::new(env!("CARGO_BIN_EXE_kshell"))
            .args(["-c", input])
            .env("PATH", "")
            .env("PATHEXT", ".EXE;.COM")
            .output()
            .unwrap()
    };
    let version = shell("ssh -V");
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stderr).contains("OpenSSH"));
    // -G prints configuration and exits. -F none disables all local config,
    // including Match exec hooks; this never attempts a server connection.
    let config = shell(
        "ssh -G -F none -o CanonicalizeHostname=no -p 2222 -l 'kshell test user' kshell-offline.invalid",
    );
    assert!(config.status.success(), "{:?}", config.stderr);
    let text = String::from_utf8(config.stdout).unwrap();
    assert!(text.contains("user kshell test user"), "{text}");
    assert!(text.contains("hostname kshell-offline.invalid"), "{text}");
    assert!(text.contains("port 2222"), "{text}");
    let invalid = shell("ssh -F none -o KShellInvalidOption=yes");
    assert_eq!(invalid.status.code(), Some(255));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("Bad configuration option"));
}

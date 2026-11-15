use kshell::{
    commands::OutputFormat,
    interactive::{completion::complete, prompt, welcome},
    presentation::{Appearance, IconMode},
    runtime::{ShellAction, ShellState},
    themes::{PromptTheme, ThemeStore},
};
use std::{fs, path::Path, process::Command};
mod common;
use common::TempDir;

#[test]
fn fresh_install_has_only_default_and_selection_survives_new_store() {
    let temp = TempDir::new();
    let root = temp.path().join("config");
    let store = ThemeStore::new(root.clone());
    assert_eq!(store.selected(), PromptTheme::Default);
    assert!(store.is_installed(PromptTheme::Default));
    assert!(!root.exists());
    for theme in PromptTheme::ALL.into_iter().skip(1) {
        assert!(!store.is_installed(theme));
        assert!(store.select(theme).is_err());
        store.install(theme).unwrap();
        store.install(theme).unwrap(); // Idempotent installation.
        store.select(theme).unwrap();
        assert_eq!(ThemeStore::new(root.clone()).selected(), theme);
    }
    store.select(PromptTheme::Default).unwrap();
    assert_eq!(store.selected(), PromptTheme::Default);
    fs::write(root.join("selected-theme"), "../../arbitrary").unwrap();
    assert_eq!(store.selected(), PromptTheme::Default);
    fs::write(root.join("selected-theme"), "cert").unwrap();
    fs::remove_file(root.join("themes/cert.theme")).unwrap();
    assert_eq!(store.selected(), PromptTheme::Default);
}

#[test]
fn installation_preserves_existing_files_and_failed_selection_preserves_previous() {
    let temp = TempDir::new();
    let store = ThemeStore::new(temp.path().into());
    fs::create_dir(temp.path().join("themes")).unwrap();
    let path = temp.path().join("themes/atomic.theme");
    fs::write(&path, "user data").unwrap();
    assert!(store.install(PromptTheme::Atomic).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "user data");
    store.select(PromptTheme::Default).unwrap();
    assert!(store.select(PromptTheme::Cert).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("selected-theme")).unwrap(),
        "default\n"
    );
}

fn plain(text: &str) -> String {
    let mut chars = text.chars();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[test]
fn all_five_designs_are_distinct_safe_and_have_identical_raw_and_styled_text() {
    let mut designs = std::collections::BTreeSet::new();
    for theme in PromptTheme::ALL.into_iter().skip(1) {
        for icons in [IconMode::Unicode, IconMode::Nerd, IconMode::Off] {
            let appearance = Appearance {
                prompt: theme,
                icons,
                ..Default::default()
            };
            let (raw, styled) = prompt::render_design(
                Path::new("D:/dev/ឯក"),
                "you\x1b[2J",
                Some("main\x07"),
                127,
                true,
                appearance,
            );
            assert_eq!(raw, plain(&styled));
            assert!(!raw.contains('\x1b'));
            assert!(!raw.contains('\x07'));
            assert!(!raw.contains("127"));
            assert!(!raw.contains("24.12")); // No fabricated tool versions.
            let no_color = prompt::render_design(
                Path::new("D:/dev/ឯក"),
                "you\x1b[2J",
                Some("main\x07"),
                0,
                false,
                appearance,
            );
            assert_eq!(no_color.0, no_color.1);
            assert_eq!(no_color.0, raw);
            if icons == IconMode::Unicode {
                designs.insert(raw);
            }
        }
    }
    assert_eq!(designs.len(), 5);
}

#[test]
fn theme_requests_picker_only_for_interactive_output_and_completes_install_names() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().into());
    let mut output = Vec::new();
    assert_eq!(
        state.execute("theme", &mut output).unwrap().action,
        ShellAction::Continue
    );
    assert!(!output.contains(&0x1b));
    output.clear();
    assert_eq!(
        state
            .execute_with_format(
                "theme",
                &mut output,
                OutputFormat {
                    terminal_width: Some(80),
                    ..Default::default()
                }
            )
            .unwrap()
            .action,
        ShellAction::ChooseTheme
    );
    assert!(output.is_empty());
    for theme in PromptTheme::ALL.into_iter().skip(1) {
        let line = format!("theme install {}", &theme.name()[..2]);
        let choices = complete(&line, line.len(), temp.path(), &[]).1;
        assert!(
            choices
                .iter()
                .any(|choice| choice.replacement == format!("{} ", theme.name()))
        );
    }
}

#[test]
fn command_install_is_explicit_and_invalid_input_never_creates_configuration() {
    let temp = TempDir::new();
    let root = temp.path().join("private-config");
    let run = |command: &str| {
        Command::new(env!("CARGO_BIN_EXE_kshell"))
            .env("KSHELL_CONFIG_DIR", &root)
            .args(["-c", command])
            .output()
            .unwrap()
    };
    for command in ["theme list", "theme", "theme status", "echo batch"] {
        assert!(run(command).status.success());
        assert!(!root.exists());
    }
    for command in [
        "theme atomic",
        "theme install ../../bad",
        "theme install unknown",
        "theme install cert extra",
    ] {
        assert!(!run(command).status.success());
        assert!(!root.exists());
    }
    assert!(run("theme install atomic").status.success());
    let store = ThemeStore::new(root.clone());
    assert!(store.is_installed(PromptTheme::Atomic));
    assert_eq!(store.selected(), PromptTheme::Default);
    assert!(run("theme atomic").status.success());
    assert_eq!(store.selected(), PromptTheme::Atomic);
    assert!(run("theme default").status.success());
    assert_eq!(store.selected(), PromptTheme::Default);
}

#[test]
fn welcome_is_explicit_or_interactive_and_uses_k_shell_branding() {
    let mut output = Vec::new();
    welcome::write(&mut output, false, 80).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("Welcome to K Shell"));
    assert!(!text.contains("oh my zsh"));
    assert!(!text.contains('\x1b'));
    let mut rainbow = Vec::new();
    welcome::write(&mut rainbow, true, 80).unwrap();
    let rainbow = String::from_utf8(rainbow).unwrap();
    for rgb in [
        "224;92;84",
        "224;187;64",
        "93;194;98",
        "81;157;212",
        "173;107;215",
    ] {
        assert!(rainbow.contains(&format!("\x1b[38;2;{rgb}m")));
    }
    assert_eq!(plain(&rainbow), text);
    assert!(
        !rainbow
            .split("Welcome to K Shell")
            .nth(1)
            .unwrap()
            .contains('\x1b')
    );
    let mut narrow = Vec::new();
    welcome::write(&mut narrow, false, 30).unwrap();
    assert!(
        !String::from_utf8(narrow)
            .unwrap()
            .contains(welcome::LOGO[0])
    );
    let batch = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "echo hello"])
        .output()
        .unwrap();
    assert_eq!(batch.stdout, b"hello\n");
    let welcome = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .arg("--welcome")
        .output()
        .unwrap();
    assert!(welcome.status.success());
    assert_eq!(welcome.stdout, text.as_bytes());
}

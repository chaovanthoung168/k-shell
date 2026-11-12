use std::{fs, path::Path, process::Command};

use kshell::{
    commands::OutputFormat,
    interactive::{completion::complete, prompt},
    presentation::{Appearance, Icon, IconMode, Palette, Style},
    runtime::ShellState,
};
use unicode_width::UnicodeWidthStr;

mod common;
use common::TempDir;

fn run(state: &mut ShellState, command: &str, terminal: bool, color: bool) -> String {
    let mut output = Vec::new();
    let appearance = state.appearance();
    state
        .execute_with_format(
            command,
            &mut output,
            OutputFormat {
                terminal_width: terminal.then_some(26),
                color,
                appearance,
            },
        )
        .unwrap();
    String::from_utf8(output).unwrap()
}

fn strip_colors(text: &str) -> String {
    let mut chars = text.chars();
    let mut result = String::new();
    while let Some(character) = chars.next() {
        if character == '\x1b' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(character);
        }
    }
    result
}

#[test]
fn theme_changes_are_per_session_and_invalid_options_do_not_mutate_settings() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    assert!(run(&mut state, "theme", false, true).contains("Theme: ocean\nIcons: unicode"));
    run(&mut state, "theme dusk", false, false);
    run(&mut state, "theme icons nerd", false, false);
    assert_eq!(
        state.appearance(),
        Appearance {
            palette: Palette::Dusk,
            icons: IconMode::Nerd,
            ..Default::default()
        }
    );
    assert_eq!(
        ShellState::new(temp.path().to_path_buf()).appearance(),
        Appearance::default()
    );
    for mode in ["soft", "bold", "underline"] {
        let before = state.appearance();
        let response = run(
            &mut state,
            &format!("theme suggestions {mode}"),
            false,
            false,
        );
        assert!(response.contains(&format!("Suggestions: {mode}")));
        assert_eq!(state.appearance().palette, before.palette);
        assert_eq!(state.appearance().icons, before.icons);
        assert_eq!(state.appearance().suggestions.name(), mode);
    }
    for command in [
        "theme unknown",
        "theme icons unknown",
        "theme dusk extra",
        "theme icons off extra",
        "theme suggestions unknown",
        "theme suggestions bold extra",
    ] {
        let before = state.appearance();
        let mut output = Vec::new();
        assert_eq!(state.execute(command, &mut output).unwrap_err().status, 2);
        assert!(output.is_empty());
        assert_eq!(state.appearance(), before);
    }
    assert!(run(&mut state, "theme --help", false, false).contains("Nerd Font"));
    assert!(run(&mut state, "theme midnight", false, false).contains("Theme: midnight"));
    assert_eq!(state.appearance().palette, Palette::Midnight);
    assert!(
        complete("theme mi", 8, temp.path(), &[])
            .1
            .iter()
            .any(|p| p.replacement == "midnight ")
    );
    run(&mut state, "theme reset", false, false);
    assert_eq!(state.appearance(), Appearance::default());
}

#[test]
fn startup_choices_fall_back_independently_and_icons_never_decorate_plain_output() {
    assert_eq!(Appearance::from_values(None, None), Appearance::default());
    assert_eq!(
        Appearance::from_values(Some("bad"), Some("nerd")),
        Appearance {
            palette: Palette::Ocean,
            icons: IconMode::Nerd,
            ..Default::default()
        }
    );
    assert_eq!(
        Appearance::from_values(Some("dusk"), Some("bad")),
        Appearance {
            palette: Palette::Dusk,
            icons: IconMode::Unicode,
            ..Default::default()
        }
    );
    for mode in [IconMode::Unicode, IconMode::Nerd, IconMode::Off] {
        let appearance = Appearance {
            icons: mode,
            ..Default::default()
        };
        assert_eq!(appearance.decorate(Icon::Folder, "folder", false), "folder");
    }
}

#[test]
fn file_icons_distinguish_code_configuration_media_and_links() {
    for (name, expected) in [
        ("main.rs", Icon::Rust),
        ("app.TSX", Icon::TypeScript),
        ("index.js", Icon::JavaScript),
        ("main.py", Icon::Python),
        ("README.md", Icon::Markdown),
        ("Cargo.toml", Icon::Config),
        ("song.mp3", Icon::Audio),
        ("movie.mp4", Icon::Video),
        ("photo.png", Icon::Image),
        ("release.zip", Icon::Archive),
        (".gitignore", Icon::Git),
        ("notes.txt", Icon::File),
    ] {
        let path = Path::new(name);
        assert_eq!(Icon::file(path, Style::file(path)), expected, "{name}");
        assert!(expected.glyph(IconMode::Unicode).is_some());
        assert!(expected.glyph(IconMode::Nerd).is_some());
        assert!(expected.glyph(IconMode::Off).is_none());
    }
    assert_eq!(
        Icon::file(Path::new("folder.mp3"), Style::Directory),
        Icon::Folder
    );
    assert_eq!(Icon::file(Path::new("link.py"), Style::Link), Icon::Link);
}

#[test]
fn icon_columns_measure_full_labels_and_quoted_names_without_truncation() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("alpha")).unwrap();
    for name in ["beta", "delta", "gamma.exe"] {
        fs::write(temp.path().join(name), b"").unwrap();
    }
    let mut state = ShellState::new(temp.path().to_path_buf());
    let output = run(&mut state, "ls", true, false);
    #[cfg(windows)]
    assert_eq!(output, "📁 alpha   📄 delta\n📄 beta    ▶ gamma.exe\n");
    assert!(
        output
            .lines()
            .all(|line| UnicodeWidthStr::width(line) <= 26)
    );
    assert!(!output.contains('\x1b'));
    fs::write(temp.path().join("界 file.tsx"), b"").unwrap();
    let output = run(&mut state, "ls", true, true);
    assert!(output.contains("‹› '界 file.tsx'"));
    assert!(output.contains("📁 alpha"));
    run(&mut state, "theme icons off", false, false);
    assert!(!run(&mut state, "ls", true, false).contains("📁"));
    let output = run(&mut state, "ls", false, true);
    assert_eq!(output, "alpha\nbeta\ndelta\ngamma.exe\n界 file.tsx\n");
}

#[test]
fn tree_shares_icon_modes_and_palette_while_plain_output_stays_undecorated() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("app")).unwrap();
    fs::write(temp.path().join("app/main.rs"), b"").unwrap();
    fs::write(temp.path().join("track.mp3"), b"").unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let plain = run(&mut state, "tree", false, true);
    assert!(plain.starts_with(".\n├── app\n│   └── main.rs\n└── track.mp3\n"));
    assert!(!plain.contains('\x1b'));
    let decorated = run(&mut state, "tree", true, false);
    assert!(decorated.starts_with("📁 .\n├── 📁 app\n│   └── ‹› main.rs\n└── ♪ track.mp3\n"));
    run(&mut state, "theme dusk", false, false);
    run(&mut state, "theme icons nerd", false, false);
    let colored = run(&mut state, "tree", true, true);
    assert!(colored.contains("\x1b[1;38;2;199;174;242m\u{f07b} app\x1b[0m"));
    assert!(colored.contains("\u{e7a8} main.rs"));
    assert_eq!(run(&mut state, "tree", false, true), plain);
}

#[test]
fn prompt_icons_keep_visible_text_identical_and_failures_use_only_marker_color() {
    let path = Path::new("D:/Projects/My App");
    for palette in [Palette::Ocean, Palette::Dusk, Palette::Midnight] {
        for icons in [IconMode::Unicode, IconMode::Nerd, IconMode::Off] {
            let appearance = Appearance {
                palette,
                icons,
                ..Default::default()
            };
            let (raw, styled) = prompt::render_with_appearance(path, 127, true, appearance);
            assert_eq!(strip_colors(&styled), raw);
            assert!(!raw.contains("127"));
            assert!(raw.ends_with("\n› "));
            assert!(styled.ends_with(&Style::Error.paint_in("› ", true, palette)));
            let (success_raw, success_styled) =
                prompt::render_with_appearance(path, 0, true, appearance);
            assert_eq!(raw, success_raw);
            assert!(success_styled.ends_with(&Style::Success.paint_in("› ", true, palette)));
            let (raw, no_color) = prompt::render_with_appearance(path, 127, false, appearance);
            assert_eq!(raw, no_color);
        }
    }
}

#[test]
fn theme_completion_offers_only_its_choices_and_command_mode_is_plain() {
    let temp = TempDir::new();
    fs::write(temp.path().join("dusk-unrelated-file"), b"").unwrap();
    for (line, expected) in [
        ("the", "theme "),
        ("theme d", "dusk "),
        ("theme icons n", "nerd "),
        ("theme icons u", "unicode "),
        ("theme s", "suggestions "),
        ("theme suggestions s", "soft "),
        ("theme suggestions b", "bold "),
        ("theme suggestions u", "underline "),
    ] {
        let (_, candidates) = complete(line, line.len(), temp.path(), &[]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].replacement, expected);
    }
    assert!(complete("theme xyz", 9, temp.path(), &[]).1.is_empty());
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "theme dusk"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"Theme: dusk\nIcons: unicode\nSuggestions: soft\n"
    );
}

#[test]
fn suggestion_looks_preserve_visible_text_and_no_color_for_every_status_and_palette() {
    use kshell::presentation::SuggestionStyle;
    let text = "cker --config 'ឯក folder' ";
    for palette in [Palette::Ocean, Palette::Dusk, Palette::Midnight] {
        for look in [
            SuggestionStyle::Soft,
            SuggestionStyle::Bold,
            SuggestionStyle::Underline,
        ] {
            let appearance = Appearance {
                palette,
                suggestions: look,
                ..Default::default()
            };
            for status in [Style::Success, Style::Error, Style::Muted] {
                let styled = appearance.paint_suggestion(status, text, true);
                assert_eq!(strip_colors(&styled), text);
                assert_eq!(appearance.paint_suggestion(status, text, false), text);
            }
        }
    }
    let soft = Appearance::default().paint_suggestion(Style::Success, text, true);
    let bold = Appearance {
        suggestions: SuggestionStyle::Bold,
        ..Default::default()
    }
    .paint_suggestion(Style::Success, text, true);
    let underline = Appearance {
        suggestions: SuggestionStyle::Underline,
        ..Default::default()
    }
    .paint_suggestion(Style::Success, text, true);
    assert_ne!(soft, bold);
    assert_ne!(soft, underline);
    assert!(soft.starts_with("\x1b[22;"));
    assert!(underline.starts_with("\x1b[4;22;"));
    assert!(underline.ends_with("\x1b[0m "));
}

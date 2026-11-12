#[cfg(windows)]
use std::fs;
use std::{borrow::Cow, path::Path};

use kshell::{
    commands::COMMAND_NAMES,
    interactive::completion::ShellHelper,
    presentation::{Appearance, Style},
};
use rustyline::{
    Context,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    history::{DefaultHistory, History},
};

mod common;
use common::TempDir;

fn paint(style: Style, text: &str, color: bool) -> String {
    Appearance::default().paint_suggestion(style, text, color)
}

fn helper(path: &Path) -> ShellHelper {
    let mut helper = ShellHelper::new(path.to_path_buf());
    helper.set_colors_enabled(true);
    helper
}

fn preview(helper: &ShellHelper, typed: &str, full: &str) -> (String, String) {
    let mut history = DefaultHistory::new();
    history.add(full).unwrap();
    let hint = helper
        .hint(typed, typed.len(), &Context::new(&history))
        .unwrap();
    assert_eq!(format!("{typed}{hint}"), full);
    assert_eq!(helper.highlight(typed, typed.len()), typed);
    let colored = helper.highlight_hint(&hint).into_owned();
    (hint, colored)
}

#[test]
fn suggested_builtin_letters_turn_green_and_invalid_suggestions_turn_red() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    for name in COMMAND_NAMES {
        let full = match *name {
            "mkdir" | "touch" | "create" | "rm" | "delete" => format!("{name} example"),
            "cp" | "copy" | "mv" | "move" | "rename" | "update" => format!("{name} example other"),
            "write" | "append" => format!("{name} example.txt 'hello world'"),
            "open" => "open example.mp3".to_owned(),
            _ => (*name).to_owned(),
        };
        let (hint, colored) = preview(&helper, &name[..1], &full);
        assert_eq!(colored, paint(Style::Success, &hint, true));
    }
    for (typed, full, valid) in [
        ("cl", "clear", true),
        ("clo", "cloops-kshell-98261", false),
        ("cl", "clear", true),
    ] {
        let (hint, colored) = preview(&helper, typed, full);
        let style = if valid { Style::Success } else { Style::Error };
        assert_eq!(colored, paint(style, &hint, true));
        for action in [CmdKind::Other, CmdKind::MoveCursor, CmdKind::ForcedRefresh] {
            assert!(helper.highlight_char(typed, typed.len(), action));
        }
    }
}

#[test]
fn file_operation_suggestions_validate_arguments_without_modifying_files() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    std::fs::write(temp.path().join("keep"), b"unchanged").unwrap();
    for (line, valid) in [
        ("history 10", true),
        ("history 0", true),
        ("history --help", true),
        ("history -c", false),
        ("history -1", false),
        ("history 2 extra", false),
        ("write keep 'new text'", true),
        ("append keep -n -- 'more text'", true),
        ("write new ''", true),
        ("write new", false),
        ("append folder/ text", false),
        ("append -file text", false),
        ("append -- -file text", true),
        ("create new", true),
        ("create 'ឯក file.txt' second.txt", true),
        ("create", false),
        ("create new -x", false),
        ("create folder/", false),
        ("delete keep", true),
        ("rm -rf keep", true),
        ("copy keep new", true),
        ("cp -r keep new", true),
        ("move keep new", true),
        ("mv keep new", true),
        ("rename keep new", true),
        ("update keep new", true),
        ("rm keep -x", false),
        ("cp keep new -f", false),
        ("mv keep", false),
        ("rename keep folder/new", false),
    ] {
        let (hint, colored) = preview(&helper, &line[..1], line);
        assert_eq!(
            colored,
            paint(
                if valid { Style::Success } else { Style::Error },
                &hint,
                true
            )
        );
        assert_eq!(
            std::fs::read(temp.path().join("keep")).unwrap(),
            b"unchanged"
        );
        assert!(!temp.path().join("new").exists());
    }
}

#[test]
fn entire_suggestion_uses_full_command_status_with_quotes_and_unicode_preserved() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    for (typed, full, command, arguments, valid) in [
        ("  ec", "  echo hello world", "ho", " hello world", true),
        ("\t'e", "\t'echo' \"ឯក 界\"", "cho'", " \"ឯក 界\"", true),
        ("'ec'\"h", "'ec'\"ho\" a b", "o\"", " a b", true),
        (
            "ឯក-co",
            "ឯក-command 'argument with spaces'",
            "mmand",
            " 'argument with spaces'",
            false,
        ),
    ] {
        let (_, colored) = preview(&helper, typed, full);
        let style = if valid { Style::Success } else { Style::Error };
        assert_eq!(
            colored,
            paint(style, &format!("{command}{arguments}"), true)
        );
    }
    for line in ["", "   ", "| echo hello", "'echo", "kshell-unknown"] {
        assert!(matches!(
            helper.highlight(line, line.len()),
            Cow::Borrowed(_)
        ));
        assert_eq!(helper.highlight(line, line.len()), line);
    }
}

#[test]
#[cfg(windows)]
fn executable_path_suggestions_are_green_but_scripts_documents_and_missing_paths_are_red() {
    let temp = TempDir::new();
    let executable = temp.path().join("kshell probe ឯក.exe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    fs::write(temp.path().join("probe.cmd"), b"must not run").unwrap();
    fs::write(temp.path().join("song.mp3"), b"must not open").unwrap();
    fs::create_dir(temp.path().join("folder.exe")).unwrap();
    let helper = helper(temp.path());
    let quoted = format!("'{}'", executable.display());
    let full = format!("{quoted} --version 'a b'");
    let (_, colored) = preview(&helper, &quoted[..3], &full);
    assert_eq!(colored, paint(Style::Muted, &full[3..], true));
    let (hint, colored) = preview(&helper, "'.\\kshell", "'.\\kshell probe ឯក.exe'");
    assert_eq!(colored, paint(Style::Success, &hint, true));
    for full in [
        ".\\missing.exe",
        ".\\probe.cmd",
        ".\\song.mp3",
        ".\\folder.exe",
    ] {
        let (hint, colored) = preview(&helper, ".\\", full);
        assert_eq!(colored, paint(Style::Error, &hint, true));
    }
}

#[test]
#[cfg(windows)]
fn suggestion_colors_follow_cd_and_recheck_deleted_files() {
    let temp = TempDir::new();
    let executable = temp.path().join("probe.exe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    fs::create_dir(temp.path().join("child")).unwrap();
    let mut helper = helper(temp.path());
    let (hint, colored) = preview(&helper, ".\\pro", ".\\probe.exe");
    assert_eq!(colored, paint(Style::Success, &hint, true));
    helper.set_cwd(&temp.path().join("child"));
    let (hint, colored) = preview(&helper, ".\\pro", ".\\probe.exe");
    assert_eq!(colored, paint(Style::Error, &hint, true));
    helper.set_cwd(temp.path());
    fs::remove_file(executable).unwrap();
    let (hint, colored) = preview(&helper, ".\\pro", ".\\probe.exe");
    assert_eq!(colored, paint(Style::Error, &hint, true));
}

#[test]
fn disabled_colors_leave_both_typed_text_and_suggestions_plain() {
    let temp = TempDir::new();
    let mut helper = helper(temp.path());
    helper.set_colors_enabled(false);
    for (typed, full) in [
        ("cl", "clear"),
        ("kshell-unknown", "kshell-unknown-command argument"),
    ] {
        let (hint, colored) = preview(&helper, typed, full);
        assert_eq!(colored, hint);
        assert!(!helper.highlight_char(typed, typed.len(), CmdKind::Other));
    }
    assert!(matches!(
        helper.highlight_hint("remaining suggestion"),
        Cow::Borrowed(_)
    ));
}

#[test]
fn unknown_history_suggestions_are_red_and_stale_context_is_not_reused() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    let (_, colored) = preview(
        &helper,
        "kshell-unknown",
        "kshell-unknown-in-history argument",
    );
    assert_eq!(colored, paint(Style::Error, "-in-history argument", true));
    let (hint, colored) = preview(&helper, "cl", "clear");
    assert_eq!(hint, "ear");
    assert_eq!(colored, paint(Style::Success, "ear", true));
    assert_eq!(
        helper.highlight_hint("unrelated suffix"),
        paint(Style::Muted, "unrelated suffix", true)
    );
    assert!(
        helper
            .hint("", 0, &Context::new(&DefaultHistory::new()))
            .is_none()
    );
    assert_eq!(
        helper.highlight_hint(&hint),
        paint(Style::Muted, &hint, true)
    );
}

#[test]
fn argument_suggestions_use_the_complete_command_status() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    for (typed, full, valid) in [
        ("echo h", "echo hello", true),
        ("cd so", "cd something", true),
        ("kshell-unknown a", "kshell-unknown argument", false),
    ] {
        let (hint, colored) = preview(&helper, typed, full);
        assert_eq!(
            colored,
            paint(
                if valid { Style::Success } else { Style::Error },
                &hint,
                true
            )
        );
    }
}

#[test]
fn invalid_docker_global_flags_color_the_entire_suffix_red_without_running_docker() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    // Docker availability is machine-specific; the pure schema tests cover it
    // everywhere. This integration check uses the installed adapter snapshot.
    if !kshell::interactive::completion::native_commands(temp.path())
        .iter()
        .any(|name| name.eq_ignore_ascii_case("docker"))
    {
        return;
    }
    for (typed, full) in [("do", "docker --sdf"), ("docker --s", "docker --sdf")] {
        let (hint, colored) = preview(&helper, typed, full);
        assert_eq!(colored, paint(Style::Error, &hint, true));
    }
    let (hint, colored) = preview(&helper, "do", "docker --help");
    assert_eq!(colored, paint(Style::Success, &hint, true));
    let (hint, colored) = preview(&helper, "do", "docker ps --unverified-option");
    assert_eq!(colored, paint(Style::Muted, &hint, true));
}

#[test]
fn execution_feedback_changes_generic_argument_suggestions_and_is_scoped_to_the_directory() {
    let temp = TempDir::new();
    let mut helper = helper(temp.path());
    let binary = format!("'{}'", std::env::current_exe().unwrap().display());
    let full = format!("{binary} --kshell-unverified");
    let (hint, colored) = preview(&helper, &binary, &full);
    assert_eq!(colored, paint(Style::Muted, &hint, true));
    helper.record_command_result(temp.path(), &full, 2);
    let (hint, colored) = preview(&helper, &binary, &full);
    assert_eq!(colored, paint(Style::Error, &hint, true));
    helper.record_command_result(temp.path(), &full, 0);
    let (hint, colored) = preview(&helper, &binary, &full);
    assert_eq!(colored, paint(Style::Success, &hint, true));
    helper.record_command_result(temp.path(), &full, 130);
    let (hint, colored) = preview(&helper, &binary, &full);
    assert_eq!(colored, paint(Style::Success, &hint, true));
}

#[test]
fn known_bad_builtin_flags_and_unsupported_operators_make_the_whole_hint_red() {
    let temp = TempDir::new();
    let helper = helper(temp.path());
    for (typed, full) in [
        ("ls -", "ls --sdf"),
        ("tr", "tree -L nope"),
        ("ec", "echo hello | cat"),
        ("cl", "clear extra"),
    ] {
        let (hint, colored) = preview(&helper, typed, full);
        assert_eq!(colored, paint(Style::Error, &hint, true));
    }
}

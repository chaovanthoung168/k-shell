use std::fs;

use kshell::interactive::completion::{ShellHelper, complete};
use kshell::language::parser::parse;
use rustyline::{
    Context,
    completion::Completer,
    hint::Hinter,
    history::{DefaultHistory, History},
};

mod common;
use common::TempDir;

#[test]
fn file_command_names_and_quoted_operands_complete_without_running_operations() {
    let temp = TempDir::new();
    fs::write(temp.path().join("file ឯក with space"), b"keep").unwrap();
    for name in [
        "create", "copy", "cp", "move", "mv", "rename", "update", "delete", "rm", "write", "append",
    ] {
        let (_, candidates) = complete(name, name.len(), temp.path(), &[]);
        assert!(candidates.iter().any(|c| c.replacement.trim() == name));
        let line = format!("{name} 'file");
        let (start, candidates) = complete(&line, line.len(), temp.path(), &[]);
        let candidate = candidates
            .iter()
            .find(|c| c.display.contains("file ឯក with space"))
            .unwrap();
        let completed = format!("{}{}", &line[..start], candidate.replacement);
        assert_eq!(
            parse(&completed).unwrap().unwrap().words[1].literal(),
            "file ឯក with space"
        );
    }
    assert_eq!(
        fs::read(temp.path().join("file ឯក with space")).unwrap(),
        b"keep"
    );
}

#[test]
fn text_commands_complete_the_filename_and_leave_content_alone() {
    let temp = TempDir::new();
    fs::write(temp.path().join("example.txt"), b"original").unwrap();
    fs::write(temp.path().join("-file.txt"), b"dash").unwrap();
    for command in ["write", "append"] {
        for (line, expected) in [
            (format!("{command} ex"), "example.txt"),
            (format!("{command} -- -fi"), "-file.txt"),
        ] {
            let (_, pairs) = complete(&line, line.len(), temp.path(), &[]);
            assert!(pairs.iter().any(|p| p.replacement.trim() == expected));
        }
        for line in [
            format!("{command} example.txt ex"),
            format!("{command} -- -file.txt ex"),
            format!("{command} example.txt "),
        ] {
            assert!(
                complete(&line, line.len(), temp.path(), &[]).1.is_empty(),
                "{line}"
            );
        }
    }
    assert_eq!(
        fs::read(temp.path().join("example.txt")).unwrap(),
        b"original"
    );
}

#[test]
fn prompt_uses_only_marker_color_for_failure_and_restores_success_color() {
    use kshell::{interactive::prompt, runtime::ShellState};
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    assert!(
        state
            .execute("kshell-design-missing-command-72981", &mut Vec::new())
            .is_err()
    );
    let (raw, styled) = prompt::render_with_status(state.cwd(), state.last_status(), true);
    assert_eq!(state.last_status(), 127);
    assert_eq!(raw, prompt::render(state.cwd()));
    assert!(raw.ends_with("\n› "));
    assert!(styled.ends_with("\x1b[1;38;2;226;141;147m› \x1b[0m"));
    assert!(!raw.contains('\x1b'));
    let (plain_raw, plain_styled) =
        prompt::render_with_status(state.cwd(), state.last_status(), false);
    assert_eq!(plain_raw, plain_styled);
    assert_eq!(plain_raw, raw);
    for status in [1, 2, 126, 127, 130, 255, -1] {
        let (failure_raw, failure_styled) = prompt::render_with_status(state.cwd(), status, true);
        assert_eq!(failure_raw, raw);
        assert!(!failure_styled.contains(&format!("[{status}]")));
        assert!(failure_styled.ends_with("\x1b[1;38;2;226;141;147m› \x1b[0m"));
        let (plain_raw, plain_styled) = prompt::render_with_status(state.cwd(), status, false);
        assert_eq!(plain_raw, raw);
        assert_eq!(plain_styled, raw);
    }
    state.execute("echo recovered", &mut Vec::new()).unwrap();
    let (raw, styled) = prompt::render_with_status(state.cwd(), state.last_status(), true);
    assert!(raw.ends_with("\n› "));
    assert!(!raw.contains("[127]"));
    assert!(styled.ends_with("\x1b[1;38;2;129;199;180m› \x1b[0m"));
}

#[test]
fn folder_prompt_follows_cd_and_keeps_the_directory_after_a_failed_cd() {
    use kshell::{interactive::prompt, platform::display_path, runtime::ShellState};
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("project ឯក with spaces")).unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let initial = prompt::render(state.cwd());
    state
        .execute("cd 'project ឯក with spaces'", &mut Vec::new())
        .unwrap();
    let updated = prompt::render(state.cwd());
    assert_ne!(updated, initial);
    assert_eq!(
        updated,
        format!(
            "📁 {}\n› ",
            display_path(
                &temp
                    .path()
                    .join("project ឯក with spaces")
                    .canonicalize()
                    .unwrap()
            )
        )
    );
    assert!(!updated.contains('\x1b'));
    assert!(state.execute("cd missing", &mut Vec::new()).is_err());
    assert_eq!(prompt::render(state.cwd()), updated);
}

#[test]
fn tab_accepts_the_visible_drive_suggestion_before_a_relative_directory() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("db")).unwrap();
    let helper = ShellHelper::new(temp.path().to_path_buf());
    let mut history = DefaultHistory::new();
    history.add(r"cd D:\").unwrap();
    let ctx = Context::new(&history);
    let line = "cd D";
    assert_eq!(helper.hint(line, line.len(), &ctx), Some(":\\".into()));
    let (start, candidates) = helper.complete(line, line.len(), &ctx).unwrap();
    assert_eq!(start, 3);
    assert_eq!(candidates[0].replacement, r"D:\");
    let completed = format!("{}{}", &line[..start], candidates[0].replacement);
    assert_eq!(
        parse(&completed).unwrap().unwrap().words[1].literal(),
        r"D:\"
    );
    #[cfg(windows)]
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.replacement == "db/")
    );
}

#[test]
fn tab_accepts_quoted_history_paths_without_losing_the_drive_or_spaces() {
    let temp = TempDir::new();
    let helper = ShellHelper::new(temp.path().to_path_buf());
    let mut history = DefaultHistory::new();
    history.add(r"cd 'D:\Program Files'").unwrap();
    let ctx = Context::new(&history);
    let line = r"cd 'D:\Program Fi";
    let (start, candidates) = helper.complete(line, line.len(), &ctx).unwrap();
    let completed = format!("{}{}", &line[..start], candidates[0].replacement);
    assert_eq!(
        parse(&completed).unwrap().unwrap().words[1].literal(),
        r"D:\Program Files"
    );
    assert_eq!(start, 3);
}

#[cfg(windows)]
#[test]
fn drive_prefix_tab_completes_to_the_root_instead_of_searching_the_current_directory() {
    let temp = TempDir::new();
    let drive = temp
        .path()
        .components()
        .next()
        .unwrap()
        .as_os_str()
        .to_str()
        .unwrap();
    let line = format!("cd {drive}");
    let (start, candidates) = complete(&line, line.len(), temp.path(), &[]);
    assert_eq!(start, 3);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].replacement, format!("{drive}\\"));
}

#[cfg(windows)]
#[test]
fn absolute_path_completion_retains_the_drive_and_typed_separator_style() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("folder one")).unwrap();
    for separator in ['\\', '/'] {
        let parent = temp
            .path()
            .to_str()
            .unwrap()
            .replace('\\', &separator.to_string());
        let line = format!("cd '{parent}{separator}fol");
        let (start, candidates) = complete(&line, line.len(), temp.path(), &[]);
        assert_eq!(candidates.len(), 1);
        let completed = format!("{}{}", &line[..start], candidates[0].replacement);
        assert_eq!(
            parse(&completed).unwrap().unwrap().words[1].literal(),
            format!("{parent}{separator}folder one{separator}")
        );
    }
}

#[test]
fn command_completion_keeps_clear_ahead_of_windows_programs_and_paths() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("Clear folder")).unwrap();
    let native_commands = vec!["ClipRenew".into(), "clear.exe".into(), "clear".into()];
    for prefix in ["cl", "cle"] {
        let (start, candidates) = complete(prefix, prefix.len(), temp.path(), &native_commands);
        assert_eq!(start, 0);
        assert_eq!(candidates[0].replacement, "clear ");
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| candidate.replacement == "clear ")
                .count(),
            1
        );
    }
    let (_, candidates) = complete("cle", 3, temp.path(), &native_commands);
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.replacement.contains("ClipRenew"))
    );
}

#[test]
fn hints_wait_for_an_argument_prefix_and_clear_never_completes_file_arguments() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("Application Data")).unwrap();
    fs::write(temp.path().join("example.txt"), b"").unwrap();
    let helper = ShellHelper::new(temp.path().to_path_buf());
    let mut history = DefaultHistory::new();
    history.add("cat example.txt").unwrap();
    for line in ["clear ", "cat ", "ClipRenew "] {
        assert_eq!(
            helper.hint(line, line.len(), &Context::new(&history)),
            None,
            "{line}"
        );
    }
    assert_eq!(
        helper.hint("cat ex", 6, &Context::new(&history)),
        Some("ample.txt".into())
    );
    assert!(complete("clear ", 6, temp.path(), &[]).1.is_empty());
    assert!(complete("clear ex", 8, temp.path(), &[]).1.is_empty());
    // Explicit Tab on an argument-taking command still offers directory paths.
    assert!(!complete("cd ", 3, temp.path(), &[]).1.is_empty());
}

#[test]
fn suggests_commands_without_history_and_prefers_matching_history() {
    let temp = TempDir::new();
    let helper = ShellHelper::new(temp.path().to_path_buf());
    let mut history = DefaultHistory::new();
    assert_eq!(
        helper.hint("mkd", 3, &Context::new(&history)),
        Some("ir ".into())
    );
    history.add("mkdir -p 'my project'").unwrap();
    assert_eq!(
        helper.hint("mkd", 3, &Context::new(&history)),
        Some("ir -p 'my project'".into())
    );
    assert!(helper.hint("", 0, &Context::new(&history)).is_none());
    assert!(helper.hint("mkdir", 2, &Context::new(&history)).is_none());
}

#[test]
fn tab_completion_quotes_paths_using_shell_rules_and_filters_cd_directories() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("dir ឯក space")).unwrap();
    fs::write(temp.path().join("dir file"), b"").unwrap();
    for input in ["cd dir", "cd 'dir", "cd \"dir"] {
        let (start, candidates) = complete(input, input.len(), temp.path(), &[]);
        assert_eq!(start, 3);
        assert_eq!(candidates.len(), 1);
        let completed = format!("{}{}", &input[..start], candidates[0].replacement);
        let parsed = parse(&completed).unwrap().unwrap();
        assert_eq!(parsed.words[1].literal(), "dir ឯក space/");
    }
    let (_, candidates) = complete("cat dir", 7, temp.path(), &[]);
    assert_eq!(candidates.len(), 2);
}

#[test]
fn path_completion_tracks_changed_directory_and_roundtrips_apostrophes() {
    let first = TempDir::new();
    let second = TempDir::new();
    fs::write(first.path().join("first.txt"), b"").unwrap();
    fs::write(second.path().join("second's file"), b"").unwrap();
    let mut helper = ShellHelper::new(first.path().to_path_buf());
    let history = DefaultHistory::new();
    assert_eq!(
        helper.hint("cat fir", 7, &Context::new(&history)),
        Some("st.txt".into())
    );
    helper.set_cwd(second.path());
    assert!(helper.hint("cat fir", 7, &Context::new(&history)).is_none());
    let (start, candidates) = complete("cat sec", 7, second.path(), &[]);
    assert_eq!(candidates.len(), 1);
    let completed = format!("{}{}", &"cat sec"[..start], candidates[0].replacement);
    assert_eq!(
        parse(&completed).unwrap().unwrap().words[1].literal(),
        "second's file"
    );
}

#[test]
fn completion_handles_unicode_byte_offsets_and_unsupported_operators() {
    let temp = TempDir::new();
    fs::write(temp.path().join("ឯកសារ"), b"").unwrap();
    let input = "cat ឯក";
    let (start, candidates) = complete(input, input.len(), temp.path(), &[]);
    assert_eq!(start, 4);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].replacement, "ឯកសារ");
    assert!(complete(input, 5, temp.path(), &[]).1.is_empty());
    assert!(complete("ls | cat", 8, temp.path(), &[]).1.is_empty());
}

#[test]
fn persistent_history_roundtrips_and_ignores_leading_spaces() {
    let temp = TempDir::new();
    let path = temp.path().join("history.txt");
    let config = rustyline::Config::builder()
        .history_ignore_space(true)
        .build();
    let mut history = DefaultHistory::with_config(&config);
    history.add("echo remembered").unwrap();
    assert!(!history.add(" echo private").unwrap());
    history.append(&path).unwrap();
    let mut restored = DefaultHistory::with_config(&config);
    restored.load(&path).unwrap();
    let helper = ShellHelper::new(temp.path().to_path_buf());
    assert_eq!(
        helper.hint("echo rem", 8, &Context::new(&restored)),
        Some("embered".into())
    );
    assert_eq!(restored.len(), 1);
}

#[cfg(windows)]
#[test]
fn windows_display_paths_hide_only_drive_and_unc_verbatim_prefixes() {
    use kshell::platform::display_path;
    use std::path::Path;
    assert_eq!(display_path(Path::new(r"\\?\D:\Projects")), r"D:\Projects");
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share")),
        r"\\server\share"
    );
    assert_eq!(
        display_path(Path::new(r"\\?\Volume{example}\")),
        r"\\?\Volume{example}\"
    );
    assert_eq!(
        kshell::interactive::prompt::render(Path::new(r"\\?\D:\My-Portfolio")),
        "📁 D:\\My-Portfolio\n› "
    );
    assert_eq!(
        kshell::interactive::prompt::render(Path::new(r"\\?\UNC\server\share")),
        "📁 \\\\server\\share\n› "
    );
}

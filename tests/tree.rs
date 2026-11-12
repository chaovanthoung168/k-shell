use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use kshell::presentation::{Appearance, IconMode};
use kshell::{commands::OutputFormat, interactive::completion::complete, runtime::ShellState};

mod common;
use common::TempDir;

fn fixture(path: &Path) {
    fs::create_dir(path.join("alpha")).unwrap();
    fs::create_dir(path.join("empty")).unwrap();
    fs::write(path.join("alpha/a.txt"), b"a").unwrap();
    fs::write(path.join("z.txt"), b"z").unwrap();
    fs::write(path.join(".hidden"), b"hidden").unwrap();
}

fn run(state: &mut ShellState, input: &str) -> String {
    let mut output = Vec::new();
    assert_eq!(state.execute(input, &mut output).unwrap().status, 0);
    String::from_utf8(output).unwrap()
}

#[test]
fn tree_draws_sorted_branches_and_counts_entries_including_empty_directories() {
    let temp = TempDir::new();
    fixture(temp.path());
    let mut state = ShellState::new(temp.path().to_path_buf());
    assert_eq!(
        run(&mut state, "tree"),
        ".\n├── alpha\n│   └── a.txt\n├── empty\n└── z.txt\n\n2 directories, 2 files\n"
    );
    assert_eq!(state.cwd(), temp.path());
    let empty = temp.path().join("empty");
    assert_eq!(
        run(&mut ShellState::new(empty), "tree"),
        ".\n\n0 directories, 0 files\n"
    );
}

#[test]
fn depth_and_directory_flags_limit_the_visible_structure_and_counts() {
    let temp = TempDir::new();
    fixture(temp.path());
    fs::create_dir(temp.path().join("alpha/nested")).unwrap();
    fs::write(temp.path().join("alpha/nested/deep.txt"), b"").unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    assert_eq!(
        run(&mut state, "tree -L 1"),
        ".\n├── alpha\n├── empty\n└── z.txt\n\n2 directories, 1 file\n"
    );
    assert_eq!(
        run(&mut state, "tree -dL2"),
        ".\n├── alpha\n│   └── nested\n└── empty\n\n3 directories\n"
    );
    assert!(!run(&mut state, "tree -L2").contains("deep.txt"));
    assert!(run(&mut state, "tree -L 3").contains("deep.txt"));
    assert!(run(&mut state, "tree -aL1").contains("├── .hidden\n"));
}

#[test]
fn explicit_quoted_paths_and_cd_use_the_shell_directory_and_color_only_when_requested() {
    let temp = TempDir::new();
    let target = temp.path().join("project ឯក with spaces");
    fs::create_dir(&target).unwrap();
    fs::create_dir(target.join("child")).unwrap();
    fs::write(target.join("video with spaces.mp4"), b"").unwrap();
    for name in ["player.EXE", "song.mp3", "archive.zip"] {
        fs::write(target.join(name), b"").unwrap();
    }
    let mut state = ShellState::new(temp.path().to_path_buf());
    let plain = run(&mut state, "tree 'project ឯក with spaces'");
    assert!(plain.starts_with("'project ឯក with spaces'\n"));
    assert!(plain.contains("'video with spaces.mp4'"));
    assert!(!plain.contains('\x1b'));
    state
        .execute("cd 'project ឯក with spaces'", &mut Vec::new())
        .unwrap();
    let mut output = Vec::new();
    state
        .execute_with_format(
            "tree -d",
            &mut output,
            OutputFormat {
                terminal_width: Some(80),
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let colored = String::from_utf8(output).unwrap();
    assert!(colored.contains("\x1b[1;38;2;129;184;234mchild\x1b[0m"));
    assert!(!colored.contains("child/"));
    assert!(!colored.contains(".mp4"));
    let mut output = Vec::new();
    state
        .execute_with_format(
            "tree",
            &mut output,
            OutputFormat {
                terminal_width: Some(80),
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let colored = String::from_utf8(output).unwrap();
    assert!(colored.contains("\x1b[38;2;111;122;138m├── \x1b[0m"));
    assert!(colored.contains("\x1b[38;2;111;122;138m└── \x1b[0m"));
    assert!(colored.contains("\x1b[38;2;197;166;232m'video with spaces.mp4'\x1b[0m"));
    assert!(colored.contains("\x1b[38;2;129;199;180msong.mp3\x1b[0m"));
    assert!(colored.contains("\x1b[1;38;2;129;199;180mplayer.EXE\x1b[0m"));
    assert!(colored.contains("\x1b[38;2;216;178;122marchive.zip\x1b[0m"));
    let mut output = Vec::new();
    state
        .execute_with_format(
            "tree",
            &mut output,
            OutputFormat {
                terminal_width: None,
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert!(!output.contains(&0x1b));
}

#[test]
fn tree_rejects_bad_flags_depths_missing_paths_and_file_roots_with_actionable_errors() {
    let temp = TempDir::new();
    fixture(temp.path());
    let mut state = ShellState::new(temp.path().to_path_buf());
    for input in [
        "tree -x",
        "tree -L",
        "tree -L 0",
        "tree -L -1",
        "tree -L nope",
        "tree -L 999999999999999999999999999",
        "tree ''",
        "tree alpha empty",
    ] {
        let mut output = Vec::new();
        assert_eq!(
            state.execute(input, &mut output).unwrap_err().status,
            2,
            "{input}"
        );
        assert!(output.is_empty());
    }
    assert_eq!(
        state
            .execute("tree missing", &mut Vec::new())
            .unwrap_err()
            .status,
        1
    );
    assert!(
        state
            .execute("tree z.txt", &mut Vec::new())
            .unwrap_err()
            .message
            .contains("expected a directory")
    );
    fs::create_dir(temp.path().join("-directory")).unwrap();
    assert!(run(&mut state, "tree -- -directory").starts_with("-directory\n"));
    assert!(run(&mut state, "tree --help").contains("-L"));
}

#[test]
fn tree_handles_output_failures_and_command_mode_is_prompt_and_color_free() {
    struct BrokenOutput;
    impl Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let temp = TempDir::new();
    fixture(temp.path());
    let mut state = ShellState::new(temp.path().to_path_buf());
    assert_eq!(
        state.execute("tree", &mut BrokenOutput).unwrap_err().status,
        1
    );
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "tree -L 1"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        ".\n├── alpha\n├── empty\n└── z.txt\n\n2 directories, 1 file\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn tree_is_in_command_completion_and_its_path_completion_shows_directories_only() {
    let temp = TempDir::new();
    fixture(temp.path());
    fs::write(temp.path().join("alpha.txt"), b"").unwrap();
    assert_eq!(
        complete("tre", 3, temp.path(), &[]).1[0].replacement,
        "tree "
    );
    let (_, candidates) = complete("tree al", 7, temp.path(), &[]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].replacement, "alpha/");
}

#[test]
fn directory_links_are_leaves_and_cannot_form_recursive_loops() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("folder")).unwrap();
    let link = temp.path().join("folder/back-to-root");
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_dir(temp.path(), &link);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(temp.path(), &link);
    if let Err(error) = result {
        if cfg!(windows) && error.raw_os_error() == Some(1314) {
            eprintln!("symlink traversal test skipped: Windows symlink privilege is unavailable");
            return;
        }
        panic!("create test link: {error}");
    }
    let mut state = ShellState::new(temp.path().to_path_buf());
    let output = run(&mut state, "tree");
    assert_eq!(output.matches("back-to-root").count(), 1);
    assert!(output.contains(" -> "));
    assert!(output.ends_with("1 directory, 1 file\n"));
    assert!(
        state
            .execute("tree folder/back-to-root", &mut Vec::new())
            .unwrap_err()
            .message
            .contains("not traversed")
    );
}

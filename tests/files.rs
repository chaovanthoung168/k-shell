use std::fs;
use std::path::Path;

use kshell::runtime::ShellState;
mod common;
use common::TempDir;

fn run(state: &mut ShellState, command: &str) {
    let mut output = Vec::new();
    assert_eq!(state.execute(command, &mut output).unwrap().status, 0);
    assert!(output.is_empty());
}

#[test]
fn create_makes_empty_files_with_quoted_unicode_and_native_paths() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::create_dir(temp.path().join("folder")).unwrap();
    let absolute = temp.path().join("absolute.txt");
    run(
        &mut state,
        &format!(
            "create 'ឯក file.txt' folder/nested.txt '{}'",
            absolute.display()
        ),
    );
    run(&mut state, "create -- -file.txt");
    for file in [
        "ឯក file.txt",
        "folder/nested.txt",
        "absolute.txt",
        "-file.txt",
    ] {
        assert!(temp.path().join(file).is_file());
        assert!(fs::read(temp.path().join(file)).unwrap().is_empty());
    }
    assert_eq!(state.cwd(), temp.path());
}

#[test]
fn create_preflights_operands_and_never_overwrites_existing_entries() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let existing = temp.path().join("keep.txt");
    fs::write(&existing, b"keep\0\xff").unwrap();
    let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(86400);
    fs::OpenOptions::new()
        .write(true)
        .open(&existing)
        .unwrap()
        .set_modified(old_time)
        .unwrap();
    let timestamp = fs::metadata(&existing).unwrap().modified().unwrap();
    fs::create_dir(temp.path().join("folder")).unwrap();
    for command in [
        "create",
        "create ''",
        "create new.txt -x",
        "create new.txt keep.txt",
        "create new.txt folder",
        "create new.txt missing/file.txt",
        "create new.txt new.txt",
        "create folder/",
    ] {
        assert!(
            state.execute(command, &mut Vec::new()).is_err(),
            "{command}"
        );
        assert!(!temp.path().join("new.txt").exists());
        assert_eq!(fs::read(&existing).unwrap(), b"keep\0\xff");
        assert_eq!(
            fs::metadata(&existing).unwrap().modified().unwrap(),
            timestamp
        );
    }
    let mut output = Vec::new();
    state.execute("help", &mut output).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("create [--] file ...")
    );
}

#[test]
fn copy_move_rename_update_and_delete_preserve_binary_unicode_files() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("original ឯក.txt"), b"hello\0\xffworld").unwrap();
    fs::create_dir(temp.path().join("destination")).unwrap();
    run(&mut state, "copy 'original ឯក.txt' 'copy space.txt'");
    assert_eq!(
        fs::read(temp.path().join("copy space.txt")).unwrap(),
        b"hello\0\xffworld"
    );
    run(&mut state, "move 'copy space.txt' destination");
    assert!(!temp.path().join("copy space.txt").exists());
    run(
        &mut state,
        "rename 'destination/copy space.txt' renamed.txt",
    );
    run(&mut state, "update destination/renamed.txt updated.mp3");
    assert_eq!(
        fs::read(temp.path().join("destination/updated.mp3")).unwrap(),
        b"hello\0\xffworld"
    );
    run(
        &mut state,
        "delete destination/updated.mp3 'original ឯក.txt'",
    );
    assert!(!temp.path().join("destination/updated.mp3").exists());
    assert!(!temp.path().join("original ឯក.txt").exists());
    assert_eq!(state.cwd(), temp.path());
}

#[test]
fn recursive_copy_includes_empty_directories_and_move_needs_no_recursive_flag() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::create_dir_all(temp.path().join("source/empty/nested")).unwrap();
    fs::write(temp.path().join("source/file"), b"payload").unwrap();
    assert!(state.execute("cp source copy", &mut Vec::new()).is_err());
    assert!(!temp.path().join("copy").exists());
    run(&mut state, "cp -r source copy");
    assert!(temp.path().join("copy/empty/nested").is_dir());
    assert_eq!(fs::read(temp.path().join("copy/file")).unwrap(), b"payload");
    run(&mut state, "mv copy moved");
    run(&mut state, "rename moved renamed");
    assert!(temp.path().join("renamed/empty/nested").is_dir());
    assert!(state.execute("rm renamed", &mut Vec::new()).is_err());
    run(&mut state, "rm -r renamed");
    assert!(!temp.path().join("renamed").exists());
    assert!(temp.path().join("source/file").exists());
}

#[test]
fn conflicts_self_copy_and_nested_destinations_leave_originals_unchanged() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("a"), b"A").unwrap();
    fs::write(temp.path().join("b"), b"B").unwrap();
    fs::create_dir(temp.path().join("folder")).unwrap();
    fs::write(temp.path().join("folder/a"), b"existing").unwrap();
    for command in [
        "cp a b",
        "mv a b",
        "rename a b",
        "update a b",
        "cp a a",
        "mv a a",
        "cp a folder",
        "mv a folder",
        "cp -r folder folder/child",
        "mv folder folder/child",
    ] {
        assert!(
            state.execute(command, &mut Vec::new()).is_err(),
            "{command}"
        );
        assert_eq!(fs::read(temp.path().join("a")).unwrap(), b"A");
        assert_eq!(fs::read(temp.path().join("b")).unwrap(), b"B");
        assert_eq!(fs::read(temp.path().join("folder/a")).unwrap(), b"existing");
        assert!(!temp.path().join("folder/child").exists());
    }
}

#[test]
fn parse_all_flags_and_targets_before_mutation_and_support_dash_names() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("keep"), b"keep").unwrap();
    for command in [
        "rm keep -x",
        "delete keep -x",
        "cp keep copy -f",
        "mv keep moved -r",
        "rename keep new -x",
        "update keep new -x",
        "rm keep missing",
        "rename keep ../outside",
        "update keep folder/name",
        "cp keep",
        "mv",
        "rm",
        "copy keep one two",
        "delete ''",
    ] {
        assert!(
            state.execute(command, &mut Vec::new()).is_err(),
            "{command}"
        );
        assert_eq!(fs::read(temp.path().join("keep")).unwrap(), b"keep");
        assert!(!temp.path().join("copy").exists());
        assert!(!temp.path().join("moved").exists());
    }
    run(&mut state, "cp -- keep -file");
    run(&mut state, "mv -- -file -moved");
    run(&mut state, "rename -- -moved -renamed");
    run(&mut state, "rm -- -renamed");
    run(&mut state, "rm -f absent");
    run(&mut state, "rm -f absent-parent/absent");
    assert!(temp.path().join("keep").exists());
}

#[test]
fn deletion_preflights_all_protected_targets_without_touching_a_fixture() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("keep"), b"keep").unwrap();
    let mut targets = vec![
        temp.path().to_path_buf(),
        temp.path().parent().unwrap().to_path_buf(),
    ];
    targets.push(temp.path().ancestors().last().unwrap().to_path_buf());
    if let Some(home) = kshell::platform::home_directory() {
        targets.push(home.into());
    }
    #[cfg(windows)]
    if let Some(system) = std::env::var_os("SystemRoot") {
        targets.push(Path::new(&system).join("System32"));
        targets.push(system.into());
    }
    for target in targets {
        let quoted = format!("'{}'", target.display());
        let error = state
            .execute(&format!("rm -rf keep {quoted}"), &mut Vec::new())
            .unwrap_err();
        assert!(error.message.contains("refusing"), "{}", error.message);
        assert_eq!(fs::read(temp.path().join("keep")).unwrap(), b"keep");
        assert!(
            state
                .execute(&format!("mv {quoted} new-name"), &mut Vec::new())
                .is_err()
        );
        assert!(temp.path().exists());
    }
    for command in [
        "rm -rf keep .",
        "rm -rf keep ..",
        "mv . new-name",
        "rename . new-name",
    ] {
        assert!(
            state.execute(command, &mut Vec::new()).is_err(),
            "{command}"
        );
        assert!(temp.path().join("keep").exists());
    }
}

#[test]
fn overlapping_recursive_delete_targets_are_removed_once() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::create_dir_all(temp.path().join("folder/nested")).unwrap();
    fs::write(temp.path().join("folder/nested/file"), b"data").unwrap();
    run(
        &mut state,
        "rm -r folder/nested/file folder folder/nested folder",
    );
    assert!(!temp.path().join("folder").exists());
}

#[cfg(windows)]
#[test]
fn windows_devices_streams_and_drive_relative_paths_are_rejected() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("keep"), b"keep").unwrap();
    for target in [
        "NUL",
        "NUL.txt",
        "COM1",
        "COM¹",
        "LPT³",
        "CONOUT$",
        "file:stream",
        "trailing.",
        "C:relative",
        "C:",
        "\\\\.\\NUL",
    ] {
        assert!(
            state
                .execute(&format!("create '{target}'"), &mut Vec::new())
                .is_err()
        );
        for command in ["cp", "mv", "rename", "update"] {
            assert!(
                state
                    .execute(&format!("{command} keep '{target}'"), &mut Vec::new())
                    .is_err(),
                "{command} {target}"
            );
            assert_eq!(fs::read(temp.path().join("keep")).unwrap(), b"keep");
        }
    }
}

fn link_directory(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_dir(target, link);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(target, link);
    match result {
        Ok(()) => true,
        #[cfg(windows)]
        Err(error) if error.raw_os_error() == Some(1314) => {
            // Junctions exercise the same traversal boundary without requiring
            // developer mode. Both paths belong to this test's temporary dirs.
            create_junction(target, link).unwrap();
            true
        }
        Err(error) => panic!("create test-owned symlink: {error}"),
    }
}

#[cfg(windows)]
fn create_junction(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::{
        Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT},
        System::{
            IO::DeviceIoControl, Ioctl::FSCTL_SET_REPARSE_POINT,
            SystemServices::IO_REPARSE_TAG_MOUNT_POINT,
        },
    };
    let mut substitute: Vec<u16> = target.canonicalize()?.as_os_str().encode_wide().collect();
    assert_eq!(&substitute[..4], &[92, 92, 63, 92]); // canonical \\?\ prefix
    substitute[1] = 63; // NT object-manager \??\ prefix
    let length = u16::try_from(substitute.len() * 2).unwrap();
    // REPARSE_DATA_BUFFER header + MountPointReparseBuffer offsets, followed by
    // the substitute name and an empty print name (both NUL terminated).
    let mut buffer = IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes().to_vec();
    for value in [length + 12, 0, 0, length, length + 2, 0] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    for value in substitute.into_iter().chain([0, 0]) {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    fs::create_dir(link)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(link)?;
    let mut returned = 0;
    // SAFETY: valid owned handle, initialized input buffer with its exact size,
    // valid result pointer, and no overlapped/asynchronous operation.
    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[test]
fn recursive_copy_rejects_links_and_recursive_delete_removes_only_the_link() {
    let temp = TempDir::new();
    let outside = TempDir::new();
    fs::write(outside.path().join("keep"), b"safe").unwrap();
    fs::create_dir(temp.path().join("source")).unwrap();
    if !link_directory(outside.path(), &temp.path().join("source/link")) {
        return;
    }
    let mut state = ShellState::new(temp.path().to_path_buf());
    assert!(state.execute("cp -r source copy", &mut Vec::new()).is_err());
    assert!(!temp.path().join("copy").exists());
    for command in [
        "rm source/link/keep",
        "create source/link/new",
        "write source/link/keep changed",
        "append source/link/keep changed",
        "write source/link changed",
        "append source/link changed",
        "mv source/link/keep moved",
        "cp source/link/keep copied",
    ] {
        assert!(
            state.execute(command, &mut Vec::new()).is_err(),
            "{command}"
        );
    }
    assert!(!outside.path().join("new").exists());
    run(&mut state, "rm -r source");
    assert_eq!(fs::read(outside.path().join("keep")).unwrap(), b"safe");
}

#[cfg(windows)]
#[test]
fn cross_drive_moves_copy_completely_before_removing_the_original() {
    let source = TempDir::new();
    let destination = TempDir::new_in(&Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
    if source.path().components().next() == destination.path().components().next() {
        eprintln!("cross-drive test requires temp and repository on different drives");
        return;
    }
    let mut state = ShellState::new(source.path().to_path_buf());
    fs::create_dir_all(source.path().join("folder/empty")).unwrap();
    fs::write(source.path().join("folder/ឯក file"), b"cross-drive\0\xff").unwrap();
    let target = destination.path().join("moved folder");
    run(&mut state, &format!("mv folder '{}'", target.display()));
    assert!(!source.path().join("folder").exists());
    assert_eq!(
        fs::read(target.join("ឯក file")).unwrap(),
        b"cross-drive\0\xff"
    );
    assert!(target.join("empty").is_dir());
    fs::write(source.path().join("single"), b"file").unwrap();
    run(
        &mut state,
        &format!("move single '{}'", destination.path().display()),
    );
    assert!(!source.path().join("single").exists());
    assert_eq!(
        fs::read(destination.path().join("single")).unwrap(),
        b"file"
    );
    fs::write(source.path().join("single"), b"keep original").unwrap();
    assert!(
        state
            .execute(
                &format!("mv single '{}'", destination.path().display()),
                &mut Vec::new()
            )
            .is_err()
    );
    assert_eq!(
        fs::read(source.path().join("single")).unwrap(),
        b"keep original"
    );
    assert_eq!(
        fs::read(destination.path().join("single")).unwrap(),
        b"file"
    );
}

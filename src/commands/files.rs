//! Conservative filesystem operations. Parse first, plan recursive work without
//! following links, then mutate. These operations are not filesystem transactions.
use std::{
    collections::HashSet,
    fs::{self, Metadata, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

use crate::{
    error::ShellError,
    platform::{self, filesystem as native},
};

#[derive(Clone, Copy)]
enum Operation {
    Create,
    Delete,
    Copy,
    Move,
    Rename,
}

struct Request<'a> {
    operation: Operation,
    recursive: bool,
    force: bool,
    paths: Vec<&'a str>,
}

fn parse<'a>(command: &str, args: &'a [String]) -> Result<Request<'a>, ShellError> {
    let (operation, flags, usage) = match command {
        "create" => (Operation::Create, "", "[--] file ..."),
        "rm" | "delete" => (Operation::Delete, "rf", "[-rf] [--] path ..."),
        "cp" | "copy" => (Operation::Copy, "r", "[-r] [--] source destination"),
        "mv" | "move" => (Operation::Move, "", "[--] source destination"),
        "rename" | "update" => (Operation::Rename, "", "[--] source new-name"),
        _ => unreachable!("dispatch only calls the file command parser for known names"),
    };
    let (flags, paths) = super::utilities::options(args, flags, command)?;
    let count_ok = match operation {
        Operation::Create | Operation::Delete => !paths.is_empty(),
        _ => paths.len() == 2,
    };
    if !count_ok {
        return Err(ShellError::new(format!("usage: {command} {usage}"), 2));
    }
    for path in &paths {
        native::validate_file_operand(Path::new(path))
            .map_err(|e| failure(command, Path::new(path), e))?;
    }
    if matches!(operation, Operation::Rename) && !leaf_name(paths[1]) {
        return Err(ShellError::new(
            format!("{command}: new-name must be one filename; use move to change folders"),
            2,
        ));
    }
    if matches!(operation, Operation::Create) && paths.iter().any(|p| p.ends_with(['/', '\\'])) {
        return Err(ShellError::new(
            "create: expected a filename, not a trailing directory separator",
            2,
        ));
    }
    Ok(Request {
        operation,
        recursive: flags.contains(&'r'),
        force: flags.contains(&'f'),
        paths,
    })
}

fn leaf_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(Component::Normal(_)))
        && parts.next().is_none()
        && !name.contains(['/', '\\'])
}

pub(super) fn valid_syntax(command: &str, args: &[String]) -> bool {
    parse(command, args).is_ok()
}

fn failure(command: &str, path: &Path, error: impl std::fmt::Display) -> ShellError {
    ShellError::new(
        format!("{command}: {}: {error}", platform::display_path(path)),
        1,
    )
}

/// Check the original ancestor spelling before canonicalizing, so a junction
/// cannot silently redirect a destructive operation. The final entry is a leaf.
fn check_ancestors(command: &str, path: &Path) -> Result<(), ShellError> {
    for parent in path.ancestors().skip(1) {
        if parent.as_os_str().is_empty() {
            continue;
        }
        let metadata = match fs::symlink_metadata(parent) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(failure(command, parent, e)),
        };
        if native::is_link_or_reparse_point(&metadata) {
            return Err(failure(
                command,
                parent,
                "refusing to traverse a link or reparse point",
            ));
        }
    }
    Ok(())
}

pub(super) fn resolve(command: &str, cwd: &Path, operand: &str) -> Result<PathBuf, ShellError> {
    let path = cwd.join(operand);
    check_ancestors(command, &path)?;
    // Canonicalize the parent only: preserve the final link as a removable leaf.
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => Ok(parent
            .canonicalize()
            .map_err(|e| failure(command, parent, e))?
            .join(name)),
        _ => path.canonicalize().map_err(|e| failure(command, &path, e)),
    }
}

fn within(path: &Path, directory: &Path) -> bool {
    #[cfg(windows)]
    {
        // Canonical paths resolve on-disk casing. Compare components rather than
        // string prefixes (C:\\Users2 is not inside C:\\Users).
        let mut path = path.components();
        directory.components().all(|part| {
            path.next().is_some_and(|p| {
                p.as_os_str().to_string_lossy().to_lowercase()
                    == part.as_os_str().to_string_lossy().to_lowercase()
            })
        })
    }
    #[cfg(not(windows))]
    {
        path.starts_with(directory)
    }
}

fn guard_source(command: &str, source: &Path, cwd: &Path) -> Result<(), ShellError> {
    let link = fs::symlink_metadata(source).is_ok_and(|m| native::is_link_or_reparse_point(&m));
    let canonical = if link {
        source.to_path_buf()
    } else {
        source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf())
    };
    let mut protected = vec![cwd.to_path_buf()];
    if let Some(home) = platform::home_directory() {
        protected.push(home.into());
    }
    #[cfg(windows)]
    let mut system_paths: Vec<PathBuf> = Vec::new();
    #[cfg(not(windows))]
    let system_paths: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    for name in [
        "SystemRoot",
        "WINDIR",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
    ] {
        if let Some(path) = std::env::var_os(name) {
            system_paths.push(path.into());
        }
    }
    let root = canonical.file_name().is_none();
    if root
        || protected
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .any(|p| within(&p, &canonical))
        || system_paths
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .any(|p| within(&p, &canonical) || within(&canonical, &p))
    {
        return Err(failure(
            command,
            source,
            "refusing to delete or move a root, home, system folder, current folder or its ancestors",
        ));
    }
    Ok(())
}

struct Entry {
    path: PathBuf,
    directory: bool,
    link: bool,
}

/// Iterative preflight, including all descendants, before writes start. Copy
/// rejects links; deletion records them as leaves, never following junctions.
fn scan(
    command: &str,
    root: &Path,
    recursive: bool,
    copying: bool,
) -> Result<Vec<Entry>, ShellError> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries = Vec::new();
    while let Some(path) = pending.pop() {
        check_ancestors(command, &path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| failure(command, &path, e))?;
        if copying {
            native::validate_file_operand(&path).map_err(|e| failure(command, &path, e))?;
        }
        let link = native::is_link_or_reparse_point(&metadata);
        let directory = metadata.is_dir() && !link;
        if link && copying {
            return Err(failure(
                command,
                &path,
                "copying links/reparse points is not supported",
            ));
        }
        if directory && !recursive {
            return Err(failure(command, &path, "directory requires -r"));
        }
        if !directory && !link && !metadata.is_file() {
            return Err(failure(command, &path, "unsupported file type"));
        }
        if directory {
            for child in fs::read_dir(&path).map_err(|e| failure(command, &path, e))? {
                pending.push(child.map_err(|e| failure(command, &path, e))?.path());
            }
        }
        entries.push(Entry {
            path,
            directory,
            link,
        });
    }
    Ok(entries)
}

fn current_metadata(command: &str, entry: &Entry) -> Result<Metadata, ShellError> {
    check_ancestors(command, &entry.path)?;
    let metadata =
        fs::symlink_metadata(&entry.path).map_err(|e| failure(command, &entry.path, e))?;
    if native::is_link_or_reparse_point(&metadata) != entry.link
        || (metadata.is_dir() && !entry.link) != entry.directory
    {
        return Err(failure(
            command,
            &entry.path,
            "entry changed since preflight; operation stopped",
        ));
    }
    Ok(metadata)
}

fn remove_entries(command: &str, entries: &mut [Entry], force: bool) -> Result<(), ShellError> {
    entries.sort_by_key(|e| std::cmp::Reverse(e.path.components().count()));
    for entry in entries {
        if force
            && fs::symlink_metadata(&entry.path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
        {
            continue;
        }
        let metadata = current_metadata(command, entry)?;
        native::remove_leaf(&entry.path, &metadata)
            .map_err(|e| failure(command, &entry.path, e))?;
    }
    Ok(())
}

fn destination(
    command: &str,
    cwd: &Path,
    operand: &str,
    source: &Path,
    rename: bool,
) -> Result<PathBuf, ShellError> {
    let mut target = if rename {
        source
            .parent()
            .ok_or_else(|| failure(command, source, "cannot rename a root"))?
            .join(operand)
    } else {
        resolve(command, cwd, operand)?
    };
    if !rename && let Ok(metadata) = fs::symlink_metadata(&target) {
        if native::is_link_or_reparse_point(&metadata) {
            return Err(failure(
                command,
                &target,
                "destination is a link or reparse point",
            ));
        }
        if metadata.is_dir() {
            target.push(
                source
                    .file_name()
                    .ok_or_else(|| failure(command, source, "cannot copy or move a root"))?,
            );
        }
    }
    check_ancestors(command, &target)?;
    match fs::symlink_metadata(&target) {
        Ok(_) => {
            return Err(failure(
                command,
                &target,
                "destination already exists; refusing to overwrite",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(failure(command, &target, e)),
    }
    // Source spelling may vary in case; canonicalize before ancestry comparison.
    let canonical_source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    if within(&target, &canonical_source) {
        return Err(failure(
            command,
            &target,
            "destination is inside the source",
        ));
    }
    Ok(target)
}

fn copy_entries(
    command: &str,
    entries: &[Entry],
    source: &Path,
    target: &Path,
) -> Result<(), ShellError> {
    for entry in entries {
        current_metadata(command, entry)?;
        let relative = entry
            .path
            .strip_prefix(source)
            .map_err(|e| failure(command, &entry.path, e))?;
        let destination = if relative.as_os_str().is_empty() {
            target.to_path_buf()
        } else {
            target.join(relative)
        };
        native::validate_file_operand(&destination)
            .map_err(|e| failure(command, &destination, e))?;
        check_ancestors(command, &destination)?;
        if entry.directory {
            fs::create_dir(&destination).map_err(|e| failure(command, &destination, e))?;
        } else {
            let mut input = native::open_copy_source(&entry.path)
                .map_err(|e| failure(command, &entry.path, e))?;
            let metadata = input
                .metadata()
                .map_err(|e| failure(command, &entry.path, e))?;
            if !metadata.is_file() || native::is_link_or_reparse_point(&metadata) {
                return Err(failure(
                    command,
                    &entry.path,
                    "source changed to a link or non-file",
                ));
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|e| failure(command, &destination, e))?;
            io::copy(&mut input, &mut output).map_err(|e| failure(command, &destination, e))?;
            output
                .sync_all()
                .map_err(|e| failure(command, &destination, e))?;
        }
    }
    Ok(())
}

pub(super) fn run(command: &str, args: &[String], cwd: &Path) -> Result<(), ShellError> {
    let request = parse(command, args)?;
    if matches!(request.operation, Operation::Create) {
        let mut targets = Vec::new();
        // Resolve and check every operand before creating any files. Exclusive
        // creation also prevents overwriting if another process wins the race.
        for operand in request.paths {
            let target = resolve(command, cwd, operand)?;
            match fs::symlink_metadata(&target) {
                Ok(_) => {
                    return Err(failure(
                        command,
                        &target,
                        "already exists; refusing to overwrite",
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(failure(command, &target, e)),
            }
            if targets.contains(&target) {
                return Err(failure(command, &target, "duplicate file operand"));
            }
            targets.push(target);
        }
        for target in targets {
            check_ancestors(command, &target)?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| failure(command, &target, e))?;
        }
        return Ok(());
    }
    if matches!(request.operation, Operation::Delete) {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        // Validate every target, including guards, before deleting any of them.
        for operand in request.paths {
            let source = match resolve(command, cwd, operand) {
                Ok(source) => source,
                Err(_)
                    if request.force
                        && fs::symlink_metadata(cwd.join(operand))
                            .is_err_and(|e| e.kind() == io::ErrorKind::NotFound) =>
                {
                    check_ancestors(command, &cwd.join(operand))?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            guard_source(command, &source, cwd)?;
            match scan(command, &source, request.recursive, false) {
                Ok(plan) => {
                    entries.extend(plan.into_iter().filter(|e| seen.insert(e.path.clone())))
                }
                Err(_)
                    if request.force
                        && fs::symlink_metadata(&source)
                            .is_err_and(|e| e.kind() == io::ErrorKind::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        return remove_entries(command, &mut entries, request.force);
    }
    let source = resolve(command, cwd, request.paths[0])?;
    fs::symlink_metadata(&source).map_err(|e| failure(command, &source, e))?;
    let moving = !matches!(request.operation, Operation::Copy);
    if moving {
        guard_source(command, &source, cwd)?;
    }
    let target = destination(
        command,
        cwd,
        request.paths[1],
        &source,
        matches!(request.operation, Operation::Rename),
    )?;
    if moving {
        match native::rename_no_replace(&source, &target) {
            Ok(()) => return Ok(()),
            Err(e) if native::is_cross_volume(&e) => (),
            Err(e) => return Err(failure(command, &source, e)),
        }
    }
    let mut entries = scan(command, &source, request.recursive || moving, true)?;
    copy_entries(command, &entries, &source, &target)?;
    if moving {
        remove_entries(command, &mut entries, false).map_err(|e| {
            ShellError::new(
                format!(
                    "{}; copy is complete at {}, source cleanup incomplete",
                    e.message,
                    platform::display_path(&target)
                ),
                1,
            )
        })?;
    }
    Ok(())
}

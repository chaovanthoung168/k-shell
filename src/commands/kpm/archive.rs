use super::{Result, error, io_error, process::Context, store::checked};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path},
};
pub fn checksum(file: &mut File, expected: &str, context: &Context) -> Result<()> {
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(error("provider did not supply a valid SHA-256 checksum", 1));
    }
    let mut hash = Sha256::new();
    let mut chunk = [0; 65536];
    loop {
        context.check()?;
        let n = file.read(&mut chunk).map_err(io_error)?;
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    if format!("{:x}", hash.finalize()) != expected.to_ascii_lowercase() {
        return Err(error("download SHA-256 mismatch; nothing was installed", 1));
    }
    Ok(())
}
pub fn extract(file: File, destination: &Path, context: &Context) -> Result<()> {
    checked(destination)?;
    fs::create_dir(destination).map_err(io_error)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| error("invalid ZIP archive", 1))?;
    if zip.len() > 100_000 {
        return Err(error("archive contains too many entries", 1));
    }
    let mut total = 0u64;
    for i in 0..zip.len() {
        context.check()?;
        let mut entry = zip
            .by_index(i)
            .map_err(|_| error("cannot read ZIP entry", 1))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| error("archive path escapes its installation directory", 1))?;
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || entry.name().contains('\\')
        {
            return Err(error("unsupported or unsafe archive path", 1));
        }
        crate::platform::filesystem::validate_file_operand(&relative).map_err(io_error)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(error("archive links are not supported", 1));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| error("archive size overflow", 1))?;
        if total > 4 * 1024 * 1024 * 1024 {
            return Err(error("expanded archive exceeds 4 GiB", 1));
        }
        let path = destination.join(relative);
        checked(&path)?;
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(io_error)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let mut output = File::create_new(&path).map_err(io_error)?;
        let mut written = 0u64;
        let mut buffer = [0; 65536];
        loop {
            context.check()?;
            let n = entry.read(&mut buffer).map_err(io_error)?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > entry.size() {
                return Err(error("ZIP entry exceeds its advertised size", 1));
            }
            output.write_all(&buffer[..n]).map_err(io_error)?;
        }
        if written != entry.size() {
            return Err(error("truncated ZIP entry", 1));
        }
    }
    Ok(())
}

use super::{
    Result,
    catalog::{Release, Tool, stable},
    error, io_error,
};
use crate::platform::filesystem::{is_link_or_reparse_point, validate_file_operand};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
const ROOT_MARKER: &str = "KPM managed store v1\n";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub tool: Tool,
    pub version: String,
    pub home: String,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub records: Vec<Record>,
    pub active: BTreeMap<Tool, String>,
}
impl Inventory {
    pub fn record(&self, tool: Tool, version: &str) -> Option<&Record> {
        self.records
            .iter()
            .filter(|r| {
                r.tool == tool
                    && (r.version == version
                        || tool == Tool::Java
                            && !version.contains(['.', '+'])
                            && r.version.split(['.', '+']).next() == Some(version))
            })
            .max_by(|a, b| super::catalog::compare(&a.version, &b.version))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pending {
    pub release: Release,
}
#[derive(Debug, Clone)]
pub(crate) struct Store {
    pub root: PathBuf,
}
pub struct Lock {
    _file: File,
}
pub fn checked(path: &Path) -> Result<()> {
    validate_file_operand(path).map_err(io_error)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(error("parent traversal is not allowed in managed paths", 1));
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if is_link_or_reparse_point(&meta) => {
                return Err(error(
                    "managed paths must not contain links or reparse points",
                    1,
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(e)),
        }
    }
    Ok(())
}
pub fn remove_owned_tree(path: &Path) -> Result<()> {
    checked(path)?;
    fn inspect(path: &Path) -> Result<()> {
        checked(path)?;
        if path.is_dir() {
            for e in fs::read_dir(path).map_err(io_error)? {
                inspect(&e.map_err(io_error)?.path())?;
            }
        }
        Ok(())
    }
    inspect(path)?;
    fs::remove_dir_all(path).map_err(io_error)
}
impl Store {
    pub fn from_environment() -> Result<Self> {
        let root = match crate::startup_setting("KPM_HOME") {
            Some(p) if p.is_empty() => {
                return Err(error("KPM storage is disabled by empty KSHELL_KPM_HOME", 1));
            }
            Some(p) => PathBuf::from(p),
            None => std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .ok_or_else(|| error("LOCALAPPDATA is unavailable; set KSHELL_KPM_HOME", 1))?
                .join("KShell/kpm"),
        };
        if !root.is_absolute() || root.parent().is_none() {
            return Err(error(
                "KPM home must be an absolute directory below a filesystem root",
                1,
            ));
        }
        checked(&root)?;
        Ok(Self { root })
    }
    pub fn lock(&self) -> Result<Lock> {
        checked(&self.root)?;
        if self.root.exists()
            && !self.root.join(".kpm-root").exists()
            && fs::read_dir(&self.root).map_err(io_error)?.next().is_some()
        {
            return Err(error("refusing nonempty unmarked KPM home", 1));
        }
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let marker = self.root.join(".kpm-root");
        checked(&marker)?;
        match File::create_new(&marker) {
            Ok(mut f) => f.write_all(ROOT_MARKER.as_bytes()).map_err(io_error)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_error(e)),
        }
        self.check_root()?;
        let path = self.root.join("operation.lock");
        checked(&path)?;
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(io_error)?;
        file.try_lock().map_err(|_| {
            error(
                "another KPM operation is running; try again when it finishes",
                1,
            )
        })?;
        Ok(Lock { _file: file })
    }
    fn check_root(&self) -> Result<()> {
        checked(&self.root)?;
        let marker = self.root.join(".kpm-root");
        checked(&marker)?;
        if fs::read_to_string(marker).map_err(io_error)? != ROOT_MARKER {
            return Err(error("invalid KPM root marker", 1));
        }
        Ok(())
    }
    fn read_json<T: serde::de::DeserializeOwned + Default>(&self, name: &str) -> Result<T> {
        if !self.root.exists() {
            return Ok(T::default());
        }
        self.check_root()?;
        let path = self.root.join(name);
        checked(&path)?;
        let mut bytes = Vec::new();
        match File::open(path) {
            Ok(f) => {
                f.take(4 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(io_error)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            Err(e) => return Err(io_error(e)),
        };
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(error("KPM metadata exceeds its size limit", 1));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            error(
                "invalid KPM metadata; preserve the store and repair its records",
                1,
            )
        })
    }
    pub fn read(&self) -> Result<Inventory> {
        let inventory: Inventory = self.read_json("installed.json")?;
        let mut keys = std::collections::BTreeSet::new();
        for r in &inventory.records {
            self.home(r)?;
            if !keys.insert((r.tool, r.version.clone())) {
                return Err(error("duplicate installation record", 1));
            }
        }
        for (tool, version) in &inventory.active {
            if !tool.switchable() || inventory.record(*tool, version).is_none() {
                return Err(error("invalid active installation record", 1));
            }
        }
        Ok(inventory)
    }
    pub fn save(&self, inventory: &Inventory) -> Result<()> {
        self.atomic("installed.json", inventory)
    }
    pub fn pending(&self) -> Result<Option<Pending>> {
        self.read_json("pending.json")
    }
    pub fn set_pending(&self, pending: &Pending) -> Result<()> {
        self.atomic("pending.json", pending)
    }
    pub fn clear_pending(&self) -> Result<()> {
        let path = self.root.join("pending.json");
        checked(&path)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error(e)),
        }
    }
    fn atomic(&self, name: &str, value: &impl Serialize) -> Result<()> {
        self.check_root()?;
        let path = self.root.join(name);
        let temp = self.root.join(format!("{name}.new"));
        checked(&path)?;
        checked(&temp)?;
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|_| error("cannot encode installation records", 1))?;
        let mut file =
            crate::platform::filesystem::open_text_target(&temp, false).map_err(io_error)?;
        file.set_len(0).map_err(io_error)?;
        file.write_all(&bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        drop(file);
        fs::rename(temp, path).map_err(io_error)
    }
    pub fn target(&self, tool: Tool, version: &str) -> Result<PathBuf> {
        if !stable(version) {
            return Err(error("invalid managed version", 1));
        }
        let path = self.root.join("packages").join(tool.name()).join(version);
        checked(&path)?;
        Ok(path)
    }
    pub fn stage(&self, tool: Tool, version: &str) -> Result<PathBuf> {
        if !stable(version) {
            return Err(error("invalid staged version", 1));
        }
        let p = self
            .root
            .join("staging")
            .join(format!("{}-{version}", tool.name()));
        checked(&p)?;
        Ok(p)
    }
    pub fn destination(&self, tool: Tool, version: &str) -> Result<PathBuf> {
        let target = self.target(tool, version)?;
        if tool == Tool::Rust {
            let path = self
                .root
                .join("rustup/toolchains")
                .join(format!("{version}-x86_64-pc-windows-msvc"));
            checked(&path)?;
            Ok(path)
        } else {
            Ok(target)
        }
    }
    pub fn home(&self, r: &Record) -> Result<PathBuf> {
        if !stable(&r.version) {
            return Err(error("invalid recorded version", 1));
        }
        let relative = Path::new(&r.home);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(error(
                "installation home must be a relative managed path",
                1,
            ));
        }
        let path = self.root.join(relative);
        let prefix = if r.tool == Tool::Rust {
            self.root
                .join("rustup/toolchains")
                .join(format!("{}-x86_64-pc-windows-msvc", r.version))
        } else {
            self.target(r.tool, &r.version)?
        };
        if !path.starts_with(&prefix) {
            return Err(error("installation home is outside its managed version", 1));
        }
        checked(&path)?;
        Ok(path)
    }
    pub fn record(&self, tool: Tool, version: &str, home: &Path) -> Result<Record> {
        let relative = home
            .strip_prefix(&self.root)
            .map_err(|_| error("provider returned an unmanaged installation path", 1))?
            .to_str()
            .ok_or_else(|| error("invalid installation path", 1))?;
        let record = Record {
            tool,
            version: version.into(),
            home: relative.into(),
        };
        self.home(&record)?;
        Ok(record)
    }
    pub fn mark(path: &Path, tool: Tool, version: &str) -> Result<()> {
        checked(path)?;
        fs::create_dir_all(path).map_err(io_error)?;
        let marker = path.join(".kpm-owned");
        checked(&marker)?;
        let mut f = File::create_new(marker).map_err(io_error)?;
        write!(f, "KPM1\n{}\n{version}\n", tool.name()).map_err(io_error)
    }
    pub fn owns(path: &Path, tool: Tool, version: &str) -> Result<()> {
        checked(path)?;
        let marker = path.join(".kpm-owned");
        checked(&marker)?;
        if fs::read_to_string(marker).map_err(io_error)?
            != format!("KPM1\n{}\n{version}\n", tool.name())
        {
            return Err(error(
                "refusing an unmarked or mismatched managed directory",
                1,
            ));
        }
        Ok(())
    }
}

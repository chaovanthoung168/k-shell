use super::{
    Result,
    catalog::Tool,
    error,
    store::{Inventory, Store},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Debug, Default, Clone)]
pub(crate) struct Environment {
    bins: Vec<PathBuf>,
    vars: BTreeMap<OsString, OsString>,
    node: Option<PathBuf>,
    pub names: Vec<String>,
}
impl Environment {
    pub fn load() -> Result<Self> {
        if crate::startup_setting("KPM_HOME").is_some_and(|v| v.is_empty())
            || (crate::startup_setting("KPM_HOME").is_none()
                && std::env::var_os("LOCALAPPDATA").is_none())
        {
            return Ok(Self::default());
        }
        let store = Store::from_environment()?;
        Self::from_inventory(&store, &store.read()?)
    }
    pub fn from_inventory(store: &Store, inventory: &Inventory) -> Result<Self> {
        let mut environment = Self::default();
        for (tool, version) in &inventory.active {
            let record = inventory
                .record(*tool, version)
                .ok_or_else(|| error("active tool has no managed record", 1))?;
            let home = store.home(record)?;
            if !home.join(tool.primary()).is_file() {
                return Err(error(
                    format!(
                        "active {} executable is missing; repair it or uninstall the managed version",
                        tool.name()
                    ),
                    1,
                ));
            }
            let bin = match tool {
                Tool::Node | Tool::Php => home.clone(),
                _ => home.join("bin"),
            };
            super::store::checked(&bin)?;
            for entry in std::fs::read_dir(&bin).map_err(super::io_error)? {
                let path = entry.map_err(super::io_error)?.path();
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
                    && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                {
                    environment.names.push(name.into());
                }
            }
            if *tool == Tool::Java {
                environment.bins.insert(0, bin);
            } else {
                environment.bins.push(bin);
            }
            match tool {
                Tool::Java => {
                    if !home.join("bin/javac.exe").is_file() {
                        return Err(error("active Java installation is not a full JDK", 1));
                    }
                    environment
                        .vars
                        .insert("JAVA_HOME".into(), home.into_os_string());
                }
                Tool::Go => {
                    environment
                        .vars
                        .insert("GOROOT".into(), home.into_os_string());
                }
                Tool::Node => {
                    environment.names.extend(["npm".into(), "npx".into()]);
                    environment.node = Some(home);
                }
                Tool::Rust => {
                    environment.vars.insert(
                        "RUSTUP_HOME".into(),
                        store.root.join("rustup").into_os_string(),
                    );
                    environment.vars.insert(
                        "CARGO_HOME".into(),
                        store.root.join("cargo").into_os_string(),
                    );
                    environment.vars.insert(
                        "RUSTUP_TOOLCHAIN".into(),
                        format!("{version}-x86_64-pc-windows-msvc").into(),
                    );
                }
                _ => {}
            }
        }
        environment.names.sort();
        environment.names.dedup();
        Ok(environment)
    }
    pub fn apply(&self, command: &mut Command) -> Result<()> {
        if self.bins.is_empty() {
            return Ok(());
        }
        let mut paths = self.bins.clone();
        paths.extend(
            std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                .unwrap_or_default(),
        );
        let path = std::env::join_paths(paths)
            .map_err(|_| error("cannot construct managed child PATH", 1))?;
        command.env("PATH", path).envs(&self.vars);
        Ok(())
    }
    pub fn command(&self, name: &str, cwd: &Path) -> Result<Option<Command>> {
        if name.contains(['/', '\\', ':']) {
            return Ok(None);
        }
        let lower = name.to_ascii_lowercase();
        if Path::new(name).extension().is_some() && !lower.ends_with(".exe") {
            return Ok(None);
        }
        let bare = lower.strip_suffix(".exe").unwrap_or(&lower);
        if matches!(bare, "npm" | "npx")
            && let Some(home) = &self.node
        {
            let script = home.join(format!("node_modules/npm/bin/{bare}-cli.js"));
            super::store::checked(&script)?;
            if !script.is_file() {
                return Err(error("managed Node.js is missing its npm CLI", 1));
            }
            let mut command = Command::new(home.join("node.exe"));
            command.arg(script).current_dir(cwd);
            return Ok(Some(command));
        }
        for bin in &self.bins {
            let path = bin.join(if lower.ends_with(".exe") {
                name.to_string()
            } else {
                format!("{name}.exe")
            });
            super::store::checked(&path)?;
            if path.is_file() {
                let mut command = Command::new(path);
                command.current_dir(cwd);
                return Ok(Some(command));
            }
        }
        Ok(None)
    }
}

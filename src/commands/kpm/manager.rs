use super::{
    Result,
    catalog::{Release, Tool, compare},
    error, io_error,
    process::Context,
    providers::{Provider, find_home},
    store::{Inventory, Pending, Record, Store, remove_owned_tree},
};
use std::{fs, path::PathBuf};
pub struct Manager<P: Provider> {
    pub store: Store,
    pub provider: P,
    pub context: Context,
}
impl<P: Provider> Manager<P> {
    pub fn new(store: Store, provider: P) -> Self {
        Self {
            store,
            provider,
            context: Context::new(),
        }
    }
    pub fn installed(&self) -> Result<Inventory> {
        self.store.read()
    }
    pub fn install(&mut self, r: &Release) -> Result<(Record, bool)> {
        let _lock = self.store.lock()?;
        self.context.check()?;
        let mut inventory = self.store.read()?;
        if let Some(record) = inventory.record(r.tool, &r.version).cloned() {
            self.provider.verify(
                record.tool,
                &record.version,
                &self.store.home(&record)?,
                &self.store,
                &self.context,
            )?;
            return Ok((record, false));
        }
        if !r.tool.switchable() && inventory.records.iter().any(|record| record.tool == r.tool) {
            return Err(error(
                "this WinGet provider manages one version; uninstall the managed version first",
                1,
            ));
        }
        if self.store.pending()?.is_some() {
            return Err(error(
                "interrupted installation found; run kpm recover before installing again",
                1,
            ));
        }
        let target = self.store.target(r.tool, &r.version)?;
        if target.exists() {
            return Err(error(
                "destination already exists without a verified installation record; inspect it before continuing",
                1,
            ));
        }
        self.provider.preflight(r, &self.store, &self.context)?;
        let stage = self.store.stage(r.tool, &r.version)?;
        if stage.exists() {
            return Err(error("staging already exists; run kpm recover", 1));
        }
        Store::mark(&stage, r.tool, &r.version)?;
        self.store.set_pending(&Pending { release: r.clone() })?;
        let home = self
            .provider
            .install(r, &stage, &self.store, &self.context)?;
        self.provider
            .verify(r.tool, &r.version, &home, &self.store, &self.context)?;
        let record = self.store.record(r.tool, &r.version, &home)?;
        inventory.records.push(record.clone());
        self.store.save(&inventory)?;
        self.store.clear_pending()?;
        Store::owns(&stage, r.tool, &r.version)?;
        remove_owned_tree(&stage)?;
        Ok((record, true))
    }
    pub fn activate(&mut self, record: &Record) -> Result<()> {
        if !record.tool.switchable() {
            return Err(error(
                "this WinGet provider does not support reliable kpm use; no selection was changed",
                1,
            ));
        }
        let _lock = self.store.lock()?;
        let mut inventory = self.store.read()?;
        let record = inventory
            .record(record.tool, &record.version)
            .ok_or_else(|| {
                error(
                    "version is not installed; kpm use never installs software",
                    1,
                )
            })?
            .clone();
        self.provider.verify(
            record.tool,
            &record.version,
            &self.store.home(&record)?,
            &self.store,
            &self.context,
        )?;
        inventory.active.insert(record.tool, record.version.clone());
        super::environment::Environment::from_inventory(&self.store, &inventory)?;
        self.store.save(&inventory)
    }
    pub fn uninstall(&mut self, record: &Record) -> Result<()> {
        let _lock = self.store.lock()?;
        self.context.check()?;
        let mut inventory = self.store.read()?;
        let record = inventory
            .record(record.tool, &record.version)
            .ok_or_else(|| error("uninstall only accepts a recorded managed version", 1))?
            .clone();
        if inventory.active.get(&record.tool) == Some(&record.version) {
            inventory.active.remove(&record.tool);
            self.store.save(&inventory)?;
        }
        let home = self.store.home(&record)?;
        self.provider.uninstall(
            record.tool,
            &record.version,
            &home,
            &self.store,
            &self.context,
        )?;
        inventory
            .records
            .retain(|r| !(r.tool == record.tool && r.version == record.version));
        self.store.save(&inventory)
    }
    pub fn recover(&mut self) -> Result<String> {
        let _lock = self.store.lock()?;
        let mut inventory = self.store.read()?;
        let Some(pending) = self.store.pending()? else {
            let staging = self.store.root.join("staging");
            super::store::checked(&staging)?;
            if staging.exists() {
                for entry in fs::read_dir(&staging).map_err(io_error)? {
                    let entry = entry.map_err(io_error)?;
                    let name = entry.file_name();
                    let name = name.to_str().ok_or_else(|| {
                        error("unknown staging directory; preserve it for inspection", 1)
                    })?;
                    let (tool, version) = name.split_once('-').ok_or_else(|| {
                        error("unknown staging directory; preserve it for inspection", 1)
                    })?;
                    let tool = Tool::parse(tool)?;
                    let expected = self.store.stage(tool, version)?;
                    Store::owns(&expected, tool, version)?;
                    remove_owned_tree(&expected)?;
                }
            }
            return Ok("No pending installation; orphaned owned staging was cleaned.".into());
        };
        let r = pending.release;
        let target = self.store.target(r.tool, &r.version)?;
        let stage = self.store.stage(r.tool, &r.version)?;
        let candidate = if r.tool == Tool::Rust {
            self.store
                .root
                .join("rustup/toolchains")
                .join(format!("{}-x86_64-pc-windows-msvc", r.version))
        } else {
            target.clone()
        };
        if matches!(r.tool, Tool::Python | Tool::Git)
            && self.provider.absent(&r, &self.store, &self.context)?
        {
            if target.exists() {
                Store::owns(&target, r.tool, &r.version)?;
                remove_owned_tree(&target)?;
            }
            if stage.exists() {
                Store::owns(&stage, r.tool, &r.version)?;
                remove_owned_tree(&stage)?;
            }
            self.store.clear_pending()?;
            return Ok(
                "WinGet confirms the package is absent; incomplete owned files were cleaned."
                    .into(),
            );
        }
        if candidate.exists() {
            if r.tool != Tool::Rust {
                Store::owns(&target, r.tool, &r.version)?;
            }
            let home = find_home(&candidate, r.tool)?;
            self.provider
                .verify(r.tool, &r.version, &home, &self.store, &self.context)?;
            let record = self.store.record(r.tool, &r.version, &home)?;
            if inventory.record(r.tool, &r.version).is_none() {
                inventory.records.push(record);
                self.store.save(&inventory)?;
            }
        } else if matches!(r.tool, Tool::Python | Tool::Git) {
            return Err(error(
                "WinGet installation was interrupted; inspect WinGet and the destination before recovery. The pending record was preserved",
                1,
            ));
        }
        if stage.exists() {
            Store::owns(&stage, r.tool, &r.version)?;
            remove_owned_tree(&stage)?;
        }
        self.store.clear_pending()?;
        Ok("Recovery complete. Verified installations were recorded; activation is explicit with kpm use.".into())
    }
    pub fn records(&self, tool: Tool) -> Result<Vec<Record>> {
        let mut r: Vec<_> = self
            .store
            .read()?
            .records
            .into_iter()
            .filter(|r| r.tool == tool)
            .collect();
        r.sort_by(|a, b| compare(&b.version, &a.version));
        Ok(r)
    }
    pub fn home(&self, record: &Record) -> Result<PathBuf> {
        self.store.home(record)
    }
}

//! Optional, built-in prompt designs. Installation writes data only, never code
//! or downloaded scripts. All filesystem access uses an explicit store root.
use std::{fs, io, path::PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PromptTheme {
    #[default]
    Default,
    Powerline,
    Atomic,
    Bubblesline,
    Cert,
    Negligible,
}

impl PromptTheme {
    pub const ALL: [Self; 6] = [
        Self::Default,
        Self::Powerline,
        Self::Atomic,
        Self::Bubblesline,
        Self::Cert,
        Self::Negligible,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Powerline => "powerline",
            Self::Atomic => "atomic",
            Self::Bubblesline => "bubblesline",
            Self::Cert => "cert",
            Self::Negligible => "negligible",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.name() == value)
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Default => "Original K Shell prompt",
            Self::Powerline => "Pink, peach and blue arrow segments",
            Self::Atomic => "Blue and yellow developer capsules",
            Self::Bubblesline => "A quiet purple username bubble",
            Self::Cert => "Coral, green and cyan blocks",
            Self::Negligible => "Minimal cyan path and green arrow",
        }
    }

    fn receipt(self) -> String {
        format!("K Shell prompt theme v1\n{}\n", self.name())
    }
}

pub struct ThemeStore {
    root: PathBuf,
}

impl ThemeStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn from_environment() -> Option<Self> {
        if let Some(root) = std::env::var_os("KSHELL_CONFIG_DIR") {
            return (!root.is_empty()).then(|| Self::new(root.into()));
        }
        std::env::var_os("APPDATA")
            .filter(|root| !root.is_empty())
            .map(|root| Self::new(PathBuf::from(root).join("kshell")))
            .or_else(|| {
                crate::platform::home_directory()
                    .map(|root| Self::new(PathBuf::from(root).join(".kshell")))
            })
    }

    fn theme_path(&self, theme: PromptTheme) -> PathBuf {
        self.root
            .join("themes")
            .join(format!("{}.theme", theme.name()))
    }

    pub fn is_installed(&self, theme: PromptTheme) -> bool {
        theme == PromptTheme::Default
            || fs::read_to_string(self.theme_path(theme))
                .is_ok_and(|contents| contents == theme.receipt())
    }

    pub fn install(&self, theme: PromptTheme) -> io::Result<()> {
        if theme == PromptTheme::Default || self.is_installed(theme) {
            return Ok(());
        }
        fs::create_dir_all(self.root.join("themes"))?;
        // Do not overwrite user-edited files or follow an existing leaf link.
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.theme_path(theme))?;
        file.write_all(theme.receipt().as_bytes())
    }

    pub fn select(&self, theme: PromptTheme) -> io::Result<()> {
        if !self.is_installed(theme) {
            return Err(io::Error::other(format!(
                "theme {} is not installed; run: theme install {}",
                theme.name(),
                theme.name()
            )));
        }
        fs::create_dir_all(&self.root)?;
        // A replacement through rename keeps a failed write from truncating the
        // prior choice, and never opens the destination through a leaf symlink.
        let temporary = self
            .root
            .join(format!("selected-theme-{}.tmp", std::process::id()));
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| {
            writeln!(file, "{}", theme.name())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, self.root.join("selected-theme"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    /// Read only at interactive startup. Missing/invalid/uninstalled selections
    /// fall back to the default without creating configuration files.
    pub fn selected(&self) -> PromptTheme {
        fs::read_to_string(self.root.join("selected-theme"))
            .ok()
            .and_then(|value| PromptTheme::parse(value.trim()))
            .filter(|theme| self.is_installed(*theme))
            .unwrap_or_default()
    }
}

use super::{Result, error};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Tool {
    Node,
    Python,
    Go,
    Rust,
    Php,
    Git,
    Java,
}
impl Tool {
    pub const ALL: [Self; 7] = [
        Self::Node,
        Self::Python,
        Self::Go,
        Self::Rust,
        Self::Php,
        Self::Git,
        Self::Java,
    ];
    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.name() == value.to_ascii_lowercase())
            .ok_or_else(|| error("unknown tool; use kpm list", 2))
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Php => "php",
            Self::Git => "git",
            Self::Java => "java",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Node => "Node.js",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Rust => "Rust",
            Self::Php => "PHP",
            Self::Git => "Git",
            Self::Java => "Java (Eclipse Temurin JDK)",
        }
    }
    pub fn provider(self) -> &'static str {
        match self {
            Self::Python | Self::Git => "WinGet (user scope)",
            Self::Rust => "rustup (private homes)",
            Self::Node => "nodejs.org",
            Self::Go => "go.dev",
            Self::Php => "downloads.php.net",
            Self::Java => "Eclipse Adoptium",
        }
    }
    pub fn switchable(self) -> bool {
        !matches!(self, Self::Python | Self::Git)
    }
    pub fn primary(self) -> &'static str {
        match self {
            Self::Node => "node.exe",
            Self::Python => "python.exe",
            Self::Go => "bin/go.exe",
            Self::Rust => "bin/rustc.exe",
            Self::Php => "php.exe",
            Self::Git => "cmd/git.exe",
            Self::Java => "bin/java.exe",
        }
    }
    pub fn version_args(self) -> &'static [&'static str] {
        match self {
            Self::Go => &["version"],
            Self::Java => &["-version"],
            _ => &["--version"],
        }
    }
    pub fn package_id(self, version: &str) -> Result<String> {
        match self {
            Self::Git => Ok("Git.Git".into()),
            Self::Python => {
                let parts: Vec<_> = version.split('.').collect();
                if parts.len() < 3 || parts[0] != "3" {
                    return Err(error("WinGet Python requires a full Python 3 version", 2));
                }
                Ok(format!("Python.Python.{}.{}", parts[0], parts[1]))
            }
            _ => Err(error("this tool does not use WinGet", 2)),
        }
    }
}

pub fn stable(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 80
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'+'))
        && !version.ends_with(['.', '+'])
        && !version.contains("..")
        && !version.contains("++")
        && version
            .split(['.', '+'])
            .all(|part| part.parse::<u64>().is_ok())
}
pub fn numbers(version: &str) -> Vec<u64> {
    let (main, build) = version
        .split_once('+')
        .map_or((version, None), |(main, build)| (main, Some(build)));
    let mut parts: Vec<u64> = main.split('.').filter_map(|s| s.parse().ok()).collect();
    if let Some(build) = build {
        while parts.len() < 3 {
            parts.push(0);
        }
        parts.extend(build.split('.').filter_map(|s| s.parse::<u64>().ok()));
    }
    parts
}
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    numbers(a).cmp(&numbers(b)).then_with(|| a.cmp(b))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Release {
    pub tool: Tool,
    pub version: String,
    pub url: Option<String>,
    pub checksum: Option<String>,
    pub note: Option<String>,
}
impl Release {
    pub fn new(tool: Tool, version: String) -> Self {
        Self {
            tool,
            version,
            url: None,
            checksum: None,
            note: None,
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{}{}",
            self.version,
            self.note
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default()
        )
    }
}
pub fn normalize(mut releases: Vec<Release>) -> Vec<Release> {
    releases.retain(|r| stable(&r.version));
    releases.sort_by(|a, b| compare(&b.version, &a.version));
    releases.dedup_by(|a, b| a.version == b.version);
    releases
}
pub fn select(releases: Vec<Release>, requested: &str, tool: Tool) -> Result<Release> {
    let releases = normalize(releases);
    let found = if requested == "latest" {
        releases.into_iter().next()
    } else {
        releases.into_iter().find(|r| {
            r.version == requested
                || (tool == Tool::Java
                    && !requested.contains(['.', '+'])
                    && r.version.split(['.', '+']).next() == Some(requested))
        })
    };
    found.ok_or_else(|| {
        error(
            "requested stable version is not available from this provider",
            1,
        )
    })
}

//! Real providers. Metadata and artifacts are restricted to named HTTPS sources.
use super::{
    Result, archive,
    catalog::{Release, Tool, normalize, select, stable},
    error, io_error,
    process::{self, Context},
    store::{Store, checked},
};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub trait Provider {
    fn absent(&mut self, release: &Release, store: &Store, _context: &Context) -> Result<bool> {
        Ok(!store.destination(release.tool, &release.version)?.exists())
    }
    fn versions(
        &mut self,
        tool: Tool,
        requested: Option<&str>,
        context: &Context,
    ) -> Result<Vec<Release>>;
    fn preflight(&mut self, release: &Release, store: &Store, context: &Context) -> Result<()>;
    fn install(
        &mut self,
        release: &Release,
        stage: &Path,
        store: &Store,
        context: &Context,
    ) -> Result<PathBuf>;
    fn verify(
        &mut self,
        tool: Tool,
        version: &str,
        home: &Path,
        store: &Store,
        context: &Context,
    ) -> Result<()>;
    fn uninstall(
        &mut self,
        tool: Tool,
        version: &str,
        home: &Path,
        store: &Store,
        context: &Context,
    ) -> Result<()>;
}

pub struct Native {
    runtime: &'static tokio::runtime::Runtime,
    client: reqwest::Client,
    cwd: PathBuf,
}
fn allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && matches!(
            url.host_str(),
            Some(
                "nodejs.org"
                    | "r2.nodejs.org"
                    | "go.dev"
                    | "dl.google.com"
                    | "windows.php.net"
                    | "downloads.php.net"
                    | "api.adoptium.net"
                    | "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}
fn net(e: reqwest::Error) -> crate::error::ShellError {
    error(
        if e.is_timeout() {
            "provider request timed out"
        } else if e.is_status() {
            "provider returned an HTTP error; the requested release may be unavailable"
        } else {
            "provider network/TLS request failed"
        },
        1,
    )
}
impl Native {
    pub fn new(cwd: PathBuf) -> Result<Self> {
        let runtime = crate::runtime::network::runtime()?;
        let client = reqwest::Client::builder()
            .user_agent("KShell-KPM/0.1")
            .https_only(true)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(1200))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > 8 || !allowed(attempt.url()) {
                    attempt.error("untrusted redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(net)?;
        Ok(Self {
            runtime,
            client,
            cwd,
        })
    }
    fn wait<T>(
        &self,
        future: impl std::future::Future<Output = Result<T>>,
        context: &Context,
    ) -> Result<T> {
        self.runtime.block_on(async {
            let mut future = std::pin::pin!(future);
            loop {
                context.check()?;
                if let Ok(result) =
                    tokio::time::timeout(Duration::from_millis(50), future.as_mut()).await
                {
                    return result;
                }
            }
        })
    }
    fn bytes(&self, url: &str, context: &Context) -> Result<Vec<u8>> {
        let url = reqwest::Url::parse(url).map_err(|_| error("invalid provider URL", 1))?;
        if !allowed(&url) {
            return Err(error("untrusted provider source", 1));
        }
        self.wait(
            async {
                let mut response = self
                    .client
                    .get(url)
                    .timeout(Duration::from_secs(60))
                    .send()
                    .await
                    .map_err(net)?
                    .error_for_status()
                    .map_err(net)?;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(net)? {
                    if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                        return Err(error("provider metadata exceeds 8 MiB", 1));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(bytes)
            },
            context,
        )
    }
    fn json(&self, url: &str, context: &Context) -> Result<Value> {
        serde_json::from_slice(&self.bytes(url, context)?)
            .map_err(|_| error("provider returned invalid JSON metadata", 1))
    }
    fn download(&self, url: &str, path: &Path, context: &Context) -> Result<()> {
        let url = reqwest::Url::parse(url).map_err(|_| error("invalid artifact URL", 1))?;
        if !allowed(&url) {
            return Err(error("untrusted artifact source", 1));
        }
        checked(path)?;
        let mut file = File::create_new(path).map_err(io_error)?;
        self.wait(
            async {
                let mut response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .map_err(net)?
                    .error_for_status()
                    .map_err(net)?;
                let mut count = 0u64;
                while let Some(chunk) = response.chunk().await.map_err(net)? {
                    count += chunk.len() as u64;
                    if count > 2 * 1024 * 1024 * 1024 {
                        return Err(error("download exceeds 2 GiB", 1));
                    }
                    file.write_all(&chunk).map_err(io_error)?;
                }
                file.sync_all().map_err(io_error)?;
                Ok(())
            },
            context,
        )
    }
    fn winget(&self, args: &[&str], context: &Context) -> Result<process::Captured> {
        let path=crate::platform::resolve_executable("winget",&self.cwd).map_err(|_|error("WinGet is unavailable; install/update Microsoft's App Installer, then reopen K Shell",1))?;
        process::capture(
            Command::new(path).args(args).current_dir(&self.cwd),
            Duration::from_secs(if args.first() == Some(&"uninstall") {
                1800
            } else {
                60
            }),
            context,
        )
    }
    fn winget_present(&self, id: &str, context: &Context) -> Result<Option<String>> {
        let result = self.winget(
            &[
                "list",
                "--id",
                id,
                "--exact",
                "--source",
                "winget",
                "--disable-interactivity",
                "--accept-source-agreements",
            ],
            context,
        )?;
        if result.status as u32 == 0x8a15_0014 {
            return Ok(None);
        }
        let text = process::succeeded(result, "WinGet installed-package query")?;
        for line in text.lines() {
            let words: Vec<_> = line.split_whitespace().collect();
            if let Some(i) = words.iter().position(|w| *w == id) {
                return words
                    .get(i + 1)
                    .filter(|v| stable(v))
                    .map(|s| Some((*s).to_owned()))
                    .ok_or_else(|| error("WinGet did not report an exact installed version", 1));
            }
        }
        Err(error(
            "cannot identify the exact package in WinGet output",
            1,
        ))
    }
    fn rustup(&self, store: &Store) -> Result<Command> {
        checked(&store.root.join("rustup"))?;
        checked(&store.root.join("cargo"))?;
        let binary=crate::platform::resolve_executable("rustup",&self.cwd).or_else(|_|std::env::var_os("USERPROFILE").map(PathBuf::from).map(|p|p.join(".cargo/bin/rustup.exe")).filter(|p|p.is_file()).ok_or_else(||error("rustup is not installed; install the native rustup client from rust-lang.org first",1)))?;
        let mut cmd = Command::new(binary);
        cmd.current_dir(&self.cwd)
            .env("RUSTUP_HOME", store.root.join("rustup"))
            .env("CARGO_HOME", store.root.join("cargo"))
            .env("RUSTUP_DIST_SERVER", "https://static.rust-lang.org")
            .env("RUSTUP_UPDATE_ROOT", "https://static.rust-lang.org/rustup");
        Ok(cmd)
    }
}

fn strings(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| error("provider metadata is missing a required field", 1))
}
pub fn node_versions(value: &Value) -> Vec<Release> {
    strings(value)
        .filter_map(|v| {
            let version = v["version"].as_str()?.strip_prefix('v')?;
            if !stable(version) || !strings(&v["files"]).any(|f| f.as_str() == Some("win-x64-zip"))
            {
                return None;
            }
            let mut r = Release::new(Tool::Node, version.into());
            r.note = v["lts"].as_str().map(|s| format!("LTS {s}"));
            Some(r)
        })
        .collect()
}
pub fn go_versions(value: &Value) -> Vec<Release> {
    strings(value)
        .filter(|v| v["stable"].as_bool() == Some(true))
        .filter_map(|v| {
            let version = v["version"].as_str()?.strip_prefix("go")?;
            let file = strings(&v["files"]).find(|f| {
                f["os"] == "windows"
                    && f["arch"] == "amd64"
                    && f["kind"] == "archive"
                    && f["filename"].as_str().is_some_and(|s| s.ends_with(".zip"))
            })?;
            let mut r = Release::new(Tool::Go, version.into());
            r.url = Some(format!("https://go.dev/dl/{}", file["filename"].as_str()?));
            r.checksum = Some(file["sha256"].as_str()?.into());
            Some(r)
        })
        .collect()
}
pub fn php_versions(value: &Value) -> Vec<Release> {
    value
        .as_object()
        .into_iter()
        .flat_map(|v| v.values())
        .filter_map(|v| {
            let version = v["version"].as_str()?;
            let (_, build) = v.as_object()?.iter().find(|(key, entry)| {
                key.starts_with("nts-")
                    && key.ends_with("-x64")
                    && entry["zip"]["sha256"].is_string()
            })?;
            let mut r = Release::new(Tool::Php, version.into());
            let file = build["zip"]["path"].as_str()?;
            if file.contains(['/', '\\']) {
                return None;
            }
            r.url = Some(format!(
                "https://downloads.php.net/~windows/releases/{file}"
            ));
            r.checksum = Some(build["zip"]["sha256"].as_str()?.into());
            r.note = Some("x64 NTS; Visual C++ runtime required".into());
            Some(r)
        })
        .collect()
}
pub fn java_versions(value: &Value) -> Vec<Release> {
    strings(value)
        .filter(|v| v["release_type"] == "ga")
        .filter_map(|v| {
            let version = v["release_name"].as_str()?.strip_prefix("jdk-")?;
            let binary = strings(&v["binaries"]).find(|b| {
                b["os"] == "windows" && b["architecture"] == "x64" && b["image_type"] == "jdk"
            })?;
            let mut r = Release::new(Tool::Java, version.into());
            r.url = Some(binary["package"]["link"].as_str()?.into());
            r.checksum = Some(binary["package"]["checksum"].as_str()?.into());
            r.note = Some(format!(
                "Temurin JDK; feature {}",
                version.split(['.', '+']).next()?
            ));
            Some(r)
        })
        .collect()
}

impl Provider for Native {
    fn absent(&mut self, release: &Release, store: &Store, context: &Context) -> Result<bool> {
        if matches!(release.tool, Tool::Python | Tool::Git) {
            Ok(self
                .winget_present(&release.tool.package_id(&release.version)?, context)?
                .is_none())
        } else {
            Ok(!store.destination(release.tool, &release.version)?.exists())
        }
    }
    fn versions(
        &mut self,
        tool: Tool,
        requested: Option<&str>,
        context: &Context,
    ) -> Result<Vec<Release>> {
        let releases = match tool {
            Tool::Node => node_versions(&self.json("https://nodejs.org/dist/index.json", context)?),
            Tool::Go => {
                go_versions(&self.json("https://go.dev/dl/?mode=json&include=all", context)?)
            }
            Tool::Php => php_versions(&self.json(
                "https://downloads.php.net/~windows/releases/releases.json",
                context,
            )?),
            Tool::Java => {
                let range = if let Some(version) = requested.filter(|s| *s != "latest") {
                    let feature = super::catalog::numbers(version)
                        .first()
                        .copied()
                        .unwrap_or(0);
                    if !(17..100).contains(&feature) {
                        return Err(error(
                            "Temurin provider supports JDK feature releases 17 and later",
                            2,
                        ));
                    }
                    format!("[{feature},{})", feature + 1)
                } else {
                    "[17,100)".into()
                };
                let mut url = reqwest::Url::parse("https://api.adoptium.net/v3/assets/version/")
                    .map_err(|_| error("invalid Adoptium endpoint", 1))?;
                url.path_segments_mut()
                    .map_err(|_| error("invalid Adoptium endpoint", 1))?
                    .pop_if_empty()
                    .push(&range);
                url.query_pairs_mut().extend_pairs([
                    ("architecture", "x64"),
                    ("os", "windows"),
                    ("image_type", "jdk"),
                    ("vendor", "eclipse"),
                    ("jvm_impl", "hotspot"),
                    ("release_type", "ga"),
                    ("page_size", "100"),
                    ("sort_order", "DESC"),
                ]);
                java_versions(&self.json(url.as_str(), context)?)
            }
            Tool::Rust => {
                if let Some(version) = requested.filter(|s| *s != "latest") {
                    let v = self.json(
                        &format!(
                            "https://api.github.com/repos/rust-lang/rust/releases/tags/{version}"
                        ),
                        context,
                    )?;
                    if v["draft"] == true || v["prerelease"] == true {
                        return Err(error("Rust prereleases are not supported", 1));
                    }
                    vec![Release::new(
                        tool,
                        text(&v, "tag_name")?.trim_start_matches('v').into(),
                    )]
                } else {
                    let v = self.json(
                        "https://api.github.com/repos/rust-lang/rust/releases?per_page=100",
                        context,
                    )?;
                    strings(&v)
                        .filter(|v| v["draft"] == false && v["prerelease"] == false)
                        .filter_map(|v| {
                            Some(Release::new(
                                tool,
                                v["tag_name"].as_str()?.trim_start_matches('v').into(),
                            ))
                        })
                        .collect()
                }
            }
            Tool::Git | Tool::Python => {
                let ids = if tool == Tool::Git {
                    vec!["Git.Git".into()]
                } else if let Some(version) = requested.filter(|s| *s != "latest") {
                    vec![tool.package_id(version)?]
                } else {
                    let result = self.winget(
                        &[
                            "search",
                            "--id",
                            "Python.Python.",
                            "--source",
                            "winget",
                            "--disable-interactivity",
                            "--accept-source-agreements",
                        ],
                        context,
                    )?;
                    let output = process::succeeded(result, "WinGet Python catalog search")?;
                    let mut ids: Vec<String> = output
                        .split_whitespace()
                        .filter(|s| s.strip_prefix("Python.Python.").is_some_and(stable))
                        .map(str::to_owned)
                        .collect();
                    ids.sort_by(|a, b| super::catalog::compare(b, a));
                    ids.dedup();
                    ids
                };
                let mut releases = Vec::new();
                for id in ids {
                    let result = self.winget(
                        &[
                            "show",
                            "--id",
                            &id,
                            "--exact",
                            "--versions",
                            "--source",
                            "winget",
                            "--disable-interactivity",
                            "--accept-source-agreements",
                        ],
                        context,
                    )?;
                    let output = process::succeeded(result, "WinGet version discovery")?;
                    releases.extend(
                        output
                            .lines()
                            .map(str::trim)
                            .filter(|v| stable(v) && v.contains('.'))
                            .map(|v| Release::new(tool, v.into())),
                    );
                    if tool == Tool::Python && normalize(releases.clone()).len() >= 5 {
                        break;
                    }
                }
                releases
            }
        };
        let releases = normalize(releases);
        if releases.is_empty() {
            return Err(error(
                "provider returned no supported Windows x64 stable releases",
                1,
            ));
        }
        Ok(releases)
    }
    fn preflight(&mut self, r: &Release, store: &Store, context: &Context) -> Result<()> {
        context.check()?;
        if !cfg!(all(windows, target_arch = "x86_64")) {
            return Err(error(
                "KPM installation providers currently require Windows x64",
                1,
            ));
        }
        match r.tool {
            Tool::Python | Tool::Git => {
                if self
                    .winget_present(&r.tool.package_id(&r.version)?, context)?
                    .is_some()
                {
                    return Err(error(
                        "that WinGet package is already installed; KPM will not adopt or overwrite unmanaged software",
                        1,
                    ));
                }
            }
            Tool::Rust => {
                if store.destination(r.tool, &r.version)?.exists() {
                    return Err(error(
                        "an unrecorded toolchain already exists in the private Rust home; inspect or recover it instead of adopting it",
                        1,
                    ));
                }
                let _ = self.rustup(store)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn install(
        &mut self,
        r: &Release,
        stage: &Path,
        store: &Store,
        context: &Context,
    ) -> Result<PathBuf> {
        match r.tool {
            Tool::Python | Tool::Git => {
                let target = store.target(r.tool, &r.version)?;
                Store::mark(&target, r.tool, &r.version)?;
                let id = r.tool.package_id(&r.version)?;
                let path = crate::platform::resolve_executable("winget", &self.cwd)
                    .map_err(|_| error("WinGet unavailable", 1))?;
                let result = process::capture(
                    Command::new(path)
                        .args([
                            "install",
                            "--id",
                            &id,
                            "--exact",
                            "--version",
                            &r.version,
                            "--source",
                            "winget",
                            "--scope",
                            "user",
                            "--architecture",
                            "x64",
                            "--silent",
                            "--disable-interactivity",
                            "--accept-source-agreements",
                            "--accept-package-agreements",
                            "--no-upgrade",
                            "--location",
                        ])
                        .arg(&target)
                        .current_dir(&self.cwd),
                    Duration::from_secs(1800),
                    context,
                )?;
                process::succeeded(result, "WinGet installation (user scope; no elevation)")?;
                Ok(target)
            }
            Tool::Rust => {
                let version = format!("{}-x86_64-pc-windows-msvc", r.version);
                let result = process::capture(
                    self.rustup(store)?.args([
                        "toolchain",
                        "install",
                        &version,
                        "--profile",
                        "minimal",
                        "--no-self-update",
                    ]),
                    Duration::from_secs(1800),
                    context,
                )?;
                process::succeeded(result, "rustup toolchain installation")?;
                Ok(store.root.join("rustup/toolchains").join(version))
            }
            _ => {
                let mut release = r.clone();
                if r.tool == Tool::Node {
                    let file = format!("node-v{}-win-x64.zip", r.version);
                    let base = format!("https://nodejs.org/dist/v{}/", r.version);
                    let sums =
                        String::from_utf8(self.bytes(&format!("{base}SHASUMS256.txt"), context)?)
                            .map_err(|_| error("invalid Node checksum list", 1))?;
                    release.checksum = sums
                        .lines()
                        .filter_map(|l| l.split_once(char::is_whitespace))
                        .find(|(_, name)| name.trim().trim_start_matches('*') == file)
                        .map(|(hash, _)| hash.to_owned());
                    release.url = Some(format!("{base}{file}"));
                }
                let url = release
                    .url
                    .as_deref()
                    .ok_or_else(|| error("provider returned no artifact URL", 1))?;
                let checksum = release
                    .checksum
                    .as_deref()
                    .ok_or_else(|| error("artifact has no checksum; refusing installation", 1))?;
                validate_artifact(r.tool, url)?;
                let path = stage.join("download.zip");
                self.download(url, &path, context)?;
                let mut file = File::open(&path).map_err(io_error)?;
                archive::checksum(&mut file, checksum, context)?;
                file.seek(SeekFrom::Start(0)).map_err(io_error)?;
                let unpacked = stage.join("unpacked");
                archive::extract(file, &unpacked, context)?;
                let home = find_home(&unpacked, r.tool)?;
                self.verify(r.tool, &r.version, &home, store, context)?;
                Store::mark(&unpacked, r.tool, &r.version)?;
                let target = store.target(r.tool, &r.version)?;
                if let Some(parent) = target.parent() {
                    checked(parent)?;
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                let relative = home
                    .strip_prefix(&unpacked)
                    .map_err(|_| error("invalid archive home", 1))?
                    .to_path_buf();
                crate::platform::filesystem::rename_no_replace(&unpacked, &target)
                    .map_err(io_error)?;
                Ok(target.join(relative))
            }
        }
    }
    fn verify(
        &mut self,
        tool: Tool,
        version: &str,
        home: &Path,
        store: &Store,
        context: &Context,
    ) -> Result<()> {
        checked(home)?;
        let executable = home.join(tool.primary());
        checked(&executable)?;
        if !executable.is_file() {
            return Err(error(
                "installed executable is missing (installer may not support the requested destination)",
                1,
            ));
        }
        let mut command = Command::new(&executable);
        command.args(tool.version_args()).current_dir(home);
        let result = process::succeeded(
            process::capture(&mut command, Duration::from_secs(20), context)?,
            "installed version verification",
        )?;
        let base = version.split('+').next().unwrap_or(version);
        let detected = match tool {
            Tool::Node => result.trim().strip_prefix('v') == Some(version),
            Tool::Go => result
                .split_whitespace()
                .any(|s| s == format!("go{version}")),
            Tool::Java => result.contains(&format!("\"{base}\"")),
            Tool::Git => result
                .split_whitespace()
                .any(|s| git_version_matches(version, s)),
            _ => result.split_whitespace().any(|s| s == version),
        };
        if !detected {
            return Err(error(
                "installed binary does not report the requested version",
                1,
            ));
        }
        if tool == Tool::Java && version.contains('+') && !result.contains(version) {
            return Err(error(
                "JDK runtime build does not match the selected release",
                1,
            ));
        }
        if tool == Tool::Java {
            let javac = home.join("bin/javac.exe");
            checked(&javac)?;
            let result = process::succeeded(
                process::capture(
                    Command::new(javac).arg("-version").current_dir(home),
                    Duration::from_secs(20),
                    context,
                )?,
                "JDK compiler verification",
            )?;
            if !result.split_whitespace().any(|s| s == base) {
                return Err(error(
                    "java and javac do not report the same JDK version",
                    1,
                ));
            }
        }
        if tool == Tool::Node && !home.join("node_modules/npm/bin/npm-cli.js").is_file() {
            return Err(error("Node archive is missing npm", 1));
        }
        if tool == Tool::Node {
            checked(&home.join("node_modules/npm/bin/npm-cli.js"))?;
        }
        if matches!(tool, Tool::Python | Tool::Git)
            && self
                .winget_present(&tool.package_id(version)?, context)?
                .as_deref()
                != Some(version)
        {
            return Err(error(
                "WinGet inventory does not match the verified installation",
                1,
            ));
        }
        if matches!(tool, Tool::Python | Tool::Git)
            && !crate::platform::package_identity::matches_user_installation(home, tool.name())
                .map_err(io_error)?
        {
            return Err(error(
                "WinGet installer identity is not bound to the KPM destination in the current-user registry; pending record preserved for inspection",
                1,
            ));
        }
        let _ = store;
        Ok(())
    }
    fn uninstall(
        &mut self,
        tool: Tool,
        version: &str,
        home: &Path,
        store: &Store,
        context: &Context,
    ) -> Result<()> {
        match tool {
            Tool::Python | Tool::Git => {
                context.check()?;
                let id = tool.package_id(version)?;
                Store::owns(&store.target(tool, version)?, tool, version)?;
                self.verify(tool, version, home, store, context)?;
                if self.winget_present(&id, context)?.as_deref() != Some(version) {
                    return Err(error(
                        "WinGet package changed or disappeared; refusing unmanaged removal",
                        1,
                    ));
                }
                let result = self.winget(
                    &[
                        "uninstall",
                        "--id",
                        &id,
                        "--exact",
                        "--version",
                        version,
                        "--source",
                        "winget",
                        "--scope",
                        "user",
                        "--silent",
                        "--disable-interactivity",
                        "--accept-source-agreements",
                    ],
                    context,
                )?;
                process::succeeded(result, "WinGet uninstall")?;
                if self.winget_present(&id, context)?.is_some()
                    || home.join(tool.primary()).is_file()
                {
                    return Err(error("WinGet uninstall verification failed", 1));
                }
                Ok(())
            }
            Tool::Rust => {
                process::succeeded(
                    process::capture(
                        self.rustup(store)?.args([
                            "toolchain",
                            "uninstall",
                            &format!("{version}-x86_64-pc-windows-msvc"),
                        ]),
                        Duration::from_secs(300),
                        context,
                    )?,
                    "rustup uninstall",
                )?;
                if home.exists() {
                    return Err(error("rustup did not remove the managed toolchain", 1));
                }
                Ok(())
            }
            _ => {
                let target = store.target(tool, version)?;
                Store::owns(&target, tool, version)?;
                super::store::remove_owned_tree(&target)
            }
        }
    }
}
pub fn git_version_matches(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    let parts: Vec<_> = expected.split('.').collect();
    if parts.len() > 3 {
        actual == format!("{}.windows.{}", parts[..3].join("."), parts[3..].join("."))
    } else {
        actual.starts_with(&format!("{expected}.windows."))
    }
}
pub fn validate_artifact(tool: Tool, url: &str) -> Result<()> {
    let url = reqwest::Url::parse(url).map_err(|_| error("invalid download URL", 1))?;
    let valid = match tool {
        Tool::Node => url.host_str() == Some("nodejs.org") && url.path().starts_with("/dist/v"),
        Tool::Go => url.host_str() == Some("go.dev") && url.path().starts_with("/dl/"),
        Tool::Php => {
            url.host_str() == Some("downloads.php.net")
                && url.path().starts_with("/~windows/releases/")
        }
        Tool::Java => {
            url.host_str() == Some("github.com")
                && url.path().starts_with("/adoptium/temurin")
                && url.path().contains("-binaries/releases/download/")
        }
        _ => false,
    };
    if !allowed(&url) || !valid || !url.path().ends_with(".zip") {
        return Err(error(
            "artifact identity does not match the selected provider",
            1,
        ));
    }
    Ok(())
}
pub fn find_home(root: &Path, tool: Tool) -> Result<PathBuf> {
    if root.join(tool.primary()).is_file() {
        return Ok(root.to_path_buf());
    }
    let mut homes = Vec::new();
    for entry in fs::read_dir(root).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        checked(&path)?;
        if path.is_dir() && path.join(tool.primary()).is_file() {
            homes.push(path);
        }
    }
    if homes.len() != 1 {
        return Err(error(
            "archive does not contain exactly one supported tool installation",
            1,
        ));
    }
    Ok(homes.remove(0))
}
pub fn resolve(
    provider: &mut impl Provider,
    tool: Tool,
    version: &str,
    context: &Context,
) -> Result<Release> {
    select(
        provider.versions(tool, Some(version), context)?,
        version,
        tool,
    )
}

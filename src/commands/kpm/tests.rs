use super::*;
use providers::Provider;
use std::{
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use store::{Record, Store};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kshell-kpm-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> Store {
        Store {
            root: self.0.join("store"),
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[derive(Default)]
struct Mock {
    releases: Vec<Release>,
    versions: usize,
    installs: usize,
    uninstalls: usize,
    fail_preflight: bool,
    fail_versions: bool,
    fail_install: bool,
    fail_verify: bool,
}
impl Provider for Mock {
    fn absent(&mut self, r: &Release, store: &Store, _: &process::Context) -> Result<bool> {
        Ok(!store
            .destination(r.tool, &r.version)?
            .join(r.tool.primary())
            .is_file())
    }
    fn versions(&mut self, _: Tool, _: Option<&str>, _: &process::Context) -> Result<Vec<Release>> {
        self.versions += 1;
        if self.fail_versions {
            return Err(error("provider network request failed", 1));
        }
        Ok(self.releases.clone())
    }
    fn preflight(&mut self, _: &Release, _: &Store, _: &process::Context) -> Result<()> {
        if self.fail_preflight {
            Err(error("WinGet is unavailable", 1))
        } else {
            Ok(())
        }
    }
    fn install(
        &mut self,
        r: &Release,
        _: &std::path::Path,
        store: &Store,
        context: &process::Context,
    ) -> Result<PathBuf> {
        context.check()?;
        self.installs += 1;
        if self.fail_install {
            return Err(error("installer cancelled", 130));
        }
        let home = if r.tool == Tool::Rust {
            store
                .root
                .join("rustup/toolchains")
                .join(format!("{}-x86_64-pc-windows-msvc", r.version))
        } else {
            store.target(r.tool, &r.version)?
        };
        if r.tool != Tool::Rust {
            Store::mark(&home, r.tool, &r.version)?;
        }
        let executable = home.join(r.tool.primary());
        fs::create_dir_all(executable.parent().unwrap()).map_err(io_error)?;
        fs::write(&executable, &r.version).map_err(io_error)?;
        if r.tool == Tool::Java {
            fs::write(home.join("bin/javac.exe"), &r.version).map_err(io_error)?;
        }
        if r.tool == Tool::Node {
            fs::create_dir_all(home.join("node_modules/npm/bin")).map_err(io_error)?;
            fs::write(home.join("node_modules/npm/bin/npm-cli.js"), b"fixture")
                .map_err(io_error)?;
        }
        Ok(home)
    }
    fn verify(
        &mut self,
        tool: Tool,
        version: &str,
        home: &std::path::Path,
        _: &Store,
        context: &process::Context,
    ) -> Result<()> {
        context.check()?;
        if self.fail_verify
            || fs::read_to_string(home.join(tool.primary())).map_err(io_error)? != version
        {
            return Err(error("verification failed", 1));
        }
        Ok(())
    }
    fn uninstall(
        &mut self,
        _: Tool,
        _: &str,
        home: &std::path::Path,
        _: &Store,
        _: &process::Context,
    ) -> Result<()> {
        self.uninstalls += 1;
        store::remove_owned_tree(home)
    }
}
fn manager(temp: &Temp) -> Manager<Mock> {
    Manager {
        store: temp.store(),
        provider: Mock::default(),
        context: process::Context::testing(),
    }
}
fn release(tool: Tool, version: &str) -> Release {
    Release::new(tool, version.into())
}
fn plain_ui() -> ui::Ui {
    ui::Ui {
        interactive: false,
        width: 80,
        color: false,
    }
}

#[test]
fn single_tool_arguments_exact_latest_and_rejection_are_pure() {
    for tool in Tool::ALL {
        let a = args::parse(&[
            "install".into(),
            format!("{}@latest", tool.name()),
            "--yes".into(),
        ])
        .unwrap();
        assert!(a.yes);
        assert_eq!(
            a.command,
            Command::Install(Some(Target {
                tool,
                version: Some("latest".into())
            }))
        );
    }
    assert!(args::parse(&["install".into()]).is_ok());
    for input in [
        vec!["install", "node", "python", "git"],
        vec!["install", "node@../../escape"],
        vec!["install", "node@1.2.0-rc.1"],
        vec!["install", "node@"],
        vec!["install", "unknown"],
        vec!["uninstall", "node"],
        vec!["uninstall", "node@latest"],
        vec!["use", "node", "--yes"],
    ] {
        assert!(
            args::parse(&input.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_err(),
            "{input:?}"
        );
    }
    let error = args::parse(&["install".into(), "node".into(), "python".into()]).unwrap_err();
    assert!(error.message.contains("Only one tool"));
}
#[test]
fn stable_sorting_dedup_latest_five_and_java_aliases_follow_provider_numbers() {
    let releases = vec![
        "24.0.1",
        "24.0.1",
        "24.0.2",
        "24.0.10",
        "23.9.9",
        "22.15.0",
        "21.8.0",
        "25.0.0-rc.1",
    ]
    .into_iter()
    .map(|v| release(Tool::Node, v))
    .collect();
    let sorted = normalize(releases);
    assert_eq!(
        sorted
            .iter()
            .take(5)
            .map(|r| r.version.as_str())
            .collect::<Vec<_>>(),
        ["24.0.10", "24.0.2", "24.0.1", "23.9.9", "22.15.0"]
    );
    assert_eq!(
        catalog::select(sorted.clone(), "latest", Tool::Node)
            .unwrap()
            .version,
        "24.0.10"
    );
    assert!(catalog::select(sorted, "99.0.0", Tool::Node).is_err());
    for version in [
        "1.0.0-alpha",
        "1.0.0b1",
        "1.0.0rc1",
        "nightly",
        "preview",
        "1.+2",
        "999999999999999999999999.0",
    ] {
        assert!(!catalog::stable(version));
    }
    assert!(catalog::stable("21.0.6+7"));
    assert!(catalog::compare("21.0.1+12", "21+35").is_gt());
    assert_eq!(
        catalog::select(vec![release(Tool::Java, "25+36")], "25", Tool::Java)
            .unwrap()
            .version,
        "25+36"
    );
    let java = vec![
        release(Tool::Java, "21.0.6+7"),
        release(Tool::Java, "21.0.7+8"),
        release(Tool::Java, "25.0.1+9"),
    ];
    assert_eq!(
        catalog::select(java, "21", Tool::Java).unwrap().version,
        "21.0.7+8"
    );
}
#[test]
fn provider_fixtures_filter_target_platform_stability_jdk_and_checksums() {
    assert!(providers::git_version_matches(
        "2.47.1.2",
        "2.47.1.windows.2"
    ));
    assert!(providers::git_version_matches("2.47.1", "2.47.1.windows.1"));
    assert!(!providers::git_version_matches(
        "2.47.1.2",
        "2.47.1.windows.3"
    ));
    use serde_json::json;
    let node = json!([{"version":"v24.1.0","files":["win-x64-zip"],"lts":"Test"},{"version":"v25.0.0-rc.1","files":["win-x64-zip"]},{"version":"v24.2.0","files":["linux-x64"]}]);
    let versions = providers::node_versions(&node);
    assert_eq!(versions.len(), 1);
    assert!(versions[0].note.as_deref().unwrap().contains("LTS"));
    let go = json!([{"version":"go1.24.1","stable":true,"files":[{"os":"windows","arch":"amd64","kind":"archive","filename":"go1.24.1.windows-amd64.zip","sha256":"a".repeat(64)}]},{"version":"go1.25beta1","stable":false,"files":[]}]);
    assert_eq!(providers::go_versions(&go)[0].version, "1.24.1");
    let php = json!({"8.4":{"version":"8.4.3","nts-vs17-x64":{"zip":{"path":"php-8.4.3-nts-Win32-vs17-x64.zip","sha256":"b".repeat(64)}}}});
    assert!(
        providers::php_versions(&php)[0]
            .url
            .as_ref()
            .unwrap()
            .contains("downloads.php.net")
    );
    let java = json!([{"release_type":"ga","release_name":"jdk-21.0.6+7","binaries":[{"os":"windows","architecture":"x64","image_type":"jdk","package":{"link":"https://github.com/adoptium/temurin21-binaries/releases/download/test/jdk.zip","checksum":"c".repeat(64)}}]},{"release_type":"ea","release_name":"jdk-26+1","binaries":[]},{"release_type":"ga","release_name":"jdk-21.0.6+7","binaries":[{"os":"windows","architecture":"x64","image_type":"jre","package":{}}]}]);
    let versions = providers::java_versions(&java);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version, "21.0.6+7");
    assert!(providers::validate_artifact(Tool::Java, versions[0].url.as_deref().unwrap()).is_ok());
    for url in [
        "http://nodejs.org/dist/v24/a.zip",
        "https://evil.invalid/node.zip",
        "https://nodejs.org.evil.invalid/dist/v1/a.zip",
        "https://user:secret@nodejs.org/dist/v1/a.zip",
    ] {
        assert!(providers::validate_artifact(Tool::Node, url).is_err());
    }
}
#[test]
fn store_lock_unmanaged_paths_atomic_metadata_and_tampering_are_guarded() {
    let temp = Temp::new();
    let store = temp.store();
    assert!(store.read().unwrap().records.is_empty());
    assert!(!store.root.exists());
    let lock = store.lock().unwrap();
    assert!(store.lock().is_err());
    drop(lock);
    drop(store.lock().unwrap());
    assert!(store.target(Tool::Node, "../../unmanaged").is_err());
    let bad = Record {
        tool: Tool::Node,
        version: "1.0.0".into(),
        home: "packages/node/1.0.0/../../outside".into(),
    };
    assert!(store.home(&bad).is_err());
    fs::write(store.root.join("installed.json"), b"invalid").unwrap();
    assert!(store.read().is_err());
    let other = Store {
        root: temp.0.join("unmanaged"),
    };
    fs::create_dir(&other.root).unwrap();
    fs::write(other.root.join("keep"), b"safe").unwrap();
    assert!(other.lock().is_err());
    assert_eq!(fs::read(other.root.join("keep")).unwrap(), b"safe");
}
#[test]
fn managed_install_switch_and_active_removal_never_download_during_use() {
    let temp = Temp::new();
    let mut manager = manager(&temp);
    let (one, created) = manager.install(&release(Tool::Node, "1.0.0")).unwrap();
    assert!(created);
    let (two, _) = manager.install(&release(Tool::Node, "2.0.0")).unwrap();
    manager.activate(&one).unwrap();
    let mut output = Vec::new();
    dispatch(
        args::Args {
            command: Command::Use(Target {
                tool: Tool::Node,
                version: Some("2.0.0".into()),
            }),
            yes: false,
        },
        &mut manager,
        &plain_ui(),
        &mut output,
    )
    .unwrap();
    assert_eq!(manager.provider.installs, 2);
    assert_eq!(manager.provider.versions, 0);
    assert_eq!(
        manager
            .installed()
            .unwrap()
            .active
            .get(&Tool::Node)
            .map(String::as_str),
        Some("2.0.0")
    );
    assert!(!manager.install(&release(Tool::Node, "2.0.0")).unwrap().1);
    assert_eq!(manager.provider.installs, 2);
    let untracked = Record {
        tool: Tool::Node,
        version: "99.0.0".into(),
        home: "outside".into(),
    };
    assert!(manager.uninstall(&untracked).is_err());
    assert_eq!(manager.provider.uninstalls, 0);
    manager.uninstall(&two).unwrap();
    assert!(
        !manager
            .installed()
            .unwrap()
            .active
            .contains_key(&Tool::Node)
    );
    assert!(manager.home(&one).unwrap().exists());
}
#[test]
fn java_home_and_rust_homes_are_child_only_and_java_alias_selects_newest_installed() {
    let temp = Temp::new();
    let mut manager = manager(&temp);
    manager.install(&release(Tool::Java, "21.0.6+7")).unwrap();
    manager.install(&release(Tool::Java, "21.0.7+8")).unwrap();
    let record = manager
        .installed()
        .unwrap()
        .record(Tool::Java, "21")
        .unwrap()
        .clone();
    assert_eq!(record.version, "21.0.7+8");
    manager.activate(&record).unwrap();
    let (rust, _) = manager.install(&release(Tool::Rust, "1.85.0")).unwrap();
    manager.activate(&rust).unwrap();
    let environment =
        environment::Environment::from_inventory(&manager.store, &manager.installed().unwrap())
            .unwrap();
    let original = std::env::var_os("JAVA_HOME");
    let mut child = std::process::Command::new("fixture.exe");
    environment.apply(&mut child).unwrap();
    let env: std::collections::BTreeMap<_, _> = child
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.unwrap().to_os_string()))
        .collect();
    assert_eq!(
        env.get(std::ffi::OsStr::new("JAVA_HOME")),
        Some(&manager.home(&record).unwrap().into_os_string())
    );
    assert_eq!(std::env::var_os("JAVA_HOME"), original);
    assert_eq!(
        env.get(std::ffi::OsStr::new("RUSTUP_HOME")),
        Some(&manager.store.root.join("rustup").into_os_string())
    );
    assert_eq!(
        env.get(std::ffi::OsStr::new("CARGO_HOME")),
        Some(&manager.store.root.join("cargo").into_os_string())
    );
    let java = environment.command("java", &temp.0).unwrap().unwrap();
    let javac = environment.command("javac", &temp.0).unwrap().unwrap();
    assert_eq!(
        std::path::Path::new(java.get_program()).parent(),
        std::path::Path::new(javac.get_program()).parent()
    );
    assert!(
        environment
            .command("../java.exe", &temp.0)
            .unwrap()
            .is_none()
    );
}
#[test]
fn failed_preflight_cancelled_install_and_failed_verification_are_recoverable() {
    let temp = Temp::new();
    let mut manager = manager(&temp);
    let r = release(Tool::Node, "1.0.0");
    manager.provider.fail_preflight = true;
    assert!(manager.install(&r).unwrap_err().message.contains("WinGet"));
    assert!(manager.store.pending().unwrap().is_none());
    assert_eq!(manager.provider.installs, 0);
    manager.provider.fail_preflight = false;
    manager.provider.fail_install = true;
    assert_eq!(manager.install(&r).unwrap_err().status, 130);
    assert!(manager.store.pending().unwrap().is_some());
    assert!(manager.installed().unwrap().records.is_empty());
    manager.provider.fail_install = false;
    manager.recover().unwrap();
    assert!(manager.store.pending().unwrap().is_none());
    manager.provider.fail_verify = true;
    assert!(manager.install(&r).is_err());
    assert!(manager.store.pending().unwrap().is_some());
    assert!(manager.installed().unwrap().records.is_empty());
    manager.provider.fail_verify = false;
    manager.recover().unwrap();
    assert_eq!(manager.installed().unwrap().records.len(), 1);
    assert!(manager.installed().unwrap().active.is_empty());
    manager.context.cancel();
    assert_eq!(
        manager
            .install(&release(Tool::Node, "2.0.0"))
            .unwrap_err()
            .status,
        130
    );
}
#[test]
fn cancelled_winget_work_cleans_only_marked_files_after_absence_is_confirmed() {
    let temp = Temp::new();
    let mut manager = manager(&temp);
    manager.provider.fail_install = true;
    let release = release(Tool::Python, "3.13.2");
    assert!(manager.install(&release).is_err());
    let target = manager
        .store
        .target(release.tool, &release.version)
        .unwrap();
    Store::mark(&target, release.tool, &release.version).unwrap();
    assert!(manager.recover().unwrap().contains("absent"));
    assert!(!target.exists());
    assert!(manager.store.pending().unwrap().is_none());
}
#[test]
fn noninteractive_version_confirmation_and_unsupported_switching_fail_without_installing() {
    let temp = Temp::new();
    let mut manager = manager(&temp);
    let target = Target {
        tool: Tool::Node,
        version: None,
    };
    assert_eq!(
        choose_release(&mut manager, &target, false, &plain_ui())
            .unwrap_err()
            .status,
        2
    );
    assert_eq!(manager.provider.versions, 0);
    assert!(plain_ui().confirm("install?", false).is_err());
    assert!(plain_ui().confirm("install?", true).unwrap());
    let (record, _) = manager.install(&release(Tool::Python, "3.13.2")).unwrap();
    assert!(manager.activate(&record).is_err());
    assert!(manager.install(&release(Tool::Python, "3.13.3")).is_err());
    assert!(
        choose_release(
            &mut manager,
            &Target {
                tool: Tool::Python,
                version: Some("latest".into())
            },
            true,
            &plain_ui()
        )
        .is_err()
    );
}
#[test]
fn checksum_archive_traversal_and_duplicate_entries_fail_before_execution() {
    use sha2::{Digest, Sha256};
    let temp = Temp::new();
    let zip_path = temp.0.join("good.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    writer
        .start_file("bin/tool.exe", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"fixture").unwrap();
    writer.finish().unwrap();
    let bytes = fs::read(&zip_path).unwrap();
    let expected = format!("{:x}", Sha256::digest(&bytes));
    assert!(
        archive::checksum(
            &mut fs::File::open(&zip_path).unwrap(),
            &expected,
            &process::Context::testing()
        )
        .is_ok()
    );
    assert!(
        archive::checksum(
            &mut fs::File::open(&zip_path).unwrap(),
            &"0".repeat(64),
            &process::Context::testing()
        )
        .is_err()
    );
    let cancelled = process::Context::testing();
    cancelled.cancel();
    assert_eq!(
        archive::checksum(
            &mut fs::File::open(&zip_path).unwrap(),
            &expected,
            &cancelled
        )
        .unwrap_err()
        .status,
        130
    );
    archive::extract(
        fs::File::open(&zip_path).unwrap(),
        &temp.0.join("good"),
        &process::Context::testing(),
    )
    .unwrap();
    assert_eq!(
        fs::read(temp.0.join("good/bin/tool.exe")).unwrap(),
        b"fixture"
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../escaped.exe", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"bad").unwrap();
    let bad = writer.finish().unwrap().into_inner();
    fs::write(temp.0.join("bad.zip"), bad).unwrap();
    assert!(
        archive::extract(
            fs::File::open(temp.0.join("bad.zip")).unwrap(),
            &temp.0.join("bad"),
            &process::Context::testing()
        )
        .is_err()
    );
    assert!(!temp.0.join("escaped.exe").exists());
    #[cfg(windows)]
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in [
            ("same.exe", b"first".as_slice()),
            ("Same.exe", b"second".as_slice()),
        ] {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body).unwrap();
        }
        fs::write(
            temp.0.join("collision.zip"),
            writer.finish().unwrap().into_inner(),
        )
        .unwrap();
        assert!(
            archive::extract(
                fs::File::open(temp.0.join("collision.zip")).unwrap(),
                &temp.0.join("collision"),
                &process::Context::testing()
            )
            .is_err()
        );
        assert_eq!(
            fs::read(temp.0.join("collision/same.exe")).unwrap(),
            b"first"
        );
    }
}
#[test]
fn animation_frames_move_right_wrap_fit_and_remain_plain_without_color() {
    for width in [8, 24, 80] {
        let frames: Vec<_> = (0..6).map(|i| ui::frame(width, i, false)).collect();
        for (i, frame) in frames.iter().enumerate() {
            assert!(!frame.contains('\x1b'));
            assert!(frame.chars().count() <= width);
            assert_eq!(
                frame.chars().position(|c| c == '●'),
                Some(1 + i % width.saturating_sub(2).clamp(1, 50))
            );
        }
    }
}

#[test]
fn cursor_cleanup_runs_on_errors_and_nonterminal_mode_has_no_ansi() {
    #[derive(Clone)]
    struct Buffer(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for enabled in [false, true] {
        let buffer = Buffer(Default::default());
        let output = buffer.clone();
        let operation = || -> Result<()> {
            let _cursor = ui::Cursor::new(enabled, output)?;
            Err(error("simulated cancelled operation", 130))
        };
        assert_eq!(operation().unwrap_err().status, 130);
        assert_eq!(
            &*buffer.0.borrow(),
            if enabled {
                b"\x1b[?25l\x1b[?25h".as_slice()
            } else {
                b"".as_slice()
            }
        );
    }
    for width in 0..3 {
        assert!(ui::frame(width, 3, false).chars().count() <= width);
    }
}

#[test]
fn exact_version_validation_and_network_errors_happen_before_installation() {
    let temp = Temp::new();
    let mut manager = manager(&temp);
    manager.provider.releases = vec![
        release(Tool::Node, "22.15.0"),
        release(Tool::Node, "24.0.0"),
    ];
    let chosen = choose_release(
        &mut manager,
        &Target {
            tool: Tool::Node,
            version: Some("22.15.0".into()),
        },
        false,
        &plain_ui(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(chosen.version, "22.15.0");
    assert_eq!(manager.provider.installs, 0);
    assert!(
        choose_release(
            &mut manager,
            &Target {
                tool: Tool::Node,
                version: Some("99.0.0".into())
            },
            false,
            &plain_ui()
        )
        .is_err()
    );
    manager.provider.fail_versions = true;
    let error = discover(&mut manager, Tool::Node, None, &plain_ui()).unwrap_err();
    assert!(error.message.contains("network"));
    assert!(!manager.store.root.exists());
}
#[test]
fn package_subprocess_status_capture_and_cancellation_reap_the_child() {
    let context = process::Context::testing();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "commands::kpm::tests::process_probe",
            "--nocapture",
        ])
        .env("KSHELL_KPM_TEST_PROBE", "emit");
    let result = process::capture(&mut command, Duration::from_secs(10), &context).unwrap();
    assert_eq!(result.status, 37);
    assert!(result.text.contains("provider output") && result.text.contains("provider diagnostic"));
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            context.cancel();
        });
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "commands::kpm::tests::process_probe",
                "--nocapture",
            ])
            .env("KSHELL_KPM_TEST_PROBE", "sleep");
        let error = process::capture(&mut command, Duration::from_secs(5), &context).unwrap_err();
        assert_eq!(error.status, 130);
    });
}
#[test]
#[ignore = "child-only process fixture; no software installation"]
fn process_probe() {
    match std::env::var("KSHELL_KPM_TEST_PROBE").as_deref() {
        Ok("emit") => {
            println!("provider output");
            eprintln!("provider diagnostic");
            std::process::exit(37);
        }
        Ok("sleep") => std::thread::sleep(Duration::from_secs(60)),
        _ => {}
    }
}

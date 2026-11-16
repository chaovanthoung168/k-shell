# KPM: developer tools and versions

[Back to the README](../README.md#package-manager-kpm)


KPM is a native K Shell builtin for installing developer tools and selecting
managed versions. It is in the updated source build; the existing v0.1.6 download
does not include it. Build the source with current stable Rust and launch the
resulting K Shell, then run `kpm --help`.

```text
kpm list
kpm search java
kpm versions node
kpm install
kpm install node
kpm install java
```

`kpm install` opens a single-tool menu. `kpm install node` (or another tool)
discovers its available Windows x64 releases and shows a separate Latest Stable
shortcut, the five newest distinct stable versions, and Other version for manual
input. The five releases need not have different major versions. Fewer entries
are shown when the provider has fewer releases. Prereleases are filtered out.
Menus use Up/Down, Enter and Esc/Ctrl+C; Esc from version selection returns to
tool selection when installation began without a tool. Discovery failures offer
Retry/Cancel. Installed and saved active versions are marked in the menu.

Exact versions skip the version menu, but still require availability validation
and installation confirmation. Version numbers below are syntax examples, not
claims that those releases are still available:

```text
kpm install node@22.15.0
kpm install python@3.13.2
kpm install go@1.24.1
kpm install rust@1.85.0
kpm install php@8.4.3
kpm install git@2.47.1
kpm install java@21
```

Java selects a full Eclipse Temurin JDK, including `javac`, for feature releases
17 and later. `java@21` resolves the newest available JDK 21 patch/build; a full
provider version such as `java@21.0.6+7` can pin a particular release. Java
versions are not treated as universal SemVer. Git Windows build suffixes are
also verified according to Git's version output.

For scripts or redirected input, specify an explicit version or `@latest` and
`--yes`. The flag skips confirmation, not validation; in a terminal it does not
skip the version menu when no version was supplied. One install command accepts
one tool only:

```text
kpm install node@latest --yes
kpm install java@21 --yes
```

Before confirmation, KPM shows the tool, resolved version, provider, destination,
and any executable found on PATH. Installation uses a cyan, indeterminate marker
moving left to right on stderr, without fake percentages. `KSHELL_KPM_FPS` sets
5-60 refreshes per second (default 30); redirected stderr uses plain messages.
Cursor visibility is restored on success, failure and cancellation.

| Tool | Provider and version management |
| --- | --- |
| Node.js | Official ZIP and SHA-256 list from nodejs.org; isolated versions, switching, native npm/npx integration. LTS labels come from metadata. |
| Python | WinGet `Python.Python.<major>.<minor>`, user scope. One recorded version; `use` and in-place `upgrade` are explicitly unsupported. |
| Go | go.dev release metadata, ZIP and SHA-256; isolated SDKs and switching. |
| Rust | Existing native rustup client; private `RUSTUP_HOME` and `CARGO_HOME`, pinned MSVC toolchains, switching. KPM never bootstraps rustup or changes the user's existing Rust homes. |
| PHP | Official Windows NTS x64 ZIPs and hashes from downloads.php.net; isolated versions and switching. Discovery is limited to the published releases JSON, so archive-only older versions are unavailable. A matching Visual C++ runtime must already be installed. |
| Git | WinGet `Git.Git`, user scope. One recorded version; `use` and in-place `upgrade` are explicitly unsupported. |
| Java | Adoptium metadata and verified Temurin JDK ZIPs; isolated JDKs, switching, matching `java`/`javac`, and child `JAVA_HOME`. |

WinGet must be available through Microsoft's App Installer for Python/Git. The
selected installer must support user scope, KPM's custom destination, and a
matching current-user registry InstallLocation. Otherwise verification fails
and the pending record remains for inspection. Existing unmanaged packages are
not adopted or overwritten. Package/source agreements are accepted as part of
the confirmed installation. WinGet's own installer may update user settings;
KPM never changes system PATH or requests administrator elevation. WinGet
uninstall rechecks exact package/version, managed files and current-user
installer location before removal. Replace a managed Python/Git version by
uninstalling it first, then installing another version.

```text
kpm installed
kpm installed node
kpm current node
kpm use node
kpm use node@22.15.0
kpm use java@21
java -version
javac -version
kpm upgrade node
kpm upgrade node@latest --yes
kpm uninstall node@22.15.0
kpm recover
```

`use` lists only installed, recorded versions and never downloads anything.
Activation runs version verification before saving the selection; Java verifies
both runtime and compiler. The selection affects subsequent commands immediately
and persists across K Shell restarts. K Shell prepends managed bin directories
and sets JAVA_HOME/GOROOT/private Rust variables for children only. Explicit
executable paths retain their normal meaning; native npm/npx use the selected
Node and its packaged CLI scripts. System PATH and the parent terminal's
environment stay unchanged. The shell still lacks variable expansion, so use
`kpm current java`, `java -version` and `javac -version` to check the selection.

The first switchable installation activates automatically after verification.
When another version is active, an interactive install offers activation;
`--yes` preserves that existing selection, so switch explicitly with `kpm use`.
Upgrade installs a newer isolated release; it keeps earlier versions. Active
uninstall confirmation explicitly includes deactivation; `--yes` authorizes
that deactivation too. Only KPM-recorded installations can be removed.

Storage defaults to `%LOCALAPPDATA%\KShell\kpm`. `KSHELL_KPM_HOME` selects an
absolute private root; an empty setting disables storage. KPM refuses nonempty
unmarked roots, unsafe paths and reparse points. An OS file lock serializes
changes, installation records are replaced atomically, and a pending journal
supports recovery after failure or interruption. Downloads require HTTPS from
the configured provider and SHA-256 validation before extraction. ZIP traversal,
links, duplicate files and excessive sizes are rejected.

`kpm recover` verifies a completed pending installation before recording it,
or cleans marked staging when no installation exists. For absent WinGet packages,
recovery also cleans the marked incomplete destination after checking WinGet's
inventory. Recovered versions are not activated automatically. Unverifiable
partial installations, especially WinGet work
outside the requested destination, remain pending for inspection. Do not delete
the store to work around an error. Cancellation drops HTTP work and kills owned
native process jobs; brokered installers can outlive WinGet, so recovery must
check their actual result. Uninstall/file cleanup is not transactional.

HTTP work shares one lazy current-thread Tokio runtime with `kreq`. Production
providers are implemented; automated tests use mock providers, local archives,
and compiled test-owned native binaries, never real software installs or public
network services. Live installation across all seven upstreams still requires
separate verification on machines with the relevant installer prerequisites.

use super::{
    Result,
    catalog::{Tool, stable},
    error,
};
pub const HELP: &str = "K Shell Package Manager\n\nUSAGE:\n  kpm install [tool[@version]] [--yes]\n  kpm versions tool\n  kpm installed [tool]\n  kpm current tool\n  kpm use tool[@version]\n  kpm upgrade tool[@version] [--yes]\n  kpm uninstall tool@version [--yes]\n  kpm recover [--yes]\n  kpm list\n  kpm search query\n\nTOOLS: node, python, go, rust, php, git, java (Temurin JDK 17+)\n\nInteractive install/upgrade shows Latest Stable, five distinct stable releases,\nand Other version. Exact versions skip selection, not validation/confirmation.\nNoninteractive installs/upgrades require @version or @latest and --yes.\nJava @21 selects the newest available JDK 21 patch/build.\nUse selects only installed managed versions; it never downloads software.\nPython/Git use WinGet user scope: one managed version, no kpm switching or\nin-place upgrade. Uninstall that version first to replace it. Rust needs an\nexisting native rustup client; its toolchains and Cargo home remain KPM-private.\nActive selections persist; PATH/JAVA_HOME changes affect K Shell children only.\nNo system PATH changes, automatic elevation or unmanaged uninstall.\nKSHELL_KPM_HOME overrides LOCALAPPDATA/KShell/kpm (empty disables KPM storage).\nKSHELL_KPM_FPS sets progress refresh (5-60, default 30); nonterminal stderr is plain.\nRecover verifies an interrupted install before recording it, or clears its staging\nfiles when no installation exists. It never removes an unmanaged installation.\n";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub tool: Tool,
    pub version: Option<String>,
}
impl Target {
    fn parse(value: &str) -> Result<Self> {
        let (tool, version) = value
            .split_once('@')
            .map_or((value, None), |(t, v)| (t, Some(v)));
        let tool = Tool::parse(tool)?;
        if let Some(v) = version
            && v != "latest"
            && !stable(v)
        {
            return Err(error(
                "invalid version; use an available stable version or @latest",
                2,
            ));
        }
        Ok(Self {
            tool,
            version: version.map(str::to_owned),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    List,
    Search(String),
    Versions(Tool),
    Installed(Option<Tool>),
    Current(Tool),
    Install(Option<Target>),
    Use(Target),
    Upgrade(Target),
    Uninstall(Target),
    Recover,
}
#[derive(Debug, Clone)]
pub struct Args {
    pub command: Command,
    pub yes: bool,
}
pub fn parse(args: &[String]) -> Result<Args> {
    let mut words = Vec::new();
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--yes" | "-y" => yes = true,
            "--help" | "-h" => {
                return Ok(Args {
                    command: Command::Help,
                    yes: false,
                });
            }
            _ if arg.starts_with('-') => return Err(error("unknown option; see kpm --help", 2)),
            _ => words.push(arg.as_str()),
        }
    }
    let command = match words.as_slice() {
        [] | ["help"] => Command::Help,
        ["list"] => Command::List,
        ["search", q] => Command::Search(q.to_string()),
        ["versions", t] => Command::Versions(Tool::parse(t)?),
        ["installed"] => Command::Installed(None),
        ["installed", t] => Command::Installed(Some(Tool::parse(t)?)),
        ["current", t] => Command::Current(Tool::parse(t)?),
        ["install"] => Command::Install(None),
        ["install", t] => Command::Install(Some(Target::parse(t)?)),
        ["install", ..] => {
            return Err(error(
                "Only one tool can be installed at a time.\nUsage: kpm install <tool>[@version]",
                2,
            ));
        }
        ["use", t] => Command::Use(Target::parse(t)?),
        ["upgrade", t] => Command::Upgrade(Target::parse(t)?),
        ["uninstall", t] => {
            let t = Target::parse(t)?;
            if t.version.as_deref().is_none_or(|v| v == "latest") {
                return Err(error("uninstall requires tool@installed-version", 2));
            }
            Command::Uninstall(t)
        }
        ["recover"] => Command::Recover,
        _ => return Err(error("invalid arguments; see kpm --help", 2)),
    };
    if yes
        && !matches!(
            command,
            Command::Install(_) | Command::Upgrade(_) | Command::Uninstall(_) | Command::Recover
        )
    {
        return Err(error(
            "--yes is only valid for install, upgrade, uninstall or recover",
            2,
        ));
    }
    Ok(Args { command, yes })
}

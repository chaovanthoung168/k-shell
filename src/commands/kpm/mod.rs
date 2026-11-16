//! KPM is part of the shell's dispatcher; providers own installer mechanics.
mod archive;
mod args;
mod catalog;
pub(crate) mod environment;
mod manager;
mod process;
mod providers;
mod store;
mod ui;

use crate::{commands::OutputFormat, error::ShellError, runtime::ShellState};
use args::{Command, Target};
use catalog::{Release, Tool, compare, normalize};
use manager::Manager;
use providers::{Native, Provider};
use std::io::Write;
use store::Store;
type Result<T> = std::result::Result<T, ShellError>;
fn error(message: impl Into<String>, status: i32) -> ShellError {
    ShellError::new(format!("kpm: {}", message.into()), status)
}
fn io_error(e: std::io::Error) -> ShellError {
    error(e.to_string(), 1)
}
pub(crate) fn valid_syntax(args: &[String]) -> bool {
    args::parse(args).is_ok()
}

pub(super) fn run(
    args: &[String],
    state: &mut ShellState,
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<()> {
    let args = args::parse(args)?;
    match &args.command {
        Command::Help => return output.write_all(args::HELP.as_bytes()).map_err(io_error),
        Command::List | Command::Search(_) => {
            let query = if let Command::Search(q) = &args.command {
                q.to_ascii_lowercase()
            } else {
                String::new()
            };
            let mut count = 0;
            for tool in Tool::ALL {
                if format!("{} {} {}", tool.name(), tool.label(), tool.provider())
                    .to_ascii_lowercase()
                    .contains(&query)
                {
                    writeln!(
                        output,
                        "{:<8} {:<32} {}{}",
                        tool.name(),
                        tool.label(),
                        tool.provider(),
                        if tool.switchable() {
                            "; isolated versions + switching"
                        } else {
                            "; one version; switching unsupported"
                        }
                    )
                    .map_err(io_error)?;
                    count += 1;
                }
            }
            if count == 0 {
                writeln!(output, "No supported tools matched. Use kpm list.").map_err(io_error)?;
            }
            return Ok(());
        }
        _ => {}
    }
    let store = Store::from_environment()?;
    // Read-only inventory commands need neither WinGet nor network initialization.
    if let Command::Installed(filter) = args.command {
        let mut inventory = store.read()?;
        inventory.records.sort_by(|a, b| {
            a.tool
                .cmp(&b.tool)
                .then_with(|| compare(&b.version, &a.version))
        });
        let mut count = 0;
        for r in inventory
            .records
            .iter()
            .filter(|r| filter.is_none_or(|t| t == r.tool))
        {
            let missing = if store.home(r)?.join(r.tool.primary()).is_file() {
                ""
            } else {
                " [missing executable; repair/uninstall]"
            };
            writeln!(
                output,
                "{}@{}{} [{}]{}",
                r.tool.name(),
                r.version,
                if inventory.active.get(&r.tool) == Some(&r.version) {
                    " (active)"
                } else {
                    ""
                },
                r.tool.provider(),
                missing
            )
            .map_err(io_error)?;
            count += 1;
        }
        if count == 0 {
            writeln!(output, "No managed installations found.").map_err(io_error)?;
        }
        return Ok(());
    }
    let ui = ui::Ui::new(format);
    if !ui.interactive
        && !args.yes
        && matches!(
            args.command,
            Command::Install(_) | Command::Upgrade(_) | Command::Uninstall(_) | Command::Recover
        )
    {
        return Err(error(
            "noninteractive changes require --yes; install/upgrade also require tool@version or tool@latest",
            2,
        ));
    }
    let provider = Native::new(state.cwd().to_path_buf())?;
    let mut manager = Manager::new(store, provider);
    let result = dispatch(args, &mut manager, &ui, output);
    // Even an interrupted uninstall may have deliberately deactivated a version.
    let environment = manager
        .installed()
        .and_then(|inventory| environment::Environment::from_inventory(&manager.store, &inventory));
    if let Ok(environment) = environment {
        state.managed = environment;
    }
    result
}

fn discover<P: Provider>(
    manager: &mut Manager<P>,
    tool: Tool,
    requested: Option<&str>,
    ui: &ui::Ui,
) -> Result<Vec<Release>> {
    loop {
        let result = {
            let _progress =
                ui::Progress::start(ui, &format!("Discovering {} versions...", tool.label()));
            manager.provider.versions(tool, requested, &manager.context)
        };
        match result {
            Ok(releases) => return Ok(normalize(releases)),
            Err(e) if ui.interactive => {
                eprintln!("{e}");
                if ui.choose(
                    "Version discovery failed",
                    &["Retry".into(), "Cancel".into()],
                    0,
                )? != Some(0)
                {
                    return Err(error("version discovery cancelled", 130));
                }
            }
            Err(e) => return Err(e),
        }
    }
}
fn choose_release<P: Provider>(
    manager: &mut Manager<P>,
    target: &Target,
    upgrade: bool,
    ui: &ui::Ui,
) -> Result<Option<Release>> {
    let tool = target.tool;
    if !ui.interactive && target.version.is_none() {
        return Err(error(
            "noninteractive install/upgrade requires tool@version or tool@latest, plus --yes",
            2,
        ));
    }
    if upgrade && !tool.switchable() {
        return Err(error(
            "WinGet in-place upgrades are not managed reliably yet; uninstall the KPM-managed version, then install the new one",
            1,
        ));
    }
    let existing = manager.records(tool)?;
    let baseline = if upgrade {
        Some(
            existing
                .first()
                .ok_or_else(|| error("no managed version to upgrade; use kpm install", 1))?
                .version
                .clone(),
        )
    } else {
        None
    };
    let mut releases = discover(manager, tool, target.version.as_deref(), ui)?;
    if let Some(base) = &baseline {
        releases.retain(|r| compare(&r.version, base).is_gt());
    }
    if let Some(version) = &target.version {
        return catalog::select(releases, version, tool).map(Some);
    }
    if releases.is_empty() {
        return Err(error("no newer stable release is available", 1));
    }
    let inventory = manager.installed()?;
    let label = |r: &Release| {
        format!(
            "{}{}{}",
            r.label(),
            if inventory.record(tool, &r.version).is_some() {
                " [installed]"
            } else {
                ""
            },
            if inventory.active.get(&tool) == Some(&r.version) {
                " [active]"
            } else {
                ""
            }
        )
    };
    let mut choices = vec![format!("Latest Stable - {}", label(&releases[0]))];
    choices.extend(releases.iter().take(5).map(label));
    choices.push("Other version...".into());
    let Some(selected) = ui.choose(
        &format!("KPM - select {} version", tool.label()),
        &choices,
        0,
    )?
    else {
        return Ok(None);
    };
    if selected == choices.len() - 1 {
        loop {
            let Some(version) = ui.manual()? else {
                return Ok(None);
            };
            if !catalog::stable(&version) {
                eprintln!("Use a stable version from this provider.");
                continue;
            }
            match providers::resolve(&mut manager.provider, tool, &version, &manager.context) {
                Ok(r)
                    if baseline
                        .as_ref()
                        .is_none_or(|base| compare(&r.version, base).is_gt()) =>
                {
                    return Ok(Some(r));
                }
                Ok(_) => eprintln!("Choose a newer release."),
                Err(e) => eprintln!("{e}"),
            }
        }
    }
    Ok(Some(
        releases[if selected == 0 { 0 } else { selected - 1 }].clone(),
    ))
}
fn dispatch<P: Provider>(
    args: args::Args,
    manager: &mut Manager<P>,
    ui: &ui::Ui,
    output: &mut dyn Write,
) -> Result<()> {
    match args.command {
        Command::Versions(tool) => {
            let releases = discover(manager, tool, None, ui)?;
            let inventory = manager.installed()?;
            for (i, r) in releases.iter().take(5).enumerate() {
                writeln!(
                    output,
                    "{}@{}{}{}{}",
                    tool.name(),
                    r.version,
                    if i == 0 { " (Latest Stable)" } else { "" },
                    if inventory.record(tool, &r.version).is_some() {
                        " [installed]"
                    } else {
                        ""
                    },
                    if inventory.active.get(&tool) == Some(&r.version) {
                        " [active]"
                    } else {
                        ""
                    }
                )
                .map_err(io_error)?;
            }
        }
        Command::Current(tool) => {
            let inventory = manager.installed()?;
            if let Some(version) = inventory.active.get(&tool) {
                let record = inventory
                    .record(tool, version)
                    .ok_or_else(|| error("active version is not recorded", 1))?;
                manager.provider.verify(
                    tool,
                    version,
                    &manager.home(record)?,
                    &manager.store,
                    &manager.context,
                )?;
                writeln!(output, "{}@{} (active, verified)", tool.name(), version)
                    .map_err(io_error)?;
            } else {
                writeln!(
                    output,
                    "{}: no active managed version{}",
                    tool.name(),
                    if tool.switchable() {
                        ""
                    } else {
                        "; this WinGet provider does not support switching"
                    }
                )
                .map_err(io_error)?;
            }
        }
        Command::Use(target) => {
            if !target.tool.switchable() {
                return Err(error("kpm use is unsupported for this WinGet provider", 1));
            }
            let records = manager.records(target.tool)?;
            let record = if let Some(version) = target.version {
                if version == "latest" {
                    return Err(error(
                        "use requires an installed exact version, not @latest",
                        2,
                    ));
                }
                manager
                    .installed()?
                    .record(target.tool, &version)
                    .cloned()
                    .ok_or_else(|| {
                        error(
                            "version is not installed; kpm use never downloads software",
                            1,
                        )
                    })?
            } else {
                if records.is_empty() {
                    return Err(error("no installed managed versions to select", 1));
                }
                let active = manager.installed()?.active.get(&target.tool).cloned();
                let choices: Vec<_> = records
                    .iter()
                    .map(|r| {
                        format!(
                            "{}{}",
                            r.version,
                            if active.as_ref() == Some(&r.version) {
                                " [active]"
                            } else {
                                ""
                            }
                        )
                    })
                    .collect();
                let selected = records
                    .iter()
                    .position(|r| active.as_ref() == Some(&r.version))
                    .unwrap_or(0);
                let Some(index) =
                    ui.choose("Select installed managed version", &choices, selected)?
                else {
                    return Ok(());
                };
                records[index].clone()
            };
            manager.activate(&record)?;
            writeln!(
                output,
                "Active {} version: {} (verified, saved)",
                record.tool.label(),
                record.version
            )
            .map_err(io_error)?;
        }
        Command::Install(target) => {
            let mut chosen = target;
            loop {
                let selected_tool_menu = chosen.is_none();
                let target = if let Some(target) = chosen.clone() {
                    target
                } else {
                    let labels: Vec<_> = Tool::ALL.iter().map(|t| t.label().into()).collect();
                    let Some(index) = ui.choose(
                        "K Shell Package Manager - select one developer tool",
                        &labels,
                        0,
                    )?
                    else {
                        return Ok(());
                    };
                    Target {
                        tool: Tool::ALL[index],
                        version: None,
                    }
                };
                let release = choose_release(manager, &target, false, ui)?;
                if let Some(release) = release {
                    install(manager, &release, ui, args.yes, output)?;
                    break;
                }
                if !selected_tool_menu {
                    break;
                }
                chosen = None;
            }
        }
        Command::Upgrade(target) => {
            if let Some(release) = choose_release(manager, &target, true, ui)? {
                install(manager, &release, ui, args.yes, output)?;
            }
        }
        Command::Uninstall(target) => {
            let version = target
                .version
                .ok_or_else(|| error("uninstall requires a version", 2))?;
            let inventory = manager.installed()?;
            let record = inventory
                .record(target.tool, &version)
                .cloned()
                .ok_or_else(|| error("only recorded managed versions can be uninstalled", 1))?;
            let active = inventory.active.get(&record.tool) == Some(&record.version);
            eprintln!(
                "Remove {}@{} from {}{}",
                record.tool.name(),
                record.version,
                manager.home(&record)?.display(),
                if active {
                    "; this also deactivates it"
                } else {
                    ""
                }
            );
            if !ui.confirm(
                "Confirm managed uninstall and any required deactivation?",
                args.yes,
            )? {
                return Ok(());
            }
            {
                let _progress = ui::Progress::start(ui, "Uninstalling managed version...");
                manager.uninstall(&record)?;
            }
            writeln!(output, "Removed {}@{}", record.tool.name(), record.version)
                .map_err(io_error)?;
        }
        Command::Recover => {
            if ui.confirm(
                "Verify interrupted installation / clean incomplete owned files?",
                args.yes,
            )? {
                let result = {
                    let _progress = ui::Progress::start(ui, "Recovering KPM installation...");
                    manager.recover()?
                };
                writeln!(output, "{result}").map_err(io_error)?;
            }
        }
        _ => return Err(error("invalid dispatch", 2)),
    }
    Ok(())
}
fn install<P: Provider>(
    manager: &mut Manager<P>,
    release: &Release,
    ui: &ui::Ui,
    yes: bool,
    output: &mut dyn Write,
) -> Result<()> {
    eprintln!(
        "Tool: {}\nVersion: {}\nProvider: {}\nDestination: {}",
        release.tool.label(),
        release.version,
        release.tool.provider(),
        manager
            .store
            .destination(release.tool, &release.version)?
            .display()
    );
    if matches!(release.tool, Tool::Python | Tool::Git) {
        eprintln!(
            "WinGet user-scope installation; package/source agreements will be accepted after confirmation. Switching is unsupported."
        );
    }
    if let Ok(path) = crate::platform::resolve_executable(
        release.tool.primary().rsplit('/').next().unwrap_or(""),
        &std::env::current_dir().map_err(io_error)?,
    ) {
        eprintln!(
            "Existing executable on PATH: {}. Managed activation takes precedence only inside K Shell.",
            path.display()
        );
    }
    if !ui.confirm("Install this verified provider release?", yes)? {
        return Ok(());
    }
    let (record, created) = {
        let _progress = ui::Progress::start(ui, &format!("Installing {}...", release.tool.label()));
        manager.install(release)?
    };
    writeln!(
        output,
        "{} {}@{} (installation verified)",
        if created {
            "Installed"
        } else {
            "Already installed"
        },
        record.tool.name(),
        record.version
    )
    .map_err(io_error)?;
    if record.tool.switchable() {
        let active = manager.installed()?.active.get(&record.tool).cloned();
        if active.is_none()
            || (active.as_deref() != Some(&record.version)
                && ui.interactive
                && !yes
                && ui.confirm("Activate the newly installed version?", false)?)
        {
            manager.activate(&record)?;
            writeln!(
                output,
                "Active version: {} (verified, saved)",
                record.version
            )
            .map_err(io_error)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

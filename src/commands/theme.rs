use std::io::Write;

use crate::{
    error::ShellError,
    presentation::{Appearance, IconMode, Palette, SuggestionStyle},
    runtime::{ShellAction, ShellState},
    themes::{PromptTheme, ThemeStore},
};

const USAGE: &str = "usage: theme                        open the picker (Up/Down, Enter, Esc)\n       theme list                   list optional prompt designs\n       theme install <name>         install a bundled design, offline\n       theme <name>                 save an installed design as your prompt\n       theme default                restore and save the default prompt\n       theme ocean|dusk|midnight|reset\n       theme icons unicode|nerd|off\n       theme suggestions soft|bold|underline\nDesigns: powerline, atomic, bubblesline, cert, negligible.\nEnter in the picker installs a design if needed and saves your choice.\nStandard Unicode icons need no special font; nerd requires a Nerd Font in your terminal.\nPalette/icon/suggestion changes last for this session. Startup defaults: KSHELL_THEME, KSHELL_ICONS, KSHELL_SUGGESTION_STYLE (legacy MYSH_* names also work).";

pub(crate) fn valid_syntax(args: &[String]) -> bool {
    match args {
        [] => true,
        [arg] => {
            matches!(arg.as_str(), "list" | "status" | "reset" | "-h" | "--help")
                || Palette::parse(arg).is_some()
                || PromptTheme::parse(arg).is_some()
        }
        [kind, value] => match kind.as_str() {
            "install" => PromptTheme::parse(value).is_some(),
            "icons" => IconMode::parse(value).is_some(),
            "suggestions" => SuggestionStyle::parse(value).is_some(),
            _ => false,
        },
        _ => false,
    }
}

pub(super) fn run(
    args: &[String],
    state: &mut ShellState,
    output: &mut dyn Write,
    interactive: bool,
) -> Result<ShellAction, ShellError> {
    if args.is_empty() && interactive {
        return Ok(ShellAction::ChooseTheme);
    }
    if matches!(args, [arg] if arg == "list") {
        for theme in PromptTheme::ALL {
            writeln!(output, "{:<12} {}", theme.name(), theme.description()).map_err(io_error)?;
        }
        writeln!(
            output,
            "Use theme install <name>, then theme <name>; or type theme in an interactive session."
        )
        .map_err(io_error)?;
        return Ok(ShellAction::Continue);
    }
    if let [kind, name] = args
        && kind == "install"
    {
        let theme = PromptTheme::parse(name).ok_or_else(|| ShellError::new(USAGE, 2))?;
        store()?.install(theme).map_err(io_error)?;
        writeln!(
            output,
            "Installed {}. Activate with: theme {}",
            theme.name(),
            theme.name()
        )
        .map_err(io_error)?;
        return Ok(ShellAction::Continue);
    }
    if let [name] = args
        && let Some(theme) = PromptTheme::parse(name)
    {
        store()?.select(theme).map_err(io_error)?;
        state.appearance.prompt = theme;
        writeln!(output, "Prompt: {} (saved)", theme.name()).map_err(io_error)?;
        return Ok(ShellAction::Continue);
    }
    let mut appearance = state.appearance();
    match args {
        [] => {}
        [arg] if arg == "status" => {}
        [arg] if matches!(arg.as_str(), "-h" | "--help") => {
            return writeln!(output, "{USAGE}")
                .map(|()| ShellAction::Continue)
                .map_err(io_error);
        }
        [arg] if arg == "reset" => appearance = Appearance::default(),
        [arg] => {
            appearance.palette = Palette::parse(arg).ok_or_else(|| ShellError::new(USAGE, 2))?;
        }
        [arg, mode] if arg == "icons" => {
            appearance.icons = IconMode::parse(mode).ok_or_else(|| ShellError::new(USAGE, 2))?;
        }
        [arg, mode] if arg == "suggestions" => {
            appearance.suggestions =
                SuggestionStyle::parse(mode).ok_or_else(|| ShellError::new(USAGE, 2))?;
        }
        _ => return Err(ShellError::new(USAGE, 2)),
    }
    if matches!(args, [arg] if arg == "status") {
        writeln!(output, "Prompt: {}", appearance.prompt.name()).map_err(io_error)?;
    }
    writeln!(
        output,
        "Theme: {}\nIcons: {}\nSuggestions: {}",
        appearance.palette.name(),
        appearance.icons.name(),
        appearance.suggestions.name()
    )
    .map_err(|e| ShellError::new(format!("theme: {e}"), 1))?;
    state.appearance = appearance;
    Ok(ShellAction::Continue)
}

fn io_error(error: std::io::Error) -> ShellError {
    ShellError::new(format!("theme: {error}"), 1)
}

fn store() -> Result<ThemeStore, ShellError> {
    ThemeStore::from_environment().ok_or_else(|| {
        ShellError::new(
            "theme: configuration directory unavailable (set KSHELL_CONFIG_DIR)",
            1,
        )
    })
}

use std::io::Write;

use crate::{
    error::ShellError,
    presentation::{Appearance, IconMode, Palette, SuggestionStyle},
    runtime::ShellState,
};

const USAGE: &str = "usage: theme [ocean|dusk|midnight|reset]\n       theme icons unicode|nerd|off\n       theme suggestions soft|bold|underline\nStandard Unicode icons need no special font; nerd requires a Nerd Font in your terminal.\nChanges last for this session. Startup defaults: KSHELL_THEME, KSHELL_ICONS, KSHELL_SUGGESTION_STYLE (legacy MYSH_* names also work).";

pub(super) fn run(
    args: &[String],
    state: &mut ShellState,
    output: &mut dyn Write,
) -> Result<(), ShellError> {
    let mut appearance = state.appearance();
    match args {
        [] => {}
        [arg] if matches!(arg.as_str(), "-h" | "--help") => {
            return writeln!(output, "{USAGE}")
                .map_err(|e| ShellError::new(format!("theme: {e}"), 1));
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
    writeln!(
        output,
        "Theme: {}\nIcons: {}\nSuggestions: {}",
        appearance.palette.name(),
        appearance.icons.name(),
        appearance.suggestions.name()
    )
    .map_err(|e| ShellError::new(format!("theme: {e}"), 1))?;
    state.appearance = appearance;
    Ok(())
}

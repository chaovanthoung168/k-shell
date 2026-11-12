use std::{borrow::Cow, path::Path};

use super::validation::{self, Feedback, Verdict};
use crate::{
    commands::COMMAND_NAMES,
    platform,
    presentation::{Appearance, Style},
};

/// Command names use the same snapshot as completion; do not scan PATH or run
/// discovery helpers per keystroke. Explicit paths use the native resolver.
fn available(name: &str, cwd: &Path, native_commands: &[String]) -> bool {
    if COMMAND_NAMES.contains(&name) {
        return true;
    }
    let path = Path::new(name);
    if path.is_absolute() || path.components().count() > 1 {
        return platform::resolve_executable(name, cwd).is_ok();
    }
    native_commands.iter().any(|command| {
        if cfg!(windows) {
            command.eq_ignore_ascii_case(name)
        } else {
            command == name
        }
    })
}

/// Context captured by Hinter, because highlight_hint receives only the suffix.
/// Keep source byte offsets so quoted/Unicode command completions stay intact.
pub(super) struct Suggestion {
    suffix: String,
    style: Style,
}

impl Suggestion {
    pub(super) fn new(
        line: &str,
        suffix: &str,
        cwd: &Path,
        native_commands: &[String],
        feedback: &Feedback,
    ) -> Self {
        let preview = format!("{line}{suffix}");
        let style = match validation::check(&preview, cwd, feedback, |name| {
            available(name, cwd, native_commands)
        }) {
            Verdict::Valid => Style::Success,
            Verdict::Invalid => Style::Error,
            Verdict::Unknown => Style::Muted,
        };
        Self {
            suffix: suffix.to_owned(),
            style,
        }
    }
}

pub(super) fn hint<'h>(
    hint: &'h str,
    suggestion: Option<&Suggestion>,
    appearance: Appearance,
    color: bool,
) -> Cow<'h, str> {
    if !color {
        return Cow::Borrowed(hint);
    }
    let Some(suggestion) = suggestion.filter(|suggestion| suggestion.suffix == hint) else {
        return Cow::Owned(appearance.paint_suggestion(Style::Muted, hint, true));
    };
    Cow::Owned(appearance.paint_suggestion(suggestion.style, hint, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_native_names_and_adapters_are_recognized_without_spawning_or_path_scans() {
        let commands: Vec<String> = ["kshell-native-probe", "npm", "npx", "code", "ssh"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        for name in &commands {
            assert!(available(name, Path::new("."), &commands));
        }
        assert!(!available("np", Path::new("."), &commands));
        if cfg!(windows) {
            assert!(available("NPM", Path::new("."), &commands));
            assert!(!available("LS", Path::new("."), &commands));
        }
    }

    #[test]
    fn command_colors_follow_the_selected_palette() {
        use crate::presentation::Palette;
        let appearance = Appearance {
            palette: Palette::Dusk,
            ..Default::default()
        };
        assert_eq!(
            hint(
                "ho",
                Some(&Suggestion::new(
                    "ec",
                    "ho",
                    Path::new("."),
                    &[],
                    &Feedback::default()
                )),
                appearance,
                true
            ),
            appearance.paint_suggestion(Style::Success, "ho", true)
        );
        assert_eq!(
            hint(
                "-unknown",
                Some(&Suggestion::new(
                    "kshell",
                    "-unknown",
                    Path::new("."),
                    &[],
                    &Feedback::default()
                )),
                appearance,
                true
            ),
            appearance.paint_suggestion(Style::Error, "-unknown", true)
        );
    }
}

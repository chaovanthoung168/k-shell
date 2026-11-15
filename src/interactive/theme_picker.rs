use crate::{
    platform::menu::{self, MenuKey},
    presentation::Appearance,
    themes::{PromptTheme, ThemeStore},
};
use std::{
    io::{self, Write},
    path::Path,
};

/// Returns a selected theme only after Enter; cancellation writes nothing to
/// the store. Missing designs are installed from trusted built-in data offline.
pub fn choose(
    output: &mut dyn Write,
    appearance: Appearance,
    color: bool,
    width: usize,
) -> io::Result<Option<PromptTheme>> {
    let store = ThemeStore::from_environment()
        .ok_or_else(|| io::Error::other("configuration directory unavailable"))?;
    let mut selected = PromptTheme::ALL
        .iter()
        .position(|theme| *theme == appearance.prompt)
        .unwrap_or(0);
    loop {
        draw(output, &store, selected, appearance, color, width)?;
        output.flush()?;
        match menu::read_key()? {
            MenuKey::Cancel => {
                writeln!(output, "Theme selection cancelled.")?;
                return Ok(None);
            }
            MenuKey::Accept => {
                let theme = PromptTheme::ALL[selected];
                store.install(theme)?;
                store.select(theme)?;
                writeln!(output, "Prompt: {} (saved)", theme.name())?;
                return Ok(Some(theme));
            }
            MenuKey::Up => selected = step(selected, false),
            MenuKey::Down => selected = step(selected, true),
        }
        // Exactly ten rows, each clipped to avoid wrapping. No colors are
        // required for cursor control, so NO_COLOR still supports selection.
        write!(output, "\x1b[10A\r")?;
    }
}

pub fn step(selected: usize, down: bool) -> usize {
    if down {
        (selected + 1) % PromptTheme::ALL.len()
    } else {
        (selected + PromptTheme::ALL.len() - 1) % PromptTheme::ALL.len()
    }
}

fn draw(
    output: &mut dyn Write,
    store: &ThemeStore,
    selected: usize,
    appearance: Appearance,
    color: bool,
    width: usize,
) -> io::Result<()> {
    fn row(output: &mut dyn Write, text: &str, width: usize) -> io::Result<()> {
        // Menu labels use ASCII. Leave the last terminal cell empty to avoid
        // automatic line wrapping in the classic console host.
        writeln!(
            output,
            "\r\x1b[2K{}",
            text.chars()
                .take(width.saturating_sub(1))
                .collect::<String>()
        )
    }
    row(
        output,
        "K Shell themes | Up/Down select | Enter install & use | Esc cancel",
        width,
    )?;
    for (index, theme) in PromptTheme::ALL.iter().enumerate() {
        let installed = if store.is_installed(*theme) {
            "installed"
        } else {
            "install on Enter"
        };
        row(
            output,
            &format!(
                "{} {:<12} {}",
                if index == selected { ">" } else { " " },
                theme.name(),
                installed
            ),
            width,
        )?;
    }
    let theme = PromptTheme::ALL[selected];
    row(output, theme.description(), width)?;
    // A fixed short preview avoids wrapping. ANSI sequences do not count
    // toward cell width, so choose a compact label for narrow windows.
    let preview = super::prompt::render_design(
        Path::new("~/dev"),
        "you",
        Some("main"),
        0,
        color,
        Appearance {
            prompt: theme,
            ..appearance
        },
    );
    if width >= 65 {
        writeln!(
            output,
            "\r\x1b[2K{}",
            preview.1.lines().next().unwrap_or("")
        )?;
    } else {
        row(output, &format!("Preview: {}", theme.name()), width)?;
    }
    row(
        output,
        "Default is always available. Other designs are optional.",
        width,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrows_wrap_at_both_ends() {
        assert_eq!(step(0, false), 5);
        assert_eq!(step(5, true), 0);
        assert_eq!(step(step(3, true), false), 3);
    }
}

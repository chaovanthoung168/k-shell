use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Palette {
    #[default]
    Ocean,
    Dusk,
    Midnight,
}

impl Palette {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ocean" => Some(Self::Ocean),
            "dusk" => Some(Self::Dusk),
            "midnight" => Some(Self::Midnight),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Ocean => "ocean",
            Self::Dusk => "dusk",
            Self::Midnight => "midnight",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IconMode {
    #[default]
    Unicode,
    Nerd,
    Off,
}

impl IconMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "unicode" => Some(Self::Unicode),
            "nerd" => Some(Self::Nerd),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Nerd => "nerd",
            Self::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SuggestionStyle {
    #[default]
    Soft,
    Bold,
    Underline,
}

impl SuggestionStyle {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "soft" => Some(Self::Soft),
            "bold" => Some(Self::Bold),
            "underline" => Some(Self::Underline),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Bold => "bold",
            Self::Underline => "underline",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Appearance {
    pub palette: Palette,
    pub icons: IconMode,
    pub suggestions: SuggestionStyle,
}

impl Appearance {
    pub fn from_environment() -> Self {
        let theme = crate::startup_setting("THEME").and_then(|value| value.into_string().ok());
        let icons = crate::startup_setting("ICONS").and_then(|value| value.into_string().ok());
        let mut appearance = Self::from_values(theme.as_deref(), icons.as_deref());
        appearance.suggestions = crate::startup_setting("SUGGESTION_STYLE")
            .and_then(|value| value.into_string().ok())
            .as_deref()
            .and_then(SuggestionStyle::parse)
            .unwrap_or_default();
        appearance
    }

    pub fn from_values(palette: Option<&str>, icons: Option<&str>) -> Self {
        Self {
            palette: palette.and_then(Palette::parse).unwrap_or_default(),
            icons: icons.and_then(IconMode::parse).unwrap_or_default(),
            ..Default::default()
        }
    }

    /// A quieter preview than accepted input; status semantics stay the same.
    /// Explicit RGB avoids host-dependent ANSI faint and does not add glyphs.
    pub fn paint_suggestion(self, status: Style, text: &str, color: bool) -> String {
        if !color {
            return text.to_owned();
        }
        if self.suggestions == SuggestionStyle::Bold {
            return status.paint_in(text, true, self.palette);
        }
        let rgb = match (self.palette, status) {
            (Palette::Ocean, Style::Success) => "125;182;155",
            (Palette::Ocean, Style::Error) => "207;137;147",
            (Palette::Ocean, _) => "140;148;162",
            (Palette::Dusk, Style::Success) => "150;187;164",
            (Palette::Dusk, Style::Error) => "208;148;160",
            (Palette::Dusk, _) => "146;137;158",
            (Palette::Midnight, Style::Success) => "132;180;157",
            (Palette::Midnight, Style::Error) => "204;135;145",
            (Palette::Midnight, _) => "139;153;174",
        };
        let emphasis = if self.suggestions == SuggestionStyle::Underline {
            "4;22"
        } else {
            "22"
        };
        if self.suggestions == SuggestionStyle::Underline {
            let visible = text.trim_end_matches(char::is_whitespace);
            // Completion may include a trailing space. Keep it insertable, but
            // do not draw an extra underline beyond the suggested words.
            return format!(
                "\x1b[{emphasis};38;2;{rgb}m{visible}\x1b[0m{}",
                &text[visible.len()..]
            );
        }
        format!("\x1b[{emphasis};38;2;{rgb}m{text}\x1b[0m")
    }

    pub fn decorate(self, icon: Icon, label: &str, terminal: bool) -> String {
        match icon.glyph(self.icons) {
            Some(glyph) if terminal => format!("{glyph} {label}"),
            _ => label.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Folder,
    File,
    Link,
    Executable,
    Audio,
    Video,
    Image,
    Archive,
    Code,
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Config,
    Markdown,
    Git,
}

impl Icon {
    pub fn file(path: &Path, style: Style) -> Self {
        match style {
            Style::Directory => return Self::Folder,
            Style::Link => return Self::Link,
            _ => {}
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if name.starts_with(".git") {
            return Self::Git;
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "rs" => Self::Rust,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "ts" | "tsx" => Self::TypeScript,
            "py" | "pyw" => Self::Python,
            "md" | "mdx" => Self::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "heic" | "tif"
            | "tiff" => Self::Image,
            _ => match style {
                Style::Executable => Self::Executable,
                Style::Audio => Self::Audio,
                Style::Media => Self::Video,
                Style::Archive => Self::Archive,
                Style::Code => Self::Code,
                Style::Config => Self::Config,
                _ => Self::File,
            },
        }
    }

    pub fn glyph(self, mode: IconMode) -> Option<&'static str> {
        Some(match mode {
            IconMode::Off => return None,
            IconMode::Unicode => match self {
                Self::Folder => "📁",
                Self::File => "📄",
                Self::Link => "↗",
                Self::Executable => "▶",
                Self::Audio => "♪",
                Self::Video => "▷",
                Self::Image => "▧",
                Self::Archive => "▤",
                Self::Config => "⚙\u{fe0e}",
                Self::Markdown => "≡",
                Self::Git => "⑂",
                Self::Code | Self::Rust | Self::JavaScript | Self::TypeScript | Self::Python => {
                    "‹›"
                }
            },
            IconMode::Nerd => match self {
                Self::Folder => "\u{f07b}",
                Self::File => "\u{f15b}",
                Self::Link => "\u{f0c1}",
                Self::Executable => "\u{f489}",
                Self::Audio => "\u{f001}",
                Self::Video => "\u{f03d}",
                Self::Image => "\u{f03e}",
                Self::Archive => "\u{f1c6}",
                Self::Code => "\u{f121}",
                Self::Rust => "\u{e7a8}",
                Self::JavaScript => "\u{e74e}",
                Self::TypeScript => "\u{e628}",
                Self::Python => "\u{e73c}",
                Self::Config => "\u{f013}",
                Self::Markdown => "\u{e73e}",
                Self::Git => "\u{e702}",
            },
        })
    }
}

/// One palette for prompts, suggestions, ls, and tree. These are foreground
/// colors only; the terminal host retains control of its font and background.
#[derive(Debug, Clone, Copy)]
pub enum Style {
    Plain,
    Directory,
    Link,
    Executable,
    Audio,
    Media,
    Archive,
    Muted,
    Success,
    Error,
    Code,
    Config,
}

impl Style {
    fn sgr(self, palette: Palette) -> &'static str {
        if palette == Palette::Midnight {
            return match self {
                Self::Plain => "",
                Self::Directory => "38;2;121;185;204",
                Self::Executable | Self::Success | Self::Link | Self::Audio => "38;2;139;203;178",
                Self::Code | Self::Media => "38;2;182;174;219",
                Self::Config | Self::Archive => "38;2;217;186;131",
                Self::Muted => "38;2;139;153;174",
                Self::Error => "38;2;224;138;144",
            };
        }
        if palette == Palette::Dusk {
            return match self {
                Self::Plain => "",
                Self::Directory => "1;38;2;199;174;242",
                Self::Executable | Self::Success => "1;38;2;166;215;185",
                Self::Link | Self::Audio => "38;2;166;215;185",
                Self::Media => "38;2;231;166;197",
                Self::Code => "38;2;199;174;242",
                Self::Config | Self::Archive => "38;2;228;192;130",
                Self::Muted => "38;2;143;134;158",
                Self::Error => "1;38;2;240;143;147",
            };
        }
        match self {
            Self::Plain => "",
            Self::Directory => "1;38;2;129;184;234", // Frost blue #81B8EA
            Self::Executable | Self::Success => "1;38;2;129;199;180", // Seafoam #81C7B4
            Self::Link | Self::Audio => "38;2;129;199;180",
            Self::Code | Self::Media => "38;2;197;166;232", // Lavender #C5A6E8
            Self::Config | Self::Archive => "38;2;216;178;122", // Amber #D8B27A
            Self::Muted => "38;2;111;122;138",              // Slate #6F7A8A
            Self::Error => "1;38;2;226;141;147",            // Rose #E28D93
        }
    }

    pub fn file(path: &Path) -> Self {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if matches!(
            name,
            ".gitignore" | ".gitattributes" | ".editorconfig" | "Dockerfile" | "Makefile"
        ) {
            return Self::Config;
        }
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            return Self::Plain;
        };
        match extension.to_ascii_lowercase().as_str() {
            "exe" | "com" if cfg!(windows) => Self::Executable,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "opus" | "m4a" | "wma" => Self::Audio,
            "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "flv" | "mpeg" | "mpg" | "m4v"
            | "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "heic" | "tif"
            | "tiff" => Self::Media,
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" => Self::Archive,
            "rs" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "py" | "pyw" | "go" | "c"
            | "h" | "cpp" | "hpp" | "cs" | "java" | "kt" | "swift" | "html" | "css" | "scss"
            | "sql" => Self::Code,
            "json" | "toml" | "yaml" | "yml" | "ini" | "conf" | "xml" | "lock" => Self::Config,
            _ => Self::Plain,
        }
    }

    pub fn paint(self, text: &str, color: bool) -> String {
        self.paint_in(text, color, Palette::Ocean)
    }

    pub fn paint_in(self, text: &str, color: bool, palette: Palette) -> String {
        if color && !self.sgr(palette).is_empty() {
            format!("\x1b[{}m{text}\x1b[0m", self.sgr(palette))
        } else {
            text.to_owned()
        }
    }

    pub fn write(self, output: &mut dyn Write, text: &str, color: bool) -> io::Result<()> {
        self.write_in(output, text, color, Palette::Ocean)
    }

    pub fn write_in(
        self,
        output: &mut dyn Write,
        text: &str,
        color: bool,
        palette: Palette,
    ) -> io::Result<()> {
        if color && !self.sgr(palette).is_empty() {
            write!(output, "\x1b[{}m{text}\x1b[0m", self.sgr(palette))
        } else {
            write!(output, "{text}")
        }
    }
}

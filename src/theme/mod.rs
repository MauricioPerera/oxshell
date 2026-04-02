use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// Available themes
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    Dark,
    Light,
    Solarized,
    Monokai,
    Nord,
}

impl ThemeName {
    pub fn all() -> &'static [ThemeName] {
        &[
            ThemeName::Dark,
            ThemeName::Light,
            ThemeName::Solarized,
            ThemeName::Monokai,
            ThemeName::Nord,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Solarized => "solarized",
            Self::Monokai => "monokai",
            Self::Nord => "nord",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "solarized" => Some(Self::Solarized),
            "monokai" => Some(Self::Monokai),
            "nord" => Some(Self::Nord),
            _ => None,
        }
    }
}

/// Resolved color palette for a theme
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: ThemeName,
    /// Background color
    #[allow(dead_code)]
    pub bg: Color,
    /// Default text
    pub fg: Color,
    /// User message text
    pub user: Color,
    /// Assistant message text
    pub assistant: Color,
    /// System/info messages
    pub system: Color,
    /// Error messages
    pub error: Color,
    /// Tool output
    pub tool: Color,
    /// Status bar background
    pub status_bg: Color,
    /// Status bar text
    pub status_fg: Color,
    /// Input prompt
    pub prompt: Color,
    /// Border color
    pub border: Color,
    /// Accent/brand color
    pub accent: Color,
}

impl Theme {
    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Dark => Self::dark(),
            ThemeName::Light => Self::light(),
            ThemeName::Solarized => Self::solarized(),
            ThemeName::Monokai => Self::monokai(),
            ThemeName::Nord => Self::nord(),
        }
    }

    fn dark() -> Self {
        Self {
            name: ThemeName::Dark,
            bg: Color::Reset,
            fg: Color::White,
            user: Color::White,
            assistant: Color::Rgb(245, 158, 11), // Cloudflare orange
            system: Color::DarkGray,
            error: Color::Red,
            tool: Color::Cyan,
            status_bg: Color::Rgb(30, 30, 30),
            status_fg: Color::Gray,
            prompt: Color::White,
            border: Color::DarkGray,
            accent: Color::Rgb(245, 158, 11),
        }
    }

    fn light() -> Self {
        Self {
            name: ThemeName::Light,
            bg: Color::Reset,
            fg: Color::Black,
            user: Color::Black,
            assistant: Color::Rgb(180, 100, 0),
            system: Color::Gray,
            error: Color::Red,
            tool: Color::Blue,
            status_bg: Color::Rgb(230, 230, 230),
            status_fg: Color::Black,
            prompt: Color::Black,
            border: Color::Gray,
            accent: Color::Rgb(180, 100, 0),
        }
    }

    fn solarized() -> Self {
        Self {
            name: ThemeName::Solarized,
            bg: Color::Reset,
            fg: Color::Rgb(131, 148, 150),     // base0
            user: Color::Rgb(238, 232, 213),    // base2
            assistant: Color::Rgb(181, 137, 0), // yellow
            system: Color::Rgb(88, 110, 117),   // base01
            error: Color::Rgb(220, 50, 47),     // red
            tool: Color::Rgb(38, 139, 210),     // blue
            status_bg: Color::Rgb(0, 43, 54),   // base03
            status_fg: Color::Rgb(131, 148, 150),
            prompt: Color::Rgb(238, 232, 213),
            border: Color::Rgb(88, 110, 117),
            accent: Color::Rgb(181, 137, 0),
        }
    }

    fn monokai() -> Self {
        Self {
            name: ThemeName::Monokai,
            bg: Color::Reset,
            fg: Color::Rgb(248, 248, 242),
            user: Color::Rgb(248, 248, 242),
            assistant: Color::Rgb(166, 226, 46),  // green
            system: Color::Rgb(117, 113, 94),
            error: Color::Rgb(249, 38, 114),      // pink
            tool: Color::Rgb(102, 217, 239),       // cyan
            status_bg: Color::Rgb(39, 40, 34),
            status_fg: Color::Rgb(117, 113, 94),
            prompt: Color::Rgb(248, 248, 242),
            border: Color::Rgb(117, 113, 94),
            accent: Color::Rgb(166, 226, 46),
        }
    }

    fn nord() -> Self {
        Self {
            name: ThemeName::Nord,
            bg: Color::Reset,
            fg: Color::Rgb(216, 222, 233),     // nord4
            user: Color::Rgb(236, 239, 244),    // nord6
            assistant: Color::Rgb(136, 192, 208), // nord8
            system: Color::Rgb(76, 86, 106),    // nord3
            error: Color::Rgb(191, 97, 106),    // nord11
            tool: Color::Rgb(163, 190, 140),    // nord14
            status_bg: Color::Rgb(46, 52, 64),  // nord0
            status_fg: Color::Rgb(216, 222, 233),
            prompt: Color::Rgb(236, 239, 244),
            border: Color::Rgb(76, 86, 106),
            accent: Color::Rgb(136, 192, 208),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

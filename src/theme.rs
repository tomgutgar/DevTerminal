use std::sync::OnceLock;

use ratatui::style::Color;

/// Application color palette. Every module takes its accent from here
/// and states (running/error/warning) use the semantic colors, so changing
/// `theme` in config.toml re-colors the whole TUI.
pub struct Palette {
    pub red: Color,
    pub orange: Color,
    pub yellow: Color,
    pub green: Color,
    pub cyan: Color,
    pub teal: Color,
    pub blue: Color,
    pub blue2: Color,
    pub magenta: Color,
    pub sel_bg: Color, // selected row background
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const NORD: Palette = Palette {
    red: rgb(0xbf616a),
    orange: rgb(0xd08770),
    yellow: rgb(0xebcb8b),
    green: rgb(0xa3be8c),
    cyan: rgb(0x88c0d0),
    teal: rgb(0x8fbcbb),
    blue: rgb(0x81a1c1),
    blue2: rgb(0x5e81ac),
    magenta: rgb(0xb48ead),
    sel_bg: rgb(0x2d3748),
};

const CATPPUCCIN: Palette = Palette {
    red: rgb(0xf38ba8),
    orange: rgb(0xfab387),
    yellow: rgb(0xf9e2af),
    green: rgb(0xa6e3a1),
    cyan: rgb(0x89dceb),
    teal: rgb(0x94e2d5),
    blue: rgb(0x89b4fa),
    blue2: rgb(0x74c7ec),
    magenta: rgb(0xcba6f7),
    sel_bg: rgb(0x313244),
};

const GRUVBOX: Palette = Palette {
    red: rgb(0xfb4934),
    orange: rgb(0xfe8019),
    yellow: rgb(0xfabd2f),
    green: rgb(0xb8bb26),
    cyan: rgb(0x8ec07c),
    teal: rgb(0x689d6a),
    blue: rgb(0x83a598),
    blue2: rgb(0x458588),
    magenta: rgb(0xd3869b),
    sel_bg: rgb(0x3c3836),
};

static PALETTE: OnceLock<&'static Palette> = OnceLock::new();

/// Sets the palette from config.toml. Call once at startup.
pub fn init(name: &str) {
    let p = match name {
        "catppuccin" => &CATPPUCCIN,
        "gruvbox" => &GRUVBOX,
        _ => &NORD, // "nord", "dark" (legacy value) and unknown values
    };
    let _ = PALETTE.set(p);
}

/// Active palette (nord if `init` was never called, e.g. in tests).
pub fn p() -> &'static Palette {
    PALETTE.get_or_init(|| &NORD)
}

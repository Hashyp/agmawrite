//! The omarchy color scheme: the palette the whole interface paints with,
//! loaded from the current omarchy theme and reloaded when it changes.
//!
//! Omarchy stores the current theme name in
//! `~/.local/state/omarchy/current/theme` and its colors in
//! `~/.local/share/omarchy/themes/<name>/colors.toml` (user themes in
//! `~/.config/omarchy/themes/<name>/` win). The file is a small
//! `key = "#rrggbb"` table — parsed here by hand, with every missing key
//! falling back to the palette's default, so an absent or partial omarchy
//! installation keeps the interface exactly as it was.

use iced::Color;

/// The palette the interface paints with: surfaces, text, and the accent
/// colors. [`Palette::current`] reads it from omarchy; [`Palette::default`]
/// is the fallback look, matching the original hardcoded colors.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Whether the omarchy theme is a light one — the markdown renderer
    /// and iced's theme follow it.
    pub light: bool,

    pub background: Color,
    pub dark_background: Color,
    pub darker_background: Color,
    pub lighter_background: Color,

    pub foreground: Color,
    pub dark_foreground: Color,
    pub light_foreground: Color,

    pub muted: Color,
    pub selection: Color,
    pub accent: Color,

    pub red: Color,
    pub yellow: Color,
    pub orange: Color,
    pub green: Color,
    pub cyan: Color,
    pub blue: Color,
    pub magenta: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            light: false,
            background: Color::BLACK,
            dark_background: Color::from_rgb(0.03, 0.03, 0.03),
            darker_background: Color::from_rgb(0.08, 0.08, 0.08),
            lighter_background: Color::from_rgb(0.17, 0.17, 0.17),

            foreground: Color::WHITE,
            dark_foreground: Color::from_rgb(0.5, 0.5, 0.5),
            light_foreground: Color::from_rgb(0.7, 0.7, 0.7),

            muted: Color::from_rgb(0.3, 0.3, 0.3),
            selection: Color::from_rgb(0.25, 0.25, 0.25),
            accent: Color::from_rgb(0.3, 0.9, 0.8),

            red: Color::from_rgb(0.9, 0.45, 0.4),
            yellow: Color::from_rgb(0.9, 0.7, 0.35),
            orange: Color::from_rgb(0.95, 0.6, 0.35),
            green: Color::from_rgb(0.4, 0.85, 0.5),
            cyan: Color::from_rgb(0.3, 0.9, 0.8),
            blue: Color::from_rgb(0.4, 0.65, 1.0),
            magenta: Color::from_rgb(0.8, 0.5, 0.9),
        }
    }
}

impl Palette {
    /// The palette of the current omarchy theme, or the default when
    /// omarchy is absent or its files unreadable.
    ///
    /// Omarchy's current state is a directory: `theme.name` holds the
    /// theme's name and `theme/colors.toml` its colors (a copy of the
    /// theme directory's `colors.toml`). A bare `theme` *file* holding
    /// just the name — the older layout — still resolves through the user
    /// and system theme directories.
    pub fn current() -> Self {
        Self::from_state(home().join(".local/state/omarchy/current"))
    }

    /// Reads the current theme from the `current` state directory: the
    /// `theme/colors.toml` copy inside it, or the named theme's colors
    /// from the user or system directories.
    fn from_state(state: std::path::PathBuf) -> Self {
        // The modern layout: a `theme` directory with a colors copy.
        let theme_dir = state.join("theme");
        if let Ok(colors) = std::fs::read_to_string(theme_dir.join("colors.toml")) {
            return Self::from_colors(&colors);
        }

        // The older layout (or a theme directory without a colors copy):
        // the name resolves through the theme directories.
        let Ok(name) = std::fs::read_to_string(state.join("theme.name"))
            .or_else(|_| std::fs::read_to_string(&theme_dir))
        else {
            return Self::default();
        };

        let name = name.trim();

        if name.is_empty() {
            return Self::default();
        }

        let user = home().join(format!(".config/omarchy/themes/{name}/colors.toml"));
        let system = home().join(format!(".local/share/omarchy/themes/{name}/colors.toml"));
        let colors = std::fs::read_to_string(&user).or_else(|_| std::fs::read_to_string(&system));

        match colors {
            Ok(colors) => Self::from_colors(&colors),
            Err(_) => Self::default(),
        }
    }

    /// A palette from a `colors.toml` body: every known key overrides its
    /// default, unknown keys and malformed lines are ignored.
    pub fn from_colors(colors: &str) -> Self {
        let mut palette = Self::default();

        for line in colors.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "mode" => palette.light = value == "light",
                _ => {
                    if let Some(color) = hex(value) {
                        match key {
                            "background" => palette.background = color,
                            "dark_background" => palette.dark_background = color,
                            "darker_background" => palette.darker_background = color,
                            "lighter_background" => palette.lighter_background = color,
                            "foreground" => palette.foreground = color,
                            "bright_foreground" => palette.foreground = color,
                            "dark_foreground" => palette.dark_foreground = color,
                            "light_foreground" => palette.light_foreground = color,
                            "muted" => palette.muted = color,
                            "selection" => palette.selection = color,
                            "accent" => palette.accent = color,
                            "red" | "bright_red" => palette.red = color,
                            "yellow" | "bright_yellow" => palette.yellow = color,
                            "orange" => palette.orange = color,
                            "green" | "bright_green" => palette.green = color,
                            "cyan" | "bright_cyan" => palette.cyan = color,
                            "blue" | "bright_blue" => palette.blue = color,
                            "magenta" | "bright_magenta" => palette.magenta = color,
                            _ => {}
                        }
                    }
                }
            }
        }

        palette
    }

    /// A translucent variant of a color, like the tint an active comment
    /// paints.
    pub fn tint(&self, color: Color, alpha: f32) -> Color {
        Color { a: alpha, ..color }
    }

    /// `color` scaled toward black by `factor` (0.0 — unchanged, 1.0 —
    /// black), for dark button surfaces.
    pub fn darkened(color: Color, factor: f32) -> Color {
        Color::from_rgb(
            color.r * (1.0 - factor),
            color.g * (1.0 - factor),
            color.b * (1.0 - factor),
        )
    }

    /// `color` scaled toward white by `factor`, for hover surfaces.
    pub fn lightened(color: Color, factor: f32) -> Color {
        let mix = |channel: f32| channel + (1.0 - channel) * factor;
        Color::from_rgb(mix(color.r), mix(color.g), mix(color.b))
    }
}

fn home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// Parses `#rrggbb` (also without the dash) into a color; anything else is
/// `None`.
fn hex(value: &str) -> Option<Color> {
    let digits = value.strip_prefix('#').unwrap_or(value);

    if digits.len() != 6 {
        return None;
    }

    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(digits.get(range)?, 16)
            .ok()
            .map(f32::from)
    };

    Some(Color::from_rgb(
        channel(0..2)? / 255.0,
        channel(2..4)? / 255.0,
        channel(4..6)? / 255.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::{hex, Palette};
    use iced::Color;

    fn close(a: Color, b: Color) -> bool {
        (a.r - b.r).abs() < 1e-3
            && (a.g - b.g).abs() < 1e-3
            && (a.b - b.b).abs() < 1e-3
            && a.a == b.a
    }

    /// The omarchy colors.toml keys map onto the palette: every color key
    /// overrides its default, mode flips the light flag, and unknown keys
    /// and malformed lines are ignored.
    #[test]
    fn parses_omarchy_colors() {
        let matte_black = "\
mode = \"dark\"

accent = \"#e68e0d\"
selection = \"#2a2a2a\"
muted = \"#333333\"

background = \"#121212\"
dark_background = \"#0d0d0d\"
darker_background = \"#090909\"
lighter_background = \"#1e1e1e\"

foreground = \"#bebebe\"
dark_foreground = \"#555555\"
light_foreground = \"#8a8a8d\"

red = \"#D35F5F\"
green = \"#FFC107\"
blue = \"#e68e0d\"

not a color line
unknown_key = \"#000000\"
broken = \"nope\"
";

        let palette = Palette::from_colors(matte_black);
        assert!(!palette.light);
        assert!(close(
            palette.background,
            Color::from_rgb8(0x12, 0x12, 0x12)
        ));
        assert!(close(palette.accent, Color::from_rgb8(0xe6, 0x8e, 0x0d)));
        assert!(close(palette.red, Color::from_rgb8(0xd3, 0x5f, 0x5f)));
        assert!(close(palette.green, Color::from_rgb8(0xff, 0xc1, 0x07)));
        assert!(close(palette.blue, Color::from_rgb8(0xe6, 0x8e, 0x0d)));
        assert!(close(palette.muted, Color::from_rgb8(0x33, 0x33, 0x33)));
        // Keys not in the file keep their defaults.
        assert!(close(palette.cyan, Palette::default().cyan));
        assert!(close(palette.orange, Palette::default().orange));

        // A light theme flips the mode.
        let light = Palette::from_colors("mode = \"light\"\n");
        assert!(light.light);

        // An empty body is the default palette.
        let empty = Palette::from_colors("");
        assert!(close(empty.background, Palette::default().background));
    }

    /// Hex parsing accepts `#rrggbb` and rejects everything else.
    #[test]
    fn hex_colors_parse_strictly() {
        assert!(close(
            hex("#ffffff").unwrap(),
            Color::from_rgb(1.0, 1.0, 1.0)
        ));
        assert!(close(hex("000000").unwrap(), Color::BLACK));
        assert!(hex("#fff").is_none());
        assert!(hex("#gggggg").is_none());
        assert!(hex("").is_none());
        assert!(hex("12").is_none());
    }

    /// The lighten/darken helpers mix toward white and black.
    #[test]
    fn lighten_and_darken_mix_channels() {
        let color = Color::from_rgb(0.5, 0.5, 0.5);

        assert!(close(
            Palette::darkened(color, 0.5),
            Color::from_rgb(0.25, 0.25, 0.25)
        ));
        assert!(close(
            Palette::lightened(color, 0.5),
            Color::from_rgb(0.75, 0.75, 0.75)
        ));
        assert!(close(Palette::darkened(color, 0.0), color));
    }

    /// An absent or unreadable omarchy state falls back to the default
    /// palette instead of failing.
    #[test]
    fn missing_omarchy_state_yields_the_default() {
        let palette = Palette::from_state(std::path::PathBuf::from("/nonexistent/omarchy/state"));

        assert!(close(palette.background, Palette::default().background));
        assert!(close(palette.accent, Palette::default().accent));
    }

    /// The modern omarchy state layout resolves: `current/theme/` is a
    /// directory whose `colors.toml` copy paints the palette, with
    /// `current/theme.name` naming the theme. The older layout — a bare
    /// `theme` file holding the name — resolves through the user theme
    /// directory.
    #[test]
    fn reads_the_current_omarchy_state_layout() {
        let state =
            std::env::temp_dir().join(format!("agmawrite-theme-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);

        // Modern layout: theme directory with the colors copy.
        std::fs::create_dir_all(state.join("theme")).unwrap();
        std::fs::write(state.join("theme.name"), "some-theme\n").unwrap();
        std::fs::write(
            state.join("theme/colors.toml"),
            "mode = \"dark\"\naccent = \"#123456\"\nbackground = \"#abcdef\"\n",
        )
        .unwrap();

        let palette = Palette::from_state(state.clone());
        assert!(!palette.light);
        assert!(close(palette.accent, Color::from_rgb8(0x12, 0x34, 0x56)));
        assert!(close(
            palette.background,
            Color::from_rgb8(0xab, 0xcd, 0xef)
        ));

        // The colors copy wins even when the named theme exists too.
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let user_theme = home.join(".config/omarchy/themes/some-theme");
        std::fs::create_dir_all(&user_theme).unwrap();
        std::fs::write(user_theme.join("colors.toml"), "accent = \"#ffffff\"\n").unwrap();
        let palette = Palette::from_state(state.clone());
        assert!(close(palette.accent, Color::from_rgb8(0x12, 0x34, 0x56)));

        // Older layout: a bare `theme` file with the name, no directory —
        // resolves through the user theme directory.
        std::fs::remove_dir_all(state.join("theme")).unwrap();
        std::fs::write(state.join("theme"), "some-theme\n").unwrap();
        let palette = Palette::from_state(state.clone());
        assert!(close(palette.accent, Color::from_rgb(1.0, 1.0, 1.0)));
        assert!(close(palette.background, Palette::default().background));

        std::fs::remove_dir_all(&state).unwrap();
        let _ = std::fs::remove_dir(&user_theme);
        let _ = std::fs::remove_dir(home.join(".config/omarchy/themes"));
    }
}

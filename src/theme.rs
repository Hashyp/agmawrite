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

use iced::futures::channel::mpsc::Sender;
use iced::{Color, Subscription};
use std::path::PathBuf;

/// The contrast a heading tint must hold against the background before it
/// may replace ordinary text.
const MIN_HEADING_CONTRAST: f32 = 4.5;

/// How finely the heading tint search backs off toward the foreground.
const CONTRAST_SEARCH_STEPS: u8 = 32;

/// A filesystem event indicating that the current palette may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Changed,
}

/// Watches Omarchy's current-theme state and reports changes that may require
/// reloading [`Palette::current`].
pub(crate) fn subscription() -> Subscription<Event> {
    let state = current_state();

    Subscription::run_with(("omarchy-palette", state), |(_, state)| {
        let state = state.clone();
        iced::stream::channel(1, move |sender| async move {
            spawn_watcher(state, sender);
            // Events arrive on the watcher thread; this runner only keeps the
            // stream alive.
            std::future::pending::<()>().await;
        })
    })
}

/// Spawns a watcher for the current-theme directory, forwarding one change
/// event per burst. A missing Omarchy installation simply yields no events.
fn spawn_watcher(state: PathBuf, mut sender: Sender<Event>) {
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};

        if !state.is_dir() {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(watcher) => watcher,
            Err(_) => return,
        };

        if watcher.watch(&state, RecursiveMode::NonRecursive).is_err() {
            return;
        }

        while let Ok(event) = rx.recv() {
            let Ok(event) = event else { continue };

            if !crate::watch::may_change_file(&event.kind) {
                continue;
            }

            // Theme switches commonly replace several state entries in one
            // burst; one reload observes the latest palette.
            while rx.try_recv().is_ok() {}

            if !forward_change(&mut sender) {
                break;
            }
        }
    });
}

/// Queues a palette reload. A full one-item channel means a reload is already
/// pending, not that the watcher has disconnected.
fn forward_change(sender: &mut Sender<Event>) -> bool {
    match sender.try_send(Event::Changed) {
        Ok(()) => true,
        Err(error) => error.is_full(),
    }
}

/// The palette the interface paints with: surfaces, text, and the accent
/// colors. [`Palette::current`] reads it from omarchy; [`Palette::default`]
/// is the fallback look, matching the original hardcoded colors.
#[derive(Debug, Clone, Copy, PartialEq)]
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
        Self::from_state(current_state())
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

    /// The raised surface color: one step lighter than the page on dark
    /// themes, one step darker on light ones — the surface code and
    /// tooltips paint on, never touching the page itself.
    pub fn raised(&self) -> Color {
        if self.light {
            self.darker_background
        } else {
            self.lighter_background
        }
    }

    /// Resolves a heading level (1–6) to an active-theme role, softened
    /// toward the theme's foreground. If that tint is too faint on the
    /// current background, progressively less of the role is used until
    /// ordinary-text contrast holds. The preview and the write mode tint
    /// headings with the same resolution.
    pub fn heading_color(&self, level: usize) -> Color {
        let (role, strength) = match level {
            1 => (self.magenta, 0.65),
            2 => (self.blue, 0.60),
            3 => (self.cyan, 0.55),
            4 => (self.green, 0.45),
            5 => (self.yellow, 0.30),
            _ => (self.magenta, 0.12),
        };

        for step in 0..=CONTRAST_SEARCH_STEPS {
            let role_weight = strength * (1.0 - f32::from(step) / f32::from(CONTRAST_SEARCH_STEPS));
            let candidate = mix(role, self.foreground, role_weight);

            if candidate.relative_contrast(self.background) >= MIN_HEADING_CONTRAST {
                return candidate;
            }
        }

        self.foreground
    }

    /// The omarchy palette mapped onto iced's theme roles, so the runtime
    /// theme and every default-styled widget follow omarchy colors.
    pub fn iced(&self) -> iced::theme::Palette {
        iced::theme::Palette {
            background: self.background,
            text: self.foreground,
            primary: self.accent,
            success: self.green,
            warning: self.yellow,
            danger: self.red,
        }
    }

    /// The iced runtime theme built from this palette — widgets without an
    /// explicit style (text inputs, scrollbars, checkboxes, rules, list
    /// bullets) inherit omarchy colors instead of iced's stock ones.
    pub fn runtime_theme(&self) -> iced::Theme {
        iced::Theme::custom("omarchy", self.iced())
    }
}

/// Mixes `role_weight` of a semantic role with the theme foreground.
fn mix(role: Color, foreground: Color, role_weight: f32) -> Color {
    let foreground_weight = 1.0 - role_weight;

    Color::from_rgb(
        role.r * role_weight + foreground.r * foreground_weight,
        role.g * role_weight + foreground.g * foreground_weight,
        role.b * role_weight + foreground.b * foreground_weight,
    )
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn current_state() -> PathBuf {
    home().join(".local/state/omarchy/current")
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
    use super::{forward_change, hex, Palette, MIN_HEADING_CONTRAST};
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

    /// The raised surface sits one step off the page on both modes and
    /// never collapses into the page or its text.
    #[test]
    fn raised_surface_stays_off_the_page() {
        let mut palette = Palette::default();

        assert!(close(palette.raised(), palette.lighter_background));
        assert_ne!(palette.raised(), palette.background);
        assert_ne!(palette.raised(), palette.foreground);

        palette.light = true;
        assert!(close(palette.raised(), palette.darker_background));
        assert_ne!(palette.raised(), palette.background);
        assert_ne!(palette.raised(), palette.foreground);
    }

    /// The runtime theme carries the omarchy roles: its iced palette maps
    /// onto this palette, so default-styled widgets inherit omarchy colors.
    #[test]
    fn runtime_theme_carries_the_omarchy_roles() {
        let palette = Palette::default();
        let theme = palette.runtime_theme();
        let roles = theme.palette();

        assert!(close(roles.background, palette.background));
        assert!(close(roles.text, palette.foreground));
        assert!(close(roles.primary, palette.accent));
        assert!(close(roles.warning, palette.yellow));
        assert!(close(roles.danger, palette.red));
    }

    /// An absent or unreadable omarchy state falls back to the default
    /// palette instead of failing.
    #[test]
    fn missing_omarchy_state_yields_the_default() {
        let palette = Palette::from_state(std::path::PathBuf::from("/nonexistent/omarchy/state"));

        assert!(close(palette.background, Palette::default().background));
        assert!(close(palette.accent, Palette::default().accent));
    }

    /// Heading levels take distinct semantic roles from the palette,
    /// softened by progressively smaller amounts toward its foreground —
    /// the tints the preview and the write mode both paint headings with.
    #[test]
    fn heading_colors_use_level_specific_palette_roles() {
        let palette = Palette {
            background: Color::BLACK,
            foreground: Color::WHITE,
            magenta: Color::from_rgb(1.0, 0.0, 0.0),
            blue: Color::from_rgb(0.0, 1.0, 0.0),
            cyan: Color::from_rgb(0.0, 0.0, 1.0),
            green: Color::from_rgb(1.0, 1.0, 0.0),
            yellow: Color::from_rgb(1.0, 0.0, 1.0),
            ..Palette::default()
        };

        let colors = [
            palette.heading_color(1),
            palette.heading_color(2),
            palette.heading_color(3),
            palette.heading_color(4),
            palette.heading_color(5),
            palette.heading_color(6),
        ];

        let expected = [
            Color::from_rgb(1.0, 0.35, 0.35),
            Color::from_rgb(0.4, 1.0, 0.4),
            Color::from_rgb(0.45, 0.45, 1.0),
            Color::from_rgb(1.0, 1.0, 0.55),
            Color::from_rgb(1.0, 0.7, 1.0),
            Color::from_rgb(1.0, 0.88, 0.88),
        ];
        let close = colors.into_iter().zip(expected).all(|(actual, expected)| {
            (actual.r - expected.r).abs() < 1e-6
                && (actual.g - expected.g).abs() < 1e-6
                && (actual.b - expected.b).abs() < 1e-6
        });

        assert!(close, "unexpected heading colors: {colors:?}");
    }

    /// A role that disappears into a light background is pulled toward the
    /// theme foreground until it reaches ordinary-text WCAG contrast.
    #[test]
    fn heading_colors_guard_contrast_on_light_themes() {
        let palette = Palette {
            background: Color::WHITE,
            foreground: Color::BLACK,
            magenta: Color::WHITE,
            blue: Color::WHITE,
            cyan: Color::WHITE,
            green: Color::WHITE,
            yellow: Color::WHITE,
            ..Palette::default()
        };

        let colors = (1..=6)
            .map(|level| palette.heading_color(level))
            .collect::<Vec<_>>();

        assert!(colors
            .into_iter()
            .all(|color| { color.relative_contrast(palette.background) >= MIN_HEADING_CONTRAST }));
    }

    /// A queued theme-change event already causes the latest palette to be
    /// loaded. A full channel therefore coalesces rather than stopping the
    /// watcher.
    #[test]
    fn full_theme_change_channel_stays_connected() {
        let (mut sender, receiver) = iced::futures::channel::mpsc::channel(0);

        assert!(forward_change(&mut sender));
        assert!(forward_change(&mut sender));

        drop(receiver);
        assert!(!forward_change(&mut sender));
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

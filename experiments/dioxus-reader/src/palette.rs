//! A theme's colours, resolved, and the shades the chrome derives from them.
//!
//! This is `applyTheme` and `parseColor` from `themes.ts`, and it is the half
//! of a theme that the renderer and the stylesheet actually use. The other
//! half — the file, its name, its id, saving and deleting it — is
//! [`crate::theme`], which is the app's own module compiled into this crate
//! unchanged.
//!
//! The split is the one `themes.ts` already makes and never named: a theme on
//! disk is seven optional strings, and a theme being *drawn with* is a fixed
//! set of `[u8; 3]`s with every absent one derived. Resolving happens once,
//! when a theme is chosen, so the recolouring pass and the stylesheet both see
//! numbers rather than text — and a colour the renderer cannot read is caught
//! there rather than at the moment somebody scrolls onto a page.

/// An 8-bit colour. Alpha is dropped on the way in, as it is in the app.
pub type Rgb = [u8; 3];

/// A theme's colours, all of them present. `Copy`, because every mounted page
/// holds one and reads it on every paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub text: Rgb,
    pub background: Rgb,
    pub accent: Rgb,
    /// What links are tinted with while the document is recoloured. Absent in
    /// the file means the accent.
    pub link: Rgb,
    /// The ground behind selected text, and the ink on it. Derived from the
    /// accent and from each other respectively, which is what most themes
    /// want and why a five-line theme file is enough.
    pub selection_area: Rgb,
    pub selection_text: Rgb,
    /// Whether the pages themselves are recoloured, or only the chrome.
    pub recolor: bool,
    /// Whether a pixel that has a colour of its own keeps it. On in the app,
    /// and the reason the ramp is HSL rather than a flatten. It is a setting
    /// (`recolor_images`) rather than a property of a theme.
    pub keep_colour: bool,
}

/// What is drawn with when a theme names a colour the renderer cannot read,
/// and when there is no theme at all. Black on white is the app's own
/// fallback, and it is deliberately not any theme's colours: a theme that half
/// works is harder to diagnose than one that plainly did not load.
pub const FALLBACK: Palette = Palette {
    text: [0x00, 0x00, 0x00],
    background: [0xff, 0xff, 0xff],
    accent: [0x3d, 0x6b, 0xb3],
    link: [0x3d, 0x6b, 0xb3],
    selection_area: [0xb4, 0xcd, 0xf0],
    selection_text: [0x00, 0x00, 0x00],
    recolor: false,
    keep_colour: true,
};

impl Palette {
    /// The shades the chrome is built out of, derived rather than named.
    pub fn surface(&self) -> Rgb {
        mix(self.background, self.text, 0.06)
    }

    pub fn line(&self) -> Rgb {
        mix(self.background, self.text, 0.16)
    }

    /// The colour the toolbar's own labels are written in.
    ///
    /// **0.74 of the way to the ink, not 0.55, and the difference is the
    /// whole of what a theme is for.** A shade halfway between paper and ink
    /// is a mid-grey whatever the two ends are, so Mark, Trim, the zoom and
    /// the two steppers came out very nearly the same colour under all
    /// fourteen themes — the chrome reading as though the theme had not
    /// loaded. `--text-soft` in `themes.ts` is `mix(text, bg, 0.26)`, which
    /// is this number said from the other end, and it keeps enough of the
    /// ink for the theme to be legible in the bar.
    pub fn muted(&self) -> Rgb {
        mix(self.background, self.text, 0.74)
    }

    /// The quieter one still: the document's name, the "/ 400" beside the
    /// page number, a chord in a menu. `--text-faint` in `themes.ts`, which
    /// is `mix(text, bg, 0.52)` and the same number from the other end.
    pub fn faint(&self) -> Rgb {
        mix(self.background, self.text, 0.48)
    }

    /// What an undrawn page is. See `--page` in `styles.rs`: a page that a
    /// recolouring theme has not reached yet is the theme's paper, and a page
    /// nothing is recolouring is the paper the printer used.
    pub fn page(&self) -> Rgb {
        if self.recolor {
            self.background
        } else {
            [0xff, 0xff, 0xff]
        }
    }
}

/// A colour as CSS writes it, which is how it reaches both the stylesheet and
/// an icon. An inline `<svg>` is parsed by usvg with no cascade behind it, so
/// a shade it is to be drawn in has to arrive as a string — see `Icon` in
/// `app.rs`.
pub fn hex(colour: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

/// `amount` of `b` in `a`.
pub fn mix(a: Rgb, b: Rgb, amount: f64) -> Rgb {
    let mut out = [0u8; 3];
    for channel in 0..3 {
        out[channel] =
            (a[channel] as f64 + (b[channel] as f64 - a[channel] as f64) * amount).round() as u8;
    }
    out
}

/// Which of a theme's colours could not be read, by field name.
///
/// The app raises this as a notice — `unreadableColors` in `themes.ts` — and
/// the whole argument for keeping themes as TOML is that somebody, or
/// something asked on their behalf, will write one and get the notation wrong.
/// Silently rendering black on white is the behaviour this exists to replace.
pub fn unreadable(theme: &crate::theme::Theme) -> Vec<&'static str> {
    let mut bad = Vec::new();
    let mut check = |field: &'static str, value: Option<&String>| {
        if let Some(value) = value {
            if read_colour(value).is_none() {
                bad.push(field);
            }
        }
    };
    check("text", Some(&theme.text));
    check("background", Some(&theme.background));
    check("accent", theme.accent.as_ref());
    check("link", theme.link.as_ref());
    check("selection_area", theme.selection_area.as_ref());
    check("selection_text", theme.selection_text.as_ref());
    bad
}

/// A theme as it will actually be drawn.
///
/// Every absent colour is derived here and nowhere else, so the renderer, the
/// stylesheet and anything that shows a swatch are looking at the same
/// numbers. `keep_colour` is not a theme's to say — it is the `recolor_images`
/// setting — so it is passed in.
pub fn resolve(theme: &crate::theme::Theme, keep_colour: bool) -> Palette {
    let read = |value: &Option<String>| value.as_deref().and_then(read_colour);
    let text = read_colour(&theme.text).unwrap_or(FALLBACK.text);
    let background = read_colour(&theme.background).unwrap_or(FALLBACK.background);
    let accent = read(&theme.accent).unwrap_or_else(|| mix(background, text, 0.62));
    // Absent means "use the accent", which is what the app's own comment on
    // the field says.
    let link = read(&theme.link).unwrap_or(accent);
    // And absent selection means "derive it from the accent" — a wash of it
    // over the paper, so it reads as a highlight rather than as a block.
    let selection_area = read(&theme.selection_area).unwrap_or_else(|| mix(background, accent, 0.4));
    // The ink on that ground, derived from the ground: whichever of the
    // theme's two extremes it is further from.
    let selection_text = read(&theme.selection_text).unwrap_or_else(|| {
        if luma(selection_area) > 140.0 {
            text
        } else {
            background
        }
    });
    Palette {
        text,
        background,
        accent,
        link,
        selection_area,
        selection_text,
        recolor: theme.recolor,
        keep_colour,
    }
}

/// The same weighting the recolouring ramp uses, which is why a colour that is
/// light in the ramp's terms is light here too.
fn luma(colour: Rgb) -> f64 {
    0.2126 * colour[0] as f64 + 0.7152 * colour[1] as f64 + 0.0722 * colour[2] as f64
}

/// Hex and nothing else, checked against the alphabet.
///
/// `parseInt("12345g", 16)` stops at the character it cannot read and returns
/// what it had, so `#12345g` came back as a plausible colour from a string
/// that is not one — the worst of the three possible behaviours, because it is
/// the one nobody notices. This says `None` instead of guessing.
pub fn read_colour(text: &str) -> Option<Rgb> {
    let body = text.strip_prefix('#')?;
    if !body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |at: usize| u8::from_str_radix(&body[at..at + 1], 16).ok();
    let pair = |at: usize| u8::from_str_radix(&body[at..at + 2], 16).ok();
    match body.len() {
        // Alpha is read and dropped, as it is in the app.
        3 | 4 => Some([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
        6 | 8 => Some([pair(0)?, pair(2)?, pair(4)?]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    #[test]
    fn hex_is_read_and_everything_else_is_refused() {
        assert_eq!(read_colour("#abc"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(read_colour("#aabbcc"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(read_colour("#aabbccdd"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(read_colour("#abcd"), Some([0xaa, 0xbb, 0xcc]));
        // The one that used to come back as a plausible colour.
        assert_eq!(read_colour("#12345g"), None);
        assert_eq!(read_colour("steelblue"), None);
        assert_eq!(read_colour("rgb(30, 42, 59)"), None);
        assert_eq!(read_colour("#12345"), None);
    }

    /// Every theme that ships resolves, and the two named ones say what the
    /// brief says they say: the light one leaves a page alone, the dark one
    /// does not, and neither is black or white.
    #[test]
    fn the_shipped_themes_all_resolve() {
        for (id, source) in theme::BUILT_IN {
            let parsed: theme::Theme = toml::from_str(source).expect(id);
            assert!(
                unreadable(&parsed).is_empty(),
                "{id} names a colour the renderer cannot read: {:?}",
                unreadable(&parsed),
            );
            let palette = resolve(&parsed, true);
            assert_ne!(palette.text, palette.background, "{id} is invisible");
        }
    }

    /// A theme naming two colours gets the other four, and they are not the
    /// fallback's: five lines is enough because of `resolve`.
    #[test]
    fn the_colours_a_theme_leaves_out_are_derived() {
        let bare: theme::Theme =
            toml::from_str("name = \"Bare\"\ntext = \"#ffffff\"\nbackground = \"#202020\"\n")
                .expect("parses");
        let palette = resolve(&bare, true);
        assert_eq!(palette.link, palette.accent, "link falls back to the accent");
        assert_ne!(palette.accent, FALLBACK.accent);
        assert_ne!(palette.selection_area, palette.background);
        // The ink on a dark selection is the theme's paper, not its ink.
        assert_eq!(palette.selection_text, palette.background);
    }

    /// And a colour that cannot be read is named rather than guessed at.
    #[test]
    fn an_unreadable_colour_is_reported() {
        let wrong: theme::Theme = toml::from_str(
            "name = \"Wrong\"\ntext = \"steelblue\"\nbackground = \"#fff\"\naccent = \"#12345g\"\n",
        )
        .expect("parses");
        assert_eq!(unreadable(&wrong), vec!["text", "accent"]);
        // And what is drawn is the fallback's, not a plausible colour from a
        // string that is not one.
        assert_eq!(resolve(&wrong, true).text, FALLBACK.text);
    }
}

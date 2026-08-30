//! A theme, and the colours the chrome derives from it.
//!
//! Phase 1 carries two themes rather than fourteen, because the fourteen are
//! already files on the disk and `theme.rs` in `src-tauri` already reads them
//! — that is the ~2,450 lines of Rust the assessment says ports unchanged, and
//! porting it is Phase 3's first item, not this one's. What is needed here is
//! the shape: five colours, a `recolor` flag, and the strictness about hex
//! that `parseColor` in `themes.ts` earned the hard way.

/// An 8-bit colour. Alpha is dropped on the way in, as it is in the app.
pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub text: Rgb,
    pub background: Rgb,
    pub accent: Rgb,
    /// Whether the pages themselves are recoloured, or only the chrome.
    pub recolor: bool,
    /// Whether a pixel that has a colour of its own keeps it. On in the app,
    /// and the reason the ramp is HSL rather than a flatten.
    pub keep_colour: bool,
}

/// The default light theme, which changes nothing about a page.
pub const LIGHT: Theme = Theme {
    name: "Hylo Light",
    text: [0x1c, 0x1c, 0x1f],
    background: [0xf7, 0xf6, 0xf3],
    accent: [0x3d, 0x6b, 0xb3],
    recolor: false,
    keep_colour: true,
};

/// The default dark theme: light ink on a slate ground, not black, because the
/// contrast would be too high.
pub const DARK: Theme = Theme {
    name: "Hylo Dark",
    text: [0xe8, 0xe6, 0xe3],
    background: [0x22, 0x24, 0x2b],
    accent: [0x7f, 0xa8, 0xd8],
    recolor: true,
    keep_colour: true,
};

pub const THEMES: [Theme; 2] = [LIGHT, DARK];

impl Theme {
    pub fn css(&self, colour: Rgb) -> String {
        format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
    }

    /// The shades the chrome is built out of, derived rather than named — five
    /// colours in a theme file is enough because of this function.
    pub fn surface(&self) -> Rgb {
        mix(self.background, self.text, 0.06)
    }

    pub fn line(&self) -> Rgb {
        mix(self.background, self.text, 0.16)
    }

    pub fn muted(&self) -> Rgb {
        mix(self.background, self.text, 0.55)
    }
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
        3 | 4 => Some([
            digit(0)? * 17,
            digit(1)? * 17,
            digit(2)? * 17,
        ]),
        6 | 8 => Some([pair(0)?, pair(2)?, pair(4)?]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

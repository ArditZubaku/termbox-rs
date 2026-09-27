//! Cell colours and text styling, packed into termbox's 16-bit attribute word.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// A cell attribute: a colour in the low byte, style flags in the high byte.
///
/// This is the equivalent of `termbox.Attribute`. Combine a colour with style
/// flags using `|`:
///
/// ```
/// use termbox_rs::Attribute;
/// let fg = Attribute::RED | Attribute::BOLD;
/// assert_eq!(fg.color(), Attribute::RED.0);
/// assert!(fg.contains(Attribute::BOLD));
/// ```
///
/// # Colour ranges
///
/// How the low bits are interpreted depends on the active
/// [`OutputMode`](crate::OutputMode):
///
/// * [`OutputMode::Normal`](crate::OutputMode::Normal) — the low 4 bits:
///   `0` is the terminal default, `1..=8` are [`BLACK`](Self::BLACK) through
///   [`WHITE`](Self::WHITE).
/// * [`OutputMode::EightBit`](crate::OutputMode::EightBit) — the low 8 bits are
///   an xterm-256 palette index, except that `0` still means "terminal
///   default", so palette entry 0 is not reachable. The named constants line up
///   one past their xterm index for this reason, exactly like termbox-go.
/// * [`OutputMode::WebSafe`](crate::OutputMode::WebSafe) — `0..=215` index the
///   6x6x6 colour cube; see [`Attribute::cube`].
/// * [`OutputMode::Grayscale`](crate::OutputMode::Grayscale) — `0..=23` index
///   the grey ramp.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Attribute(pub u16);

impl Attribute {
    /// Whatever the terminal uses by default.
    pub const DEFAULT: Attribute = Attribute(0x00);
    /// Black.
    pub const BLACK: Attribute = Attribute(0x01);
    /// Red.
    pub const RED: Attribute = Attribute(0x02);
    /// Green.
    pub const GREEN: Attribute = Attribute(0x03);
    /// Yellow.
    pub const YELLOW: Attribute = Attribute(0x04);
    /// Blue.
    pub const BLUE: Attribute = Attribute(0x05);
    /// Magenta.
    pub const MAGENTA: Attribute = Attribute(0x06);
    /// Cyan.
    pub const CYAN: Attribute = Attribute(0x07);
    /// White.
    pub const WHITE: Attribute = Attribute(0x08);
    /// Dark grey. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const DARK_GRAY: Attribute = Attribute(0x09);
    /// Light red. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_RED: Attribute = Attribute(0x0a);
    /// Light green. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_GREEN: Attribute = Attribute(0x0b);
    /// Light yellow. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_YELLOW: Attribute = Attribute(0x0c);
    /// Light blue. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_BLUE: Attribute = Attribute(0x0d);
    /// Light magenta. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_MAGENTA: Attribute = Attribute(0x0e);
    /// Light cyan. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_CYAN: Attribute = Attribute(0x0f);
    /// Light grey. Needs [`OutputMode::EightBit`](crate::OutputMode::EightBit).
    pub const LIGHT_GRAY: Attribute = Attribute(0x10);

    /// Bold text. On a background attribute the terminal blinks instead, which
    /// is what [`BLINK`](Self::BLINK) is named after.
    pub const BOLD: Attribute = Attribute(0x0100);
    /// Blinking text: [`BOLD`](Self::BOLD) set on a *background* attribute.
    pub const BLINK: Attribute = Attribute(0x0100);
    /// Underlined text.
    pub const UNDERLINE: Attribute = Attribute(0x0200);
    /// Swapped foreground and background.
    pub const REVERSE: Attribute = Attribute(0x0400);

    /// Bits holding the colour.
    pub const COLOR_MASK: u16 = 0x00ff;
    /// Bits holding the style flags.
    pub const STYLE_MASK: u16 = 0xff00;

    /// An arbitrary xterm-256 palette index.
    ///
    /// Remember that index `0` is read as "terminal default" by termbox; use
    /// [`Attribute::cube`] or [`Attribute::gray`] to name a dark colour.
    #[inline]
    pub const fn byte(index: u8) -> Attribute {
        Attribute(index as u16)
    }

    /// A colour from the 6x6x6 cube, each component in `0..=5`.
    ///
    /// Components are clamped. Under
    /// [`OutputMode::EightBit`](crate::OutputMode::EightBit) this yields the
    /// xterm index `16 + 36r + 6g + b`; under
    /// [`OutputMode::WebSafe`](crate::OutputMode::WebSafe) termbox adds the
    /// offset itself, so use [`Attribute::byte`] with `36r + 6g + b` there.
    #[inline]
    pub const fn cube(r: u8, g: u8, b: u8) -> Attribute {
        let (r, g, b) = (min_u8(r, 5), min_u8(g, 5), min_u8(b, 5));
        Attribute(16 + 36 * r as u16 + 6 * g as u16 + b as u16)
    }

    /// A shade from the 24 step grey ramp, `0` darkest.
    ///
    /// The shade is clamped. Under
    /// [`OutputMode::EightBit`](crate::OutputMode::EightBit) this yields the
    /// xterm index `232 + shade`; under
    /// [`OutputMode::Grayscale`](crate::OutputMode::Grayscale) termbox adds the
    /// offset itself, so use [`Attribute::byte`] with the raw shade there.
    #[inline]
    pub const fn gray(shade: u8) -> Attribute {
        Attribute(232 + min_u8(shade, 23) as u16)
    }

    /// The colour bits on their own.
    #[inline]
    pub const fn color(self) -> u16 {
        self.0 & Self::COLOR_MASK
    }

    /// The style bits on their own.
    #[inline]
    pub const fn style(self) -> u16 {
        self.0 & Self::STYLE_MASK
    }

    /// True if every flag in `other` is set.
    #[inline]
    pub const fn contains(self, other: Attribute) -> bool {
        self.0 & other.0 == other.0
    }

    /// True if this is the terminal's default colour with no styling.
    #[inline]
    pub const fn is_default(self) -> bool {
        self.0 == 0
    }

    /// The same colour with `flags` added.
    #[inline]
    pub const fn with(self, flags: Attribute) -> Attribute {
        Attribute(self.0 | flags.0)
    }

    /// The same colour with `flags` removed.
    #[inline]
    pub const fn without(self, flags: Attribute) -> Attribute {
        Attribute(self.0 & !flags.0)
    }

    /// The nearest xterm-256 palette entry to an RGB triple.
    ///
    /// See [`rgb_to_attribute`], which this forwards to.
    #[inline]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Attribute {
        rgb_to_attribute(r, g, b)
    }
}

const fn min_u8(a: u8, b: u8) -> u8 {
    if a < b { a } else { b }
}

/// The nearest xterm-256 palette entry to an RGB triple.
///
/// termbox's C backend has no 24-bit colour mode, so unlike
/// `termbox.RGBToAttribute` this quantises to the 256 colour palette: the
/// 6x6x6 cube and the 24 step grey ramp are searched and the closer of the two
/// wins. Use the result under
/// [`OutputMode::EightBit`](crate::OutputMode::EightBit).
pub const fn rgb_to_attribute(r: u8, g: u8, b: u8) -> Attribute {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

    const fn nearest_level(v: u8) -> usize {
        let mut i = 0;
        let mut best = 0;
        let mut best_d = u32::MAX;
        while i < 6 {
            let d = diff(v, LEVELS[i]);
            if d < best_d {
                best_d = d;
                best = i;
            }
            i += 1;
        }
        best
    }

    const fn diff(a: u8, b: u8) -> u32 {
        let (a, b) = (a as i32, b as i32);
        ((a - b) * (a - b)) as u32
    }

    let (ri, gi, bi) = (nearest_level(r), nearest_level(g), nearest_level(b));
    let cube_d = diff(r, LEVELS[ri]) + diff(g, LEVELS[gi]) + diff(b, LEVELS[bi]);

    // The grey ramp runs 8, 18, .. 238; rounding the average to that grid gives
    // the closest shade without a search.
    let avg = (r as u32 + g as u32 + b as u32) / 3;
    let shade = if avg < 8 {
        0
    } else {
        let s = (avg - 8 + 5) / 10;
        if s > 23 { 23 } else { s }
    };
    let grey = (8 + shade * 10) as u8;
    let grey_d = diff(r, grey) + diff(g, grey) + diff(b, grey);

    if grey_d < cube_d {
        Attribute(232 + shade as u16)
    } else {
        Attribute(16 + 36 * ri as u16 + 6 * gi as u16 + bi as u16)
    }
}

impl BitOr for Attribute {
    type Output = Attribute;
    #[inline]
    fn bitor(self, rhs: Attribute) -> Attribute {
        Attribute(self.0 | rhs.0)
    }
}

impl BitOrAssign for Attribute {
    #[inline]
    fn bitor_assign(&mut self, rhs: Attribute) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Attribute {
    type Output = Attribute;
    #[inline]
    fn bitand(self, rhs: Attribute) -> Attribute {
        Attribute(self.0 & rhs.0)
    }
}

impl BitAndAssign for Attribute {
    #[inline]
    fn bitand_assign(&mut self, rhs: Attribute) {
        self.0 &= rhs.0;
    }
}

impl Not for Attribute {
    type Output = Attribute;
    #[inline]
    fn not(self) -> Attribute {
        Attribute(!self.0)
    }
}

impl From<u16> for Attribute {
    #[inline]
    fn from(bits: u16) -> Attribute {
        Attribute(bits)
    }
}

impl From<Attribute> for u16 {
    #[inline]
    fn from(attr: Attribute) -> u16 {
        attr.0
    }
}

impl From<rustbox::Color> for Attribute {
    fn from(color: rustbox::Color) -> Attribute {
        use rustbox::Color::*;
        match color {
            Default => Attribute::DEFAULT,
            Black => Attribute::BLACK,
            Red => Attribute::RED,
            Green => Attribute::GREEN,
            Yellow => Attribute::YELLOW,
            Blue => Attribute::BLUE,
            Magenta => Attribute::MAGENTA,
            Cyan => Attribute::CYAN,
            White => Attribute::WHITE,
            Byte(b) => Attribute(b),
        }
    }
}

impl From<Attribute> for rustbox::Color {
    fn from(attr: Attribute) -> rustbox::Color {
        match attr.color() {
            0 => rustbox::Color::Default,
            c => rustbox::Color::Byte(c),
        }
    }
}

impl fmt::Debug for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Attribute({}", self.color())?;
        for (flag, name) in [
            (Attribute::BOLD, "BOLD"),
            (Attribute::UNDERLINE, "UNDERLINE"),
            (Attribute::REVERSE, "REVERSE"),
        ] {
            if self.contains(flag) {
                write!(f, " | {name}")?;
            }
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_and_colour_do_not_overlap() {
        let a = Attribute::RED | Attribute::BOLD | Attribute::UNDERLINE;
        assert_eq!(a.color(), 2);
        assert_eq!(a.style(), 0x0300);
        assert!(a.contains(Attribute::UNDERLINE));
        assert!(!a.without(Attribute::UNDERLINE).contains(Attribute::UNDERLINE));
    }

    #[test]
    fn rgb_quantises_to_the_palette() {
        assert_eq!(rgb_to_attribute(0, 0, 0), Attribute(16));
        assert_eq!(rgb_to_attribute(255, 255, 255), Attribute(231));
        assert_eq!(rgb_to_attribute(255, 0, 0), Attribute(196));
        assert_eq!(rgb_to_attribute(128, 128, 128), Attribute(244));
        assert_eq!(Attribute::cube(5, 0, 0), Attribute(196));
        assert_eq!(Attribute::gray(0), Attribute(232));
        assert_eq!(Attribute::gray(99), Attribute(255));
    }

    #[test]
    fn rustbox_colours_round_trip() {
        assert_eq!(Attribute::from(rustbox::Color::Cyan), Attribute::CYAN);
        assert_eq!(Attribute::from(rustbox::Color::Default), Attribute::DEFAULT);
        assert_eq!(rustbox::Color::from(Attribute::DEFAULT), rustbox::Color::Default);
        assert_eq!(rustbox::Color::from(Attribute::byte(42)), rustbox::Color::Byte(42));
    }
}

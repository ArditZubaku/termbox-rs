//! The screen cell.

use crate::attr::Attribute;

/// One character on screen with its colours.
///
/// The layout matches termbox's `struct tb_cell` exactly (8 bytes: a 4 byte
/// scalar followed by two 16-bit attributes), which lets whole runs of cells be
/// handed to the C library with a single memcpy instead of a call per cell.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub struct Cell {
    /// The character to draw.
    pub ch: char,
    /// Foreground colour and style.
    pub fg: Attribute,
    /// Background colour and style.
    pub bg: Attribute,
}

impl Cell {
    /// A space in the terminal's default colours.
    pub const BLANK: Cell = Cell {
        ch: ' ',
        fg: Attribute::DEFAULT,
        bg: Attribute::DEFAULT,
    };

    /// A cell from its parts.
    #[inline]
    pub const fn new(ch: char, fg: Attribute, bg: Attribute) -> Cell {
        Cell { ch, fg, bg }
    }

    /// A space in the given colours.
    #[inline]
    pub const fn blank(fg: Attribute, bg: Attribute) -> Cell {
        Cell { ch: ' ', fg, bg }
    }
}

impl Default for Cell {
    #[inline]
    fn default() -> Cell {
        Cell::BLANK
    }
}

const _: () = {
    assert!(size_of::<Cell>() == size_of::<termbox_sys::RawCell>());
    assert!(align_of::<Cell>() == align_of::<termbox_sys::RawCell>());
    assert!(std::mem::offset_of!(Cell, ch) == 0);
    assert!(std::mem::offset_of!(Cell, fg) == 4);
    assert!(std::mem::offset_of!(Cell, bg) == 6);
};

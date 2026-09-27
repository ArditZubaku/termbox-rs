//! Keyboard, mouse and resize events.

use crate::error::{Error, Result};

/// Modifier bits carried by an event.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct Modifier(pub u8);

impl Modifier {
    /// No modifier.
    pub const NONE: Modifier = Modifier(0x00);
    /// Alt was held. Only reported under
    /// [`InputMode::Alt`](crate::InputMode::Alt).
    pub const ALT: Modifier = Modifier(0x01);
    /// The mouse moved with a button held.
    pub const MOTION: Modifier = Modifier(0x02);

    /// True if every bit in `other` is set.
    #[inline]
    pub const fn contains(self, other: Modifier) -> bool {
        self.0 & other.0 == other.0
    }

    /// True if no bit is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for Modifier {
    type Output = Modifier;
    #[inline]
    fn bitor(self, rhs: Modifier) -> Modifier {
        Modifier(self.0 | rhs.0)
    }
}

/// A key press.
///
/// termbox reports printable input as a character and everything else as a key
/// code; both end up here, so a single `match` covers all input.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    /// A function key, `F(1)` through `F(12)`.
    F(u8),
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PgUp,
    /// Page down.
    PgDn,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Tab, which is also `Ctrl+I`.
    Tab,
    /// Enter, which is also `Ctrl+M`.
    Enter,
    /// Escape.
    Esc,
    /// Space.
    Space,
    /// Backspace, which is also `Ctrl+H`.
    Backspace,
    /// The other backspace code, `0x7f`, sent by most terminals.
    Backspace2,
    /// A control chord, held as its lowercase letter or symbol.
    Ctrl(char),
    /// A printable character.
    Char(char),
    /// A key code termbox reported but this crate does not name.
    Unknown(u16),
}

impl Key {
    /// Decodes termbox's `(key, ch)` pair.
    pub fn from_code(code: u16, ch: u32) -> Key {
        match code {
            0 => match char::from_u32(ch) {
                Some('\0') | None => Key::Ctrl('~'),
                Some(c) => Key::Char(c),
            },
            0x01..=0x07 => Key::Ctrl((b'a' + code as u8 - 1) as char),
            0x08 => Key::Backspace,
            0x09 => Key::Tab,
            0x0a..=0x0c => Key::Ctrl((b'a' + code as u8 - 1) as char),
            0x0d => Key::Enter,
            0x0e..=0x1a => Key::Ctrl((b'a' + code as u8 - 1) as char),
            0x1b => Key::Esc,
            0x1c => Key::Ctrl('\\'),
            0x1d => Key::Ctrl(']'),
            0x1e => Key::Ctrl('6'),
            0x1f => Key::Ctrl('/'),
            0x20 => Key::Space,
            0x7f => Key::Backspace2,
            // 0xffe4..=0xffe9 are the mouse buttons, which arrive as their own
            // event type rather than as a key.
            0xffe4..=0xffe9 => Key::Unknown(code),
            0xffea..=0xffff => match 0xffff - code {
                n @ 0..=11 => Key::F(n as u8 + 1),
                12 => Key::Insert,
                13 => Key::Delete,
                14 => Key::Home,
                15 => Key::End,
                16 => Key::PgUp,
                17 => Key::PgDn,
                18 => Key::ArrowUp,
                19 => Key::ArrowDown,
                20 => Key::ArrowLeft,
                21 => Key::ArrowRight,
                _ => Key::Unknown(code),
            },
            _ => match char::from_u32(ch) {
                Some(c) if ch != 0 => Key::Char(c),
                _ => Key::Unknown(code),
            },
        }
    }

    /// The termbox key code for this key, or `0` for a plain character.
    pub fn to_code(self) -> u16 {
        match self {
            Key::F(n) => 0xffff - (n.clamp(1, 12) as u16 - 1),
            Key::Insert => 0xffff - 12,
            Key::Delete => 0xffff - 13,
            Key::Home => 0xffff - 14,
            Key::End => 0xffff - 15,
            Key::PgUp => 0xffff - 16,
            Key::PgDn => 0xffff - 17,
            Key::ArrowUp => 0xffff - 18,
            Key::ArrowDown => 0xffff - 19,
            Key::ArrowLeft => 0xffff - 20,
            Key::ArrowRight => 0xffff - 21,
            Key::Tab => 0x09,
            Key::Enter => 0x0d,
            Key::Esc => 0x1b,
            Key::Space => 0x20,
            Key::Backspace => 0x08,
            Key::Backspace2 => 0x7f,
            Key::Ctrl('\\') => 0x1c,
            Key::Ctrl(']') => 0x1d,
            Key::Ctrl('6') => 0x1e,
            Key::Ctrl('/') => 0x1f,
            Key::Ctrl('~') => 0x00,
            Key::Ctrl(c) if c.is_ascii_alphabetic() => {
                (c.to_ascii_lowercase() as u16) - b'a' as u16 + 1
            }
            Key::Ctrl(_) | Key::Char(_) => 0,
            Key::Unknown(code) => code,
        }
    }

    /// The character this key stands for, if it produces one.
    pub fn ch(self) -> Option<char> {
        match self {
            Key::Char(c) => Some(c),
            Key::Space => Some(' '),
            Key::Tab => Some('\t'),
            Key::Enter => Some('\r'),
            _ => None,
        }
    }

    /// True for either backspace code.
    #[inline]
    pub fn is_backspace(self) -> bool {
        matches!(self, Key::Backspace | Key::Backspace2)
    }
}

impl From<rustbox::Key> for Key {
    fn from(key: rustbox::Key) -> Key {
        match key {
            rustbox::Key::Tab => Key::Tab,
            rustbox::Key::Enter => Key::Enter,
            rustbox::Key::Esc => Key::Esc,
            rustbox::Key::Backspace => Key::Backspace2,
            rustbox::Key::Right => Key::ArrowRight,
            rustbox::Key::Left => Key::ArrowLeft,
            rustbox::Key::Up => Key::ArrowUp,
            rustbox::Key::Down => Key::ArrowDown,
            rustbox::Key::Delete => Key::Delete,
            rustbox::Key::Insert => Key::Insert,
            rustbox::Key::Home => Key::Home,
            rustbox::Key::End => Key::End,
            rustbox::Key::PageUp => Key::PgUp,
            rustbox::Key::PageDown => Key::PgDn,
            rustbox::Key::Char(' ') => Key::Space,
            rustbox::Key::Char(c) => Key::Char(c),
            rustbox::Key::Ctrl(c) => Key::Ctrl(c),
            rustbox::Key::F(n) => Key::F(n.clamp(1, 12) as u8),
            rustbox::Key::Unknown(code) => Key::Unknown(code),
        }
    }
}

/// A mouse button, wheel direction or release.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Middle button.
    Middle,
    /// Right button.
    Right,
    /// A button was released.
    Release,
    /// Wheel scrolled up.
    WheelUp,
    /// Wheel scrolled down.
    WheelDown,
}

impl MouseButton {
    /// Decodes termbox's mouse key code.
    pub fn from_code(code: u16) -> Option<MouseButton> {
        match 0xffff - code {
            22 => Some(MouseButton::Left),
            23 => Some(MouseButton::Right),
            24 => Some(MouseButton::Middle),
            25 => Some(MouseButton::Release),
            26 => Some(MouseButton::WheelUp),
            27 => Some(MouseButton::WheelDown),
            _ => None,
        }
    }

    /// The termbox key code for this button.
    pub fn to_code(self) -> u16 {
        0xffff
            - match self {
                MouseButton::Left => 22,
                MouseButton::Right => 23,
                MouseButton::Middle => 24,
                MouseButton::Release => 25,
                MouseButton::WheelUp => 26,
                MouseButton::WheelDown => 27,
            }
    }
}

/// Which variant an [`Event`] is, for code ported from termbox-go's
/// `EventType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EventKind {
    /// A key was pressed.
    Key,
    /// The terminal was resized.
    Resize,
    /// The mouse was used.
    Mouse,
    /// A poll was interrupted.
    Interrupt,
    /// An undecoded event.
    Raw,
    /// Nothing happened before the timeout.
    None,
}

/// Something that happened in the terminal.
///
/// termbox-go models this as one struct with a type tag and a field per
/// variant; an enum carries the same information without the invalid
/// combinations. [`Event::kind`] gives the tag back when you need it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// A key was pressed.
    Key {
        /// Modifier bits held with the key.
        modifier: Modifier,
        /// The key itself.
        key: Key,
    },
    /// The terminal was resized.
    Resize {
        /// New width in cells.
        width: usize,
        /// New height in cells.
        height: usize,
    },
    /// The mouse was clicked, released, moved or scrolled.
    Mouse {
        /// Modifier bits held with the click.
        modifier: Modifier,
        /// Which button.
        button: MouseButton,
        /// Column, zero based.
        x: usize,
        /// Row, zero based.
        y: usize,
    },
    /// [`Termbox::interrupt`](crate::Termbox::interrupt) woke the poll up.
    Interrupt,
    /// An event straight from termbox, undecoded.
    Raw(RawEvent),
    /// Nothing happened before the timeout ran out.
    None,
}

impl Event {
    /// Which variant this is.
    pub fn kind(&self) -> EventKind {
        match self {
            Event::Key { .. } => EventKind::Key,
            Event::Resize { .. } => EventKind::Resize,
            Event::Mouse { .. } => EventKind::Mouse,
            Event::Interrupt => EventKind::Interrupt,
            Event::Raw(_) => EventKind::Raw,
            Event::None => EventKind::None,
        }
    }

    /// The key, if this is a key press.
    pub fn key(&self) -> Option<Key> {
        match *self {
            Event::Key { key, .. } => Some(key),
            _ => None,
        }
    }

    /// The character typed, if this is a key press that produced one.
    pub fn ch(&self) -> Option<char> {
        self.key().and_then(Key::ch)
    }

    /// The modifier bits, if this event carries any.
    pub fn modifier(&self) -> Modifier {
        match *self {
            Event::Key { modifier, .. } | Event::Mouse { modifier, .. } => modifier,
            _ => Modifier::NONE,
        }
    }
}

/// An event exactly as termbox reported it.
///
/// termbox's C backend decodes the input stream itself and never exposes the
/// bytes, so this, rather than a byte slice, is what
/// [`Termbox::poll_raw_event`](crate::Termbox::poll_raw_event) hands back. To
/// decode bytes from somewhere else, use [`parse_event`](crate::parse_event).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct RawEvent {
    /// 1 for a key, 2 for a resize, 3 for the mouse.
    pub etype: u8,
    /// Modifier bits.
    pub emod: u8,
    /// Key code, `0` when a character was typed.
    pub key: u16,
    /// The character typed, `0` when a key code was sent.
    pub ch: u32,
    /// Width, on a resize.
    pub w: i32,
    /// Height, on a resize.
    pub h: i32,
    /// Column, on a mouse event.
    pub x: i32,
    /// Row, on a mouse event.
    pub y: i32,
}

impl RawEvent {
    /// Decodes this into an [`Event`].
    pub fn decode(self) -> Event {
        match self.etype {
            1 => Event::Key {
                modifier: Modifier(self.emod),
                key: Key::from_code(self.key, self.ch),
            },
            2 => Event::Resize {
                width: self.w.max(0) as usize,
                height: self.h.max(0) as usize,
            },
            3 => match MouseButton::from_code(self.key) {
                Some(button) => Event::Mouse {
                    modifier: Modifier(self.emod),
                    button,
                    x: self.x.max(0) as usize,
                    y: self.y.max(0) as usize,
                },
                None => Event::Raw(self),
            },
            _ => Event::None,
        }
    }
}

impl From<termbox_sys::RawEvent> for RawEvent {
    fn from(ev: termbox_sys::RawEvent) -> RawEvent {
        RawEvent {
            etype: ev.etype,
            emod: ev.emod,
            key: ev.key,
            ch: ev.ch,
            w: ev.w,
            h: ev.h,
            x: ev.x,
            y: ev.y,
        }
    }
}

/// Turns termbox's return code plus event struct into a result.
pub(crate) fn unpack(rc: i32, ev: termbox_sys::RawEvent) -> Result<Event> {
    match rc {
        0 => Ok(Event::None),
        n if n < 0 => Err(Error::Event(n as isize)),
        _ => Ok(RawEvent::from(ev).decode()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_codes_round_trip() {
        for key in [
            Key::F(1),
            Key::F(12),
            Key::Insert,
            Key::Delete,
            Key::Home,
            Key::End,
            Key::PgUp,
            Key::PgDn,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Tab,
            Key::Enter,
            Key::Esc,
            Key::Space,
            Key::Backspace,
            Key::Backspace2,
            Key::Ctrl('a'),
            Key::Ctrl('z'),
            Key::Ctrl('\\'),
        ] {
            assert_eq!(Key::from_code(key.to_code(), 0), key, "{key:?}");
        }
    }

    #[test]
    fn characters_come_through_ch() {
        assert_eq!(Key::from_code(0, 'q' as u32), Key::Char('q'));
        assert_eq!(Key::from_code(0, 'ä' as u32), Key::Char('ä'));
    }

    #[test]
    fn mouse_codes_round_trip() {
        for button in [
            MouseButton::Left,
            MouseButton::Middle,
            MouseButton::Right,
            MouseButton::Release,
            MouseButton::WheelUp,
            MouseButton::WheelDown,
        ] {
            assert_eq!(MouseButton::from_code(button.to_code()), Some(button));
        }
    }

    #[test]
    fn raw_events_decode() {
        let ev = RawEvent {
            etype: 2,
            w: 80,
            h: 24,
            ..RawEvent::default()
        };
        assert_eq!(
            ev.decode(),
            Event::Resize {
                width: 80,
                height: 24
            }
        );
        assert_eq!(ev.decode().kind(), EventKind::Resize);
    }
}

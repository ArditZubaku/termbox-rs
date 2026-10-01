//! Decoding a raw terminal byte stream into events.

use crate::event::{Event, Key, Modifier, MouseButton};

/// Decodes the first event in `data`.
///
/// Returns the event and how many bytes it used. A count of `0` means `data`
/// ends in an incomplete sequence and more input is needed; an event of
/// [`Event::None`] with a non-zero count means those bytes carry nothing worth
/// reporting — either a recognised sequence with no event in it or a byte that
/// cannot begin one — so skip them and carry on. Bytes that can never become a
/// sequence are always consumed, so draining a stream of arbitrary input
/// terminates.
///
/// This is the counterpart to `termbox.ParseEvent`. termbox's C backend decodes
/// input internally and never hands the byte stream out, so this parser is
/// self-contained: point it at a pty, a recorded session or any other source of
/// terminal input.
///
/// ```
/// use termbox_rs::{parse_event, Event, Key};
///
/// let (event, used) = parse_event(b"\x1b[A");
/// assert_eq!(used, 3);
/// assert_eq!(event.key(), Some(Key::ArrowUp));
/// assert_eq!(parse_event(b"\x1b["), (Event::None, 0));
/// ```
pub fn parse_event(data: &[u8]) -> (Event, usize) {
    match data.first() {
        None => (Event::None, 0),
        Some(0x1b) => parse_escape(data),
        Some(_) => parse_plain(data),
    }
}

fn key(modifier: Modifier, key: Key) -> Event {
    Event::Key { modifier, key }
}

fn parse_plain(data: &[u8]) -> (Event, usize) {
    let b = data[0];
    if b < 0x20 || b == 0x7f {
        return (key(Modifier::NONE, Key::from_code(b as u16, 0)), 1);
    }
    match decode_utf8(data) {
        Utf8::Char(' ', n) => (key(Modifier::NONE, Key::Space), n),
        Utf8::Char(c, n) => (key(Modifier::NONE, Key::Char(c)), n),
        Utf8::Incomplete => (Event::None, 0),
        // Dropping the byte rather than asking for more is what keeps a stream
        // with junk in it draining; a zero count here would wedge the caller.
        Utf8::Invalid => (Event::None, 1),
    }
}

fn parse_escape(data: &[u8]) -> (Event, usize) {
    // Each escape that is not the start of a sequence is the Alt modifier on
    // whatever follows, so a run of them collapses into one event. Walking the
    // run rather than recursing through it keeps a stream of 0x1b bytes from
    // overflowing the stack.
    let mut skipped = 0;
    let (event, used) = loop {
        match data.get(skipped + 1) {
            None => break (key(Modifier::NONE, Key::Esc), skipped + 1),
            Some(b'[') => break shift(parse_csi(&data[skipped..]), skipped),
            Some(b'O') => break shift(parse_ss3(&data[skipped..]), skipped),
            Some(0x1b) => skipped += 1,
            Some(_) => {
                let (event, used) = shift(parse_plain(&data[skipped + 1..]), skipped + 1);
                return with_alt(event, used);
            }
        }
    };
    if skipped == 0 {
        return (event, used);
    }
    with_alt(event, used)
}

fn with_alt(event: Event, used: usize) -> (Event, usize) {
    match event {
        _ if used == 0 => (Event::None, 0),
        Event::Key { modifier, key: k } => (key(modifier | Modifier::ALT, k), used),
        event => (event, used),
    }
}

/// Re-bases a sub-parse onto the bytes that were skipped before it.
fn shift((event, used): (Event, usize), offset: usize) -> (Event, usize) {
    if used == 0 {
        (Event::None, 0)
    } else {
        (event, used + offset)
    }
}

fn parse_ss3(data: &[u8]) -> (Event, usize) {
    let Some(&b) = data.get(2) else {
        return (Event::None, 0);
    };
    let k = match b {
        b'P' => Key::F(1),
        b'Q' => Key::F(2),
        b'R' => Key::F(3),
        b'S' => Key::F(4),
        b'A' => Key::ArrowUp,
        b'B' => Key::ArrowDown,
        b'C' => Key::ArrowRight,
        b'D' => Key::ArrowLeft,
        b'H' => Key::Home,
        b'F' => Key::End,
        _ => return (Event::None, 3),
    };
    (key(Modifier::NONE, k), 3)
}

fn parse_csi(data: &[u8]) -> (Event, usize) {
    match data.get(2) {
        None => (Event::None, 0),
        Some(b'M') => parse_x10_mouse(data),
        Some(b'<') => parse_sgr_mouse(data),
        Some(_) => parse_csi_key(data),
    }
}

fn parse_csi_key(data: &[u8]) -> (Event, usize) {
    let mut params = [0u32; 4];
    let mut count = 0;
    let mut i = 2;

    while i < data.len() {
        let b = data[i];
        match b {
            b'0'..=b'9' => {
                if count == 0 {
                    count = 1;
                }
                let slot = &mut params[(count - 1).min(params.len() - 1)];
                *slot = slot.saturating_mul(10) + (b - b'0') as u32;
                i += 1;
            }
            b';' => {
                count = (count + 1).max(2);
                i += 1;
            }
            0x40..=0x7e => {
                let modifier = csi_modifier(params.get(1).copied().unwrap_or(0));
                let k = match b {
                    b'A' => Key::ArrowUp,
                    b'B' => Key::ArrowDown,
                    b'C' => Key::ArrowRight,
                    b'D' => Key::ArrowLeft,
                    b'H' => Key::Home,
                    b'F' => Key::End,
                    b'~' => match params[0] {
                        1 | 7 => Key::Home,
                        2 => Key::Insert,
                        3 => Key::Delete,
                        4 | 8 => Key::End,
                        5 => Key::PgUp,
                        6 => Key::PgDn,
                        n @ 11..=15 => Key::F(n as u8 - 10),
                        n @ 17..=21 => Key::F(n as u8 - 11),
                        n @ 23..=24 => Key::F(n as u8 - 12),
                        _ => return (Event::None, i + 1),
                    },
                    _ => return (Event::None, i + 1),
                };
                return (key(modifier, k), i + 1);
            }
            _ => {
                i += 1;
            }
        }
    }
    (Event::None, 0)
}

/// xterm reports modifiers as a 1-based bitmask: shift 1, alt 2, ctrl 4.
fn csi_modifier(param: u32) -> Modifier {
    if param >= 2 && (param - 1) & 0x02 != 0 {
        Modifier::ALT
    } else {
        Modifier::NONE
    }
}

fn mouse_from_bits(bits: u32, released: bool) -> (Modifier, MouseButton) {
    let modifier = if bits & 0x20 != 0 {
        Modifier::MOTION
    } else {
        Modifier::NONE
    };
    let button = if bits & 0x40 != 0 {
        if bits & 0x01 == 0 {
            MouseButton::WheelUp
        } else {
            MouseButton::WheelDown
        }
    } else if released {
        MouseButton::Release
    } else {
        match bits & 0x03 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::Release,
        }
    };
    (modifier, button)
}

fn parse_x10_mouse(data: &[u8]) -> (Event, usize) {
    if data.len() < 6 {
        return (Event::None, 0);
    }
    let bits = data[3].wrapping_sub(32) as u32;
    let (modifier, button) = mouse_from_bits(bits, bits & 0x03 == 3);
    let x = data[4].saturating_sub(33) as usize;
    let y = data[5].saturating_sub(33) as usize;
    (
        Event::Mouse {
            modifier,
            button,
            x,
            y,
        },
        6,
    )
}

fn parse_sgr_mouse(data: &[u8]) -> (Event, usize) {
    let mut params = [0u32; 3];
    let mut count = 0;
    let mut i = 3;

    while i < data.len() {
        match data[i] {
            b'0'..=b'9' => {
                let slot = &mut params[count.min(params.len() - 1)];
                *slot = slot.saturating_mul(10) + (data[i] - b'0') as u32;
                i += 1;
            }
            b';' => {
                count = (count + 1).min(params.len() - 1);
                i += 1;
            }
            b'M' | b'm' => {
                let (modifier, button) = mouse_from_bits(params[0], data[i] == b'm');
                return (
                    Event::Mouse {
                        modifier,
                        button,
                        x: params[1].saturating_sub(1) as usize,
                        y: params[2].saturating_sub(1) as usize,
                    },
                    i + 1,
                );
            }
            _ => return (Event::None, i + 1),
        }
    }
    (Event::None, 0)
}

/// What a byte at the head of the stream turned out to be.
enum Utf8 {
    /// A scalar and the bytes it took.
    Char(char, usize),
    /// A valid prefix cut short; more input would finish it.
    Incomplete,
    /// Not a scalar and never will be, however much more arrives.
    Invalid,
}

/// Decodes one UTF-8 scalar from the head of `data`.
fn decode_utf8(data: &[u8]) -> Utf8 {
    let Some(&b0) = data.first() else {
        return Utf8::Incomplete;
    };
    // The low bound rejects the overlong encodings the lead byte alone cannot.
    let (len, lowest) = match b0 {
        0x00..=0x7f => return Utf8::Char(b0 as char, 1),
        0xc2..=0xdf => (2, 0x80),
        0xe0..=0xef => (3, 0x800),
        0xf0..=0xf4 => (4, 0x10000),
        _ => return Utf8::Invalid,
    };
    if data.len() < len {
        return Utf8::Incomplete;
    }
    let mut code = match len {
        2 => (b0 & 0x1f) as u32,
        3 => (b0 & 0x0f) as u32,
        _ => (b0 & 0x07) as u32,
    };
    for &b in &data[1..len] {
        if b & 0xc0 != 0x80 {
            return Utf8::Invalid;
        }
        code = (code << 6) | (b & 0x3f) as u32;
    }
    match char::from_u32(code) {
        Some(c) if code >= lowest => Utf8::Char(c, len),
        _ => Utf8::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(data: &[u8]) -> (Modifier, Key, usize) {
        match parse_event(data) {
            (Event::Key { modifier, key }, n) => (modifier, key, n),
            other => panic!("expected a key, got {other:?}"),
        }
    }

    #[test]
    fn plain_characters() {
        assert_eq!(k(b"q"), (Modifier::NONE, Key::Char('q'), 1));
        assert_eq!(k(b" "), (Modifier::NONE, Key::Space, 1));
        assert_eq!(k("ä".as_bytes()), (Modifier::NONE, Key::Char('ä'), 2));
        assert_eq!(k("🦀".as_bytes()), (Modifier::NONE, Key::Char('🦀'), 4));
        assert_eq!(parse_event(&[0xf0, 0x9f]), (Event::None, 0));
    }

    #[test]
    fn control_keys() {
        assert_eq!(k(b"\x03"), (Modifier::NONE, Key::Ctrl('c'), 1));
        assert_eq!(k(b"\r"), (Modifier::NONE, Key::Enter, 1));
        assert_eq!(k(b"\t"), (Modifier::NONE, Key::Tab, 1));
        assert_eq!(k(b"\x7f"), (Modifier::NONE, Key::Backspace2, 1));
        assert_eq!(k(b"\x1b"), (Modifier::NONE, Key::Esc, 1));
    }

    #[test]
    fn escape_sequences() {
        assert_eq!(k(b"\x1b[A"), (Modifier::NONE, Key::ArrowUp, 3));
        assert_eq!(k(b"\x1b[D"), (Modifier::NONE, Key::ArrowLeft, 3));
        assert_eq!(k(b"\x1bOP"), (Modifier::NONE, Key::F(1), 3));
        assert_eq!(k(b"\x1b[15~"), (Modifier::NONE, Key::F(5), 5));
        assert_eq!(k(b"\x1b[24~"), (Modifier::NONE, Key::F(12), 5));
        assert_eq!(k(b"\x1b[3~"), (Modifier::NONE, Key::Delete, 4));
        assert_eq!(k(b"\x1b[6~"), (Modifier::NONE, Key::PgDn, 4));
        assert_eq!(k(b"\x1b[1;3A"), (Modifier::ALT, Key::ArrowUp, 6));
        assert_eq!(k(b"\x1ba"), (Modifier::ALT, Key::Char('a'), 2));
        assert_eq!(parse_event(b"\x1b[1;"), (Event::None, 0));
    }

    #[test]
    fn mouse_sequences() {
        assert_eq!(
            parse_event(b"\x1b[<0;10;5M"),
            (
                Event::Mouse {
                    modifier: Modifier::NONE,
                    button: MouseButton::Left,
                    x: 9,
                    y: 4
                },
                10
            )
        );
        assert_eq!(
            parse_event(b"\x1b[<2;1;1m"),
            (
                Event::Mouse {
                    modifier: Modifier::NONE,
                    button: MouseButton::Release,
                    x: 0,
                    y: 0
                },
                9
            )
        );
        assert_eq!(
            parse_event(b"\x1b[<64;3;3M"),
            (
                Event::Mouse {
                    modifier: Modifier::NONE,
                    button: MouseButton::WheelUp,
                    x: 2,
                    y: 2
                },
                10
            )
        );
        assert_eq!(
            parse_event(b"\x1b[M \x21\x21"),
            (
                Event::Mouse {
                    modifier: Modifier::NONE,
                    button: MouseButton::Left,
                    x: 0,
                    y: 0
                },
                6
            )
        );
        assert_eq!(parse_event(b"\x1b[<0;1"), (Event::None, 0));
    }

    #[test]
    fn junk_is_consumed_rather_than_awaited() {
        // A zero count means "send more bytes", so a byte that can never start
        // a sequence must not report one or a draining caller spins forever.
        for bad in [
            &[0xff][..],
            &[0x80][..],
            &[0xc0, 0x80][..],
            &[0xf5, 0x80][..],
        ] {
            assert_eq!(parse_event(bad), (Event::None, 1), "{bad:x?}");
        }
        // Overlong and surrogate encodings are junk too, not short reads.
        assert_eq!(parse_event(&[0xe0, 0x80, 0x80]), (Event::None, 1));
        assert_eq!(parse_event(&[0xed, 0xa0, 0x80]), (Event::None, 1));
        // A genuine short read still asks for more.
        assert_eq!(parse_event(&[0xf0, 0x9f]), (Event::None, 0));
    }

    #[test]
    fn a_run_of_escapes_does_not_recurse() {
        // One stack frame per escape used to overflow at around 20k bytes.
        let escapes = vec![0x1b_u8; 200_000];
        assert_eq!(k(&escapes), (Modifier::ALT, Key::Esc, 200_000));

        let mut run = vec![0x1b_u8; 200_000];
        run.extend_from_slice(b"[A");
        assert_eq!(k(&run), (Modifier::ALT, Key::ArrowUp, 200_002));
    }

    #[test]
    fn a_stream_drains_cleanly() {
        let mut data: &[u8] = b"ab\x1b[Ac\xff\x1b[3~";
        let mut keys = Vec::new();
        while !data.is_empty() {
            let (event, n) = parse_event(data);
            assert!(n > 0);
            if let Some(key) = event.key() {
                keys.push(key);
            }
            data = &data[n..];
        }
        assert_eq!(
            keys,
            vec![
                Key::Char('a'),
                Key::Char('b'),
                Key::ArrowUp,
                Key::Char('c'),
                Key::Delete
            ]
        );
    }
}

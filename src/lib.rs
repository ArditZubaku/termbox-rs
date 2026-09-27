//! A [termbox-go] shaped API on top of [`rustbox`].
//!
//! [rustbox] binds the termbox C library, but its drawing surface is thin: one
//! FFI call per cell, colours that panic when mixed with the wrong output mode,
//! and no buffer of your own to work against. This crate adds the layer
//! termbox-go has — cells, attributes, events and a back buffer that knows what
//! changed — and keeps the names close enough that Go code can be transcribed
//! line by line.
//!
//! [termbox-go]: https://pkg.go.dev/github.com/nsf/termbox-go
//! [rustbox]: https://docs.rs/rustbox
//!
//! # Getting started
//!
//! ```no_run
//! use termbox_rs::{Attribute, Key, OutputMode, Termbox};
//!
//! let mut tb = Termbox::init()?;
//! tb.set_output_mode(OutputMode::EightBit);
//! tb.clear(Attribute::DEFAULT, Attribute::DEFAULT);
//! tb.print(2, 1, Attribute::byte(220) | Attribute::BOLD, Attribute::DEFAULT, "press q");
//! tb.hide_cursor();
//! tb.flush();
//!
//! loop {
//!     match tb.poll_event()?.key() {
//!         Some(Key::Char('q') | Key::Esc | Key::Ctrl('c')) => break,
//!         _ => {}
//!     }
//! }
//!
//! tb.close();
//! # Ok::<(), termbox_rs::Error>(())
//! ```
//!
//! # How drawing works
//!
//! [`Termbox`] owns a [`Cell`] buffer laid out exactly like termbox's own, and
//! every setter writes into that buffer and notes which part of the row moved.
//! [`Termbox::flush`] skips untouched rows entirely and hands each touched span
//! to termbox as a single memcpy; termbox then diffs against what is on screen
//! and writes only the cells that really changed. Redrawing a corner of the
//! screen costs one `memcpy` and a few bytes of terminal output, not a call per
//! cell.
//!
//! # Threads
//!
//! termbox tolerates one reader and one writer at the same time.
//! [`Termbox::events`] hands out an [`EventSource`] that can be moved to
//! another thread to read input while the main thread draws, and
//! [`Termbox::spawn`] starts threads that [`Termbox::close`] stops and joins
//! for you:
//!
//! ```no_run
//! # use std::sync::atomic::Ordering;
//! # use std::time::Duration;
//! # use termbox_rs::Termbox;
//! let mut tb = Termbox::init()?;
//! tb.spawn(|running| {
//!     while running.load(Ordering::Relaxed) {
//!         std::thread::sleep(Duration::from_millis(16));
//!     }
//! });
//! tb.close();
//! # Ok::<(), termbox_rs::Error>(())
//! ```
//!
//! # Coming from termbox-go
//!
//! | termbox-go | here |
//! |---|---|
//! | `Init`, `Close` | [`Termbox::init`], [`Termbox::close`] (or just drop it) |
//! | `IsInit` | [`Termbox::is_init`] |
//! | `Size` | [`Termbox::size`] |
//! | `Clear`, `Flush`, `Sync` | [`Termbox::clear`], [`Termbox::flush`], [`Termbox::sync`] |
//! | `SetCell`, `SetChar`, `SetFg`, `SetBg` | [`Termbox::set_cell`], [`Termbox::set_char`], [`Termbox::set_fg`], [`Termbox::set_bg`] |
//! | `GetCell`, `CellBuffer` | [`Termbox::get_cell`], [`Termbox::cell_buffer`] |
//! | `SetCursor`, `HideCursor` | [`Termbox::set_cursor`], [`Termbox::hide_cursor`] |
//! | `SetInputMode`, `SetOutputMode` | [`Termbox::set_input_mode`], [`Termbox::set_output_mode`] |
//! | `PollEvent`, `PollRawEvent` | [`Termbox::poll_event`], [`Termbox::poll_raw_event`] |
//! | `ParseEvent` | [`parse_event`] |
//! | `Interrupt` | [`Termbox::interrupt`] |
//! | `RGBToAttribute` | [`rgb_to_attribute`] |
//! | `Attribute`, `Cell`, `Event` | [`Attribute`], [`Cell`], [`Event`] |
//!
//! The package level functions have direct counterparts in [`global`] if you
//! would rather keep the singleton shape.
//!
//! Three things do not carry over one for one:
//!
//! * `Flush`, `Clear` and `Sync` return nothing. The C library's `tb_present`
//!   reports no errors, so there is none to pass on.
//! * `PollRawEvent` gives back a [`RawEvent`], not input bytes. The C library
//!   decodes the input stream itself and never exposes it. [`parse_event`]
//!   decodes bytes from any other source.
//! * `AttributeToRGB` is not implemented, and [`rgb_to_attribute`] quantises to
//!   the 256 colour palette, because the C library has no 24-bit colour mode.
//!
//! # Attributes
//!
//! [`Attribute`] is a colour plus style flags in one 16-bit word, and how the
//! colour bits are read depends on the [`OutputMode`]. See its documentation
//! for the ranges; the short version is that `0` always means "whatever the
//! terminal uses" and [`OutputMode::EightBit`] is what you want for
//! [`Attribute::byte`], [`Attribute::cube`], [`Attribute::gray`] and
//! [`rgb_to_attribute`].

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

mod attr;
mod cell;
mod error;
mod event;
mod parse;
mod termbox;

pub mod global;

pub use crate::attr::{Attribute, rgb_to_attribute};
pub use crate::cell::Cell;
pub use crate::error::{Error, Result};
pub use crate::event::{Event, EventKind, Key, Modifier, MouseButton, RawEvent};
pub use crate::parse::parse_event;
pub use crate::termbox::{
    EventSource, InitOptions, InputMode, OutputMode, Termbox,
};

/// The crate this is built on, re-exported so its API stays reachable without
/// a second version of it in your dependency tree.
pub use rustbox;

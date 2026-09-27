//! The package level API, for code ported straight from termbox-go.
//!
//! termbox-go keeps one terminal per process and exposes it through package
//! functions. This module does the same over a private singleton, so
//! `termbox.SetCell(x, y, ch, fg, bg)` becomes
//! `termbox_rs::global::set_cell(x, y, ch, fg, bg)`.
//!
//! Every call takes a lock, which the [`Termbox`] methods do not, so prefer
//! owning a [`Termbox`] where you can. Event polling releases the lock before
//! it blocks, so input and drawing do not deadlock against each other.
//!
//! ```no_run
//! use termbox_rs::{Attribute, Key, global as tb};
//!
//! tb::init()?;
//! tb::clear(Attribute::DEFAULT, Attribute::DEFAULT)?;
//! tb::set_cell(0, 0, '!', Attribute::RED, Attribute::DEFAULT);
//! tb::flush()?;
//! while let Ok(event) = tb::poll_event() {
//!     if event.key() == Some(Key::Esc) {
//!         break;
//!     }
//! }
//! tb::close();
//! # Ok::<(), termbox_rs::Error>(())
//! ```

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use crate::attr::Attribute;
use crate::cell::Cell;
use crate::error::{Error, Result};
use crate::event::Event;
use crate::termbox::{EventSource, InitOptions, InputMode, OutputMode, Termbox};

static TERMBOX: OnceLock<Mutex<Option<Termbox>>> = OnceLock::new();

fn slot() -> MutexGuard<'static, Option<Termbox>> {
    TERMBOX
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn with<R>(func: impl FnOnce(&mut Termbox) -> R) -> Result<R> {
    match slot().as_mut() {
        Some(tb) => Ok(func(tb)),
        None => Err(Error::NotRunning),
    }
}

/// Starts the terminal with the default options.
pub fn init() -> Result<()> {
    init_with(InitOptions::default())
}

/// Starts the terminal.
pub fn init_with(opts: InitOptions) -> Result<()> {
    let mut slot = slot();
    if slot.is_some() {
        return Err(Error::AlreadyOpen);
    }
    *slot = Some(Termbox::init_with(opts)?);
    Ok(())
}

/// Stops threads started with [`spawn`], joins them and restores the terminal.
pub fn close() {
    let tb = slot().take();
    if let Some(tb) = tb {
        tb.close();
    }
}

/// True while the terminal is running.
pub fn is_init() -> bool {
    Termbox::is_init()
}

/// Width and height in cells, or `(0, 0)` before [`init`].
pub fn size() -> (usize, usize) {
    with(|tb| tb.size()).unwrap_or((0, 0))
}

/// Fills the buffer with blanks in the given colours.
pub fn clear(fg: Attribute, bg: Attribute) -> Result<()> {
    with(|tb| tb.clear(fg, bg))
}

/// Writes a character and both colours.
pub fn set_cell(x: usize, y: usize, ch: char, fg: Attribute, bg: Attribute) {
    let _ = with(|tb| tb.set_cell(x, y, ch, fg, bg));
}

/// Writes a character, leaving the colours alone.
pub fn set_char(x: usize, y: usize, ch: char) {
    let _ = with(|tb| tb.set_char(x, y, ch));
}

/// Writes the foreground colour.
pub fn set_fg(x: usize, y: usize, fg: Attribute) {
    let _ = with(|tb| tb.set_fg(x, y, fg));
}

/// Writes the background colour.
pub fn set_bg(x: usize, y: usize, bg: Attribute) {
    let _ = with(|tb| tb.set_bg(x, y, bg));
}

/// The cell at `(x, y)`, or a blank one if that is off screen.
pub fn get_cell(x: usize, y: usize) -> Cell {
    with(|tb| tb.get_cell(x, y)).unwrap_or(Cell::BLANK)
}

/// A copy of the whole buffer, row major.
///
/// termbox-go hands out the buffer itself; behind a lock that has to be a copy.
/// Use [`with_cell_buffer_mut`] to write into it in place.
pub fn cell_buffer() -> Vec<Cell> {
    with(|tb| tb.cell_buffer().to_vec()).unwrap_or_default()
}

/// Runs `func` on the buffer itself and marks every row dirty.
pub fn with_cell_buffer_mut<R>(func: impl FnOnce(&mut [Cell]) -> R) -> Result<R> {
    with(|tb| func(tb.cell_buffer_mut()))
}

/// Writes a string along a row and returns how many cells it filled.
pub fn print(x: usize, y: usize, fg: Attribute, bg: Attribute, text: &str) -> usize {
    with(|tb| tb.print(x, y, fg, bg, text)).unwrap_or(0)
}

/// Pushes everything that changed to the terminal.
pub fn flush() -> Result<()> {
    with(|tb| tb.flush())
}

/// Repaints the whole terminal from scratch.
pub fn sync() -> Result<()> {
    with(|tb| tb.sync())
}

/// Moves the terminal cursor. Negative coordinates hide it.
pub fn set_cursor(x: i32, y: i32) {
    let _ = with(|tb| tb.set_cursor(x, y));
}

/// Hides the terminal cursor.
pub fn hide_cursor() {
    let _ = with(|tb| tb.hide_cursor());
}

/// Selects the input mode and returns the one now in effect.
pub fn set_input_mode(mode: InputMode) -> InputMode {
    with(|tb| tb.set_input_mode(mode)).unwrap_or(InputMode::Current)
}

/// Selects the output mode and returns the one now in effect.
pub fn set_output_mode(mode: OutputMode) -> OutputMode {
    with(|tb| tb.set_output_mode(mode)).unwrap_or(OutputMode::Current)
}

/// A handle for reading events, cheap to clone and safe to move to another
/// thread.
pub fn events() -> Result<EventSource> {
    with(|tb| tb.events())
}

/// Waits for the next event.
pub fn poll_event() -> Result<Event> {
    events()?.poll_event()
}

/// Waits for the next event, giving up after `timeout` with [`Event::None`].
pub fn peek_event(timeout: Duration) -> Result<Event> {
    events()?.peek_event(timeout)
}

/// Waits for the next event and returns it undecoded, as [`Event::Raw`].
pub fn poll_raw_event() -> Result<Event> {
    events()?.poll_raw_event()
}

/// Wakes a blocked poll up with [`Event::Interrupt`].
pub fn interrupt() {
    let _ = with(|tb| tb.interrupt());
}

/// The flag that [`close`] clears.
pub fn running() -> Result<Arc<AtomicBool>> {
    with(|tb| tb.running())
}

/// Starts a thread that is stopped and joined by [`close`].
pub fn spawn<F>(func: F) -> Result<()>
where
    F: FnOnce(Arc<AtomicBool>) + Send + 'static,
{
    with(|tb| tb.spawn(func))
}

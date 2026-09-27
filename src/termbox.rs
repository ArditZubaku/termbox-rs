//! The terminal handle and its back buffer.

use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustbox::RustBox;

use crate::attr::Attribute;
use crate::cell::Cell;
use crate::error::{Error, Result};
use crate::event::{self, Event, RawEvent};

const NIL_EVENT: termbox_sys::RawEvent = termbox_sys::RawEvent {
    etype: 0,
    emod: 0,
    key: 0,
    ch: 0,
    w: 0,
    h: 0,
    x: 0,
    y: 0,
};

/// How input is reported.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(i32)]
pub enum InputMode {
    /// Leave the mode alone.
    #[default]
    Current = 0,
    /// An escape byte that starts no known sequence is reported as
    /// [`Key::Esc`](crate::Key::Esc).
    Esc = 1,
    /// An escape byte that starts no known sequence sets
    /// [`Modifier::ALT`](crate::Modifier::ALT) on the next key.
    Alt = 2,
    /// [`Esc`](Self::Esc) plus mouse reporting.
    EscMouse = 5,
    /// [`Alt`](Self::Alt) plus mouse reporting.
    AltMouse = 6,
}

impl InputMode {
    /// Decodes termbox's input mode bits.
    pub fn from_bits(bits: i32) -> InputMode {
        match bits {
            1 => InputMode::Esc,
            2 => InputMode::Alt,
            5 => InputMode::EscMouse,
            6 => InputMode::AltMouse,
            _ => InputMode::Current,
        }
    }

    /// The same mode with mouse reporting turned on.
    pub fn with_mouse(self) -> InputMode {
        match self {
            InputMode::Alt | InputMode::AltMouse => InputMode::AltMouse,
            _ => InputMode::EscMouse,
        }
    }

    /// True if mouse reporting is on.
    pub fn has_mouse(self) -> bool {
        matches!(self, InputMode::EscMouse | InputMode::AltMouse)
    }
}

/// How [`Attribute`] values are turned into colours.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(i32)]
pub enum OutputMode {
    /// Leave the mode alone.
    #[default]
    Current = 0,
    /// 8 colours plus the terminal default. `termbox.OutputNormal`.
    Normal = 1,
    /// The 256 colour palette. `termbox.Output256`.
    EightBit = 2,
    /// The 216 colour cube. `termbox.Output216`.
    WebSafe = 3,
    /// The 24 step grey ramp. `termbox.OutputGrayscale`.
    Grayscale = 4,
}

impl OutputMode {
    /// Decodes termbox's output mode bits.
    pub fn from_bits(bits: i32) -> OutputMode {
        match bits {
            1 => OutputMode::Normal,
            2 => OutputMode::EightBit,
            3 => OutputMode::WebSafe,
            4 => OutputMode::Grayscale,
            _ => OutputMode::Current,
        }
    }
}

impl From<rustbox::OutputMode> for OutputMode {
    fn from(mode: rustbox::OutputMode) -> OutputMode {
        OutputMode::from_bits(mode as i32)
    }
}

impl From<rustbox::InputMode> for InputMode {
    fn from(mode: rustbox::InputMode) -> InputMode {
        InputMode::from_bits(mode as i32)
    }
}

/// How to start the terminal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InitOptions {
    /// The input mode to select once the terminal is up.
    pub input_mode: InputMode,
    /// The output mode to select once the terminal is up.
    pub output_mode: OutputMode,
    /// Buffer stderr while the terminal is running and replay it on exit, so a
    /// panic message is not painted over.
    pub buffer_stderr: bool,
    /// How often a blocked poll looks at the interrupt flag.
    ///
    /// [`Duration::ZERO`] blocks in the C library instead, which is one syscall
    /// cheaper but makes [`Termbox::interrupt`] a no-op.
    pub poll_granularity: Duration,
}

impl Default for InitOptions {
    fn default() -> InitOptions {
        InitOptions {
            input_mode: InputMode::Current,
            output_mode: OutputMode::Current,
            buffer_stderr: false,
            poll_granularity: Duration::from_millis(25),
        }
    }
}

impl InitOptions {
    /// The defaults.
    pub fn new() -> InitOptions {
        InitOptions::default()
    }

    /// Sets [`input_mode`](Self::input_mode).
    pub fn input_mode(mut self, mode: InputMode) -> InitOptions {
        self.input_mode = mode;
        self
    }

    /// Sets [`output_mode`](Self::output_mode).
    pub fn output_mode(mut self, mode: OutputMode) -> InitOptions {
        self.output_mode = mode;
        self
    }

    /// Sets [`buffer_stderr`](Self::buffer_stderr).
    pub fn buffer_stderr(mut self, buffer: bool) -> InitOptions {
        self.buffer_stderr = buffer;
        self
    }

    /// Sets [`poll_granularity`](Self::poll_granularity).
    pub fn poll_granularity(mut self, granularity: Duration) -> InitOptions {
        self.poll_granularity = granularity;
        self
    }
}

#[derive(Clone, Copy)]
struct Span {
    start: u32,
    end: u32,
}

impl Span {
    const CLEAN: Span = Span { start: 0, end: 0 };

    #[inline]
    fn touch(&mut self, x: u32) {
        if self.start >= self.end {
            self.start = x;
            self.end = x + 1;
        } else {
            if x < self.start {
                self.start = x;
            }
            if x >= self.end {
                self.end = x + 1;
            }
        }
    }

    #[inline]
    fn touch_range(&mut self, start: u32, end: u32) {
        if self.start >= self.end {
            self.start = start;
            self.end = end;
        } else {
            if start < self.start {
                self.start = start;
            }
            if end > self.end {
                self.end = end;
            }
        }
    }
}

/// A running terminal and the buffer that is drawn into it.
///
/// Everything is written into a local buffer first; [`flush`](Self::flush)
/// hands the rows that changed to termbox in one memcpy each and lets it work
/// out which cells actually have to reach the terminal. Nothing touches the
/// terminal until you flush.
///
/// Dropping this shuts the terminal back down, so the screen is restored even
/// on a panic.
///
/// ```no_run
/// use termbox_rs::{Attribute, Event, Key, OutputMode, Termbox};
///
/// let mut tb = Termbox::init()?;
/// tb.set_output_mode(OutputMode::EightBit);
/// tb.clear(Attribute::DEFAULT, Attribute::DEFAULT);
/// tb.print(0, 0, Attribute::WHITE | Attribute::BOLD, Attribute::BLUE, "hello");
/// tb.flush();
///
/// while let Ok(event) = tb.poll_event() {
///     if let Some(Key::Esc | Key::Char('q')) = event.key() {
///         break;
///     }
/// }
/// tb.close();
/// # Ok::<(), termbox_rs::Error>(())
/// ```
pub struct Termbox {
    rb: Arc<RustBox>,
    cells: Vec<Cell>,
    width: usize,
    height: usize,
    dirty: Vec<Span>,
    all_dirty: bool,
    any_dirty: bool,
    events: EventSource,
    threads: Vec<JoinHandle<()>>,
    running: Arc<AtomicBool>,
    input_mode: InputMode,
    output_mode: OutputMode,
}

impl Termbox {
    /// Starts the terminal with the default options.
    ///
    /// Fails with [`Error::AlreadyOpen`] if another [`Termbox`] is still alive
    /// in this process; termbox is a singleton.
    pub fn init() -> Result<Termbox> {
        Termbox::init_with(InitOptions::default())
    }

    /// Starts the terminal.
    pub fn init_with(opts: InitOptions) -> Result<Termbox> {
        let rb = RustBox::init(rustbox::InitOptions {
            input_mode: rustbox::InputMode::Current,
            output_mode: rustbox::OutputMode::Current,
            buffer_stderr: opts.buffer_stderr,
        })?;
        let rb = Arc::new(rb);
        let (width, height) = (rb.width(), rb.height());

        let mut tb = Termbox {
            events: EventSource {
                inner: Arc::new(EventInner {
                    rb: Arc::clone(&rb),
                    lock: Mutex::new(()),
                    interrupt: AtomicBool::new(false),
                    granularity: opts.poll_granularity,
                }),
            },
            rb,
            cells: vec![Cell::BLANK; width * height],
            width,
            height,
            dirty: vec![Span::CLEAN; height],
            all_dirty: true,
            any_dirty: true,
            threads: Vec::new(),
            running: Arc::new(AtomicBool::new(true)),
            input_mode: InputMode::Current,
            output_mode: OutputMode::Current,
        };

        tb.set_input_mode(opts.input_mode);
        tb.set_output_mode(opts.output_mode);
        Ok(tb)
    }

    /// True while a terminal is running in this process.
    ///
    /// The equivalent of termbox-go's `IsInit`.
    pub fn is_init() -> bool {
        rustbox::running()
    }

    // ---------------------------------------------------------------- modes

    /// Selects the input mode and returns the one now in effect.
    ///
    /// [`InputMode::Current`] only reads the mode back.
    pub fn set_input_mode(&mut self, mode: InputMode) -> InputMode {
        let bits = unsafe { termbox_sys::tb_select_input_mode(mode as c_int) };
        self.input_mode = InputMode::from_bits(bits);
        self.input_mode
    }

    /// Selects the output mode and returns the one now in effect.
    ///
    /// [`OutputMode::Current`] only reads the mode back. Changing the mode
    /// changes how the [`Attribute`] values already in the buffer are read, so
    /// a [`sync`](Self::sync) afterwards is usually what you want.
    pub fn set_output_mode(&mut self, mode: OutputMode) -> OutputMode {
        let bits = unsafe { termbox_sys::tb_select_output_mode(mode as c_int) };
        self.output_mode = OutputMode::from_bits(bits);
        self.output_mode
    }

    /// The input mode in effect.
    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    /// The output mode in effect.
    pub fn output_mode(&self) -> OutputMode {
        self.output_mode
    }

    // --------------------------------------------------------------- buffer

    /// Width and height of the buffer, in cells.
    #[inline]
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Width of the buffer, in cells.
    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height of the buffer, in cells.
    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Fills the whole buffer with blanks in the given colours.
    ///
    /// The colours are also what termbox paints with when the terminal grows or
    /// when [`sync`](Self::sync) repaints.
    pub fn clear(&mut self, fg: Attribute, bg: Attribute) {
        self.cells.fill(Cell::blank(fg, bg));
        unsafe { termbox_sys::tb_set_clear_attributes(fg.0, bg.0) };
        self.all_dirty = true;
        self.any_dirty = true;
    }

    /// Writes a character and both colours.
    ///
    /// Positions outside the buffer are ignored, as in termbox-go.
    #[inline]
    pub fn set_cell(&mut self, x: usize, y: usize, ch: char, fg: Attribute, bg: Attribute) {
        if x < self.width && y < self.height {
            let index = y * self.width + x;
            self.cells[index] = Cell { ch, fg, bg };
            self.dirty[y].touch(x as u32);
            self.any_dirty = true;
        }
    }

    /// Writes a character, leaving the colours alone.
    #[inline]
    pub fn set_char(&mut self, x: usize, y: usize, ch: char) {
        if x < self.width && y < self.height {
            let index = y * self.width + x;
            self.cells[index].ch = ch;
            self.dirty[y].touch(x as u32);
            self.any_dirty = true;
        }
    }

    /// Writes the foreground colour, leaving the character and background
    /// alone.
    #[inline]
    pub fn set_fg(&mut self, x: usize, y: usize, fg: Attribute) {
        if x < self.width && y < self.height {
            let index = y * self.width + x;
            self.cells[index].fg = fg;
            self.dirty[y].touch(x as u32);
            self.any_dirty = true;
        }
    }

    /// Writes the background colour, leaving the character and foreground
    /// alone.
    #[inline]
    pub fn set_bg(&mut self, x: usize, y: usize, bg: Attribute) {
        if x < self.width && y < self.height {
            let index = y * self.width + x;
            self.cells[index].bg = bg;
            self.dirty[y].touch(x as u32);
            self.any_dirty = true;
        }
    }

    /// The cell at `(x, y)`, or a blank one if that is off screen.
    #[inline]
    pub fn get_cell(&self, x: usize, y: usize) -> Cell {
        self.cell(x, y).copied().unwrap_or(Cell::BLANK)
    }

    /// The cell at `(x, y)`, if it is on screen.
    #[inline]
    pub fn cell(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.width && y < self.height {
            self.cells.get(y * self.width + x)
        } else {
            None
        }
    }

    /// The whole buffer, row major.
    #[inline]
    pub fn cell_buffer(&self) -> &[Cell] {
        &self.cells
    }

    /// The whole buffer for writing, row major.
    ///
    /// Every row is marked dirty, since there is no way to tell what you
    /// changed. Reach for this to blast a prepared frame in with
    /// `copy_from_slice`; the per-cell setters keep flushes smaller.
    #[inline]
    pub fn cell_buffer_mut(&mut self) -> &mut [Cell] {
        self.all_dirty = true;
        self.any_dirty = true;
        &mut self.cells
    }

    /// Writes a string along a row and returns how many cells it filled.
    ///
    /// Stops at the right edge. Wide characters are not measured: every
    /// [`char`] takes one cell.
    pub fn print(
        &mut self,
        x: usize,
        y: usize,
        fg: Attribute,
        bg: Attribute,
        text: &str,
    ) -> usize {
        if y >= self.height || x >= self.width {
            return 0;
        }
        let row = y * self.width;
        let mut end = x;
        for ch in text.chars() {
            if end >= self.width {
                break;
            }
            self.cells[row + end] = Cell { ch, fg, bg };
            end += 1;
        }
        if end > x {
            self.dirty[y].touch_range(x as u32, end as u32);
            self.any_dirty = true;
        }
        end - x
    }

    /// Writes a single character with both colours.
    #[inline]
    pub fn print_char(&mut self, x: usize, y: usize, fg: Attribute, bg: Attribute, ch: char) {
        self.set_cell(x, y, ch, fg, bg);
    }

    /// Fills a rectangle with one cell, clipped to the buffer.
    pub fn fill(&mut self, x: usize, y: usize, width: usize, height: usize, cell: Cell) {
        let right = (x + width).min(self.width);
        let bottom = (y + height).min(self.height);
        if x >= right || y >= bottom {
            return;
        }
        for row in y..bottom {
            let offset = row * self.width;
            self.cells[offset + x..offset + right].fill(cell);
            self.dirty[row].touch_range(x as u32, right as u32);
        }
        self.any_dirty = true;
    }

    // --------------------------------------------------------------- output

    /// Pushes everything that changed to the terminal.
    ///
    /// Rows that were not written since the last flush are skipped outright;
    /// the rest are handed over one memcpy at a time, and termbox then writes
    /// only the cells that differ from what is on screen.
    pub fn flush(&mut self) {
        self.check_resize();

        if self.all_dirty {
            unsafe {
                termbox_sys::tb_blit(
                    0,
                    0,
                    self.width as c_int,
                    self.height as c_int,
                    self.cells.as_ptr().cast(),
                );
            }
        } else if self.any_dirty {
            for y in 0..self.height {
                let span = self.dirty[y];
                if span.start >= span.end {
                    continue;
                }
                let start = span.start as usize;
                let len = (span.end - span.start) as usize;
                unsafe {
                    termbox_sys::tb_blit(
                        start as c_int,
                        y as c_int,
                        len as c_int,
                        1,
                        self.cells[y * self.width + start..].as_ptr().cast(),
                    );
                }
            }
        }

        self.mark_clean();
        self.rb.present();
    }

    /// Repaints the whole terminal from scratch.
    ///
    /// Use this when something outside the library has written to the screen.
    pub fn sync(&mut self) {
        self.check_resize();
        self.rb.clear();
        self.rb.present();
        self.all_dirty = true;
        self.any_dirty = true;
        self.flush();
    }

    /// Moves the terminal cursor. Negative coordinates hide it.
    pub fn set_cursor(&self, x: i32, y: i32) {
        self.rb.set_cursor(x as isize, y as isize);
    }

    /// Hides the terminal cursor.
    pub fn hide_cursor(&self) {
        self.rb.set_cursor(-1, -1);
    }

    /// Runs `func` with the terminal shut down, then starts it again.
    ///
    /// Handy for shelling out to an editor. The buffer survives; the screen is
    /// repainted on the way back.
    pub fn suspend<F: FnOnce()>(&mut self, func: F) {
        self.rb.suspend(func);
        self.sync();
    }

    // --------------------------------------------------------------- events

    /// Waits for the next event.
    ///
    /// Returns [`Event::Interrupt`] if [`interrupt`](Self::interrupt) is called
    /// while this is waiting.
    pub fn poll_event(&self) -> Result<Event> {
        self.events.poll_event()
    }

    /// Waits for the next event, giving up after `timeout` with
    /// [`Event::None`].
    pub fn peek_event(&self, timeout: Duration) -> Result<Event> {
        self.events.peek_event(timeout)
    }

    /// Waits for the next event without decoding it.
    ///
    /// The event comes back as [`Event::Raw`]; see [`RawEvent`] for why this
    /// is not a byte slice.
    pub fn poll_raw_event(&self) -> Result<Event> {
        self.events.poll_raw_event()
    }

    /// Wakes a blocked [`poll_event`](Self::poll_event) up with
    /// [`Event::Interrupt`].
    ///
    /// The wake up lands within one
    /// [`poll_granularity`](InitOptions::poll_granularity). If no poll is
    /// running, the next one returns immediately.
    pub fn interrupt(&self) {
        self.events.interrupt();
    }

    /// A handle for polling events from another thread.
    ///
    /// termbox tolerates one reader and one writer at a time, so this is how
    /// you read input while the buffer is being drawn into somewhere else.
    /// Handles keep the terminal alive: the terminal is not shut down while one
    /// is still around.
    pub fn events(&self) -> EventSource {
        self.events.clone()
    }

    // ------------------------------------------------------------- shutdown

    /// The flag that [`close`](Self::close) clears.
    ///
    /// Threads started with [`spawn`](Self::spawn) get a clone of this; hand it
    /// to any other thread that should stop when the terminal does.
    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    /// True until [`close`](Self::close) is called.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Starts a thread that is stopped and joined by [`close`](Self::close).
    ///
    /// The closure is handed the [`running`](Self::running) flag and is
    /// expected to return once it is false.
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use std::sync::atomic::Ordering;
    /// # use std::time::Duration;
    /// # use termbox_rs::{Attribute, Termbox};
    /// let mut tb = Termbox::init()?;
    /// let frame = Arc::new(Mutex::new(0u64));
    ///
    /// let counter = Arc::clone(&frame);
    /// tb.spawn(move |running| {
    ///     while running.load(Ordering::Relaxed) {
    ///         *counter.lock().unwrap() += 1;
    ///         std::thread::sleep(Duration::from_millis(16));
    ///     }
    /// });
    ///
    /// tb.close(); // stops the thread, joins it, restores the terminal
    /// # Ok::<(), termbox_rs::Error>(())
    /// ```
    pub fn spawn<F>(&mut self, func: F)
    where
        F: FnOnce(Arc<AtomicBool>) + Send + 'static,
    {
        let running = Arc::clone(&self.running);
        self.threads.push(thread::spawn(move || func(running)));
    }

    /// Stops and joins everything started with [`spawn`](Self::spawn), then
    /// restores the terminal.
    ///
    /// Dropping the terminal does the same thing, so this is only needed when
    /// you want the shutdown to happen at a particular point, or want to keep
    /// using the terminal afterwards.
    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.interrupt();
        for handle in self.threads.drain(..) {
            let _ = handle.join();
        }
    }

    /// The [`RustBox`] underneath, for the parts of its API this crate does not
    /// wrap.
    ///
    /// Drawing through it writes straight to termbox's buffer and is invisible
    /// to the dirty tracking here, so follow that with a [`sync`](Self::sync).
    pub fn rustbox(&self) -> &Arc<RustBox> {
        &self.rb
    }

    // -------------------------------------------------------------- private

    fn mark_clean(&mut self) {
        if self.any_dirty {
            self.dirty.fill(Span::CLEAN);
            self.any_dirty = false;
        }
        self.all_dirty = false;
    }

    fn check_resize(&mut self) {
        let (width, height) = (self.rb.width(), self.rb.height());
        if width != self.width || height != self.height {
            self.resize_to(width, height);
        }
    }

    fn resize_to(&mut self, width: usize, height: usize) {
        let mut cells = vec![Cell::BLANK; width * height];
        let rows = height.min(self.height);
        let cols = width.min(self.width);
        for y in 0..rows {
            let src = y * self.width;
            let dst = y * width;
            cells[dst..dst + cols].copy_from_slice(&self.cells[src..src + cols]);
        }
        self.cells = cells;
        self.width = width;
        self.height = height;
        self.dirty = vec![Span::CLEAN; height];
        self.all_dirty = true;
        self.any_dirty = true;
    }
}

impl std::fmt::Debug for Termbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Termbox")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("input_mode", &self.input_mode)
            .field("output_mode", &self.output_mode)
            .field("threads", &self.threads.len())
            .finish_non_exhaustive()
    }
}

impl Drop for Termbox {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct EventInner {
    // Keeps termbox alive for as long as anything can still read from it.
    #[allow(dead_code)]
    rb: Arc<RustBox>,
    lock: Mutex<()>,
    interrupt: AtomicBool,
    granularity: Duration,
}

/// A handle for reading events, cheap to clone and safe to move to another
/// thread.
///
/// Reads are serialised against each other, so several of these can exist
/// without stepping on one another.
#[derive(Clone)]
pub struct EventSource {
    inner: Arc<EventInner>,
}

impl EventSource {
    /// Waits for the next event.
    pub fn poll_event(&self) -> Result<Event> {
        let _guard = self.lock();
        if !rustbox::running() {
            return Err(Error::NotRunning);
        }

        let granularity = self.granularity_ms();
        loop {
            if self.take_interrupt() {
                return Ok(Event::Interrupt);
            }
            if granularity == 0 {
                let mut raw = NIL_EVENT;
                let rc = unsafe { termbox_sys::tb_poll_event(&mut raw) };
                return event::unpack(rc, raw);
            }
            let mut raw = NIL_EVENT;
            let rc = unsafe { termbox_sys::tb_peek_event(&mut raw, granularity) };
            if rc == 0 {
                continue;
            }
            return event::unpack(rc, raw);
        }
    }

    /// Waits for the next event, giving up after `timeout` with
    /// [`Event::None`].
    pub fn peek_event(&self, timeout: Duration) -> Result<Event> {
        let _guard = self.lock();
        if !rustbox::running() {
            return Err(Error::NotRunning);
        }

        let deadline = Instant::now() + timeout;
        let granularity = self.granularity_ms();
        loop {
            if self.take_interrupt() {
                return Ok(Event::Interrupt);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(Event::None);
            }
            let slice = match granularity {
                0 => left,
                ms => left.min(Duration::from_millis(ms as u64)),
            };
            let mut raw = NIL_EVENT;
            let rc = unsafe {
                termbox_sys::tb_peek_event(&mut raw, slice.as_millis().max(1) as c_int)
            };
            if rc == 0 {
                continue;
            }
            return event::unpack(rc, raw);
        }
    }

    /// Waits for the next event and returns it undecoded, as
    /// [`Event::Raw`].
    pub fn poll_raw_event(&self) -> Result<Event> {
        let _guard = self.lock();
        if !rustbox::running() {
            return Err(Error::NotRunning);
        }

        let granularity = self.granularity_ms();
        loop {
            if self.take_interrupt() {
                return Ok(Event::Interrupt);
            }
            let mut raw = NIL_EVENT;
            let rc = if granularity == 0 {
                unsafe { termbox_sys::tb_poll_event(&mut raw) }
            } else {
                unsafe { termbox_sys::tb_peek_event(&mut raw, granularity) }
            };
            match rc {
                0 if granularity != 0 => continue,
                0 => return Ok(Event::None),
                n if n < 0 => return Err(Error::Event(n as isize)),
                _ => return Ok(Event::Raw(RawEvent::from(raw))),
            }
        }
    }

    /// Makes the running or next poll return [`Event::Interrupt`].
    pub fn interrupt(&self) {
        self.inner.interrupt.store(true, Ordering::Release);
    }

    fn take_interrupt(&self) -> bool {
        self.inner.interrupt.swap(false, Ordering::AcqRel)
    }

    fn granularity_ms(&self) -> c_int {
        self.inner.granularity.as_millis().min(c_int::MAX as u128) as c_int
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.inner.lock.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl std::fmt::Debug for EventSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSource")
            .field("granularity", &self.inner.granularity)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_grow_to_cover_what_was_touched() {
        let mut span = Span::CLEAN;
        assert!(span.start >= span.end);

        span.touch(5);
        assert_eq!((span.start, span.end), (5, 6));

        span.touch(2);
        assert_eq!((span.start, span.end), (2, 6));

        span.touch(9);
        assert_eq!((span.start, span.end), (2, 10));

        span.touch(4);
        assert_eq!((span.start, span.end), (2, 10));
    }

    #[test]
    fn ranges_merge_into_a_span() {
        let mut span = Span::CLEAN;
        span.touch_range(4, 8);
        assert_eq!((span.start, span.end), (4, 8));

        span.touch_range(1, 3);
        assert_eq!((span.start, span.end), (1, 8));

        span.touch_range(20, 24);
        assert_eq!((span.start, span.end), (1, 24));

        span.touch(0);
        assert_eq!((span.start, span.end), (0, 24));
    }

    #[test]
    fn modes_decode_from_termbox_bits() {
        assert_eq!(InputMode::from_bits(6), InputMode::AltMouse);
        assert_eq!(InputMode::from_bits(99), InputMode::Current);
        assert_eq!(InputMode::Alt.with_mouse(), InputMode::AltMouse);
        assert_eq!(InputMode::Esc.with_mouse(), InputMode::EscMouse);
        assert!(InputMode::EscMouse.has_mouse());
        assert!(!InputMode::Esc.has_mouse());
        assert_eq!(OutputMode::from_bits(2), OutputMode::EightBit);
        assert_eq!(OutputMode::from_bits(0), OutputMode::Current);
    }
}

# termbox-rs

A [termbox-go](https://pkg.go.dev/github.com/nsf/termbox-go) shaped API on top of
[rustbox](https://docs.rs/crate/rustbox/latest).

rustbox binds the termbox C library, but its drawing surface is thin: one FFI
call per cell, colours that panic when mixed with the wrong output mode, and no
buffer of your own to work against. This crate adds the layer termbox-go has —
cells, attributes, events and a back buffer that knows what changed — and keeps
the names close enough that Go code can be transcribed line by line.

```toml
[dependencies]
termbox-rs = "0.1"
```

```rust
use termbox_rs::{Attribute, Key, OutputMode, Termbox};

let mut tb = Termbox::init()?;
tb.set_output_mode(OutputMode::EightBit);

tb.clear(Attribute::DEFAULT, Attribute::DEFAULT);
tb.print(2, 1, Attribute::byte(220) | Attribute::BOLD, Attribute::DEFAULT, "press q");
tb.set_cell(0, 0, '#', Attribute::RED, Attribute::byte(236));
tb.hide_cursor();
tb.flush();

loop {
    match tb.poll_event()?.key() {
        Some(Key::Char('q') | Key::Esc | Key::Ctrl('c')) => break,
        _ => {}
    }
}

tb.close();
```

`cargo run --example demo` runs a threaded demo: a background thread recolours
the screen while the main thread handles input.

## What it does

* **`SetCell`, `SetChar`, `SetFg`, `SetBg`, `Clear`, `Flush`, `Sync`,
  `CellBuffer`** and the rest of the termbox-go surface, as methods on
  `Termbox` or as package-level functions in `termbox_rs::global`.
* **Flushes that stay small.** Every setter notes the span of the row it
  touched. `flush()` skips untouched rows outright and hands each touched span
  to termbox as a single `memcpy`; termbox then writes only the cells that
  differ from what is on screen. Repainting a corner costs one memcpy and a few
  bytes of terminal output, not a call per cell.
* **8-byte cells**, laid out exactly like termbox's own, so those memcpys need
  no conversion.
* **Attributes that do not panic.** `Attribute` is a colour plus style flags in
  one 16-bit word, with named colours, `byte()` for the 256 palette, `cube()`,
  `gray()` and `rgb_to_attribute()`.
* **Real events.** A `Key` enum covering everything termbox reports, modifier
  bits, mouse buttons, resize, and an `Interrupt` that wakes a blocked poll.
* **Threads that shut down cleanly.** `events()` hands out a pollable handle for
  another thread, `spawn()` starts threads tied to the terminal's lifetime, and
  `close()` stops them, joins them and restores the screen — which dropping the
  `Termbox` does too, so a panic still leaves a usable terminal.
* **Resize without ceremony.** The buffer follows the terminal, keeping what
  still fits.

## Coming from termbox-go

| termbox-go | here |
| --- | --- |
| `Init`, `Close` | `Termbox::init`, `Termbox::close` (or just drop it) |
| `IsInit` | `Termbox::is_init` |
| `Size` | `Termbox::size` |
| `Clear`, `Flush`, `Sync` | `Termbox::clear`, `Termbox::flush`, `Termbox::sync` |
| `SetCell`, `SetChar`, `SetFg`, `SetBg` | `Termbox::set_cell`, `set_char`, `set_fg`, `set_bg` |
| `GetCell`, `CellBuffer` | `Termbox::get_cell`, `Termbox::cell_buffer` |
| `SetCursor`, `HideCursor` | `Termbox::set_cursor`, `Termbox::hide_cursor` |
| `SetInputMode`, `SetOutputMode` | `Termbox::set_input_mode`, `set_output_mode` |
| `PollEvent`, `PollRawEvent` | `Termbox::poll_event`, `poll_raw_event` |
| `ParseEvent` | `parse_event` |
| `Interrupt` | `Termbox::interrupt` |
| `RGBToAttribute` | `rgb_to_attribute` |
| `Attribute`, `Cell`, `Event` | `Attribute`, `Cell`, `Event` |

Three things do not carry over one for one:

* `Flush`, `Clear` and `Sync` return nothing. The C library's `tb_present`
  reports no errors, so there is none to pass on.
* `PollRawEvent` gives back a `RawEvent`, not input bytes: the C library decodes
  the input stream itself and never exposes it. `parse_event` decodes bytes from
  any other source — a pty, a recorded session — and is a complete parser for
  UTF-8, control keys, CSI and SS3 sequences, and both mouse protocols.
* `AttributeToRGB` is not implemented, and `rgb_to_attribute` quantises to the
  256 colour palette, because the C library has no 24-bit colour mode.

## Building

`rustbox` pulls in `termbox-sys`, whose build script clones the termbox C
sources and builds them with `waf`. That needs **git** and a **Python 3.10 or
older** on `PATH` — waf 2.0.14 uses `imp` and `open(..., 'rU')`, both gone in
newer Pythons. With asdf, the version is resolved from the build script's own
directory, so pin it for the whole build:

```sh
ASDF_PYTHON_VERSION=3.10.21 cargo build
```

## License

MIT.

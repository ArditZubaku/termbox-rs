//! The letter storm: a background thread recolours the field once a second
//! while the main thread handles input.
//!
//! Run it with `cargo run --example demo`, then use `h`/`l` to cycle the
//! background colour and `q` to leave.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use termbox_rs::{Attribute, Event, Key, OutputMode, Termbox};

const LETTERS: [char; 7] = ['o', 'x', 'i', 'n', 'u', 's', ' '];
const TITLE: &str = " [h/l]: change bg | [q/esc]: quit ";

/// xorshift64, so the example needs no dependencies of its own.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

fn main() -> termbox_rs::Result<()> {
    let mut tb = Termbox::init()?;
    tb.set_output_mode(OutputMode::EightBit);

    let mut rng = Rng(0x2545f4914f6cdd1d);
    let (width, height) = tb.size();

    tb.clear(Attribute::DEFAULT, Attribute::DEFAULT);
    for y in 0..height {
        for x in 0..width {
            tb.set_char(x, y, LETTERS[rng.below(LETTERS.len() as u64) as usize]);
        }
    }
    tb.print(0, 0, Attribute::byte(15), Attribute::byte(4), TITLE);
    tb.hide_cursor();
    tb.flush();

    // The event source is what lets the main thread read input while the
    // animation thread holds the drawing lock.
    let events = tb.events();
    let bg = Arc::new(Mutex::new(Attribute::DEFAULT));
    let screen = Arc::new(Mutex::new(tb));

    // `running()` is the flag `close()` clears; the animation thread watches it
    // so the two shut down together.
    let running = screen.lock().unwrap().running();
    let animated = Arc::clone(&screen);
    let animated_bg = Arc::clone(&bg);
    let animated_running = Arc::clone(&running);
    let animation = std::thread::spawn(move || {
        let mut rng = Rng(0x9e3779b97f4a7c15);
        while animated_running.load(Ordering::Relaxed) {
            {
                let mut tb = animated.lock().unwrap();
                let (width, height) = tb.size();
                let bg = *animated_bg.lock().unwrap();
                for y in 1..height {
                    for x in 0..width {
                        tb.set_fg(x, y, Attribute::byte(rng.below(9) as u8));
                        tb.set_bg(x, y, bg);
                    }
                }
                tb.flush();
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    let mut color: i16 = 0;
    loop {
        let event = events.poll_event()?;
        let Event::Key { key, .. } = event else {
            if let Event::Resize { .. } = event {
                screen.lock().unwrap().sync();
            }
            continue;
        };

        match key {
            Key::Char('q') | Key::Esc | Key::Ctrl('c') => break,
            Key::Char('h') | Key::ArrowLeft => color -= 1,
            Key::Char('l') | Key::ArrowRight => color += 1,
            _ => continue,
        }

        color = color.rem_euclid(9);
        let next = Attribute::byte(color as u8);
        *bg.lock().unwrap() = next;

        let mut tb = screen.lock().unwrap();
        let (width, height) = tb.size();
        for y in 1..height {
            for x in 0..width {
                tb.set_bg(x, y, next);
            }
        }
        tb.flush();
    }

    // Graceful exit: stop the animation thread, join it, then restore the
    // terminal.
    running.store(false, Ordering::Relaxed);
    let _ = animation.join();
    Arc::into_inner(screen)
        .expect("the animation thread held the only other handle")
        .into_inner()
        .unwrap()
        .close();
    Ok(())
}

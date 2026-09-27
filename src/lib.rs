//! A [termbox-go] shaped API on top of [`rustbox`].
//!
//! [termbox-go]: https://pkg.go.dev/github.com/nsf/termbox-go
//! [rustbox]: https://docs.rs/rustbox

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]


mod attr;
mod cell;
mod error;
mod event;
mod parse;

pub use crate::attr::{Attribute, rgb_to_attribute};
pub use crate::cell::Cell;
pub use crate::error::{Error, Result};
pub use crate::event::{Event, EventKind, Key, Modifier, MouseButton, RawEvent};
pub use crate::parse::parse_event;

/// The crate this is built on, re-exported so its API stays reachable without
/// a second version of it in your dependency tree.
pub use rustbox;

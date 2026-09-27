//! A [termbox-go] shaped API on top of [`rustbox`].
//!
//! [termbox-go]: https://pkg.go.dev/github.com/nsf/termbox-go
//! [rustbox]: https://docs.rs/rustbox

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]


mod attr;

pub use crate::attr::{Attribute, rgb_to_attribute};

/// The crate this is built on, re-exported so its API stays reachable without
/// a second version of it in your dependency tree.
pub use rustbox;

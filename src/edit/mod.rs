//! The editing core: selection, transactions, history and commands.
//!
//! Everything here works on the [`Document`](crate::Document) model without a
//! window, so it can be tested without GPUI.

pub(crate) mod selection;

pub(crate) use selection::Selection;

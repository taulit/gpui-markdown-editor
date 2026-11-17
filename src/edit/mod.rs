//! The editing core: selection, transactions, history and commands.
//!
//! Everything here works on the [`Document`](crate::Document) model without a
//! window, so it can be tested without GPUI. [`EditorState`] is the entry
//! point; the view feeds it keystrokes, IME events and commands and reads
//! back the document, the selection and what changed.

// The view wires this API to keyboard, mouse and IME next; until then much
// of it is only reached from tests.
#![allow(dead_code)]

pub(crate) mod commands;
pub(crate) mod history;
pub(crate) mod ime;
pub(crate) mod inline_ops;
pub(crate) mod input_rules;
pub(crate) mod ops;
pub(crate) mod order;
pub(crate) mod selection;
pub(crate) mod state;
pub(crate) mod step;
pub(crate) mod transaction;

#[cfg(test)]
mod tests;

pub(crate) use selection::Selection;
#[allow(unused_imports)] // the view's entry points, wired next
pub(crate) use state::{Changes, EditorState, Unit};

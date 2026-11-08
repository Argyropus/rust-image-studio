//! UI language (English default, Korean). The choice is a process-wide flag so any
//! thread (UI, engine worker, captioner) can pick its strings without passing it around.
//!
//! `t!("한국어", "English")` picks a `&'static str`; `tf!("…{x}…", "…{x}…", args…)` is
//! `format!` with one format string per language (inline `{x}` captures work in both).

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(true);

/// True when the UI is in English.
pub fn en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::Relaxed);
}

/// Picks the Korean or English text of a pair (for tables of `(ko, en)`).
pub fn pick<'a>(pair: &(&'a str, &'a str)) -> &'a str {
    if en() { pair.1 } else { pair.0 }
}

macro_rules! t {
    ($ko:literal, $en:literal $(,)?) => {
        if $crate::i18n::en() { $en } else { $ko }
    };
}

macro_rules! tf {
    ($ko:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::en() { format!($en $(, $arg)*) } else { format!($ko $(, $arg)*) }
    };
}

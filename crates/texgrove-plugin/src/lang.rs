//! German or English, as Meshgrove speaks. File names stay German or English as they were
//! made; only what the panel shows follows the language.

use std::cell::Cell;

thread_local! {
    static GERMAN: Cell<bool> = const { Cell::new(true) };
}

/// Takes the language from what Meshgrove says of itself ("de", "en", …).
pub fn set(language: &str) {
    GERMAN.with(|g| g.set(language.starts_with("de")));
}

pub fn german() -> bool {
    GERMAN.with(Cell::get)
}

/// The German or the English text.
pub fn tr(de: &'static str, en: &'static str) -> &'static str {
    if german() { de } else { en }
}

/// `format!` with a German and an English pattern side by side.
#[macro_export]
macro_rules! tf {
    ($de:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::lang::german() { format!($de $(, $arg)*) } else { format!($en $(, $arg)*) }
    };
}

/// A number with a decimal comma in German.
pub fn number(value: f32, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if german() { text.replace('.', ",") } else { text }
}

/// A count with thousands set apart: 1 000 000 in German, 1,000,000 in English.
pub fn count(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(if german() { '\u{202f}' } else { ',' });
        }
        out.push(c);
    }
    out
}

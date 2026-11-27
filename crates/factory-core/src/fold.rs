//! The fold of SPEC VAL-5d: which spellings of a path are one name.
//!
//! Two strings have the same fold exactly when one is a canonical caseless
//! match for the other (Unicode Standard 18.0.0, D145). The matcher of VAL-5b
//! folds a pattern and a path with it, and capture folds the paths of a tree
//! with it (WS-12, WS-13); this module is the one place it is written.
//!
//! The tables are those compiled into `icu_casemap` and `icu_normalizer` at
//! the exact pin of SPEC 19.2, which are those of Unicode 18.0.0. A change of
//! the pin can change which names are one.

use icu_casemap::CaseMapper;
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};

/// Returns toNFC(toCasefold(toNFD(`name`))): canonical decomposition, full
/// case folding by the default mappings, with no rule of a single language,
/// then canonical composition (VAL-5d).
///
/// ```
/// use factory_core::fold::fold;
///
/// assert_eq!(fold("STRA\u{DF}E"), "strasse");
/// assert_eq!(fold("AGENT\u{17F}.md"), fold("agents.MD"));
/// assert_ne!(fold("\u{FF41}"), fold("a"));
/// ```
#[must_use]
pub fn fold(name: &str) -> String {
    let decomposed = DecomposingNormalizerBorrowed::new_nfd().normalize(name);
    let folded = CaseMapper::new().fold_string(&decomposed);
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(&folded)
        .into_owned()
}

/// Folds a name that may not be valid UTF-8: each ill-formed sequence is
/// replaced by U+FFFD first, and the result is folded as [`fold`] does
/// (VAL-5d, WS-12).
///
/// ```
/// use factory_core::fold::fold_bytes;
///
/// assert_eq!(fold_bytes(b"A\xFFB"), "a\u{FFFD}b");
/// ```
#[must_use]
pub fn fold_bytes(name: &[u8]) -> String {
    fold(&String::from_utf8_lossy(name))
}

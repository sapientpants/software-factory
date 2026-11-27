//! The fold of SPEC VAL-5d: the pairs of probe P-29 and the examples of
//! VAL-5d as tables, and the property of canonical equivalence (19.4).

use factory_core::fold::{fold, fold_bytes};
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use proptest::prelude::*;

/// The 21 pairs that APFS was asked about in probe P-29, with its answer:
/// whether it took the two names for one. The fold gives the same answer for
/// each (`docs/research/probes.md`, P-29).
const P29_APFS: &[(&str, &str, &str, bool)] = &[
    ("ASCII case", "AGENTS.md", "agents.MD", true),
    ("Latin-1 case", "\u{C9}.md", "\u{E9}.md", true),
    ("NFC and NFD", "\u{E9}.md", "e\u{301}.md", true),
    ("Kelvin sign", "workflows", "wor\u{212A}flows", true),
    ("Long s", "AGENTS.md", "AGENT\u{17F}.md", true),
    ("Sharp s, full folding", "stra\u{DF}e", "STRASSE", true),
    ("Capital sharp s", "stra\u{DF}e", "STRA\u{1E9E}E", true),
    ("Final sigma", "\u{3C3}", "\u{3C2}", true),
    ("Capital sigma and final sigma", "\u{3A3}", "\u{3C2}", true),
    ("Titlecase digraph", "\u{1C6}", "\u{1C5}", true),
    ("Ligature, full folding", "fi", "\u{FB01}", true),
    (
        "Iota subscript, full folding",
        "\u{1F80}",
        "\u{1F00}\u{3B9}",
        true,
    ),
    ("Ypogegrammeni", "\u{345}x", "\u{3B9}x", true),
    ("Cherokee", "\u{13A0}", "\u{AB70}", true),
    ("Dotted capital I", "i", "\u{130}", false),
    ("Dotless i", "I", "\u{131}", false),
    ("Fullwidth letter", "a", "\u{FF41}", false),
    ("Zero-width non-joiner", ".git2", ".g\u{200C}it2", false),
    ("Byte order mark", ".git3", ".git3\u{FEFF}", false),
    ("Trailing dot", "name", "name.", false),
    ("Trailing space", "name2", "name2 ", false),
];

#[test]
fn the_pairs_of_p29_fold_as_apfs_compared_them() {
    let wrong: Vec<&str> = P29_APFS
        .iter()
        .filter(|(_, first, second, one)| (fold(first) == fold(second)) != *one)
        .map(|(pair, ..)| *pair)
        .collect();
    assert!(wrong.is_empty(), "pairs folded against P-29: {wrong:?}");
}

/// The pairs of the table of VAL-5d whose pattern holds no wildcard: whether
/// the two have one fold. The row of `stra?e` is the matcher's (M1a).
const VAL_5D: &[(&str, &str, bool)] = &[
    ("AGENTS.md", "agents.MD", true),
    ("AGENTS.md", "AGENT\u{17F}.md", true),
    (".github", ".GitHub", true),
    ("stra\u{DF}e", "STRASSE", true),
    ("\u{E9}.md", "E\u{301}.md", true),
    ("fi", "\u{FB01}", true),
    ("a", "\u{FF41}", false),
    ("i", "\u{130}", false),
];

#[test]
fn the_examples_of_val_5d_fold_as_the_table_says() {
    let wrong: Vec<(&str, &str)> = VAL_5D
        .iter()
        .filter(|(pattern, path, one)| (fold(pattern) == fold(path)) != *one)
        .map(|(pattern, path, _)| (*pattern, *path))
        .collect();
    assert!(
        wrong.is_empty(),
        "examples folded against VAL-5d: {wrong:?}"
    );
}

/// The fold of `stra?e` is the matcher's question, but the folded form that
/// its row names is the fold's: `strasse`.
#[test]
fn sharp_s_folds_to_two_letters() {
    assert_eq!(fold("stra\u{DF}e"), "strasse");
}

/// The composition at the end: `é` is one code point after the fold, however
/// it was written, so that `?` of VAL-5c stands for it.
#[test]
fn the_fold_is_composed() {
    assert_eq!(fold("E\u{301}"), "\u{E9}");
    assert_eq!(fold("\u{C9}"), "\u{E9}");
}

/// A ypogegrammeni written before an accent folds as the composed letter
/// U+1FB4 does, the case of the sketch of P-29.
#[test]
fn a_ypogegrammeni_before_an_accent_folds_as_the_composed_letter() {
    assert_eq!(fold("\u{3B1}\u{345}\u{301}"), fold("\u{1FB4}"));
}

#[test]
fn ill_formed_utf8_folds_with_a_replacement_character() {
    assert_eq!(fold_bytes(b"A\xFFB"), "a\u{FFFD}b");
    assert_eq!(fold_bytes(b"A\xFFB"), fold_bytes(b"a\xFEb"));
    assert_eq!(fold_bytes("STRA\u{DF}E".as_bytes()), "strasse");
}

/// Characters among which canonical equivalence is common: Latin letters,
/// combining marks of several classes, Greek, Hangul and the singletons that
/// decompose to another letter.
fn spelling() -> impl Strategy<Value = String> {
    let pieces = prop_oneof![
        prop::char::range('A', 'z'),
        prop::char::range('\u{C0}', '\u{17F}'),
        prop::char::range('\u{300}', '\u{36F}'),
        prop::char::range('\u{370}', '\u{3FF}'),
        prop::char::range('\u{1E00}', '\u{1FFF}'),
        prop::char::range('\u{AC00}', '\u{AC40}'),
        prop::char::range('\u{1100}', '\u{1112}'),
        prop::char::range('\u{1161}', '\u{1175}'),
        Just('\u{212A}'),
        Just('\u{212B}'),
        Just('\u{2126}'),
        Just('\u{FB01}'),
        Just('/'),
        Just('.'),
        any::<char>(),
    ];
    prop::collection::vec(pieces, 0..12).prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    /// Two canonically equivalent spellings of a string have the same fold
    /// (19.4). Its NFD and its NFC are each canonically equivalent to it.
    #[test]
    fn canonically_equivalent_spellings_have_one_fold(name in spelling()) {
        let nfd = DecomposingNormalizerBorrowed::new_nfd().normalize(&name);
        let nfc = ComposingNormalizerBorrowed::new_nfc().normalize(&name);
        let folded = fold(&name);
        prop_assert_eq!(&fold(&nfd), &folded);
        prop_assert_eq!(&fold(&nfc), &folded);
    }

    /// Folding twice changes nothing: a folded name is its own fold.
    #[test]
    fn the_fold_is_idempotent(name in spelling()) {
        let folded = fold(&name);
        prop_assert_eq!(fold(&folded), folded);
    }
}

//! How globset 0.4.21 reads a pattern, and what the case folding of `caseless`
//! and of ICU4X gives for a set of names. Throwaway code for probe P-29.

use globset::GlobBuilder;
use unicode_normalization::UnicodeNormalization;

fn m(pat: &str, path: &str, lit: bool, ci: bool) -> String {
    match GlobBuilder::new(pat).literal_separator(lit).case_insensitive(ci).backslash_escape(true).build() {
        Ok(g) => format!("{}", g.compile_matcher().is_match(path)),
        Err(e) => format!("ERR({})", e.kind()),
    }
}

fn main() {
    println!("unicode-normalization UNICODE_VERSION = {:?}", unicode_normalization::UNICODE_VERSION);
    println!("std UNICODE_VERSION = {:?}", std::char::UNICODE_VERSION);
    println!("\n== grammar (literal_separator=true | false), case-sensitive");
    let cases: &[(&str, &str)] = &[
        ("**/*.snap", "a.snap"), ("**/*.snap", "x/y/a.snap"), ("*.snap", "x/a.snap"),
        ("crates/**", "crates"), ("crates/**", "crates/"), ("crates/**", "crates/a.rs"), ("crates/**", "crates/a/b.rs"),
        ("a/**/b", "a/b"), ("a/**/b", "a/x/y/b"),
        ("**", "a/b"), ("**", ""),
        ("*", "a/b"), ("a?b", "a/b"), ("a?b", "axb"), ("a[/]b", "a/b"), ("a[!x]b", "a/b"),
        ("a**b", "axxb"), ("a**b", "ax/xb"), ("a/**b", "a/x/yb"), ("**a", "x/ya"), ("a**", "a/x"),
        ("[a-c].rs", "b.rs"), ("[!a-c].rs", "d.rs"), ("[^a-c].rs", "d.rs"), ("[]a].rs", "].rs"),
        ("{a,b}.rs", "a.rs"), ("{a,b{c,d}}.rs", "bd.rs"), ("{a,}.rs", ".rs"), ("x{a,b/c}", "xb/c"),
        ("\\*.rs", "*.rs"), ("\\*.rs", "a.rs"), ("a\\", "a\\"),
        ("", ""), ("", "a"), ("/", "/"), ("/a", "a"), ("/a", "/a"),
        ("[a", "a"), ("{a", "a"), ("a]", "a]"), ("a}", "a}"), ("[z-a]", "b"), ("[]", "a"), ("[!]", "a"),
        ("migrations", "migrations/001.sql"), ("migrations/", "migrations/001.sql"), ("migrations/", "migrations/"),
        ("*.rs", ".rs"), ("*", ".hidden"), ("a/*", "a/.hidden"), ("?", "é"), ("?", "e\u{301}"),
        ("a//b", "a/b"), ("a/./b", "a/b"),
    ];
    for (p, s) in cases {
        println!("{:16} vs {:18} -> {:28} | {}", format!("{p:?}"), format!("{s:?}"), m(p, s, true, false), m(p, s, false, false));
    }
    println!("\n== case_insensitive(true), literal_separator(true)");
    let ci: &[(&str, &str)] = &[
        ("AGENTS.md", "agents.MD"), ("straße", "STRASSE"), ("strasse", "straße"), ("ß", "ẞ"),
        ("k", "\u{212A}"), ("s", "ſ"), ("i", "İ"), ("I", "ı"), ("ǆ", "ǅ"), ("σ", "ς"), ("Σ", "ς"),
        ("é", "É"), ("é", "e\u{301}"), ("É", "e\u{301}"), ("ﬁ", "fi"), ("ａ", "a"), ("ΐ", "\u{1FD3}"),
        ("[a-c]", "B"), ("[!a-c]", "B"), (".git", ".G\u{200C}IT"), ("workflows", "wor\u{212A}flows"), ("AGENTS.md", "AGENT\u{17F}.md"),
    ];
    for (p, s) in ci {
        println!("{:14} vs {:14} -> {}", format!("{p:?}"), format!("{s:?}"), m(p, s, true, true));
    }
    println!("\n== folds");
    let cm = icu_casemap::CaseMapper::new();
    let nfc = icu_normalizer::ComposingNormalizerBorrowed::new_nfc();
    let nfkc = icu_normalizer::ComposingNormalizerBorrowed::new_nfkc();
    for s in ["AGENTS.md", "Straße", "ẞ", "\u{212A}", "ſ", "İ", "ı", "ǅ", "ς", "É", "e\u{301}", "ﬁ", "ａ", ".G\u{200C}IT", "\u{1FD3}", "ΐ", "\u{0345}", "ᾈ"] {
        let c: String = caseless::default_case_fold_str(s);
        let cc: String = caseless::default_case_fold_str(&s.nfd().collect::<String>()).nfd().collect::<String>().nfc().collect();
        let icu_full = cm.fold_string(s).into_owned();
        let n1 = nfc.normalize(s).into_owned();
        let f1 = cm.fold_string(&n1).into_owned();
        let icu_canon = nfc.normalize(&f1).into_owned();
        let k1 = nfkc.normalize(s).into_owned();
        let k2 = cm.fold_string(&k1).into_owned();
        let k3 = nfkc.normalize(&k2).into_owned();
        let k4 = cm.fold_string(&k3).into_owned();
        let icu_compat = nfkc.normalize(&k4).into_owned();
        println!("{:14} caseless.fold={:10} caseless.canon={:10} icu.fold={:10} icu.canon={:10} icu.compat={:?}", format!("{s:?}"), format!("{c:?}"), format!("{cc:?}"), format!("{:?}", icu_full), format!("{:?}", icu_canon), icu_compat);
    }
    println!("canonical_caseless_match(straße, STRASSE) = {}", caseless::canonical_caseless_match_str("straße", "STRASSE"));
    println!("compat_caseless_match(.git, .G<ZWNJ>IT) = {}", caseless::compatibility_caseless_match_str(".git", ".G\u{200C}IT"));
}

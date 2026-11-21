//! A sketch of the matcher that D-29 decided, to check the examples the
//! specification gives for VAL-5b to VAL-5d. Throwaway code for probe P-29.

use icu_casemap::CaseMapper;
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};

/// toNFC(toCasefold(toNFD(s))): Unicode's canonical caseless key (D145),
/// composed at the end so that `?` meets one code point where there is one.
fn key(s: &str) -> String {
    let nfd = DecomposingNormalizerBorrowed::new_nfd().normalize(s).into_owned();
    let folded = CaseMapper::new().fold_string(&nfd).into_owned();
    ComposingNormalizerBorrowed::new_nfc().normalize(&folded).into_owned()
}

struct Pattern {
    anchored: bool,
    components: Vec<String>,
}

fn parse(pattern: &str) -> Result<Pattern, &'static str> {
    let folded = key(pattern);
    let (anchored, rest) = match folded.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, folded.as_str()),
    };
    if rest.is_empty() {
        return Err("names nothing");
    }
    if rest.ends_with('/') {
        return Err("ends in '/'; write '/**' for everything below a directory");
    }
    let mut components = Vec::new();
    for c in rest.split('/') {
        if c.is_empty() {
            return Err("empty component");
        }
        if c == "." || c == ".." {
            return Err("'.' or '..' component");
        }
        if c.contains(['[', ']', '{', '}', '\\']) {
            return Err("reserved character");
        }
        if c.contains("**") && c != "**" {
            return Err("'**' is not a whole component");
        }
        components.push(c.to_owned());
    }
    Ok(Pattern { anchored, components })
}

fn component(p: &[char], s: &[char]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some(('*', rest)) => (0..=s.len()).any(|i| component(rest, &s[i..])),
        Some(('?', rest)) => !s.is_empty() && component(rest, &s[1..]),
        Some((c, rest)) => s.first() == Some(c) && component(rest, &s[1..]),
    }
}

fn components(p: &[String], s: &[&str]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        // The last component: one or more. Elsewhere: zero or more.
        Some((c, [])) if c == "**" => !s.is_empty(),
        Some((c, rest)) if c == "**" => (0..=s.len()).any(|i| components(rest, &s[i..])),
        Some((c, rest)) => s.first().is_some_and(|first| {
            let (p, f): (Vec<char>, Vec<char>) = (c.chars().collect(), first.chars().collect());
            component(&p, &f) && components(rest, &s[1..])
        }),
    }
}

fn matches(pattern: &Pattern, path: &[u8]) -> bool {
    let path = key(&String::from_utf8_lossy(path));
    let parts: Vec<&str> = path.split('/').collect();
    if pattern.anchored {
        components(&pattern.components, &parts)
    } else {
        (0..parts.len()).any(|i| components(&pattern.components, &parts[i..]))
    }
}

fn main() {
    let cases: &[(&str, &str, bool)] = &[
        // VAL-5b, as D-25 wrote it
        ("Cargo.toml", "crates/factory-core/Cargo.toml", true),
        ("crates/**", "vendor/crates/a.rs", true),
        ("SPEC.md", "docs/specs/billing/spec.md", true),
        ("/SPEC.md", "SPEC.md", true),
        ("/SPEC.md", "spec.md", true),
        ("/SPEC.md", "docs/SPEC.md", false),
        ("/crates/**", "vendor/crates/a.rs", false),
        // the wildcards
        ("*.snap", "x/a.snap", true),
        ("/*.snap", "x/a.snap", false),
        ("/*.snap", "a.snap", true),
        ("**/*.snap", "a.snap", true),
        ("/**/*.snap", "a.snap", true),
        ("/**/*.snap", "x/y/a.snap", true),
        ("crates/**", "crates/a/b.rs", true),
        ("crates/**", "crates", false),
        ("/crates/**", "crates", false),
        ("a/**/b", "a/b", true),
        ("a/**/b", "a/x/y/b", true),
        ("a/**/b", "a/x/y/c", false),
        ("**", "a", true),
        ("/**", "a/b", true),
        ("a?b", "a/b", false),
        ("a?b", "axb", true),
        ("a?b", "ab", false),
        ("a*b", "ab", true),
        ("a*b", "a/b", false),
        ("*", ".hidden", true),
        ("docs/*", "docs/.hidden", true),
        ("docs/*", "docs/a/b.md", false),
        ("app/?id?/*.tsx", "app/[id]/page.tsx", true),
        ("?", "\u{e9}", true),
        ("?", "e\u{301}", true),
        ("?", "x\u{323}\u{307}\u{301}", false),
        // a directory named without a wildcard
        ("migrations", "migrations/001.sql", false),
        ("migrations", "db/migrations", true),
        ("migrations/**", "migrations/001.sql", true),
        ("migrations/**", "db/migrations/001.sql", true),
        // one name
        ("AGENTS.md", "agents.MD", true),
        ("AGENTS.md", "AGENT\u{17f}.md", true),
        (".github/**", ".GitHub/workflows/x.yml", true),
        (".github/**", ".github/wor\u{212a}flows/x.yml", true),
        ("stra\u{df}e", "STRASSE", true),
        ("STRASSE", "stra\u{df}e", true),
        ("stra?e", "stra\u{df}e", false),
        ("stra??e", "stra\u{df}e", true),
        ("\u{e9}.md", "E\u{301}.md", true),
        ("fi", "\u{fb01}", true),
        ("a", "\u{ff41}", false),
        ("i", "\u{130}", false),
        ("I", "\u{131}", false),
        ("\u{3c3}", "\u{3c2}", true),
        ("\u{1f80}", "\u{1f08}\u{399}", true),
        ("\u{3b1}\u{345}\u{301}", "\u{1fb4}", true),
        (".git", ".g\u{200c}it", false),
        ("AGENTS.md", "AGENTS.md.", false),
    ];
    let mut wrong = 0;
    for (pattern, path, expected) in cases {
        let got = parse(pattern).map(|p| matches(&p, path.as_bytes()));
        if got != Ok(*expected) {
            wrong += 1;
        }
        println!("{:18} {:34} {:6} {}", format!("{pattern:?}"), format!("{path:?}"), format!("{got:?}"), if got == Ok(*expected) { "" } else { "<- NOT AS EXPECTED" });
    }
    let p = parse("*.md").unwrap();
    println!("{:18} {:34} {:?}", "\"*.md\"", "b\"a\\xff.md\" (not UTF-8)", matches(&p, b"a\xff.md"));
    let p = parse("a?.md").unwrap();
    println!("{:18} {:34} {:?}", "\"a?.md\"", "b\"a\\xff.md\" (not UTF-8)", matches(&p, b"a\xff.md"));
    let p = parse("a.md").unwrap();
    println!("{:18} {:34} {:?}", "\"a.md\"", "b\"a\\xff.md\" (not UTF-8)", matches(&p, b"a\xff.md"));
    println!();
    for pattern in ["", "/", "migrations/", "/migrations/", "a//b", "//a", "./a", "a/../b", "a**b", "**a", "a/***", "[a-c].rs", "a]", "{a,b}.rs", "a}", "a\\b", "\\*.rs"] {
        println!("{:18} {}", format!("{pattern:?}"), match parse(pattern) { Ok(_) => "valid <- NOT AS EXPECTED".to_owned(), Err(e) => format!("invalid: {e}") });
        if parse(pattern).is_ok() { wrong += 1; }
    }
    for pattern in ["**", "/**", "a/**/**/b", "*", "a b", "#a", "!a", ".factory/**", "AGENTS.md", "docs/specs/**", "target/**", "/docs/**", "**/*.snap", "/Cargo.lock", "a\u{301}"] {
        println!("{:18} {}", format!("{pattern:?}"), match parse(pattern) { Ok(_) => "valid".to_owned(), Err(e) => format!("invalid: {e} <- NOT AS EXPECTED") });
        if parse(pattern).is_err() { wrong += 1; }
    }
    println!("\n{wrong} not as expected");
}

//! `cargo xtask references`: the cross-reference check (SPEC DEV-2, DEV-2c).
//! A requirement id or a section number that `SPEC.md`, `GLOSSARY.md`,
//! `AGENTS.md` or the code cites, and that `SPEC.md` does not define, fails it.

use std::collections::HashSet;
use std::path::Path;
use std::process::ExitCode;

/// The Markdown files that the check reads. It reads every other file of the
/// tree as code, and no other Markdown file: the records of `docs/research/`
/// cite the drafts that they were written against.
const DOCUMENTS: [&str; 3] = ["SPEC.md", "GLOSSARY.md", "AGENTS.md"];

pub fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let broken = broken(&root);
    for citation in &broken {
        eprintln!("xtask references: {citation}");
    }
    if broken.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every citation below `root` that `SPEC.md` does not define, each as
/// `file:line: citation` and what it is not. A document that cannot be read
/// fails the check; a file of code that is no UTF-8 is no text and is not read.
pub fn broken(root: &Path) -> Vec<String> {
    let spec = match std::fs::read_to_string(root.join("SPEC.md")) {
        Ok(spec) => spec,
        Err(error) => return vec![format!("SPEC.md: cannot be read: {error}")],
    };
    let defined = Defined::read(&spec);
    let mut broken = Vec::new();
    for file in crate::tree::files(root) {
        let name = file.to_string_lossy();
        let document = DOCUMENTS.contains(&name.as_ref());
        if !document && file.extension().is_some_and(|extension| extension == "md") {
            continue;
        }
        match std::fs::read_to_string(root.join(&file)) {
            Ok(text) => broken.extend(defined.unresolved(&name, &text)),
            Err(error) if document => broken.push(format!("{name}: cannot be read: {error}")),
            Err(_) => {}
        }
    }
    broken
}

/// What `SPEC.md` defines: its ids, the capitals that begin them, and its
/// section numbers.
pub struct Defined {
    ids: HashSet<String>,
    prefixes: HashSet<String>,
    sections: HashSet<String>,
}

impl Defined {
    /// What `spec` defines (DEV-2c). An id is defined where it begins a line,
    /// or an item `- ` of a list, in bold and a full stop follows it, as in
    /// `**DEV-2.`, and where it is the first cell of a table row, as in
    /// `| D-21 |`. A section is defined by a heading of two to four `#` that
    /// begins with its number.
    pub fn read(spec: &str) -> Self {
        let mut ids = HashSet::new();
        for line in spec.lines() {
            let line = line.trim_start();
            let line = line.strip_prefix("- ").unwrap_or(line);
            let defined = if let Some(rest) = line.strip_prefix("**") {
                ids_in(rest)
                    .first()
                    .filter(|&&(at, id)| at == 0 && rest[id.len()..].starts_with('.'))
                    .map(|&(_, id)| id)
            } else if let Some(rest) = line.strip_prefix("| ") {
                ids_in(rest)
                    .first()
                    .filter(|&&(at, id)| at == 0 && rest[id.len()..].starts_with(" |"))
                    .map(|&(_, id)| id)
            } else {
                None
            };
            if let Some(id) = defined {
                ids.insert(id.to_owned());
            }
        }
        let prefixes = ids.iter().map(|id| prefix(id).to_owned()).collect();
        let sections = spec
            .lines()
            .filter_map(|line| {
                let rest = ["## ", "### ", "#### "]
                    .iter()
                    .find_map(|marks| line.strip_prefix(marks))?;
                let number = dotted(rest);
                let after = rest[number.len()..]
                    .strip_prefix('.')
                    .unwrap_or(&rest[number.len()..]);
                (!number.is_empty() && after.starts_with(' ')).then(|| number.to_owned())
            })
            .collect();
        Self {
            ids,
            prefixes,
            sections,
        }
    }

    /// The section number that begins `rest` where it stands as one
    /// (DEV-2c), after `context`, the text of its line before it.
    fn section_in<'a>(&self, context: &str, rest: &'a str) -> Option<&'a str> {
        let number = dotted(rest);
        let after = rest[number.len()..].chars().next();
        let parts: Vec<&str> = number.split('.').collect();
        let well_formed = !number.is_empty()
            && !after.is_some_and(|letter| letter.is_alphanumeric() || letter == '_')
            && parts.iter().all(|part| !part.starts_with('0'));
        let read = match (parts.len(), cited_after(context)) {
            (_, None) => false,
            (1, Some(cited)) => cited == Cited::AnyNumber,
            (2, Some(_)) => true,
            (3, Some(_)) => self.sections.contains(&parts[..2].join(".")),
            _ => false,
        };
        (well_formed && read).then_some(number)
    }

    /// Every citation of `text`, the contents of `file`, that `SPEC.md` does
    /// not define, each as `file:line: citation` and what it is not.
    pub fn unresolved(&self, file: &str, text: &str) -> Vec<String> {
        let mut unresolved = Vec::new();
        for (number, line) in text.lines().enumerate() {
            for (_, id) in ids_in(line) {
                if self.prefixes.contains(prefix(id)) && !self.ids.contains(id) {
                    unresolved.push(format!("{file}:{}: {id} is no id of SPEC.md", number + 1));
                }
            }
            for (start, _) in line.char_indices() {
                if let Some(section) = self.section_in(&line[..start], &line[start..])
                    && !self.sections.contains(section)
                {
                    unresolved.push(format!(
                        "{file}:{}: {section} is no section of SPEC.md",
                        number + 1
                    ));
                }
            }
        }
        unresolved
    }
}

/// The words after which a number of any form is cited as a section, as in
/// "section 14" (DEV-2c).
const NAMING: [&str; 3] = ["spec", "section", "sections"];

/// The words after which a number of two or three parts is cited as a section
/// (DEV-2c).
const CITING: [&str; 15] = [
    "of", "in", "and", "or", "to", "under", "per", "by", "see", "as", "that", "which", "what",
    "where", "from",
];

/// Which numbers a context cites as section numbers.
#[derive(Debug, PartialEq)]
enum Cited {
    /// A number of one part too: after a word of `NAMING`.
    AnyNumber,
    /// A number of two or three parts alone.
    Dotted,
}

/// Which numbers stand as section numbers after `context`, the text of the
/// line before them, if any: after a `(` that begins the line or follows a
/// space, after `, ` or `; `, or after a word of `NAMING` or `CITING` and a
/// space, where no hyphen joins the word to the one before.
fn cited_after(context: &str) -> Option<Cited> {
    if [" (", ", ", "; "]
        .iter()
        .any(|mark| context.ends_with(mark))
        || context == "("
    {
        return Some(Cited::Dotted);
    }
    let words = context.strip_suffix(' ')?;
    let word = words
        .rsplit(|letter: char| !letter.is_alphabetic())
        .next()
        .unwrap_or_default();
    let before = words[..words.len() - word.len()].chars().next_back();
    let word = word.to_lowercase();
    if before == Some('-') {
        None
    } else if NAMING.contains(&word.as_str()) {
        Some(Cited::AnyNumber)
    } else {
        CITING.contains(&word.as_str()).then_some(Cited::Dotted)
    }
}

/// The digits and the full stops between them that begin `text`, as in
/// `19.1`; empty where it begins with no digit.
fn dotted(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len()
        && (bytes[end].is_ascii_digit()
            || (bytes[end] == b'.'
                && end > 0
                && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)))
    {
        end += 1;
    }
    &text[..end]
}

/// The capitals of `id`, before its hyphen.
fn prefix(id: &str) -> &str {
    id.split_once('-').map_or(id, |(prefix, _)| prefix)
}

/// Whether `letter` continues a word, so that no id begins or ends beside it.
fn in_a_word(letter: char) -> bool {
    letter.is_alphanumeric() || letter == '_' || letter == '-'
}

/// Each word of `line` in the form of an id (DEV-2c), with its byte offset:
/// capitals, a hyphen, digits and at most one small letter, with no letter,
/// digit, `_` or `-` on either side.
fn ids_in(line: &str) -> Vec<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut ids = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let before = line[..start].chars().next_back();
        if !bytes[start].is_ascii_uppercase() || before.is_some_and(in_a_word) {
            start += line[start..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_uppercase() {
            end += 1;
        }
        let digits = end + 1;
        let mut after = digits;
        if bytes.get(end) == Some(&b'-') {
            while after < bytes.len() && bytes[after].is_ascii_digit() {
                after += 1;
            }
        }
        if after > digits {
            if bytes.get(after).is_some_and(u8::is_ascii_lowercase) {
                after += 1;
            }
            if !line[after..].chars().next().is_some_and(in_a_word) {
                ids.push((start, &line[start..after]));
            }
        }
        start = end.max(start + 1);
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Tree;

    /// A specification that defines `DEV-1`, `DEV-2a` and `D-3`, and the
    /// five sections that its headings number.
    const SPEC: &str = "# Spec\n\
                        \n\
                        ## 1. One\n\
                        \n\
                        ### 1.1 One one\n\
                        \n\
                        - **DEV-1.** A rule.\n  - **DEV-2a. A titled rule.** Text.\n\
                        \n\
                        ## 2. Two\n\
                        \n\
                        ### 2.1 Two one\n\
                        \n\
                        #### 2.1.1 Deep\n\
                        \n\
                        | Id | Decision |\n|---|---|\n| D-3 | A decision. |\n";

    /// The id `prefix-number`, so that this file itself cites none (DEV-2c).
    fn id(prefix: &str, number: &str) -> String {
        format!("{prefix}-{number}")
    }

    #[test]
    fn an_id_that_the_spec_does_not_define_fails_and_one_that_it_defines_does_not() {
        let text = format!(
            "As {} and {} say,\nand {} too (see {}).\n",
            id("DEV", "1"),
            id("DEV", "2a"),
            id("D", "3"),
            id("DEV", "9"),
        );

        let unresolved = Defined::read(SPEC).unresolved("AGENTS.md", &text);

        assert_eq!(
            unresolved,
            [format!(
                "AGENTS.md:2: {} is no id of SPEC.md",
                id("DEV", "9")
            )]
        );
    }

    /// `text` with each `{}` replaced by the next of `numbers`, so that this
    /// file itself cites none of them (DEV-2c).
    fn with(text: &str, numbers: &[&str]) -> String {
        numbers.iter().fold(text.to_owned(), |text, number| {
            text.replacen("{}", number, 1)
        })
    }

    #[test]
    fn a_section_number_that_the_spec_does_not_define_fails_and_one_that_it_defines_does_not() {
        let text = with(
            "As {} says ({}, {}; {}),\nsee section {} and SPEC {}, and {} (x).\n",
            &["1.1", "2.1.1", "1.1", "2", "2", "1", "1.2"],
        );

        let unresolved = Defined::read(SPEC).unresolved("SPEC.md", &text);

        assert_eq!(unresolved, ["SPEC.md:2: 1.2 is no section of SPEC.md"]);
    }

    #[test]
    fn a_number_is_a_section_number_only_where_it_stands_as_one() {
        let text = with(
            // A version after a name, a number alone without `section` or
            // `SPEC`, a part that is 0 or begins with one, four parts, and
            // three of which the first two are no section, and a word joined
            // to the one before by a hyphen.
            "git {} (macOS {}), of {} items, in {} and {}, of {}, in {}, by {}, opt-in {}.\n",
            &[
                "2.47", "27.0.1", "3", "1.0", "1.01", "1.1.1.1", "3.13.5", "1.1x", "1.2",
            ],
        );

        let unresolved = Defined::read(SPEC).unresolved("x.rs", &text);

        assert!(unresolved.is_empty(), "{unresolved:?}");
    }

    #[test]
    fn a_number_of_three_parts_is_read_where_its_first_two_are_a_section() {
        let text = with("Of {} and {}.\n", &["2.1.1", "2.1.2"]);

        let unresolved = Defined::read(SPEC).unresolved("x.rs", &text);

        assert_eq!(unresolved, ["x.rs:1: 2.1.2 is no section of SPEC.md"]);
    }

    #[test]
    fn the_check_reads_the_spec_the_glossary_agents_md_and_the_code_and_no_other_document() {
        let bad = format!("{}\n", id("DEV", "9"));
        let tree = Tree::new("references-walk")
            .with("SPEC.md", format!("{SPEC}{bad}"))
            .with("GLOSSARY.md", &bad)
            .with("AGENTS.md", with("In {}.\n", &["1.2"]))
            .with("xtask/src/main.rs", format!("// {bad}"))
            .with(".githooks/pre-commit", &bad)
            .with("docs/research/01-a.md", &bad)
            .with("docs/agents/domain.md", &bad)
            .with("README.md", &bad)
            .with(".git/config", &bad)
            .with("target/debug/x.rs", &bad)
            .with("image.png", [0xff, 0xfe, 0x00]);

        let unresolved = broken(&tree.0);

        let dev9 = id("DEV", "9");
        assert_eq!(
            unresolved,
            [
                format!(".githooks/pre-commit:1: {dev9} is no id of SPEC.md"),
                "AGENTS.md:1: 1.2 is no section of SPEC.md".to_owned(),
                format!("GLOSSARY.md:1: {dev9} is no id of SPEC.md"),
                format!("SPEC.md:19: {dev9} is no id of SPEC.md"),
                format!("xtask/src/main.rs:1: {dev9} is no id of SPEC.md"),
            ]
        );
    }

    #[test]
    fn a_tree_without_a_spec_fails() {
        let tree = Tree::new("references-no-spec").with("AGENTS.md", "Text.\n");

        let unresolved = broken(&tree.0);

        assert_eq!(unresolved.len(), 1, "{unresolved:?}");
        assert!(
            unresolved[0].starts_with("SPEC.md: cannot be read"),
            "{unresolved:?}"
        );
    }

    #[test]
    fn a_word_is_an_id_only_alone_and_with_the_capitals_of_an_id_that_the_spec_defines() {
        let text = format!(
            "{} {} {} {}-x x-{} {}_ {}b {}\n",
            id("UTF", "8"),
            id("AC", "12"),
            id("SHA", "256"),
            id("DEV", "9"),
            id("DEV", "9"),
            id("DEV", "9"),
            id("DEV", "2a"),
            id("D", "03"),
        );

        let unresolved = Defined::read(SPEC).unresolved("x.rs", &text);

        assert_eq!(
            unresolved,
            [format!("x.rs:1: {} is no id of SPEC.md", id("D", "03"))]
        );
    }
}

//! Enough of the shell's grammar to split a command line into its commands
//! and each command into its words, with the quoting undone.
//!
//! Commands inside `$( )`, backquotes and parentheses are read too. The text
//! of a here-document is skipped, since it is a command's input and not its
//! arguments. Nothing is expanded.

use std::iter::Peekable;
use std::str::Chars;

/// The commands of a shell command line, each as its words.
pub fn commands(line: &str) -> Vec<Vec<String>> {
    let mut parser = Parser {
        rest: line.chars().peekable(),
        commands: Vec::new(),
        heredocs: Vec::new(),
    };
    parser.commands_until(None);
    parser.commands
}

struct Parser<'a> {
    rest: Peekable<Chars<'a>>,
    commands: Vec<Vec<String>>,
    /// The here-documents announced on the current line, whose text starts
    /// after the line's end.
    heredocs: Vec<Heredoc>,
}

struct Heredoc {
    delimiter: String,
    /// `<<-`: leading tabs are stripped from each line of the text.
    strip_tabs: bool,
}

/// The words of one command while it is being read.
#[derive(Default)]
struct Words {
    finished: Vec<String>,
    unfinished: Option<String>,
    /// The next word is the target of a redirection and no argument.
    redirected: bool,
}

impl Words {
    /// The word being read, started if there was none: an empty pair of
    /// quotes is a word too.
    fn word_mut(&mut self) -> &mut String {
        self.unfinished.get_or_insert_default()
    }

    fn end_word(&mut self) {
        if let Some(word) = self.unfinished.take()
            && !std::mem::take(&mut self.redirected)
        {
            self.finished.push(word);
        }
    }

    /// Notes a redirection operator. Digits written against it name a file
    /// descriptor, as in `2>`, and are no word.
    fn redirect(&mut self) {
        if let Some(word) = &self.unfinished
            && word.bytes().all(|byte| byte.is_ascii_digit())
        {
            self.unfinished = None;
        }
        self.end_word();
        self.redirected = true;
    }

    /// Ends the command and returns its words, leaving none for the next.
    fn end(&mut self) -> Vec<String> {
        self.end_word();
        self.redirected = false;
        std::mem::take(&mut self.finished)
    }
}

impl Parser<'_> {
    /// Reads commands up to the character `close`, or to the end of the line.
    fn commands_until(&mut self, close: Option<char>) {
        let mut command = Words::default();
        while let Some(character) = self.rest.next() {
            if Some(character) == close {
                break;
            }
            match character {
                ' ' | '\t' => command.end_word(),
                '\n' => {
                    self.commands.push(command.end());
                    self.skip_heredocs();
                }
                '&' if self.rest.next_if_eq(&'>').is_some() => {
                    self.rest.next_if_eq(&'>');
                    command.redirect();
                }
                ';' | '|' | '&' | ')' => self.commands.push(command.end()),
                '(' => {
                    self.commands.push(command.end());
                    self.commands_until(Some(')'));
                }
                '`' => {
                    command.word_mut();
                    self.commands_until(Some('`'));
                }
                '$' if self.rest.next_if_eq(&'(').is_some() => {
                    command.word_mut();
                    self.commands_until(Some(')'));
                }
                '$' if self.rest.next_if_eq(&'\'').is_some() => {
                    let word = command.word_mut();
                    while let Some(next) = self.rest.next().filter(|next| *next != '\'') {
                        // An escape is kept as the character it escapes, which
                        // is right for a quote and a backslash only.
                        word.extend(if next == '\\' {
                            self.rest.next()
                        } else {
                            Some(next)
                        });
                    }
                }
                '<' if self.rest.next_if_eq(&'<').is_some() => {
                    if self.rest.next_if_eq(&'<').is_some() {
                        command.redirect();
                    } else {
                        command.end_word();
                        self.heredoc();
                    }
                }
                '<' | '>' if self.rest.next_if_eq(&'(').is_some() => {
                    command.word_mut();
                    self.commands_until(Some(')'));
                }
                '<' | '>' => {
                    while self
                        .rest
                        .next_if(|next| matches!(next, '>' | '&' | '|'))
                        .is_some()
                    {}
                    command.redirect();
                }
                '#' if command.unfinished.is_none() => {
                    while self.rest.next_if(|next| *next != '\n').is_some() {}
                }
                '\\' => match self.rest.next() {
                    Some('\n') | None => {}
                    Some(escaped) => command.word_mut().push(escaped),
                },
                '\'' => {
                    let word = command.word_mut();
                    word.extend(self.rest.by_ref().take_while(|next| *next != '\''));
                }
                '"' => self.double_quoted(command.word_mut()),
                other => command.word_mut().push(other),
            }
        }
        self.commands.push(command.end());
    }

    /// Reads the rest of a double-quoted string into `word`. A command
    /// substitution inside it is read as commands and adds nothing to the
    /// word.
    fn double_quoted(&mut self, word: &mut String) {
        while let Some(character) = self.rest.next() {
            match character {
                '"' => break,
                '\\' => match self.rest.next() {
                    Some('\n') | None => {}
                    Some(escaped @ ('"' | '\\' | '$' | '`')) => word.push(escaped),
                    Some(other) => word.extend(['\\', other]),
                },
                '$' if self.rest.next_if_eq(&'(').is_some() => self.commands_until(Some(')')),
                '`' => self.commands_until(Some('`')),
                other => word.push(other),
            }
        }
    }

    /// Reads the delimiter that follows `<<` and notes the here-document.
    fn heredoc(&mut self) {
        let strip_tabs = self.rest.next_if_eq(&'-').is_some();
        while self
            .rest
            .next_if(|next| matches!(next, ' ' | '\t'))
            .is_some()
        {}
        let mut delimiter = String::new();
        while let Some(character) = self.rest.next_if(|next| {
            !matches!(
                next,
                ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
            )
        }) {
            match character {
                quote @ ('\'' | '"') => {
                    delimiter.extend(self.rest.by_ref().take_while(|next| *next != quote));
                }
                '\\' => delimiter.extend(self.rest.next()),
                other => delimiter.push(other),
            }
        }
        self.heredocs.push(Heredoc {
            delimiter,
            strip_tabs,
        });
    }

    /// Skips the text of each here-document announced on the line just ended,
    /// through the line that holds its delimiter. Where no such line follows,
    /// what was taken for a here-document was something else, such as a shift
    /// in an arithmetic expansion, and nothing is skipped.
    fn skip_heredocs(&mut self) {
        for heredoc in std::mem::take(&mut self.heredocs) {
            let start = self.rest.clone();
            let mut found = false;
            while !found && self.rest.peek().is_some() {
                let line: String = self
                    .rest
                    .by_ref()
                    .take_while(|next| *next != '\n')
                    .collect();
                let line = if heredoc.strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                found = line == heredoc.delimiter;
            }
            if !found {
                self.rest = start;
            }
        }
    }
}

//! Segmentation of text into tokens and sentences with positions in the source text.
//!
//! The text is never changed: a token or a sentence is a [`Span`] of byte offsets into the text
//! it was cut from. [`Segmenter::segment`] gives a [`Document`]: tokens, words with the columns of
//! CoNLL-U, sentences, paragraphs and lines, read from and written to CoNLL-U by [`read_conllu`],
//! [`Document::to_conllu`] and [`ConlluWriter`]. Every language is a Cargo feature, on by default:
//! `ru` (Russian) and `es` (Spanish).
//!
//! ```
//! # #[cfg(feature = "ru")] {
//! use anyseg::Segmenter;
//!
//! let segmenter = Segmenter::new("ru").unwrap();
//! let text = "Мама мыла раму";
//! let tokens: Vec<&str> = segmenter.tokenize(text).iter().map(|span| span.slice(text)).collect();
//! assert_eq!(tokens, ["Мама", "мыла", "раму"]);
//! let doc = segmenter.segment(text);
//! assert_eq!(doc.sentence(0).span.slice(doc.text()), "Мама мыла раму");
//! # }
//! ```

#![warn(missing_docs)]

use std::error::Error;
use std::fmt;

#[cfg(feature = "cli")]
pub mod cli;
mod conllu;
mod document;
mod lang;
mod layout;
mod sentences;
mod state;
mod tokens;
mod words;

use sentences::Split;
use tokens::Exceptions;

pub use conllu::{ConlluError, ConlluWriter, read_conllu};
#[doc(hidden)]
pub use document::CheckedColumn;
pub use document::{Column, Document, DocumentError, Line, LineBreak, Paragraph, Sentence};

/// A piece of a text: `&text[start..end]`, in bytes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    /// The offset of the first byte
    pub start: usize,
    /// The offset after the last byte
    pub end: usize,
}

impl Span {
    /// The piece of the text the span was cut from.
    ///
    /// # Panics
    ///
    /// If the span is out of the text or not on character boundaries, as slicing does.
    pub fn slice(self, text: &str) -> &str {
        &text[self.start..self.end]
    }
}

/// A language whose conventions the segmenter follows
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Language {
    /// Russian, `"ru"`
    #[cfg(feature = "ru")]
    Russian,
    /// Spanish, `"es"`
    #[cfg(feature = "es")]
    Spanish,
}

impl Language {
    /// The languages of this build
    pub const ALL: &[Language] = &[
        #[cfg(feature = "ru")]
        Language::Russian,
        #[cfg(feature = "es")]
        Language::Spanish,
    ];

    /// ISO 639-1 code: `"ru"`
    pub fn code(self) -> &'static str {
        match self {
            #[cfg(feature = "ru")]
            Language::Russian => "ru",
            #[cfg(feature = "es")]
            Language::Spanish => "es",
        }
    }

    /// The language with an ISO 639-1 code, if this build supports it
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|language| language.code() == code)
    }
}

/// The language code is not one of [`Language::ALL`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownLanguage(String);

impl UnknownLanguage {
    /// The code that was given
    pub fn code(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UnknownLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let codes: Vec<&str> = Language::ALL
            .iter()
            .map(|language| language.code())
            .collect();
        write!(
            f,
            "unsupported language {:?}, expected one of: {}",
            self.0,
            codes.join(", ")
        )
    }
}

impl Error for UnknownLanguage {}

/// A string that a list of the user cannot take
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidEntry {
    /// Not a word of letters, with a period after it or without
    Abbreviation(String),
    /// Empty, with whitespace or control characters, or with an invisible separator at an edge
    Token(String),
}

impl fmt::Display for InvalidEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidEntry::Abbreviation(word) => write!(
                f,
                "abbreviation {word:?} is not a word of letters with an optional final period"
            ),
            InvalidEntry::Token(token) => write!(
                f,
                "token {token:?} is empty, has whitespace or control characters or starts or ends with an invisible separator"
            ),
        }
    }
}

impl Error for InvalidEntry {}

/// Segmenter of texts in one language
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Segmenter {
    language: Language,
    exceptions: Exceptions,
}

impl Segmenter {
    /// The segmenter of a language given by its ISO 639-1 code.
    ///
    /// # Errors
    ///
    /// [`UnknownLanguage`] if the build does not support the language.
    pub fn new(code: &str) -> Result<Self, UnknownLanguage> {
        Language::from_code(code)
            .map(|language| Segmenter {
                language,
                exceptions: Exceptions::default(),
            })
            .ok_or_else(|| UnknownLanguage(code.to_owned()))
    }

    /// The segmenter with more abbreviations: words that keep a period after them, given with the period
    /// or without it, in any case.
    ///
    /// # Errors
    ///
    /// [`InvalidEntry::Abbreviation`] if a word has characters other than letters.
    pub fn with_abbreviations<I>(mut self, words: I) -> Result<Self, InvalidEntry>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let tables = lang::tables(self.language);
        for word in words {
            let word = word.as_ref();
            let letters = word.strip_suffix('.').unwrap_or(word);
            if !tokens::is_letters(letters) {
                return Err(InvalidEntry::Abbreviation(word.to_owned()));
            }
            self.exceptions.abbreviations.insert(tables.key(letters));
        }
        Ok(self)
    }

    /// The segmenter with strings that are one token each: «C++», «т.н.». A string matches as
    /// written where a token would start and no letter or digit adjoins it; of several, the longest.
    ///
    /// # Errors
    ///
    /// [`InvalidEntry::Token`] if a string is empty, has whitespace or control characters or starts
    /// or ends with an invisible separator.
    pub fn with_tokens<I>(mut self, strings: I) -> Result<Self, InvalidEntry>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        for string in strings {
            let string = string.as_ref();
            if string.is_empty()
                || string.chars().any(|c| c.is_whitespace() || c.is_control())
                || string.starts_with(layout::is_separator)
                || string.ends_with(layout::is_separator)
            {
                return Err(InvalidEntry::Token(string.to_owned()));
            }
            self.exceptions.add_token(string);
        }
        Ok(self)
    }

    /// The language of the segmenter
    pub fn language(&self) -> Language {
        self.language
    }

    /// The abbreviations added to the language, in order: lowercase, without the period and the marks that the language
    /// leaves out of its lists
    pub fn abbreviations(&self) -> impl ExactSizeIterator<Item = &str> {
        self.exceptions.abbreviations.iter().map(String::as_str)
    }

    /// The strings that are one token each, in order
    pub fn tokens(&self) -> impl ExactSizeIterator<Item = &str> {
        self.exceptions.tokens.iter().map(String::as_str)
    }

    /// The tokens of a text in order. Whitespace, control characters and the invisible separators
    /// (zero width space, word joiner, byte order mark, and a soft hyphen and zero width
    /// (non-)joiner outside a word) separate tokens and are in none. Words keep their combining marks and soft hyphens;
    /// numbers with decimal marks, dates and times, URLs, e-mail addresses, file names, mentions,
    /// hashtags, emoticons and runs of emoji are one token each. Abbreviations and initials keep
    /// their period, except at the end of a sentence. The lists and conventions of the language
    /// decide which words are abbreviations and how hyphens, ranges, digit groups, final marks and
    /// other marks inside a word split; the lists of the segmenter add abbreviations and strings
    /// that are one token each.
    pub fn tokenize(&self, text: &str) -> Vec<Span> {
        self.split(text, false).tokens
    }

    /// The sentences of a text in order, from the start of the first token to the end of the last.
    /// A paragraph, the part of the text between blank lines, starts a sentence; inside it a
    /// sentence ends at final marks («.», «?!», «...») with the closing quotes and brackets after
    /// them, when the start of a sentence follows, such as a capital letter, a digit or an inverted
    /// mark, maybe after a dash or an opening quote. An abbreviation ends a sentence before a capital
    /// letter when the language lists it or it follows a number, and an initial as a rule does not;
    /// the lists and conventions of the language decide the rest. A line break starts a sentence
    /// before a list item or a line of dialogue, at a line without words and after a line without
    /// final punctuation before a capital letter, except in verse and in text wrapped at the width
    /// of the page (a line of the paragraph goes on with a lowercase letter).
    pub fn sentenize(&self, text: &str) -> Vec<Span> {
        let split = self.split(text, false);
        let tokens = &split.tokens;
        (0..split.starts.len())
            .map(|index| {
                let end = split.starts.get(index + 1).map_or(tokens.len(), |&end| end);
                Span {
                    start: tokens[split.starts[index]].start,
                    end: tokens[end - 1].end,
                }
            })
            .collect()
    }

    /// The document of a text: its tokens, their words, its sentences as
    /// [`sentenize`](Self::sentenize) gives them, its paragraphs and their lines, each with how it
    /// begins: outside verse, in a paragraph where some line goes on with a lowercase letter, a
    /// line that does not start a sentence is a wrap at the width of the page; other lines are
    /// breaks of the author. A token holds one word, except where the language joins several in
    /// one token: Spanish «del» is «de» and «el». A string given to
    /// [`with_tokens`](Self::with_tokens) stays one word
    pub fn segment(&self, text: &str) -> Document {
        self.segment_string(text.to_owned())
    }

    /// The document of a text as [`segment`](Self::segment) gives it, without a copy of the text
    pub fn segment_string(&self, text: String) -> Document {
        let split = self.split(&text, true);
        let mut document = Document::segmented(
            text,
            split.tokens,
            split.starts,
            split.paragraphs,
            split.lines.unwrap_or_default(),
        );
        let tables = lang::tables(self.language);
        if words::has_multiword(tables) {
            document.expand_tokens(tables, &self.exceptions);
        }
        document
    }

    /// The document without the words of its tokens
    #[cfg(feature = "cli")]
    pub(crate) fn segment_tokens(&self, text: String) -> Document {
        let split = self.split(&text, true);
        Document::segmented(
            text,
            split.tokens,
            split.starts,
            split.paragraphs,
            split.lines.unwrap_or_default(),
        )
    }

    fn split(&self, text: &str, lines: bool) -> Split {
        split_text(text, lang::tables(self.language), &self.exceptions, lines)
    }
}

fn split_text(text: &str, tables: &lang::Tables, exceptions: &Exceptions, lines: bool) -> Split {
    let mut split = Split {
        tokens: Vec::with_capacity(text.len() / 8),
        lines: lines.then(Vec::new),
        ..Split::default()
    };
    let mut paragraph = Vec::new();
    for paragraph_lines in layout::paragraphs(text) {
        let span = Span {
            start: paragraph_lines[0].start,
            end: paragraph_lines[paragraph_lines.len() - 1].end,
        };
        paragraph.clear();
        tokens::tokenize(text, span, tables, exceptions, &mut paragraph);
        sentences::split(
            text,
            &paragraph_lines,
            &paragraph,
            tables,
            exceptions,
            &mut split,
        );
    }
    split
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use super::*;

    fn pieces<'a>(text: &'a str, spans: &[Span]) -> Vec<&'a str> {
        spans.iter().map(|span| span.slice(text)).collect()
    }

    #[test]
    fn language_codes() {
        assert_eq!(Language::from_code("ru"), Some(Language::Russian));
        assert_eq!(Language::Russian.code(), "ru");
        assert_eq!(Language::from_code("xx"), None);
        assert!(Language::ALL.contains(&Language::Russian));
        #[cfg(feature = "es")]
        assert_eq!(Language::from_code("es"), Some(Language::Spanish));
    }

    #[test]
    fn unknown_language() {
        let error = Segmenter::new("xx").unwrap_err();
        assert_eq!(error, UnknownLanguage("xx".to_owned()));
        assert_eq!(error.code(), "xx");
        assert!(
            error
                .to_string()
                .starts_with("unsupported language \"xx\", expected one of: ru")
        );
        assert_eq!(Segmenter::new("ru").unwrap().language(), Language::Russian);
    }

    #[test]
    fn tokens_are_byte_spans() {
        let text = " Ёж\u{a0}идёт\t😀\r\n";
        let spans = Segmenter::new("ru").unwrap().tokenize(text);
        assert_eq!(
            spans,
            [
                Span { start: 1, end: 5 },
                Span { start: 7, end: 15 },
                Span { start: 16, end: 20 },
            ]
        );
        assert_eq!(pieces(text, &spans), ["Ёж", "идёт", "😀"]);
    }

    #[test]
    fn blank_text() {
        let segmenter = Segmenter::new("ru").unwrap();
        for text in ["", " ", "\n\u{2028}\u{3000}"] {
            assert!(segmenter.tokenize(text).is_empty());
            assert!(segmenter.sentenize(text).is_empty());
        }
    }

    #[test]
    fn sentences_inside_paragraphs() {
        let text = "\n Мама мыла раму.\nРама чистая.\u{a0}\n \n\u{feff}Утро";
        let segmenter = Segmenter::new("ru").unwrap();
        let spans = segmenter.sentenize(text);
        assert_eq!(
            pieces(text, &spans),
            ["Мама мыла раму.", "Рама чистая.", "Утро"]
        );
        let doc = segmenter.segment(text);
        assert_eq!(doc, segmenter.segment_string(text.to_owned()));
        assert_eq!(doc.tokens(), segmenter.tokenize(text));
        let sentences: Vec<Span> = (0..doc.sentence_count())
            .map(|index| doc.sentence(index).span)
            .collect();
        assert_eq!(sentences, spans);
        assert_eq!((doc.paragraph_count(), doc.lines().len()), (2, 3));
        assert_eq!(doc.paragraph(0).sentences, 0..2);
    }

    #[test]
    fn lists_of_the_user() {
        let text = "На C++, C++11 и aC++ сущ. и глаг. т.н.\n\nТаков т.н.";
        let default = Segmenter::new("ru").unwrap();
        assert_eq!(
            pieces(text, &default.tokenize(text)),
            [
                "На",
                "C",
                "++",
                ",",
                "C",
                "++",
                "11",
                "и",
                "aC",
                "++",
                "сущ",
                ".",
                "и",
                "глаг",
                ".",
                "т.",
                "н",
                ".",
                "Таков",
                "т.",
                "н",
                "."
            ]
        );
        let segmenter = default
            .with_abbreviations(["Сущ.", "глаг", "сущ"])
            .unwrap()
            .with_tokens(["C+", "C++", "т.н."])
            .unwrap();
        assert_eq!(
            pieces(text, &segmenter.tokenize(text)),
            [
                "На",
                "C++",
                ",",
                "C+",
                "+",
                "11",
                "и",
                "aC",
                "++",
                "сущ.",
                "и",
                "глаг.",
                "т.н.",
                "Таков",
                "т.н."
            ]
        );
        assert_eq!(
            segmenter.abbreviations().collect::<Vec<_>>(),
            ["глаг", "сущ"]
        );
        assert_eq!(
            segmenter.tokens().collect::<Vec<_>>(),
            ["C+", "C++", "т.н."]
        );
        assert_ne!(segmenter, Segmenter::new("ru").unwrap());
    }

    #[test]
    fn invalid_entries() {
        let segmenter = Segmenter::new("ru").unwrap();
        for word in ["", ".", "т.н", "г-н", "2-й", "с "] {
            assert_eq!(
                segmenter.clone().with_abbreviations([word]),
                Err(InvalidEntry::Abbreviation(word.to_owned()))
            );
        }
        for token in ["", "a b", " a", "\u{ad}a", "a\u{200b}", "a\u{1f}b"] {
            assert_eq!(
                segmenter.clone().with_tokens([token]),
                Err(InvalidEntry::Token(token.to_owned()))
            );
        }
        assert_eq!(
            InvalidEntry::Abbreviation("т.н".to_owned()).to_string(),
            "abbreviation \"т.н\" is not a word of letters with an optional final period"
        );
    }

    #[test]
    fn documents_keep_only_what_they_hold() {
        let doc = Segmenter::new("ru")
            .unwrap()
            .segment(&"Ёж идёт домой. Кот спит.\n".repeat(1000));
        assert_eq!(doc.tokens.capacity(), doc.tokens.len());
        assert_eq!(doc.sentence_tokens.capacity(), doc.sentence_tokens.len());
        assert_eq!(
            doc.paragraph_sentences.capacity(),
            doc.paragraph_sentences.len()
        );
        assert_eq!(doc.lines.capacity(), doc.lines.len());
    }

    #[test]
    fn many_tokens_of_the_user() {
        // A lookup by every token of the list takes seconds here
        let strings: Vec<String> = (0..50_000).map(|number| format!("x{number}+")).collect();
        let segmenter = Segmenter::new("ru").unwrap().with_tokens(&strings).unwrap();
        let text = "x1+ x49999+ x ".repeat(10_000);
        assert_eq!(segmenter.tokenize(&text).len(), 30_000);
    }
}

//! Segmentation of text into tokens and sentences with positions in the source text.
//!
//! The text is never changed: a token or a sentence is a [`Span`] of byte offsets into the text
//! it was cut from. [`Segmenter::segment`] gives a [`Document`]: tokens, words with the columns of
//! CoNLL-U, sentences, paragraphs and lines, read from and written to CoNLL-U by [`read_conllu`]
//! and [`Document::to_conllu`]. Every language is a Cargo feature: `ru` (Russian, on by default).
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

use std::error::Error;
use std::fmt;

mod conllu;
mod document;
mod layout;

pub use conllu::{ConlluError, read_conllu};
pub use document::{Column, Document, DocumentError, Line, LineBreak, Paragraph, Sentence};

/// A piece of a text: `&text[start..end]`, in bytes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
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
    #[cfg(feature = "ru")]
    Russian,
}

impl Language {
    /// The languages of this build
    pub const ALL: &[Language] = &[
        #[cfg(feature = "ru")]
        Language::Russian,
    ];

    /// ISO 639-1 code: `"ru"`
    pub fn code(self) -> &'static str {
        match self {
            #[cfg(feature = "ru")]
            Language::Russian => "ru",
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
pub struct UnknownLanguage(pub String);

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

/// Segmenter of texts in one language
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Segmenter {
    language: Language,
}

impl Segmenter {
    /// The segmenter of a language given by its ISO 639-1 code.
    ///
    /// # Errors
    ///
    /// [`UnknownLanguage`] if the build does not support the language.
    pub fn new(code: &str) -> Result<Self, UnknownLanguage> {
        Language::from_code(code)
            .map(|language| Segmenter { language })
            .ok_or_else(|| UnknownLanguage(code.to_owned()))
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// The tokens of a text in order: the runs of characters between whitespace and the invisible
    /// separators (zero width space, word joiner, byte order mark)
    pub fn tokenize(&self, text: &str) -> Vec<Span> {
        let mut spans = Vec::new();
        layout::runs(
            text,
            Span {
                start: 0,
                end: text.len(),
            },
            &mut spans,
        );
        spans
    }

    /// The sentences of a text in order: every paragraph, the part of the text between blank lines,
    /// without the separators around it
    pub fn sentenize(&self, text: &str) -> Vec<Span> {
        layout::paragraphs(text)
            .iter()
            .map(|lines| Span {
                start: lines[0].start,
                end: lines[lines.len() - 1].end,
            })
            .collect()
    }

    /// The document of a text: its tokens, one word per token, its sentences as
    /// [`sentenize`](Self::sentenize) gives them, its paragraphs and their lines
    pub fn segment(&self, text: &str) -> Document {
        Document::assemble(text.to_owned(), self.tokenize(text), None)
            .expect("tokens of the segmenter lie inside paragraphs")
    }
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
    }

    #[test]
    fn unknown_language() {
        let error = Segmenter::new("xx").unwrap_err();
        assert_eq!(error, UnknownLanguage("xx".to_owned()));
        assert_eq!(
            error.to_string(),
            "unsupported language \"xx\", expected one of: ru"
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
    fn sentence_is_paragraph() {
        let text = "\n Мама мыла раму.\nРама чистая.\u{a0}\n \n\u{feff}Утро";
        let segmenter = Segmenter::new("ru").unwrap();
        let spans = segmenter.sentenize(text);
        assert_eq!(
            pieces(text, &spans),
            ["Мама мыла раму.\nРама чистая.", "Утро"]
        );
        let doc = segmenter.segment(text);
        assert_eq!(doc.tokens(), segmenter.tokenize(text));
        let sentences: Vec<Span> = (0..doc.sentence_count())
            .map(|index| doc.sentence(index).span)
            .collect();
        assert_eq!(sentences, spans);
        assert_eq!((doc.paragraph_count(), doc.lines().len()), (2, 3));
    }
}

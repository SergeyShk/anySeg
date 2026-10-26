//! The document model: tokens, words with the columns of CoNLL-U, sentences, paragraphs and lines

use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::Span;
use crate::layout::{self, is_separator};

/// How a line of a paragraph begins after the line before it
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineBreak {
    /// A break the author made: a verse line, a list item, a paragraph per line
    Author,
    /// A wrap at the width of the page, as in text extracted from PDF
    Typeset,
}

impl LineBreak {
    pub fn name(self) -> &'static str {
        match self {
            LineBreak::Author => "author",
            LineBreak::Typeset => "typeset",
        }
    }
}

/// A line of a paragraph
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Line {
    /// The line without the separators around it
    pub span: Span,
    /// `None` for the first line of a paragraph
    pub break_before: Option<LineBreak>,
}

/// A sentence: from the start of its first token to the end of its last one
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sentence {
    pub span: Span,
    pub tokens: Range<usize>,
    pub words: Range<usize>,
}

/// A paragraph: from the start of its first token to the end of its last one
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Paragraph {
    pub span: Span,
    pub sentences: Range<usize>,
    pub tokens: Range<usize>,
}

/// A column of CoNLL-U that a word holds as text; `head` is a number and `form` comes from the tokens
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Lemma,
    Upos,
    Xpos,
    Feats,
    Deprel,
    Deps,
    Misc,
}

impl Column {
    pub const ALL: [Column; 7] = [
        Column::Lemma,
        Column::Upos,
        Column::Xpos,
        Column::Feats,
        Column::Deprel,
        Column::Deps,
        Column::Misc,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Column::Lemma => "lemma",
            Column::Upos => "upos",
            Column::Xpos => "xpos",
            Column::Feats => "feats",
            Column::Deprel => "deprel",
            Column::Deps => "deps",
            Column::Misc => "misc",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.name() == name)
    }
}

/// Wrong tokens, sentence starts or column values
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentError(pub String);

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DocumentError {}

/// The parts of CoNLL-U that a document read from it writes back as they were
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Verbatim {
    /// The comment lines of every sentence
    pub(crate) comments: Vec<Vec<String>>,
    /// Columns 2-10 of the line of every multiword token, by token
    pub(crate) multiword: Vec<(usize, String)>,
    /// The empty nodes of every sentence, with the number of token and word lines before each
    pub(crate) empty: Vec<Vec<(usize, String)>>,
}

/// A segmented text. Tokens are spans of the text, which is never changed; a token holds one word or, for a
/// multiword token, several. Sentences hold whole tokens, paragraphs hold whole sentences, and lines are those of
/// the paragraphs. Words have the columns of CoNLL-U, empty until set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub(crate) text: String,
    pub(crate) tokens: Vec<Span>,
    /// The first word of every token, and the number of words
    pub(crate) token_words: Vec<usize>,
    pub(crate) word_tokens: Vec<usize>,
    /// Empty when every word is the text of its token
    pub(crate) forms: Vec<Option<String>>,
    /// Every column is empty until set
    pub(crate) columns: [Vec<Option<String>>; 7],
    pub(crate) heads: Vec<Option<u32>>,
    /// The first token of every sentence, and the number of tokens
    pub(crate) sentence_tokens: Vec<usize>,
    /// The first sentence of every paragraph, and the number of sentences
    pub(crate) paragraph_sentences: Vec<usize>,
    pub(crate) lines: Vec<Line>,
    pub(crate) verbatim: Option<Box<Verbatim>>,
}

impl Document {
    /// A document of the given tokens of a text, one word per token. Paragraphs are the parts of the text between
    /// blank lines, every paragraph starts a sentence; without sentence starts a paragraph is one sentence.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if a token is empty, out of the text, not on character boundaries, overlaps the token
    /// before it, starts or ends with whitespace, or runs over a paragraph boundary, or if the sentence starts are
    /// not increasing token indices.
    pub fn from_tokens(
        text: String,
        tokens: Vec<Span>,
        sentence_starts: Option<&[usize]>,
    ) -> Result<Self, DocumentError> {
        let mut previous = 0;
        for (index, span) in tokens.iter().enumerate() {
            let error = |message: &str| Err(DocumentError(format!("token {index}: {message}")));
            if span.start >= span.end {
                return error("is empty");
            }
            if span.end > text.len() {
                return error("ends after the text");
            }
            if !text.is_char_boundary(span.start) || !text.is_char_boundary(span.end) {
                return error("is not on character boundaries");
            }
            if span.start < previous {
                return error("overlaps or precedes the token before it");
            }
            let piece = span.slice(&text);
            if piece.starts_with(is_separator) || piece.ends_with(is_separator) {
                return error("starts or ends with whitespace");
            }
            previous = span.end;
        }
        if let Some(starts) = sentence_starts {
            for (position, &start) in starts.iter().enumerate() {
                if start >= tokens.len() || (position > 0 && start <= starts[position - 1]) {
                    return Err(DocumentError(
                        "sentence starts must be increasing token indices".to_owned(),
                    ));
                }
            }
        }
        Self::assemble(text, tokens, sentence_starts)
    }

    /// The document of valid tokens; fails only on a token that runs over a paragraph boundary
    pub(crate) fn assemble(
        text: String,
        tokens: Vec<Span>,
        sentence_starts: Option<&[usize]>,
    ) -> Result<Self, DocumentError> {
        let mut starts = Vec::new();
        let mut paragraph_starts = Vec::new();
        let mut lines = Vec::new();
        let mut token = 0;
        for paragraph in layout::paragraphs(&text) {
            let end = paragraph.last().map_or(0, |line| line.end);
            let first = token;
            while token < tokens.len() && tokens[token].start < end {
                if tokens[token].end > end {
                    return Err(DocumentError(format!(
                        "token {token}: runs over a paragraph boundary"
                    )));
                }
                token += 1;
            }
            if token == first {
                continue;
            }
            paragraph_starts.push(first);
            starts.push(first);
            lines.extend(paragraph.iter().enumerate().map(|(index, &span)| Line {
                span,
                break_before: (index > 0).then_some(LineBreak::Author),
            }));
        }
        if let Some(given) = sentence_starts {
            starts.extend_from_slice(given);
            starts.sort_unstable();
            starts.dedup();
        }
        let paragraph_sentences = paragraph_starts
            .iter()
            .map(|first| starts.partition_point(|start| start < first))
            .chain([starts.len()])
            .collect();
        let count = tokens.len();
        starts.push(count);
        Ok(Document {
            text,
            tokens,
            token_words: (0..=count).collect(),
            word_tokens: (0..count).collect(),
            forms: Vec::new(),
            columns: Default::default(),
            heads: Vec::new(),
            sentence_tokens: starts,
            paragraph_sentences,
            lines,
            verbatim: None,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tokens(&self) -> &[Span] {
        &self.tokens
    }

    /// The words of a token
    pub fn token_words(&self, token: usize) -> Range<usize> {
        self.token_words[token]..self.token_words[token + 1]
    }

    pub fn word_count(&self) -> usize {
        self.word_tokens.len()
    }

    /// The token that holds a word
    pub fn word_token(&self, word: usize) -> usize {
        self.word_tokens[word]
    }

    /// The form of a word: the text of its token unless it is a word of a multiword token
    pub fn form(&self, word: usize) -> &str {
        match self.forms.get(word) {
            Some(Some(form)) => form,
            _ => self.tokens[self.word_tokens[word]].slice(&self.text),
        }
    }

    pub fn column(&self, column: Column, word: usize) -> Option<&str> {
        self.columns[column as usize]
            .get(word)
            .and_then(Option::as_deref)
    }

    /// The head of a word: the ID of a word of its sentence or 0 for the root
    pub fn head(&self, word: usize) -> Option<u32> {
        self.heads.get(word).copied().flatten()
    }

    pub fn sentence_count(&self) -> usize {
        self.sentence_tokens.len() - 1
    }

    pub fn sentence(&self, index: usize) -> Sentence {
        let tokens = self.sentence_tokens[index]..self.sentence_tokens[index + 1];
        Sentence {
            span: self.cover(&tokens),
            words: self.token_words[tokens.start]..self.token_words[tokens.end],
            tokens,
        }
    }

    /// The sentence that holds a token
    pub fn token_sentence(&self, token: usize) -> usize {
        self.sentence_tokens
            .partition_point(|&start| start <= token)
            - 1
    }

    /// The sentence that holds a word
    pub fn word_sentence(&self, word: usize) -> usize {
        self.token_sentence(self.word_tokens[word])
    }

    /// The ID of a word in its sentence, from 1
    pub fn word_id(&self, word: usize) -> usize {
        let first = self.sentence_tokens[self.word_sentence(word)];
        word - self.token_words[first] + 1
    }

    pub fn paragraph_count(&self) -> usize {
        self.paragraph_sentences.len() - 1
    }

    pub fn paragraph(&self, index: usize) -> Paragraph {
        let sentences = self.paragraph_sentences[index]..self.paragraph_sentences[index + 1];
        let tokens = self.sentence_tokens[sentences.start]..self.sentence_tokens[sentences.end];
        Paragraph {
            span: self.cover(&tokens),
            sentences,
            tokens,
        }
    }

    /// The paragraph that holds a sentence
    pub fn sentence_paragraph(&self, sentence: usize) -> usize {
        self.paragraph_sentences
            .partition_point(|&start| start <= sentence)
            - 1
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    fn cover(&self, tokens: &Range<usize>) -> Span {
        Span {
            start: self.tokens[tokens.start].start,
            end: self.tokens[tokens.end - 1].end,
        }
    }

    /// Set word columns and heads at once; nothing changes if a value is wrong.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if a list has not one value per word, a text value is empty or holds a tab or a line
    /// break, or a head is not 0 or the ID of a word of its sentence.
    pub fn set_columns(
        &mut self,
        columns: Vec<(Column, Vec<Option<String>>)>,
        heads: Option<Vec<Option<u32>>>,
    ) -> Result<(), DocumentError> {
        let count = self.word_count();
        let length = |name: &str, length: usize| {
            if length == count {
                Ok(())
            } else {
                Err(DocumentError(format!(
                    "{name} has {length} values for {count} words"
                )))
            }
        };
        for (column, values) in &columns {
            length(column.name(), values.len())?;
            for (word, value) in values.iter().enumerate() {
                if let Some(value) = value
                    && (value.is_empty() || value.contains(['\t', '\n', '\r']))
                {
                    return Err(DocumentError(format!(
                        "{} of word {word} is empty or holds a tab or a line break",
                        column.name()
                    )));
                }
            }
        }
        if let Some(heads) = &heads {
            length("head", heads.len())?;
            for (word, head) in heads.iter().enumerate() {
                if let Some(head) = *head {
                    let words = self.sentence(self.word_sentence(word)).words.len();
                    if head as usize > words {
                        return Err(DocumentError(format!(
                            "head of word {word} is {head}, but its sentence has {words} words"
                        )));
                    }
                }
            }
        }
        for (column, values) in columns {
            self.columns[column as usize] = if values.iter().all(Option::is_none) {
                Vec::new()
            } else {
                values
            };
        }
        if let Some(heads) = heads {
            self.heads = if heads.iter().all(Option::is_none) {
                Vec::new()
            } else {
                heads
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str, starts: Option<&[usize]>) -> Document {
        let mut tokens = Vec::new();
        layout::runs(
            text,
            Span {
                start: 0,
                end: text.len(),
            },
            &mut tokens,
        );
        Document::from_tokens(text.to_owned(), tokens, starts).unwrap()
    }

    #[test]
    fn paragraphs_start_sentences() {
        let doc = document("Ёж идёт.\nКот спит.\n\n Утро.", Some(&[1, 3]));
        assert_eq!(doc.sentence_tokens, [0, 1, 3, 4, 5]);
        assert_eq!(doc.paragraph_sentences, [0, 3, 4]);
        assert_eq!(doc.paragraph(1).span.slice(doc.text()), "Утро.");
        assert_eq!(doc.sentence(1).span.slice(doc.text()), "идёт.\nКот");
        assert_eq!(doc.sentence_paragraph(2), 0);
        assert_eq!(doc.token_sentence(4), 3);
        let lines: Vec<(&str, Option<LineBreak>)> = doc
            .lines()
            .iter()
            .map(|line| (line.span.slice(doc.text()), line.break_before))
            .collect();
        assert_eq!(
            lines,
            [
                ("Ёж идёт.", None),
                ("Кот спит.", Some(LineBreak::Author)),
                ("Утро.", None)
            ]
        );
    }

    #[test]
    fn words_are_tokens() {
        let doc = document("Ёж идёт", None);
        assert_eq!(doc.word_count(), 2);
        assert_eq!(doc.form(1), "идёт");
        assert_eq!(doc.word_token(1), 1);
        assert_eq!(doc.token_words(1), 1..2);
        assert_eq!(doc.word_id(1), 2);
        assert_eq!(doc.column(Column::Lemma, 0), None);
        assert_eq!(doc.head(0), None);
    }

    #[test]
    fn blank_text() {
        let doc = document(" \n\n ", None);
        assert_eq!(
            (
                doc.sentence_count(),
                doc.paragraph_count(),
                doc.lines().len()
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn wrong_tokens() {
        let text = "Ёж идёт\n\nдомой".to_owned();
        let span = |start, end| Span { start, end };
        for (tokens, message) in [
            (vec![span(0, 0)], "token 0: is empty"),
            (vec![span(0, 99)], "token 0: ends after the text"),
            (vec![span(0, 1)], "token 0: is not on character boundaries"),
            (
                vec![span(5, 13), span(0, 4)],
                "token 1: overlaps or precedes the token before it",
            ),
            (vec![span(4, 13)], "token 0: starts or ends with whitespace"),
            (vec![span(5, 19)], "token 0: runs over a paragraph boundary"),
        ] {
            assert_eq!(
                Document::from_tokens(text.clone(), tokens, None).unwrap_err(),
                DocumentError(message.to_owned())
            );
        }
        for starts in [&[0, 0][..], &[1, 0], &[3]] {
            assert!(
                Document::from_tokens(text.clone(), vec![span(0, 4), span(5, 13)], Some(starts))
                    .is_err()
            );
        }
    }

    #[test]
    fn set_columns() {
        let mut doc = document("Ёж идёт. Кот", Some(&[2]));
        doc.set_columns(
            vec![(Column::Upos, vec![Some("NOUN".to_owned()), None, None])],
            Some(vec![Some(2), Some(0), Some(0)]),
        )
        .unwrap();
        assert_eq!(doc.column(Column::Upos, 0), Some("NOUN"));
        assert_eq!(doc.head(0), Some(2));
        for (columns, heads, message) in [
            (
                vec![(Column::Lemma, vec![None])],
                None,
                "lemma has 1 values for 3 words",
            ),
            (
                vec![(Column::Feats, vec![None, Some("a\tb".to_owned()), None])],
                None,
                "feats of word 1 is empty or holds a tab or a line break",
            ),
            (
                vec![],
                Some(vec![None, None, Some(2)]),
                "head of word 2 is 2, but its sentence has 1 words",
            ),
        ] {
            assert_eq!(
                doc.set_columns(columns, heads).unwrap_err(),
                DocumentError(message.to_owned())
            );
        }
        assert_eq!(doc.column(Column::Upos, 0), Some("NOUN"));
        doc.set_columns(vec![(Column::Upos, vec![None, None, None])], None)
            .unwrap();
        assert!(doc.columns[Column::Upos as usize].is_empty());
    }
}

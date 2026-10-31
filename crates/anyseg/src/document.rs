//! The document model: tokens, words with the columns of CoNLL-U, sentences, paragraphs and lines

use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::Span;
use crate::layout::{self, is_separator};

/// How a line of a paragraph begins after the line before it
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LineBreak {
    /// A break the author made: a verse line, a list item, a paragraph per line
    Author,
    /// A wrap at the width of the page, as in text extracted from PDF
    Typeset,
}

impl LineBreak {
    /// The name of the kind: `"author"` or `"typeset"`
    pub fn name(self) -> &'static str {
        match self {
            LineBreak::Author => "author",
            LineBreak::Typeset => "typeset",
        }
    }
}

/// A line of a paragraph
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Line {
    /// The line without the separators around it
    pub span: Span,
    /// `None` for the first line of a paragraph
    pub break_before: Option<LineBreak>,
}

/// A sentence of a [`Document`]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Sentence {
    /// From the start of its first token to the end of its last one
    pub span: Span,
    /// The indices of its tokens
    pub tokens: Range<usize>,
    /// The indices of its words
    pub words: Range<usize>,
}

/// A paragraph of a [`Document`]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Paragraph {
    /// From the start of its first token to the end of its last one
    pub span: Span,
    /// The indices of its sentences
    pub sentences: Range<usize>,
    /// The indices of its tokens
    pub tokens: Range<usize>,
}

/// A column of CoNLL-U that a word holds as text; `head` is a number and `form` comes from the tokens
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    /// LEMMA: the lemma or the stem
    Lemma,
    /// UPOS: the universal part of speech
    Upos,
    /// XPOS: the part of speech of the language
    Xpos,
    /// FEATS: the morphological features
    Feats,
    /// DEPREL: the relation to the head
    Deprel,
    /// DEPS: the enhanced dependency graph
    Deps,
    /// MISC: any other annotation
    Misc,
}

impl Column {
    /// The columns in the order of CoNLL-U
    pub const ALL: [Column; 7] = [
        Column::Lemma,
        Column::Upos,
        Column::Xpos,
        Column::Feats,
        Column::Deprel,
        Column::Deps,
        Column::Misc,
    ];

    /// The name of the column in lowercase: `"lemma"`, `"upos"`
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

    /// The column with a name in lowercase
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.name() == name)
    }
}

/// Wrong tokens, sentence starts or column values
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentError(pub(crate) String);

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DocumentError {}

/// A vector that grew by doubling, copied to one of its length: a document keeps only what it holds. Shrinking it
/// in place leaves the rest of a large block that some allocators do not reuse (macOS)
fn fitted<T: Copy>(vector: Vec<T>) -> Vec<T> {
    if vector.capacity() > vector.len() {
        vector.as_slice().to_vec()
    } else {
        vector
    }
}

/// The bit of an end in [`Values`] that tells that the item has a value, which may be empty
const PRESENT: u32 = 1 << 31;

/// Optional strings, one per item, in one buffer: the end of every item in it, with the bit [`PRESENT`]. No items
/// at all when none has a value
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Values {
    text: String,
    ends: Vec<u32>,
}

/// The values of one document overflow the buffer of [`Values`]: 2 GiB
#[derive(Debug)]
pub(crate) struct TooLarge;

impl Values {
    /// The values of the items in order
    pub(crate) fn collect<'a>(
        items: impl IntoIterator<Item = Option<&'a str>>,
    ) -> Result<Self, TooLarge> {
        let mut values = Values::default();
        for item in items {
            values.push(item)?;
        }
        Ok(values.finished())
    }

    pub(crate) fn push(&mut self, item: Option<&str>) -> Result<(), TooLarge> {
        let mut end = self.text.len();
        if let Some(value) = item {
            end += value.len();
            if end >= PRESENT as usize {
                return Err(TooLarge);
            }
            self.text.push_str(value);
        }
        // Fits: the buffer stays below PRESENT
        let end = end as u32;
        self.ends
            .push(if item.is_some() { end | PRESENT } else { end });
        Ok(())
    }

    /// The values with no spare capacity, or none if no item has a value
    pub(crate) fn finished(self) -> Self {
        if self.ends.iter().all(|end| end & PRESENT == 0) {
            return Values::default();
        }
        Values {
            text: self.text.as_str().to_owned(),
            ends: fitted(self.ends),
        }
    }

    pub(crate) fn get(&self, item: usize) -> Option<&str> {
        let end = *self.ends.get(item)?;
        if end & PRESENT == 0 {
            return None;
        }
        let start = item
            .checked_sub(1)
            .map_or(0, |previous| self.ends[previous] & !PRESENT);
        Some(&self.text[start as usize..(end & !PRESENT) as usize])
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
}

/// The parts of CoNLL-U that a document read from it writes back as they were
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Verbatim {
    /// The comment lines of every sentence, joined by line breaks
    pub(crate) comments: Values,
    /// Columns 2-10 of the line of every multiword token, by token
    pub(crate) multiword: Vec<(usize, String)>,
    /// The empty nodes: the sentence, the number of its token and word lines before the node, and the line
    pub(crate) empty: Vec<(usize, usize, String)>,
}

/// A segmented text. Tokens are spans of the text, which is never changed; a token holds one word or, for a
/// multiword token, several. Sentences hold whole tokens, paragraphs hold whole sentences, and lines are those of
/// the paragraphs. Words have the columns of CoNLL-U, empty until set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub(crate) text: String,
    pub(crate) tokens: Vec<Span>,
    /// The first word of every token, and the number of words; both empty when every token is one word
    pub(crate) token_words: Vec<usize>,
    pub(crate) word_tokens: Vec<usize>,
    /// The forms that differ from the text of their token; empty when there are none
    pub(crate) forms: Values,
    /// The columns, none until one is set: a document of a short text stays small
    pub(crate) columns: Option<Box<[Values; 7]>>,
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
    /// before it, starts or ends with whitespace, or runs over a paragraph boundary, if the text between the
    /// tokens holds anything but whitespace, or if the sentence starts are not increasing token indices.
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
            if !text[previous..span.start].chars().all(is_separator) {
                return error("the text before it holds characters that no token has");
            }
            previous = span.end;
        }
        if !text[previous..].chars().all(is_separator) {
            return Err(DocumentError(
                "the text after the last token holds characters that no token has".to_owned(),
            ));
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

    /// The document of a text as the segmenter splits it: tokens, the first token of every sentence, the first
    /// sentence of every paragraph and the lines
    pub(crate) fn segmented(
        text: String,
        tokens: Vec<Span>,
        mut sentence_tokens: Vec<usize>,
        mut paragraph_sentences: Vec<usize>,
        lines: Vec<Line>,
    ) -> Self {
        let count = tokens.len();
        paragraph_sentences.push(sentence_tokens.len());
        sentence_tokens.push(count);
        Document {
            text,
            tokens: fitted(tokens),
            token_words: Vec::new(),
            word_tokens: Vec::new(),
            forms: Values::default(),
            columns: None,
            heads: Vec::new(),
            sentence_tokens: fitted(sentence_tokens),
            paragraph_sentences: fitted(paragraph_sentences),
            lines: fitted(lines),
            verbatim: None,
        }
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
        starts.push(tokens.len());
        Ok(Document {
            text,
            tokens: fitted(tokens),
            token_words: Vec::new(),
            word_tokens: Vec::new(),
            forms: Values::default(),
            columns: None,
            heads: Vec::new(),
            sentence_tokens: fitted(starts),
            paragraph_sentences,
            lines: fitted(lines),
            verbatim: None,
        })
    }

    /// The text of the document
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The tokens in order
    pub fn tokens(&self) -> &[Span] {
        &self.tokens
    }

    /// The words of a token.
    ///
    /// # Panics
    ///
    /// If there is no such token.
    pub fn token_words(&self, token: usize) -> Range<usize> {
        self.check_token(token);
        self.first_word(token)..self.first_word(token + 1)
    }

    /// The first word of a token, or the number of words after the last token
    fn first_word(&self, token: usize) -> usize {
        if self.token_words.is_empty() {
            token
        } else {
            self.token_words[token]
        }
    }

    fn check_token(&self, token: usize) {
        let count = self.tokens.len();
        assert!(
            token < count,
            "token {token} is out of range for {count} tokens"
        );
    }

    /// The number of words
    pub fn word_count(&self) -> usize {
        if self.word_tokens.is_empty() {
            self.tokens.len()
        } else {
            self.word_tokens.len()
        }
    }

    /// The token that holds a word.
    ///
    /// # Panics
    ///
    /// If there is no such word.
    pub fn word_token(&self, word: usize) -> usize {
        self.check_word(word);
        if self.word_tokens.is_empty() {
            word
        } else {
            self.word_tokens[word]
        }
    }

    /// The form of a word: the text of its token unless it is a word of a multiword token.
    ///
    /// # Panics
    ///
    /// If there is no such word.
    pub fn form(&self, word: usize) -> &str {
        match self.forms.get(word) {
            Some(form) => form,
            None => self.tokens[self.word_token(word)].slice(&self.text),
        }
    }

    /// The value of a column of a word, `None` until set.
    ///
    /// # Panics
    ///
    /// If there is no such word.
    pub fn column(&self, column: Column, word: usize) -> Option<&str> {
        self.check_word(word);
        self.columns.as_ref()?[column as usize].get(word)
    }

    /// The head of a word: the ID of a word of its sentence or 0 for the root, `None` until set.
    ///
    /// # Panics
    ///
    /// If there is no such word.
    pub fn head(&self, word: usize) -> Option<u32> {
        self.check_word(word);
        self.heads.get(word).copied().flatten()
    }

    fn check_word(&self, word: usize) {
        let count = self.word_count();
        assert!(
            word < count,
            "word {word} is out of range for {count} words"
        );
    }

    /// The number of sentences
    pub fn sentence_count(&self) -> usize {
        self.sentence_tokens.len() - 1
    }

    /// A sentence by its index.
    ///
    /// # Panics
    ///
    /// If there is no such sentence.
    pub fn sentence(&self, index: usize) -> Sentence {
        let tokens = self.sentence_tokens[index]..self.sentence_tokens[index + 1];
        Sentence {
            span: self.cover(&tokens),
            words: self.first_word(tokens.start)..self.first_word(tokens.end),
            tokens,
        }
    }

    /// The sentence that holds a token.
    ///
    /// # Panics
    ///
    /// If there is no such token.
    pub fn token_sentence(&self, token: usize) -> usize {
        self.check_token(token);
        self.sentence_tokens
            .partition_point(|&start| start <= token)
            - 1
    }

    /// The sentence that holds a word.
    ///
    /// # Panics
    ///
    /// If there is no such word.
    pub fn word_sentence(&self, word: usize) -> usize {
        self.token_sentence(self.word_token(word))
    }

    /// The ID of a word in its sentence, from 1.
    ///
    /// # Panics
    ///
    /// If there is no such word.
    pub fn word_id(&self, word: usize) -> usize {
        let first = self.sentence_tokens[self.word_sentence(word)];
        word - self.first_word(first) + 1
    }

    /// The number of paragraphs
    pub fn paragraph_count(&self) -> usize {
        self.paragraph_sentences.len() - 1
    }

    /// A paragraph by its index.
    ///
    /// # Panics
    ///
    /// If there is no such paragraph.
    pub fn paragraph(&self, index: usize) -> Paragraph {
        let sentences = self.paragraph_sentences[index]..self.paragraph_sentences[index + 1];
        let tokens = self.sentence_tokens[sentences.start]..self.sentence_tokens[sentences.end];
        Paragraph {
            span: self.cover(&tokens),
            sentences,
            tokens,
        }
    }

    /// The paragraph that holds a sentence.
    ///
    /// # Panics
    ///
    /// If there is no such sentence.
    pub fn sentence_paragraph(&self, sentence: usize) -> usize {
        let count = self.sentence_count();
        assert!(
            sentence < count,
            "sentence {sentence} is out of range for {count} sentences"
        );
        self.paragraph_sentences
            .partition_point(|&start| start <= sentence)
            - 1
    }

    /// The lines of the paragraphs in order
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
    /// [`DocumentError`] if a list has not one value per word, a value is empty, a lemma or MISC holds a tab or
    /// a line break or another value holds whitespace, or a head is not 0 or the ID of a word of its sentence.
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
            // CoNLL-U allows spaces in lemmas and MISC, not in the other columns
            let (wrong, what): (fn(char) -> bool, &str) = match column {
                Column::Lemma | Column::Misc => {
                    (|c| matches!(c, '\t' | '\n' | '\r'), "a tab or a line break")
                }
                _ => (char::is_whitespace, "whitespace"),
            };
            for (word, value) in values.iter().enumerate() {
                if let Some(value) = value
                    && (value.is_empty() || value.contains(wrong))
                {
                    return Err(DocumentError(format!(
                        "{} of word {word} is empty or holds {what}",
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
        let columns = columns
            .into_iter()
            .map(|(column, values)| {
                Values::collect(values.iter().map(Option::as_deref))
                    .map(|values| (column, values))
                    .map_err(|TooLarge| {
                        DocumentError(format!("{} of the document exceeds 2 GiB", column.name()))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut all = self.columns.take().map(|all| *all).unwrap_or_default();
        for (column, values) in columns {
            all[column as usize] = values;
        }
        self.columns = (!all.iter().all(Values::is_empty)).then(|| Box::new(all));
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
    fn indices_out_of_range() {
        let doc = document("Ёж идёт", None);
        for call in [
            |doc: &Document| {
                let _ = doc.column(Column::Lemma, 2);
            },
            |doc: &Document| {
                let _ = doc.head(2);
            },
            |doc: &Document| {
                let _ = doc.form(2);
            },
            |doc: &Document| {
                let _ = doc.token_sentence(2);
            },
            |doc: &Document| {
                let _ = doc.sentence_paragraph(1);
            },
            |doc: &Document| {
                let _ = doc.sentence(1);
            },
        ] {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(&doc)));
            assert!(result.is_err());
        }
    }

    #[test]
    fn fitted_vectors_are_new_blocks() {
        // Shrinking in place would keep the block, and its rest would not be reused on macOS
        let mut vector = Vec::with_capacity(200_000);
        vector.resize(150_000, Span { start: 0, end: 1 });
        let block = vector.as_ptr();
        let vector = fitted(vector);
        assert_eq!(vector.capacity(), vector.len());
        assert_ne!(vector.as_ptr(), block);
    }

    #[test]
    fn values() {
        let values = Values::collect([Some("NOUN"), None, Some(""), Some("VERB")]).unwrap();
        assert_eq!(
            (0..5).map(|item| values.get(item)).collect::<Vec<_>>(),
            [Some("NOUN"), None, Some(""), Some("VERB"), None]
        );
        assert!(Values::collect([None, None]).unwrap().is_empty());
        let mut values = Values::default();
        let large = "x".repeat(1 << 30);
        values.push(Some(&large)).unwrap();
        assert!(values.push(Some(&large)).is_err());
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
                vec![span(0, 4), span(5, 13), span(0, 4)],
                "token 2: overlaps or precedes the token before it",
            ),
            (vec![span(4, 13)], "token 0: starts or ends with whitespace"),
            (
                vec![span(5, 13), span(15, 25)],
                "token 0: the text before it holds characters that no token has",
            ),
            (
                vec![span(0, 4), span(5, 13)],
                "the text after the last token holds characters that no token has",
            ),
            (
                vec![span(0, 4), span(5, 25)],
                "token 1: runs over a paragraph boundary",
            ),
        ] {
            assert_eq!(
                Document::from_tokens(text.clone(), tokens, None).unwrap_err(),
                DocumentError(message.to_owned())
            );
        }
        let tokens = vec![span(0, 4), span(5, 13), span(15, 25)];
        for starts in [&[0, 0][..], &[1, 0], &[3]] {
            assert_eq!(
                Document::from_tokens(text.clone(), tokens.clone(), Some(starts)).unwrap_err(),
                DocumentError("sentence starts must be increasing token indices".to_owned())
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
                vec![(Column::Feats, vec![None, Some("a b".to_owned()), None])],
                None,
                "feats of word 1 is empty or holds whitespace",
            ),
            (
                vec![(Column::Lemma, vec![None, Some("a\nb".to_owned()), None])],
                None,
                "lemma of word 1 is empty or holds a tab or a line break",
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
        assert!(doc.columns.is_none());
        assert!(doc.token_words.is_empty() && doc.word_tokens.is_empty());
    }
}

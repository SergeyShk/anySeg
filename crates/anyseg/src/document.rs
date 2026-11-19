//! The document model: tokens, words with the columns of CoNLL-U, sentences, paragraphs and lines

use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::Span;
use crate::conllu;
use crate::lang::Tables;
use crate::layout::{self, is_separator};
use crate::tokens::Exceptions;
use crate::words;

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

/// A column of CoNLL-U that a word holds as text; `head` is a number and `form` is set with the tokens
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
pub(crate) const PRESENT: u32 = 1 << 31;

/// Optional strings, one per item, in one buffer: the end of every item in it, with the bit [`PRESENT`]. No items
/// at all when none has a value
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Values {
    pub(crate) text: String,
    pub(crate) ends: Vec<u32>,
}

/// The values of one document overflow the buffer of [`Values`]: 2 GiB
#[derive(Debug)]
pub(crate) struct TooLarge;

impl Values {
    /// The values of the items in order
    #[cfg(test)]
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

fn check_starts(starts: &[usize], tokens: usize) -> Result<(), DocumentError> {
    for (position, &start) in starts.iter().enumerate() {
        if start >= tokens || (position > 0 && start <= starts[position - 1]) {
            return Err(DocumentError(
                "sentence starts must be increasing token indices".to_owned(),
            ));
        }
    }
    Ok(())
}

fn wrong_length(name: &str, given: usize, words: usize) -> DocumentError {
    DocumentError(format!("{name} has {given} values for {words} words"))
}

/// The values of a column, checked for a document of a number of words: the Python module fills it from a list while
/// the document may be in use, and sets it under the lock
#[doc(hidden)]
#[derive(Debug)]
pub struct CheckedColumn {
    column: Column,
    words: usize,
    values: Values,
}

impl CheckedColumn {
    /// The values of a column of a document of `words` words.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if there is not one value per word, a value is empty, a lemma or MISC holds a tab or a line
    /// break or another value holds whitespace.
    pub fn new<S: AsRef<str>>(
        column: Column,
        values: impl IntoIterator<Item = Option<S>>,
        words: usize,
    ) -> Result<Self, DocumentError> {
        // CoNLL-U allows spaces in lemmas and MISC, not in the other columns
        let (wrong, what): (fn(char) -> bool, &str) = match column {
            Column::Lemma | Column::Misc => {
                (|c| matches!(c, '\t' | '\n' | '\r'), "a tab or a line break")
            }
            _ => (char::is_whitespace, "whitespace"),
        };
        let mut collected = Values::default();
        collected.ends.reserve_exact(words);
        let (mut given, mut invalid, mut large) = (0, None, false);
        for value in values {
            let value = value.as_ref().map(|value| value.as_ref());
            if value.is_some_and(|value| value.is_empty() || value.contains(wrong)) {
                invalid.get_or_insert(given);
            }
            if given < words {
                large |= collected.push(value).is_err();
            }
            given += 1;
        }
        if given != words {
            return Err(wrong_length(column.name(), given, words));
        }
        if let Some(word) = invalid {
            return Err(DocumentError(format!(
                "{} of word {word} is empty or holds {what}",
                column.name()
            )));
        }
        if large {
            return Err(DocumentError(format!(
                "{} of the document exceeds 2 GiB",
                column.name()
            )));
        }
        Ok(CheckedColumn {
            column,
            words,
            values: collected.finished(),
        })
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
    pub(crate) token_words: Arc<[usize]>,
    pub(crate) word_tokens: Arc<[usize]>,
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
            check_starts(starts, tokens.len())?;
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
            token_words: Arc::from([]),
            word_tokens: Arc::from([]),
            forms: Values::default(),
            columns: None,
            heads: Vec::new(),
            sentence_tokens: fitted(sentence_tokens),
            paragraph_sentences: fitted(paragraph_sentences),
            lines: fitted(lines),
            verbatim: None,
        }
    }

    /// Gives the words of the multiword tokens of a language to a segmented document. A token whose forms do not fit
    /// in 2 GiB stays one word
    pub(crate) fn expand_tokens(&mut self, tables: &Tables, exceptions: &Exceptions) {
        let mut multiword = Vec::new();
        for sentence in self.sentence_tokens.windows(2) {
            for token in sentence[0]..sentence[1] {
                let piece = self.tokens[token].slice(&self.text);
                if !words::may_hold_several(tables, piece) {
                    continue;
                }
                // A token at the start of a line the author broke, as in verse, is not after a word: «…volver¶Al
                // lugar»; after a wrap at the width of the page it is: «Vicente¶Del Bosque»
                let start = self.tokens[token].start;
                let line = self.lines.partition_point(|line| line.span.start <= start);
                let line_start = token > sentence[0]
                    && line > 0
                    && self.lines[line - 1].span.start == start
                    && self.lines[line - 1].break_before == Some(LineBreak::Author);
                let before = |back: usize| {
                    (!line_start && token >= sentence[0] + back)
                        .then(|| self.tokens[token - back].slice(&self.text))
                };
                let after =
                    || (token + 1 < sentence[1]).then(|| self.tokens[token + 1].slice(&self.text));
                if let Some(forms) = words::expand(tables, piece, before(1), || before(2), after)
                    && !exceptions.tokens.contains(piece)
                {
                    multiword.push((token, forms));
                }
            }
        }
        let mut size = 0;
        multiword.retain(|(_, forms): &(usize, Vec<String>)| {
            let length: usize = forms.iter().map(String::len).sum();
            let fits = size + length < PRESENT as usize;
            if fits {
                size += length;
            }
            fits
        });
        self.set_words(multiword);
    }

    /// The document with several words in some of its tokens: a token and the forms of its words, in the order of
    /// the tokens. A form holds what CoNLL-U allows, whatever the text of its token.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the document has columns, several words in a token or was read from CoNLL-U, whose lines
    /// it writes back, the tokens are not increasing indices of tokens of the document, a token has fewer than two forms, a form is empty or holds a tab
    /// or a line break, or the forms exceed 2 GiB.
    pub fn with_multiword_tokens<I, S>(mut self, tokens: I) -> Result<Self, DocumentError>
    where
        I: IntoIterator<Item = (usize, Vec<S>)>,
        S: Into<String>,
    {
        if self.columns.is_some()
            || !self.heads.is_empty()
            || !self.word_tokens.is_empty()
            || self.verbatim.is_some()
        {
            return Err(DocumentError(
                "multiword tokens must be set first, on a document of tokens".to_owned(),
            ));
        }
        let mut multiword = Vec::new();
        let (mut next, mut size) = (0, 0);
        for (token, forms) in tokens {
            if token < next || token >= self.tokens.len() {
                return Err(DocumentError(format!(
                    "multiword token {token} is not in order among {} tokens",
                    self.tokens.len()
                )));
            }
            next = token + 1;
            let forms: Vec<String> = forms.into_iter().map(Into::into).collect();
            if forms.len() < 2 {
                return Err(DocumentError(format!(
                    "multiword token {token} has fewer than two forms"
                )));
            }
            if forms
                .iter()
                .any(|form| form.is_empty() || form.contains(['\t', '\n', '\r']))
            {
                return Err(DocumentError(format!(
                    "a form of multiword token {token} is empty or holds a tab or a line break"
                )));
            }
            size += forms.iter().map(String::len).sum::<usize>();
            if size >= PRESENT as usize {
                return Err(DocumentError(
                    "the forms of the document exceed 2 GiB".to_owned(),
                ));
            }
            multiword.push((token, forms));
        }
        self.set_words(multiword);
        Ok(self)
    }

    /// Gives some tokens several words, in the order of the tokens; their forms fit in 2 GiB
    fn set_words(&mut self, multiword: Vec<(usize, Vec<String>)>) {
        if multiword.is_empty() {
            return;
        }
        let mut forms = Values::default();
        let mut token_words = Vec::with_capacity(self.tokens.len() + 1);
        let extra: usize = multiword.iter().map(|(_, words)| words.len() - 1).sum();
        let mut word_tokens = Vec::with_capacity(self.tokens.len() + extra);
        let mut multiword = multiword.into_iter().peekable();
        for token in 0..self.tokens.len() {
            token_words.push(word_tokens.len());
            match multiword.next_if(|(multi, _)| *multi == token) {
                Some((_, words)) => {
                    for word in &words {
                        // Fits: checked above
                        let _ = forms.push(Some(word));
                        word_tokens.push(token);
                    }
                }
                None => {
                    // Fits: no text
                    let _ = forms.push(None);
                    word_tokens.push(token);
                }
            }
        }
        token_words.push(word_tokens.len());
        self.token_words = token_words.into();
        self.word_tokens = word_tokens.into();
        self.forms = forms.finished();
    }

    /// The document with other words in some of its tokens: a token and the forms of its words, in the order of the
    /// tokens. Several forms make a multiword token, whatever the text of the token, and one form, the text of the
    /// token, a token of one word. The other words keep their forms and columns, the new ones have none; a document
    /// read from CoNLL-U keeps its comments and the lines of its other multiword tokens.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the document has heads, enhanced dependencies or empty nodes, which refer to the IDs of
    /// words, the tokens are not increasing indices of tokens of the document, a token has no forms, a form is empty
    /// or holds a tab or a line break, a single form is not the text of its token, whitespace aside, or the forms
    /// exceed 2 GiB.
    pub fn with_words<I, S>(mut self, tokens: I) -> Result<Self, DocumentError>
    where
        I: IntoIterator<Item = (usize, Vec<S>)>,
        S: Into<String>,
    {
        self.check_no_ids("words")?;
        let count = self.tokens.len();
        let mut changes = Vec::new();
        let mut next = 0;
        for (token, forms) in tokens {
            if token >= count {
                return Err(DocumentError(format!(
                    "token {token} is not in the document"
                )));
            }
            if token < next {
                return Err(DocumentError(
                    "the tokens of the words must be increasing indices".to_owned(),
                ));
            }
            next = token + 1;
            let forms: Vec<String> = forms.into_iter().map(Into::into).collect();
            if forms.is_empty() {
                return Err(DocumentError(format!("token {token} has no forms")));
            }
            if forms
                .iter()
                .any(|form| form.is_empty() || form.contains(['\t', '\n', '\r']))
            {
                return Err(DocumentError(format!(
                    "a form of token {token} is empty or holds a tab or a line break"
                )));
            }
            // CoNLL-U finds the form of a token of one word in the text of its sentence
            let piece = self.tokens[token].slice(&self.text);
            let whole = Span {
                start: 0,
                end: piece.len(),
            };
            if let [form] = forms.as_slice()
                && conllu::align(piece, 0, form) != Some(whole)
            {
                return Err(DocumentError(format!(
                    "the single form of token {token} is not its text"
                )));
            }
            changes.push((token, forms));
        }
        if changes.is_empty() {
            return Ok(self);
        }
        let too_large = |_| DocumentError("the forms of the document exceed 2 GiB".to_owned());
        let mut forms = Values::default();
        let mut columns: Option<[Values; 7]> = self.columns.as_ref().map(|_| Default::default());
        let mut token_words = Vec::with_capacity(count + 1);
        let mut word_tokens = Vec::with_capacity(self.word_count());
        // A token of one word in a document read from CoNLL-U carries the space after it in MISC
        let mut spaces = Vec::new();
        let mut changed = changes.iter().peekable();
        for token in 0..count {
            token_words.push(word_tokens.len());
            match changed.next_if(|(change, _)| *change == token) {
                Some((_, new)) => {
                    let text = self.tokens[token].slice(&self.text);
                    for form in new {
                        let kept = (new.len() > 1 || form != text).then_some(form.as_str());
                        forms.push(kept).map_err(too_large)?;
                        for values in columns.iter_mut().flatten() {
                            values.push(None).map_err(too_large)?;
                        }
                        word_tokens.push(token);
                    }
                    if new.len() == 1 && self.verbatim.is_some() {
                        spaces.extend(
                            self.layout(token, true)
                                .map(|misc| (word_tokens.len() - 1, misc)),
                        );
                    }
                }
                None => {
                    for word in self.token_words(token) {
                        forms.push(self.forms.get(word)).map_err(too_large)?;
                        if let (Some(columns), Some(old)) = (&mut columns, &self.columns) {
                            for (values, old) in columns.iter_mut().zip(old.iter()) {
                                values.push(old.get(word)).map_err(too_large)?;
                            }
                        }
                        word_tokens.push(token);
                    }
                }
            }
        }
        token_words.push(word_tokens.len());
        if !spaces.is_empty() {
            let mut misc = Values::default();
            let old = columns
                .as_ref()
                .map(|columns| &columns[Column::Misc as usize]);
            let mut spaces = spaces.into_iter().peekable();
            for word in 0..word_tokens.len() {
                let value = match spaces.next_if(|(spaced, _)| *spaced == word) {
                    Some((_, space)) => Some(space),
                    None => old.and_then(|old| old.get(word)).map(str::to_owned),
                };
                misc.push(value.as_deref()).map_err(too_large)?;
            }
            columns.get_or_insert_with(Default::default)[Column::Misc as usize] = misc;
        }
        if self.verbatim.is_some() {
            let generated: Vec<(usize, String)> = changes
                .iter()
                .filter(|(_, new)| new.len() > 1)
                .map(|&(token, _)| (token, self.multiword_line(token)))
                .collect();
            if let Some(verbatim) = self.verbatim.as_deref_mut() {
                let changed: Vec<usize> = changes.iter().map(|(token, _)| *token).collect();
                let mut lines = std::mem::take(&mut verbatim.multiword);
                lines.retain(|(token, _)| changed.binary_search(token).is_err());
                lines.extend(generated);
                lines.sort_unstable_by_key(|(token, _)| *token);
                verbatim.multiword = lines;
            }
        }
        if word_tokens.len() == count {
            token_words.clear();
            word_tokens.clear();
        }
        self.token_words = token_words.as_slice().into();
        self.word_tokens = word_tokens.as_slice().into();
        self.forms = forms.finished();
        self.columns = columns
            .map(|columns| columns.map(Values::finished))
            .filter(|columns| !columns.iter().all(Values::is_empty))
            .map(Box::new);
        Ok(self)
    }

    /// The document with other sentences, which start at the given tokens as in [`from_tokens`](Self::from_tokens);
    /// a paragraph always starts a sentence. Tokens, words, their columns, paragraphs and lines stay. A document
    /// read from CoNLL-U with other sentences is written as a segmented one: its comments and the lines of its
    /// multiword tokens are made anew.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the document has heads, enhanced dependencies or empty nodes, which refer to the IDs of
    /// words, or the starts are not increasing token indices.
    pub fn with_sentences(mut self, starts: &[usize]) -> Result<Self, DocumentError> {
        self.check_no_ids("sentences")?;
        let count = self.tokens.len();
        check_starts(starts, count)?;
        let paragraphs: Vec<usize> = self.paragraph_sentences[..self.paragraph_count()]
            .iter()
            .map(|&sentence| self.sentence_tokens[sentence])
            .collect();
        let mut sentence_tokens: Vec<usize> = paragraphs.iter().chain(starts).copied().collect();
        sentence_tokens.sort_unstable();
        sentence_tokens.dedup();
        let sentences = sentence_tokens.len();
        sentence_tokens.push(count);
        if sentence_tokens == self.sentence_tokens {
            return Ok(self);
        }
        self.paragraph_sentences = paragraphs
            .iter()
            .map(|first| sentence_tokens.partition_point(|start| start < first))
            .chain([sentences])
            .collect();
        self.sentence_tokens = fitted(sentence_tokens);
        self.verbatim = None;
        Ok(self)
    }

    /// The first sentences of a segmented document, with the text up to `end`: at or after the end of their last token
    /// and before the next one. Their tokens, words, paragraphs and lines are as the whole document has them
    pub(crate) fn truncated(mut self, sentences: usize, end: usize) -> Document {
        let tokens = self.sentence_tokens[sentences];
        let words = self.first_word(tokens);
        self.text.truncate(end);
        self.tokens.truncate(tokens);
        self.tokens = fitted(std::mem::take(&mut self.tokens));
        if !self.word_tokens.is_empty() {
            if words == tokens {
                self.token_words = Arc::from([]);
                self.word_tokens = Arc::from([]);
            } else {
                self.token_words = self.token_words[..=tokens].into();
                self.word_tokens = self.word_tokens[..words].into();
            }
            let mut forms = Values::default();
            for word in 0..words {
                // Fits: no larger than before
                let _ = forms.push(self.forms.get(word));
            }
            self.forms = forms.finished();
        }
        self.sentence_tokens.truncate(sentences + 1);
        let paragraphs = self
            .paragraph_sentences
            .partition_point(|&start| start < sentences);
        self.paragraph_sentences.truncate(paragraphs);
        self.paragraph_sentences.push(sentences);
        let last = self.tokens.last().map_or(0, |span| span.end);
        self.lines.retain(|line| line.span.start < last);
        if let Some(line) = self.lines.last_mut() {
            line.span.end = line.span.end.min(last);
        }
        self.lines = fitted(std::mem::take(&mut self.lines));
        self
    }

    /// Heads, enhanced dependencies and empty nodes refer to the IDs of words in their sentences
    fn check_no_ids(&self, what: &str) -> Result<(), DocumentError> {
        let deps = self
            .columns
            .as_ref()
            .is_some_and(|columns| !columns[Column::Deps as usize].is_empty());
        let empty = self
            .verbatim
            .as_ref()
            .is_some_and(|verbatim| !verbatim.empty.is_empty());
        if !self.heads.is_empty() || deps || empty {
            return Err(DocumentError(format!(
                "the {what} of a document with heads, enhanced dependencies or empty nodes cannot change: they refer \
                 to the IDs of words"
            )));
        }
        Ok(())
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
            token_words: Arc::from([]),
            word_tokens: Arc::from([]),
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

    /// The first word of every token with the number of words, and the token of every word, shared with the document;
    /// both are empty when every token is one word
    #[doc(hidden)]
    pub fn word_maps(&self) -> (Arc<[usize]>, Arc<[usize]>) {
        (Arc::clone(&self.token_words), Arc::clone(&self.word_tokens))
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

    /// Set word columns and heads at once; nothing changes if a value is wrong. The values of a column are any
    /// iterable of optional strings, so that they need not be on the heap one by one; heads alone are set by
    /// [`set_heads`](Self::set_heads).
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if a list has not one value per word, a value is empty, a lemma or MISC holds a tab or
    /// a line break or another value holds whitespace, or a head is not 0 or the ID of a word of its sentence.
    pub fn set_columns<V, S>(
        &mut self,
        columns: impl IntoIterator<Item = (Column, V)>,
        heads: Option<Vec<Option<u32>>>,
    ) -> Result<(), DocumentError>
    where
        V: IntoIterator<Item = Option<S>>,
        S: AsRef<str>,
    {
        let count = self.word_count();
        let columns = columns
            .into_iter()
            .map(|(column, values)| CheckedColumn::new(column, values, count))
            .collect::<Result<Vec<_>, _>>()?;
        self.set_checked_columns(columns, heads)
    }

    /// Set the heads of the words; nothing changes if a head is wrong.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if there is not one head per word or a head is not 0 or the ID of a word of its sentence.
    pub fn set_heads(&mut self, heads: Vec<Option<u32>>) -> Result<(), DocumentError> {
        self.set_checked_columns(Vec::new(), Some(heads))
    }

    /// Set columns that are checked already, and heads, at once; nothing changes if a head is wrong.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if a column is not of this document or a head is not 0 or the ID of a word of its sentence.
    #[doc(hidden)]
    pub fn set_checked_columns(
        &mut self,
        columns: Vec<CheckedColumn>,
        heads: Option<Vec<Option<u32>>>,
    ) -> Result<(), DocumentError> {
        let count = self.word_count();
        if let Some(column) = columns.iter().find(|column| column.words != count) {
            return Err(wrong_length(column.column.name(), column.words, count));
        }
        if let Some(heads) = &heads {
            if heads.len() != count {
                return Err(wrong_length("head", heads.len(), count));
            }
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
        let mut all = self.columns.take().map(|all| *all).unwrap_or_default();
        for column in columns {
            all[column.column as usize] = column.values;
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
    fn multiword_tokens() {
        let tokens = vec![
            Span { start: 0, end: 1 },
            Span { start: 2, end: 3 },
            Span { start: 4, end: 5 },
        ];
        let build = || Document::from_tokens("a b c".to_owned(), tokens.clone(), None).unwrap();
        let doc = build()
            .with_multiword_tokens([(1, vec!["x", "y"])])
            .unwrap();
        assert_eq!((doc.tokens().len(), doc.word_count()), (3, 4));
        assert_eq!(doc.token_words(1), 1..3);
        assert_eq!(
            (doc.form(0), doc.form(1), doc.form(2), doc.form(3)),
            ("a", "x", "y", "c")
        );
        assert_eq!(
            build()
                .with_multiword_tokens(Vec::<(usize, Vec<&str>)>::new())
                .unwrap(),
            build()
        );
        for (words, message) in [
            (
                vec![(3, vec!["x", "y"])],
                "multiword token 3 is not in order",
            ),
            (
                vec![(1, vec!["x", "y"]), (1, vec!["x", "y"])],
                "multiword token 1 is not in order",
            ),
            (vec![(0, vec!["x"])], "fewer than two forms"),
            (vec![(0, vec!["x", ""])], "is empty or holds"),
            (vec![(0, vec!["x", "y\tz"])], "is empty or holds"),
        ] {
            let error = build().with_multiword_tokens(words).unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
        let mut annotated = build();
        annotated
            .set_columns(
                [(Column::Upos, vec![Some("X"), Some("X"), Some("X")])],
                None,
            )
            .unwrap();
        assert!(
            annotated
                .with_multiword_tokens([(0, vec!["x", "y"])])
                .is_err()
        );
        let read = crate::read_conllu("# text = a\n1\ta\t_\t_\t_\t_\t_\t_\t_\t_\n\n").unwrap();
        assert!(
            read[0]
                .clone()
                .with_multiword_tokens([(0, vec!["x", "y"])])
                .is_err()
        );
        let twice = build()
            .with_multiword_tokens([(0, vec!["x", "y"])])
            .unwrap();
        assert!(twice.with_multiword_tokens([(1, vec!["x", "y"])]).is_err());
    }

    /// The forms of the words of every token
    fn words_of(doc: &Document) -> Vec<Vec<&str>> {
        (0..doc.tokens().len())
            .map(|token| doc.token_words(token).map(|word| doc.form(word)).collect())
            .collect()
    }

    fn upos(doc: &Document) -> Vec<Option<&str>> {
        (0..doc.word_count())
            .map(|word| doc.column(Column::Upos, word))
            .collect()
    }

    /// A sentence read from CoNLL-U, without heads, enhanced dependencies and empty nodes
    const READ: &str = "\
# newdoc id = d1
# sent_id = 1
# text = Dámelo del norte.
1\tDámelo\tdar\tVERB\t_\t_\t_\t_\t_\t_
2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_
2\tde\tde\tADP\t_\t_\t_\t_\t_\t_
3\tel\tel\tDET\t_\t_\t_\t_\t_\t_
4\tnorte\tnorte\tNOUN\t_\t_\t_\t_\t_\tSpaceAfter=No
5\t.\t.\tPUNCT\t_\t_\t_\t_\t_\t_

";

    #[test]
    fn with_words() {
        let mut doc = document("dámelo del norte", None)
            .with_multiword_tokens([(1, vec!["de", "el"])])
            .unwrap();
        let values = [Some("VERB"), Some("ADP"), Some("DET"), Some("NOUN")];
        doc.set_columns([(Column::Upos, values)], None).unwrap();
        let doc = doc
            .with_words([(0, vec!["da", "me", "lo"]), (1, vec!["del"])])
            .unwrap();
        assert_eq!(
            words_of(&doc),
            [vec!["da", "me", "lo"], vec!["del"], vec!["norte"]]
        );
        assert_eq!(upos(&doc), [None, None, None, None, Some("NOUN")]);
        assert_eq!(doc.word_id(4), 5);
        // One word per token again: the maps are the identity, and forms the texts of the tokens
        let doc = doc.with_words([(0, vec!["dámelo"])]).unwrap();
        assert!(doc.token_words.is_empty() && doc.word_tokens.is_empty() && doc.forms.is_empty());
        assert_eq!(upos(&doc), [None, None, Some("NOUN")]);
        let doc = doc.with_words([(2, vec!["norte"])]).unwrap();
        assert_eq!(words_of(&doc)[2], ["norte"]);
        assert!(doc.columns.is_none() && doc.forms.is_empty());
        // A single form may differ from the text in whitespace only, as CoNLL-U reads it
        let text = "x 10\u{a0}000".to_owned();
        let tokens = vec![Span { start: 0, end: 1 }, Span { start: 2, end: 9 }];
        let spaced = Document::from_tokens(text, tokens, None)
            .unwrap()
            .with_words([(1, vec!["10 000"])])
            .unwrap();
        assert_eq!(spaced.form(1), "10 000");
        assert_eq!(
            crate::read_conllu(&spaced.to_conllu()).unwrap()[0].form(1),
            "10 000"
        );
        assert_eq!(
            doc.clone()
                .with_words(Vec::<(usize, Vec<&str>)>::new())
                .unwrap(),
            doc
        );
    }

    #[test]
    fn with_words_wrong() {
        let build = || document("a b c", None);
        for (words, message) in [
            (vec![(3, vec!["x"])], "token 3 is not in the document"),
            (
                vec![(1, vec!["b"]), (0, vec!["a"])],
                "the tokens of the words must be increasing indices",
            ),
            (
                vec![(1, vec!["b"]), (1, vec!["b"])],
                "the tokens of the words must be increasing indices",
            ),
            (vec![(0, vec![])], "token 0 has no forms"),
            (
                vec![(0, vec!["x", ""])],
                "a form of token 0 is empty or holds a tab or a line break",
            ),
            (
                vec![(0, vec!["x\ny"])],
                "a form of token 0 is empty or holds a tab or a line break",
            ),
            (
                vec![(1, vec!["B"])],
                "the single form of token 1 is not its text",
            ),
            (
                vec![(1, vec![" "])],
                "the single form of token 1 is not its text",
            ),
        ] {
            assert_eq!(
                build().with_words(words).unwrap_err(),
                DocumentError(message.to_owned())
            );
        }
        let mut parsed = build();
        parsed.set_heads(vec![Some(0), Some(1), Some(1)]).unwrap();
        let mut enhanced = build();
        enhanced
            .set_columns([(Column::Deps, [Some("0:root"), None, None])], None)
            .unwrap();
        let empty = "# text = a b\n1\ta\t_\t_\t_\t_\t_\t_\t_\t_\n1.1\tz\t_\t_\t_\t_\t_\t_\t_\t_\n\
                     2\tb\t_\t_\t_\t_\t_\t_\t_\t_\n\n";
        let empty = crate::read_conllu(empty).unwrap().remove(0);
        for doc in [parsed, enhanced, empty] {
            assert_eq!(
                doc.clone().with_words([(0, vec!["x", "y"])]).unwrap_err(),
                DocumentError(
                    "the words of a document with heads, enhanced dependencies or empty nodes cannot change: \
                     they refer to the IDs of words"
                        .to_owned()
                )
            );
            assert!(doc.with_sentences(&[1]).is_err());
        }
    }

    #[test]
    fn with_words_read_from_conllu() {
        let doc = crate::read_conllu(READ).unwrap().remove(0);
        let doc = doc
            .with_words([
                (0, vec!["Da", "me", "lo"]),
                (1, vec!["del"]),
                (2, vec!["nor", "te"]),
            ])
            .unwrap();
        // The comments stay; the new multiword tokens carry the space after them, as a segmented document writes it
        let written = doc.to_conllu();
        assert_eq!(
            written,
            "\
# newdoc id = d1
# sent_id = 1
# text = Dámelo del norte.
1-3\tDámelo\t_\t_\t_\t_\t_\t_\t_\t_
1\tDa\t_\t_\t_\t_\t_\t_\t_\t_
2\tme\t_\t_\t_\t_\t_\t_\t_\t_
3\tlo\t_\t_\t_\t_\t_\t_\t_\t_
4\tdel\t_\t_\t_\t_\t_\t_\t_\t_
5-6\tnorte\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No
5\tnor\t_\t_\t_\t_\t_\t_\t_\t_
6\tte\t_\t_\t_\t_\t_\t_\t_\t_
7\t.\t.\tPUNCT\t_\t_\t_\t_\t_\t_

"
        );
        assert_eq!(crate::read_conllu(&written).unwrap(), [doc]);
        // A token of one word keeps the space after it in MISC
        let joined = crate::read_conllu(READ)
            .unwrap()
            .remove(0)
            .with_words([(2, vec!["nor", "te"])])
            .unwrap()
            .with_words([(2, vec!["norte"])])
            .unwrap();
        assert_eq!(joined.column(Column::Misc, 3), Some("SpaceAfter=No"));
        assert_eq!(crate::read_conllu(&joined.to_conllu()).unwrap(), [joined]);
    }

    #[test]
    fn with_sentences() {
        let mut doc = document("Ёж идёт. Кот спит.\n\nУтро.", None);
        doc.set_columns(
            [(Column::Upos, [Some("NOUN"), None, None, None, None])],
            None,
        )
        .unwrap();
        let split = doc.clone().with_sentences(&[2]).unwrap();
        assert_eq!(split.sentence_tokens, [0, 2, 4, 5]);
        assert_eq!(split.paragraph_sentences, [0, 2, 3]);
        assert_eq!(split.sentence(1).span.slice(split.text()), "Кот спит.");
        assert_eq!(
            (split.column(Column::Upos, 0), split.lines()),
            (Some("NOUN"), doc.lines())
        );
        // Every paragraph starts a sentence
        assert_eq!(split.clone().with_sentences(&[]).unwrap(), doc);
        assert_eq!(
            split.with_sentences(&[1, 4]).unwrap().sentence_tokens,
            [0, 1, 4, 5]
        );
        for starts in [&[5][..], &[2, 2], &[3, 1]] {
            assert_eq!(
                doc.clone().with_sentences(starts).unwrap_err(),
                DocumentError("sentence starts must be increasing token indices".to_owned())
            );
        }
    }

    #[test]
    fn with_sentences_read_from_conllu() {
        let doc = crate::read_conllu(READ).unwrap().remove(0);
        assert_eq!(doc.clone().with_sentences(&[0]).unwrap(), doc);
        // Other sentences: written as a segmented document, with the forms and columns read
        let split = doc.with_sentences(&[3]).unwrap();
        let written = split.to_conllu();
        assert_eq!(
            written,
            "\
# newdoc
# newpar
# sent_id = 1
# text = Dámelo del norte
1\tDámelo\tdar\tVERB\t_\t_\t_\t_\t_\t_
2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_
2\tde\tde\tADP\t_\t_\t_\t_\t_\t_
3\tel\tel\tDET\t_\t_\t_\t_\t_\t_
4\tnorte\tnorte\tNOUN\t_\t_\t_\t_\t_\tSpaceAfter=No

# sent_id = 2
# text = .
1\t.\t.\tPUNCT\t_\t_\t_\t_\t_\t_

"
        );
        let back = crate::read_conllu(&written).unwrap().remove(0);
        assert_eq!(
            (back.text(), back.tokens(), words_of(&back)),
            (split.text(), split.tokens(), words_of(&split))
        );
        assert_eq!(
            (back.sentence_tokens, back.columns),
            (split.sentence_tokens, split.columns)
        );
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
        // Any iterable of optional strings; a wrong length of an iterator counts all of it
        let words = ["Ёж", "идти", "кот"];
        doc.set_columns(
            [(Column::Lemma, words.iter().map(|word| Some(*word)))],
            None,
        )
        .unwrap();
        assert_eq!(doc.column(Column::Lemma, 1), Some("идти"));
        assert_eq!(
            doc.set_columns(
                [(
                    Column::Lemma,
                    words.iter().chain(&words).map(|word| Some(*word))
                )],
                None
            )
            .unwrap_err(),
            DocumentError("lemma has 6 values for 3 words".to_owned())
        );
        doc.set_columns([(Column::Lemma, [None::<&str>; 3])], None)
            .unwrap();
        doc.set_heads(vec![None, Some(1), None]).unwrap();
        assert_eq!(doc.head(1), Some(1));
        assert_eq!(
            doc.set_heads(vec![None]).unwrap_err(),
            DocumentError("head has 1 values for 3 words".to_owned())
        );
        doc.set_columns([(Column::Upos, [None::<&str>; 3])], None)
            .unwrap();
        assert!(doc.columns.is_none());
        assert!(doc.token_words.is_empty() && doc.word_tokens.is_empty());
    }
}

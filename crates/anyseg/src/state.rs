//! The state of a document for the Python module, which pickles it: everything but the text, in numbers of
//! variable length, and read back with the checks that the accessors of a document rely on

use crate::Span;
use crate::document::{
    Column, Document, DocumentError, Line, LineBreak, PRESENT, Values, Verbatim,
};

/// The number of the format: a state of another one is refused
const FORMAT: u8 = 1;

fn put(out: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    put(out, text.len());
    out.extend_from_slice(text.as_bytes());
}

/// Increasing numbers, as the differences between them
fn put_increasing(out: &mut Vec<u8>, values: &[usize]) {
    put(out, values.len());
    let mut previous = 0;
    for &value in values {
        put(out, value - previous);
        previous = value;
    }
}

/// Spans in order, each as its distance from the last and its length
fn put_span(out: &mut Vec<u8>, span: Span, previous: &mut usize) {
    put(out, span.start - *previous);
    put(out, span.end - span.start);
    *previous = span.end;
}

fn put_values(out: &mut Vec<u8>, values: &Values) {
    put_string(out, &values.text);
    put(out, values.ends.len());
    let mut previous = 0;
    for &end in &values.ends {
        let position = end & !PRESENT;
        put(
            out,
            ((position - previous) as usize) << 1 | usize::from(end & PRESENT != 0),
        );
        previous = position;
    }
}

fn damaged() -> DocumentError {
    DocumentError("the state of the document is damaged".to_owned())
}

/// A condition of a valid state
fn check(condition: bool) -> Result<(), DocumentError> {
    if condition { Ok(()) } else { Err(damaged()) }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn number(&mut self) -> Result<usize, DocumentError> {
        let (mut value, mut shift) = (0usize, 0);
        loop {
            let (&byte, rest) = self.0.split_first().ok_or_else(damaged)?;
            self.0 = rest;
            let bits = usize::from(byte & 0x7F);
            check(shift < usize::BITS && (bits << shift) >> shift == bits)?;
            value |= bits << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    /// A number of items of at least one byte each: a damaged one does not reserve memory
    fn count(&mut self) -> Result<usize, DocumentError> {
        let count = self.number()?;
        check(count <= self.0.len())?;
        Ok(count)
    }

    fn sum(&mut self, previous: usize) -> Result<usize, DocumentError> {
        previous.checked_add(self.number()?).ok_or_else(damaged)
    }

    fn string(&mut self) -> Result<&'a str, DocumentError> {
        let length = self.count()?;
        let (text, rest) = self.0.split_at(length);
        self.0 = rest;
        std::str::from_utf8(text).map_err(|_| damaged())
    }

    /// A span in order, on the characters of the text
    fn span(&mut self, text: &str, previous: &mut usize) -> Result<Span, DocumentError> {
        let start = self.sum(*previous)?;
        let end = self.sum(start)?;
        check(text.is_char_boundary(start) && text.is_char_boundary(end))?;
        *previous = end;
        Ok(Span { start, end })
    }

    /// The first items of parts, and the number of the items: from 0 and growing to `end`
    fn starts(&mut self, end: Option<usize>) -> Result<Vec<usize>, DocumentError> {
        let count = self.count()?;
        let mut values = Vec::with_capacity(count);
        let mut value = 0;
        for index in 0..count {
            let next = self.sum(value)?;
            check((index == 0 && next == 0) || (index > 0 && next > value))?;
            value = next;
            values.push(value);
        }
        check(count > 0 && end.is_none_or(|end| end == value))?;
        Ok(values)
    }

    fn values(&mut self) -> Result<Values, DocumentError> {
        let text = self.string()?;
        check(text.len() < PRESENT as usize)?;
        let count = self.count()?;
        let mut ends = Vec::with_capacity(count);
        let mut position = 0usize;
        for _ in 0..count {
            let number = self.number()?;
            position = position.checked_add(number >> 1).ok_or_else(damaged)?;
            check(text.is_char_boundary(position))?;
            // Fits: within the text, which is below PRESENT
            let end = position as u32;
            ends.push(if number & 1 == 1 { end | PRESENT } else { end });
        }
        Ok(Values {
            text: text.to_owned(),
            ends,
        })
    }

    fn flag(&mut self) -> Result<bool, DocumentError> {
        match self.number()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(damaged()),
        }
    }
}

impl Document {
    /// The state of the document without its text, for [`from_state`](Self::from_state)
    #[doc(hidden)]
    pub fn to_state(&self) -> Vec<u8> {
        let mut out = vec![FORMAT];
        put(&mut out, self.tokens.len());
        let mut previous = 0;
        for &token in &self.tokens {
            put_span(&mut out, token, &mut previous);
        }
        put_increasing(&mut out, &self.token_words);
        put_values(&mut out, &self.forms);
        put(&mut out, usize::from(self.columns.is_some()));
        for values in self.columns.iter().flat_map(|columns| columns.iter()) {
            put_values(&mut out, values);
        }
        put(&mut out, self.heads.len());
        for head in &self.heads {
            put(&mut out, head.map_or(0, |head| head as usize + 1));
        }
        put_increasing(&mut out, &self.sentence_tokens);
        put_increasing(&mut out, &self.paragraph_sentences);
        put(&mut out, self.lines.len());
        let mut previous = 0;
        for line in &self.lines {
            put_span(&mut out, line.span, &mut previous);
            put(
                &mut out,
                match line.break_before {
                    None => 0,
                    Some(LineBreak::Author) => 1,
                    Some(LineBreak::Typeset) => 2,
                },
            );
        }
        put(&mut out, usize::from(self.verbatim.is_some()));
        if let Some(verbatim) = &self.verbatim {
            put_values(&mut out, &verbatim.comments);
            put(&mut out, verbatim.multiword.len());
            for (token, line) in &verbatim.multiword {
                put(&mut out, *token);
                put_string(&mut out, line);
            }
            put(&mut out, verbatim.empty.len());
            for (sentence, lines, line) in &verbatim.empty {
                put(&mut out, *sentence);
                put(&mut out, *lines);
                put_string(&mut out, line);
            }
        }
        out
    }

    /// The document of a text with the state [`to_state`](Self::to_state) gave.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the state is damaged, of another format or not that of the text.
    #[doc(hidden)]
    pub fn from_state(text: String, state: &[u8]) -> Result<Self, DocumentError> {
        let (&format, state) = state.split_first().ok_or_else(damaged)?;
        if format != FORMAT {
            return Err(DocumentError(format!(
                "the state of the document has the format {format}, not {FORMAT}"
            )));
        }
        let mut reader = Reader(state);
        let count = reader.count()?;
        let mut tokens = Vec::with_capacity(count);
        let mut previous = 0;
        for _ in 0..count {
            let token = reader.span(&text, &mut previous)?;
            check(token.start < token.end)?;
            tokens.push(token);
        }
        let token_words = match reader.0.first() {
            Some(0) => {
                reader.number()?;
                Vec::new()
            }
            _ => reader.starts(None)?,
        };
        check(token_words.is_empty() || token_words.len() == tokens.len() + 1)?;
        let forms = reader.values()?;
        // The words of a multiword token have forms: a damaged number of words does not reserve memory
        let words = match token_words.last() {
            Some(&words) => {
                check(words == forms.ends.len())?;
                words
            }
            None => tokens.len(),
        };
        let by_word = |values: &Values| values.ends.is_empty() || values.ends.len() == words;
        check(by_word(&forms))?;
        let mut word_tokens = Vec::new();
        if !token_words.is_empty() {
            word_tokens.reserve_exact(words);
            for (token, pair) in token_words.windows(2).enumerate() {
                word_tokens.extend(std::iter::repeat_n(token, pair[1] - pair[0]));
            }
        }
        let columns = if reader.flag()? {
            let mut columns: Box<[Values; 7]> = Box::default();
            for column in Column::ALL {
                let values = reader.values()?;
                check(by_word(&values))?;
                columns[column as usize] = values;
            }
            Some(columns)
        } else {
            None
        };
        let count = reader.count()?;
        check(count == 0 || count == words)?;
        let mut heads = Vec::with_capacity(count);
        for _ in 0..count {
            heads.push(match reader.number()? {
                0 => None,
                head => Some(u32::try_from(head - 1).map_err(|_| damaged())?),
            });
        }
        let sentence_tokens = reader.starts(Some(tokens.len()))?;
        let paragraph_sentences = reader.starts(Some(sentence_tokens.len() - 1))?;
        let count = reader.count()?;
        let mut lines = Vec::with_capacity(count);
        let mut previous = 0;
        for _ in 0..count {
            let span = reader.span(&text, &mut previous)?;
            let break_before = match reader.number()? {
                0 => None,
                1 => Some(LineBreak::Author),
                2 => Some(LineBreak::Typeset),
                _ => return Err(damaged()),
            };
            lines.push(Line { span, break_before });
        }
        let verbatim = if reader.flag()? {
            let comments = reader.values()?;
            check(comments.ends.is_empty() || comments.ends.len() == sentence_tokens.len() - 1)?;
            let count = reader.count()?;
            let mut multiword = Vec::with_capacity(count);
            for _ in 0..count {
                let token = reader.number()?;
                let later = multiword.last().is_none_or(|&(last, _)| last < token);
                check(later && token < tokens.len())?;
                multiword.push((token, reader.string()?.to_owned()));
            }
            let count = reader.count()?;
            let mut empty: Vec<(usize, usize, String)> = Vec::with_capacity(count);
            for _ in 0..count {
                let sentence = reader.number()?;
                let lines = reader.number()?;
                let later = empty.last().is_none_or(|last| {
                    last.0 < sentence || (last.0 == sentence && last.1 <= lines)
                });
                check(later && sentence < sentence_tokens.len() - 1)?;
                empty.push((sentence, lines, reader.string()?.to_owned()));
            }
            Some(Box::new(Verbatim {
                comments,
                multiword,
                empty,
            }))
        } else {
            None
        };
        check(reader.0.is_empty())?;
        Ok(Document {
            text,
            tokens,
            token_words,
            word_tokens,
            forms,
            columns,
            heads,
            sentence_tokens,
            paragraph_sentences,
            lines,
            verbatim,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_conllu;

    /// Every accessor of a document, which must not panic on any state that is read
    fn read_all(doc: &Document) {
        let _ = (doc.to_conllu(), doc.to_state());
        for word in 0..doc.word_count() {
            let _ = (doc.form(word), doc.word_id(word), doc.head(word));
            let _ = (doc.word_sentence(word), doc.column(Column::Misc, word));
        }
        for token in 0..doc.tokens().len() {
            let _ = (doc.token_words(token), doc.token_sentence(token));
        }
        for sentence in 0..doc.sentence_count() {
            let _ = (doc.sentence(sentence), doc.sentence_paragraph(sentence));
        }
        for paragraph in 0..doc.paragraph_count() {
            let _ = doc.paragraph(paragraph);
        }
    }

    const CONLLU: &str = "# newpar\n# sent_id = a\n# text = Del ё.\n\
        1-2\tDel\t_\t_\t_\t_\t_\t_\t_\t_\n1\tde\tde\tADP\t_\t_\t0\troot\t_\t_\n2\tel\t_\t_\t_\t_\t1\t_\t_\t_\n\
        3\tё\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n3.1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n4\t.\t_\t_\t_\t_\t_\t_\t_\t_\n\n\
        # text = x\n1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n\n";

    fn reads_back(doc: &Document) {
        let read = Document::from_state(doc.text.clone(), &doc.to_state()).unwrap();
        assert_eq!(&read, doc);
        assert_eq!(read.to_conllu(), doc.to_conllu());
    }

    #[test]
    fn documents_read_back() {
        reads_back(&read_conllu(CONLLU).unwrap()[0]);
    }

    #[test]
    #[cfg(feature = "ru")]
    fn segmented_documents_read_back() {
        let segmenter = crate::Segmenter::new("ru").unwrap();
        let mut doc = segmenter.segment("Ёж идёт.\nмимо\n\n\nКот");
        doc.set_columns(
            [(Column::Upos, [Some("NOUN"), None, None, None, None])],
            Some(vec![Some(2), Some(0), None, None, None]),
        )
        .unwrap();
        reads_back(&doc);
        reads_back(&segmenter.segment(""));
        reads_back(&segmenter.segment("Ёж идёт.\nмимо"));
    }

    #[test]
    fn damaged_states_do_not_panic() {
        let doc = &read_conllu(CONLLU).unwrap()[0];
        let state = doc.to_state();
        let read = |state: &[u8]| {
            if let Ok(doc) = Document::from_state(doc.text.clone(), state) {
                read_all(&doc);
            }
        };
        for length in 0..state.len() {
            assert!(Document::from_state(doc.text.clone(), &state[..length]).is_err());
        }
        for at in 0..state.len() {
            for value in [0, 1, 2, 0x7F, 0x80, 0xFF] {
                let mut changed = state.clone();
                changed[at] = value;
                read(&changed);
            }
        }
        let mut other = state.clone();
        other[0] = 200;
        assert_eq!(
            Document::from_state(doc.text.clone(), &other).unwrap_err(),
            DocumentError("the state of the document has the format 200, not 1".to_owned())
        );
        let mut longer = state.clone();
        longer.push(0);
        assert!(Document::from_state(doc.text.clone(), &longer).is_err());
        assert!(Document::from_state("x".to_owned(), &state).is_err());
    }
}

//! Reading and writing CoNLL-U

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;

use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::Span;
use crate::document::{
    Column, Document, DocumentError, Line, LineBreak, Sentence, TooLarge, Values, Verbatim,
};
use crate::layout::{self, is_separator, one_line};

/// A line of CoNLL-U that cannot be read
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConlluError {
    /// The number of the line, from 1
    pub line: usize,
    /// What is wrong with it
    pub message: String,
}

impl fmt::Display for ConlluError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl Error for ConlluError {}

/// A word line, whose columns are read again when the document is made: a sentence keeps one string per word
struct Word<'a> {
    line: &'a str,
    head: Option<u32>,
}

impl<'a> Word<'a> {
    /// The form and the columns other than ID and HEAD; the line has 10 columns
    fn columns(&self) -> (&'a str, [Option<&'a str>; 7]) {
        let mut fields = self.line.split('\t').skip(1);
        let mut next = || fields.next().unwrap_or_default();
        let form = next();
        let [lemma, upos, xpos, feats] = [next(), next(), next(), next()];
        let _head = next();
        let [deprel, deps, misc] = [next(), next(), next()];
        (
            form,
            [lemma, upos, xpos, feats, deprel, deps, misc].map(field),
        )
    }
}

struct Token<'a> {
    form: &'a str,
    /// Columns 2-10 of the line of a multiword token
    multiword: Option<&'a str>,
    misc: Option<&'a str>,
    words: usize,
    /// The number of its line
    line: usize,
}

/// A sentence as its lines give it
#[derive(Default)]
struct Block<'a> {
    comments: Vec<&'a str>,
    tokens: Vec<Token<'a>>,
    words: Vec<Word<'a>>,
    empty: Vec<(usize, String)>,
    /// The number of multiword token and word lines so far
    lines: usize,
    /// The number of the first line, 0 before it
    line: usize,
}

impl Block<'_> {
    fn has_comment(&self, name: &str) -> bool {
        self.comments.iter().any(|comment| {
            comment[1..]
                .trim_start()
                .strip_prefix(name)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        })
    }

    fn text(&self) -> Option<&str> {
        self.comments.iter().find_map(|comment| {
            let rest = comment[1..].trim_start().strip_prefix("text")?;
            let value = rest.trim_start().strip_prefix('=')?;
            Some(value.strip_prefix(' ').unwrap_or(value))
        })
    }
}

fn field(value: &str) -> Option<&str> {
    (value != "_").then_some(value)
}

fn misc_has(misc: Option<&str>, item: &str) -> bool {
    misc.is_some_and(|misc| misc.split('|').any(|value| value == item))
}

/// The value of `SpacesAfter`, if it holds only whitespace
fn spaces_after(misc: Option<&str>) -> Option<String> {
    let value = misc?
        .split('|')
        .find_map(|item| item.strip_prefix("SpacesAfter="))?;
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some('p') => out.push('|'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out.chars().all(is_separator).then_some(out)
}

fn escape_spaces(spaces: &str) -> String {
    let mut out = String::with_capacity(spaces.len() * 2);
    for character in spaces.chars() {
        match character {
            ' ' => out.push_str("\\s"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '|' => out.push_str("\\p"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(character),
        }
    }
    out
}

fn parse<'a>(
    sentence: &mut Block<'a>,
    number: usize,
    line: &'a str,
    multiword: &mut Option<(usize, usize)>,
) -> Result<(), ConlluError> {
    let error = |message: String| ConlluError {
        line: number,
        message,
    };
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 10 {
        return Err(error(format!(
            "expected 10 tab-separated columns, got {}",
            fields.len()
        )));
    }
    let id = fields[0];
    if id.contains('.') {
        sentence.empty.push((sentence.lines, line.to_owned()));
        return Ok(());
    }
    sentence.lines += 1;
    let next = sentence.words.len() + 1;
    let rest = &line[id.len() + 1..];
    if let Some((first, last)) = id.split_once('-') {
        let (first, last): (usize, usize) = match (first.parse(), last.parse()) {
            (Ok(first), Ok(last)) => (first, last),
            _ => return Err(error(format!("cannot read the ID {id:?}"))),
        };
        if first != next || last <= first || multiword.is_some() {
            return Err(error(format!(
                "multiword token {id} does not start at word {next}"
            )));
        }
        *multiword = Some((first, last));
        sentence.tokens.push(Token {
            form: fields[1],
            multiword: Some(rest),
            misc: field(fields[9]),
            words: 0,
            line: number,
        });
        return Ok(());
    }
    if id.parse::<usize>().ok() != Some(next) {
        return Err(error(format!("expected word {next}, got {id:?}")));
    }
    let head = match fields[6] {
        "_" => None,
        head => Some(
            head.parse()
                .map_err(|_| error(format!("cannot read the head {head:?}")))?,
        ),
    };
    sentence.words.push(Word { line, head });
    match *multiword {
        Some((_, last)) => {
            if let Some(token) = sentence.tokens.last_mut() {
                token.words += 1;
            }
            if next == last {
                *multiword = None;
            }
        }
        None => sentence.tokens.push(Token {
            form: fields[1],
            multiword: None,
            misc: field(fields[9]),
            words: 1,
            line: number,
        }),
    }
    Ok(())
}

/// The documents of a CoNLL-U text: a `# newdoc` comment starts a document and `# newpar` a paragraph. The text
/// of a sentence is its `# text` comment or, without it, the forms of its tokens with the spaces of `SpaceAfter`
/// and `SpacesAfter`; sentences of a paragraph are joined by a space (none after `SpaceAfter=No`) and paragraphs
/// by a blank line. Comments, multiword token lines and empty nodes are kept, so that
/// [`Document::to_conllu`] writes the text back as it was, with LF line ends and heads as plain numbers. A byte
/// order mark at the start is skipped.
///
/// # Errors
///
/// [`ConlluError`] on a line that is not a comment, a blank line or a token line of 10 columns, on an ID or a head
/// that cannot be read, on IDs out of order, on a multiword token without all its words, on a comment after the
/// token lines of a sentence, on a sentence without words, on forms that do not match the `# text` of their
/// sentence or a `# text` with characters that no token has, and on a text that does not end with a blank line.
pub fn read_conllu(input: &str) -> Result<Vec<Document>, ConlluError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut documents = Vec::new();
    // The sentences of the document being read: a document is made as soon as the next one starts
    let mut sentences: Vec<Block<'_>> = Vec::new();
    let mut sentence = Block::default();
    let mut multiword = None;
    for (index, line) in input.lines().enumerate() {
        let number = index + 1;
        if line.is_empty() {
            if sentence.line > 0 {
                if sentence.words.is_empty() {
                    return Err(ConlluError {
                        line: number,
                        message: "a sentence without words".to_owned(),
                    });
                }
                if multiword.is_some() {
                    return Err(ConlluError {
                        line: number,
                        message: "a multiword token without all its words".to_owned(),
                    });
                }
                if sentence.has_comment("newdoc") && !sentences.is_empty() {
                    documents.push(document(&sentences)?);
                    sentences.clear();
                }
                sentences.push(std::mem::take(&mut sentence));
            }
        } else {
            if sentence.line == 0 {
                sentence.line = number;
            }
            if !line.starts_with('#') {
                parse(&mut sentence, number, line, &mut multiword)?;
            } else if sentence.lines > 0 || !sentence.empty.is_empty() {
                return Err(ConlluError {
                    line: number,
                    message: "a comment after token lines".to_owned(),
                });
            } else {
                sentence.comments.push(line);
            }
        }
    }
    if sentence.line > 0 {
        return Err(ConlluError {
            line: input.lines().count(),
            message: "the text does not end with a blank line".to_owned(),
        });
    }
    if !sentences.is_empty() {
        documents.push(document(&sentences)?);
    }
    Ok(documents)
}

/// The non-whitespace characters of `form` found in order in `text` from `position`, past the separators
/// between them: the span they cover
fn align(text: &str, position: usize, form: &str) -> Option<Span> {
    let mut chars = text[position..].char_indices();
    let mut span: Option<Span> = None;
    for expected in form.chars().filter(|character| !character.is_whitespace()) {
        let (index, character) =
            chars.find(|&(_, character)| character == expected || !is_separator(character))?;
        if character != expected {
            return None;
        }
        let start = span.map_or(position + index, |span| span.start);
        span = Some(Span {
            start,
            end: position + index + character.len_utf8(),
        });
    }
    span
}

/// The space after a token as its MISC gives it: `SpacesAfter`, none after `SpaceAfter=No`, else `default`
fn space_after<'a>(misc: Option<&str>, default: &'a str) -> Cow<'a, str> {
    match spaces_after(misc) {
        Some(spaces) => Cow::Owned(spaces),
        None if misc_has(misc, "SpaceAfter=No") => Cow::Borrowed(""),
        None => Cow::Borrowed(default),
    }
}

fn document(sentences: &[Block<'_>]) -> Result<Document, ConlluError> {
    let mut text = String::new();
    let mut tokens = Vec::new();
    let mut token_words = vec![0];
    let mut word_tokens = Vec::new();
    let mut forms = Values::default();
    let mut columns: [Values; 7] = Default::default();
    let mut heads = Vec::new();
    let mut sentence_tokens = Vec::new();
    let mut paragraph_sentences = Vec::new();
    let mut verbatim = Verbatim::default();
    for (index, sentence) in sentences.iter().enumerate() {
        let new_paragraph = index == 0 || sentence.has_comment("newpar");
        if new_paragraph {
            paragraph_sentences.push(index);
            if index > 0 {
                text.push_str("\n\n");
            }
        } else {
            let last = sentences[index - 1]
                .tokens
                .last()
                .and_then(|token| token.misc);
            text.push_str(&space_after(last, " "));
        }
        let source: Cow<'_, str> = match sentence.text() {
            Some(sentence_text) => Cow::Borrowed(sentence_text),
            None => {
                let mut built = String::new();
                for (position, token) in sentence.tokens.iter().enumerate() {
                    if position > 0 {
                        built.push_str(&space_after(sentence.tokens[position - 1].misc, " "));
                    }
                    built.push_str(token.form);
                }
                Cow::Owned(built)
            }
        };
        sentence_tokens.push(tokens.len());
        let mut cursor = 0;
        let mut words = sentence.words.iter();
        for (position, token) in sentence.tokens.iter().enumerate() {
            let found = align(&source, cursor, token.form).ok_or_else(|| ConlluError {
                line: token.line,
                message: format!(
                    "the form {:?} does not match the text of the sentence",
                    token.form
                ),
            })?;
            // The space between tokens comes from SpacesAfter when MISC has it, else from the text
            let gap = &source[cursor..found.start];
            match position {
                0 => text.push_str(gap),
                _ => text.push_str(&space_after(
                    sentence.tokens[position - 1]
                        .misc
                        .filter(|misc| misc.contains("SpacesAfter=")),
                    gap,
                )),
            }
            let span = Span {
                start: text.len(),
                end: text.len() + found.end - found.start,
            };
            text.push_str(found.slice(&source));
            cursor = found.end;
            let token_index = tokens.len();
            tokens.push(span);
            if let Some(line) = token.multiword {
                verbatim.multiword.push((token_index, line.to_owned()));
            }
            for word in words.by_ref().take(token.words) {
                let (form, values) = word.columns();
                let form = (token.multiword.is_some() || form != span.slice(&text)).then_some(form);
                forms.push(form).map_err(|TooLarge| too_large(sentence))?;
                for (column, value) in columns.iter_mut().zip(values) {
                    column.push(value).map_err(|TooLarge| too_large(sentence))?;
                }
                heads.push(word.head);
                word_tokens.push(token_index);
            }
            token_words.push(word_tokens.len());
        }
        let rest = &source[cursor..];
        if !rest.chars().all(is_separator) {
            return Err(ConlluError {
                line: sentence.line,
                message: "the text of the sentence holds characters that no token has".to_owned(),
            });
        }
        text.push_str(rest);
        let comments = (!sentence.comments.is_empty()).then(|| sentence.comments.join("\n"));
        verbatim
            .comments
            .push(comments.as_deref())
            .map_err(|TooLarge| too_large(sentence))?;
        verbatim.empty.extend(
            sentence
                .empty
                .iter()
                .map(|(lines, line)| (index, *lines, line.clone())),
        );
    }
    sentence_tokens.push(tokens.len());
    paragraph_sentences.push(sentences.len());
    let mut lines = Vec::new();
    for paragraph in paragraph_sentences.windows(2) {
        let start = tokens[sentence_tokens[paragraph[0]]].start;
        let end = tokens[sentence_tokens[paragraph[1]] - 1].end;
        let parts = layout::paragraphs(&text[start..end]);
        for (position, line) in parts.iter().flatten().enumerate() {
            lines.push(Line {
                span: Span {
                    start: start + line.start,
                    end: start + line.end,
                },
                break_before: (position > 0).then_some(LineBreak::Author),
            });
        }
    }
    // Every token one word: the maps are the identity
    if word_tokens.len() == tokens.len() {
        token_words = Vec::new();
        word_tokens = Vec::new();
    }
    verbatim.comments = verbatim.comments.finished();
    Ok(Document {
        text,
        tokens: tokens.as_slice().to_vec(),
        token_words: token_words.as_slice().to_vec(),
        word_tokens: word_tokens.as_slice().to_vec(),
        forms: forms.finished(),
        columns: columns.map(Values::finished),
        heads: if heads.iter().all(Option::is_none) {
            Vec::new()
        } else {
            heads.as_slice().to_vec()
        },
        sentence_tokens,
        paragraph_sentences,
        lines: lines.as_slice().to_vec(),
        verbatim: Some(Box::new(verbatim)),
    })
}

fn too_large(sentence: &Block<'_>) -> ConlluError {
    ConlluError {
        line: sentence.line,
        message: "the values of a document exceed 2 GiB".to_owned(),
    }
}

impl Document {
    /// The document in CoNLL-U. A document read from CoNLL-U is written as it was read, with the columns set
    /// since. Otherwise every sentence gets `# sent_id` (its number from 1) and `# text`, the first one
    /// `# newdoc`, every paragraph `# newpar`, and the space after a token inside a paragraph goes to MISC:
    /// `SpaceAfter=No` when there is none and `SpacesAfter` when it is not one space. In `SpacesAfter` the
    /// invisible separators are left out, line breaks other than CR and LF become LF and whitespace other than
    /// tabs becomes a space; `# text` is the tokens with these spaces, line breaks turned into spaces. Forms,
    /// `# text` and column values are in Unicode NFC, as CoNLL-U requires; the document keeps its text.
    pub fn to_conllu(&self) -> String {
        self.conllu(None)
    }

    /// The document in CoNLL-U as [`to_conllu`](Self::to_conllu) writes it, with `# newdoc id = ID` and
    /// `# sent_id = ID-N` (the ID in NFC), so that the sentences of several documents in one file have distinct
    /// IDs.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the ID is empty or holds whitespace.
    pub fn to_conllu_with_id(&self, id: &str) -> Result<String, DocumentError> {
        if id.is_empty() || id.contains(char::is_whitespace) {
            return Err(DocumentError(format!(
                "document ID {id:?} is empty or holds whitespace"
            )));
        }
        Ok(self.conllu(Some(&nfc(id))))
    }

    fn conllu(&self, id: Option<&str>) -> String {
        let mut out = String::new();
        for sentence in 0..self.sentence_count() {
            self.write_sentence(sentence, id, &mut out);
        }
        out
    }

    /// `# text` of a sentence: its tokens with the spaces between them as MISC gives them, line breaks turned
    /// into spaces
    fn sentence_text(&self, sentence: &Sentence) -> String {
        let mut out = String::new();
        for token in sentence.tokens.clone() {
            if token > sentence.tokens.start {
                let gap = &self.text[self.tokens[token - 1].end..self.tokens[token].start];
                out.push_str(&one_line(&normalize(gap)));
            }
            out.push_str(&nfc(&clean(self.tokens[token].slice(&self.text))));
        }
        out
    }

    fn write_sentence(&self, index: usize, id: Option<&str>, out: &mut String) {
        let sentence = self.sentence(index);
        let verbatim = self.verbatim.as_deref();
        match verbatim {
            Some(verbatim) => {
                for comment in verbatim
                    .comments
                    .get(index)
                    .into_iter()
                    .flat_map(|comments| comments.split('\n'))
                {
                    out.push_str(comment);
                    out.push('\n');
                }
            }
            None => {
                let paragraph = self.sentence_paragraph(index);
                if index == 0 {
                    match id {
                        Some(id) => {
                            let _ = writeln!(out, "# newdoc id = {id}");
                        }
                        None => out.push_str("# newdoc\n"),
                    }
                }
                if self.paragraph_sentences[paragraph] == index {
                    out.push_str("# newpar\n");
                }
                match id {
                    Some(id) => {
                        let _ = writeln!(out, "# sent_id = {id}-{}", index + 1);
                    }
                    None => {
                        let _ = writeln!(out, "# sent_id = {}", index + 1);
                    }
                }
                let _ = writeln!(out, "# text = {}", self.sentence_text(&sentence));
            }
        }
        let empty: &[(usize, usize, String)] = verbatim.map_or(&[], |verbatim| {
            let first = verbatim.empty.partition_point(|node| node.0 < index);
            let end = verbatim.empty.partition_point(|node| node.0 <= index);
            &verbatim.empty[first..end]
        });
        let mut empty = empty.iter().peekable();
        let mut lines = 0;
        let mut flush = |lines: usize, out: &mut String| {
            while let Some((_, _, line)) = empty.next_if(|node| node.1 <= lines) {
                out.push_str(line);
                out.push('\n');
            }
        };
        for token in sentence.tokens.clone() {
            let words = self.token_words(token);
            let layout = match verbatim {
                Some(_) => None,
                None => self.layout(token),
            };
            if words.len() > 1 {
                flush(lines, out);
                let first = words.start - sentence.words.start + 1;
                let _ = write!(out, "{first}-{}\t", first + words.len() - 1);
                match verbatim.and_then(|verbatim| {
                    verbatim
                        .multiword
                        .binary_search_by_key(&token, |(token, _)| *token)
                        .ok()
                        .map(|position| &verbatim.multiword[position].1)
                }) {
                    Some(line) => out.push_str(line),
                    None => {
                        let form = self.tokens[token].slice(&self.text);
                        let _ = write!(
                            out,
                            "{}\t_\t_\t_\t_\t_\t_\t_\t{}",
                            nfc(&clean(form)),
                            layout.as_deref().unwrap_or("_")
                        );
                    }
                }
                out.push('\n');
                lines += 1;
            }
            for word in words.clone() {
                flush(lines, out);
                // A document read from CoNLL-U is written as it was, a segmented one in NFC
                let generated = verbatim.is_none();
                let misc = self
                    .column(Column::Misc, word)
                    .map(|misc| written(generated, misc));
                let misc = match (&layout, words.len()) {
                    (Some(layout), 1) => Some(merge(misc.as_deref(), layout)),
                    _ => misc.map(Cow::into_owned),
                };
                let value = |column| written(generated, self.column(column, word).unwrap_or("_"));
                let head = self
                    .head(word)
                    .map_or_else(|| "_".to_owned(), |head| head.to_string());
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    word - sentence.words.start + 1,
                    written(generated, &clean(self.form(word))),
                    value(Column::Lemma),
                    value(Column::Upos),
                    value(Column::Xpos),
                    value(Column::Feats),
                    head,
                    value(Column::Deprel),
                    value(Column::Deps),
                    misc.as_deref().unwrap_or("_"),
                );
                lines += 1;
            }
        }
        flush(usize::MAX, out);
        out.push('\n');
    }

    /// `SpaceAfter=No` or `SpacesAfter` for the space after a token inside its paragraph
    fn layout(&self, token: usize) -> Option<String> {
        let next = self.tokens.get(token + 1)?;
        let sentence = self.token_sentence(token);
        if self.sentence_tokens[sentence + 1] == token + 1 {
            let paragraph = self.sentence_paragraph(sentence);
            if self.paragraph_sentences[paragraph + 1] == sentence + 1 {
                return None;
            }
        }
        match normalize(&self.text[self.tokens[token].end..next.start]).as_str() {
            "" => Some("SpaceAfter=No".to_owned()),
            " " => None,
            spaces => Some(format!("SpacesAfter={}", escape_spaces(spaces))),
        }
    }
}

/// The space between two tokens as CoNLL-U keeps it: tabs, CR and LF as they are, the other line breaks as LF,
/// the other whitespace as a space, the invisible separators left out
fn normalize(gap: &str) -> String {
    gap.chars()
        .filter_map(|character| match character {
            '\u{200b}' | '\u{2060}' | '\u{feff}' | '\u{ad}' | '\u{200c}' | '\u{200d}' => None,
            '\t' | '\r' | '\n' => Some(character),
            '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => Some('\n'),
            _ if character.is_control() => None,
            _ => Some(' '),
        })
        .collect()
}

/// MISC of a word with the space after it, unless it already says
fn merge(misc: Option<&str>, layout: &str) -> String {
    match misc {
        Some(misc)
            if misc.split('|').any(|item| {
                item.starts_with("SpaceAfter=") || item.starts_with("SpacesAfter=")
            }) =>
        {
            misc.to_owned()
        }
        Some(misc) => format!("{misc}|{layout}"),
        None => layout.to_owned(),
    }
}

/// A text in Unicode NFC
fn nfc(text: &str) -> Cow<'_, str> {
    if is_nfc(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    }
}

/// A value of a segmented document as CoNLL-U writes it: in NFC
fn written(generated: bool, value: &str) -> Cow<'_, str> {
    match generated {
        true => nfc(value),
        false => Cow::Borrowed(value),
    }
}

/// A form fit for a column of CoNLL-U: tabs and line breaks turned into spaces
fn clean(form: &str) -> Cow<'_, str> {
    const BREAKS: [char; 8] = [
        '\t', '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
    ];
    if form.contains(BREAKS) {
        one_line(form).replace('\t', " ").into()
    } else {
        form.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UD: &str = "\
# newdoc id = d1
# sent_id = 1
# text = Vino  del norte, ¿no?
1\tVino\tvenir\tVERB\t_\tMood=Ind\t0\troot\t_\t_
2-3\tdel\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No
2\tde\tde\tADP\t_\t_\t4\tcase\t4:case\t_
3\tel\tel\tDET\t_\t_\t4\tdet\t4:det\t_
3.1\tvino\tvenir\tVERB\t_\t_\t_\t_\t0:root\t_
4\tnorte\tnorte\tNOUN\t_\t_\t1\tobl\t1:obl\tSpaceAfter=No
5\t,\t,\tPUNCT\t_\t_\t7\tpunct\t_\t_
6\t¿\t¿\tPUNCT\t_\t_\t7\tpunct\t_\tSpaceAfter=No
7\tno\tno\tADV\t_\t_\t1\tadvmod\t_\tSpaceAfter=No
8\t?\t?\tPUNCT\t_\t_\t1\tpunct\t_\t_

# newpar
# sent_id = 2
1\tSí\tsí\tINTJ\t_\t_\t0\troot\t_\tSpaceAfter=No
2\t.\t.\tPUNCT\t_\t_\t1\tpunct\t_\t_

# newdoc
# sent_id = 3
# text = 10 000 рублей
1\t10 000\t10 000\tNUM\t_\t_\t2\tnummod\t_\t_
2\tрублей\tрубль\tNOUN\t_\t_\t0\troot\t_\t_

";

    #[test]
    fn round_trip() {
        let documents = read_conllu(UD).unwrap();
        assert_eq!(documents.len(), 2);
        let written: String = documents.iter().map(Document::to_conllu).collect();
        assert_eq!(written, UD);
        let with_mark = format!("\u{feff}{UD}");
        assert_eq!(read_conllu(&with_mark).unwrap(), documents);
    }

    #[test]
    fn read_document() {
        let doc = &read_conllu(UD).unwrap()[0];
        assert_eq!(doc.text(), "Vino  del norte, ¿no?\n\nSí.");
        let tokens: Vec<&str> = doc
            .tokens()
            .iter()
            .map(|span| span.slice(doc.text()))
            .collect();
        assert_eq!(
            tokens,
            ["Vino", "del", "norte", ",", "¿", "no", "?", "Sí", "."]
        );
        assert_eq!(doc.token_words(1), 1..3);
        assert_eq!(
            (doc.form(1), doc.form(2), doc.form(3)),
            ("de", "el", "norte")
        );
        assert_eq!(doc.column(Column::Deps, 1), Some("4:case"));
        assert_eq!(doc.head(0), Some(0));
        assert_eq!(doc.word_id(2), 3);
        assert_eq!((doc.sentence_count(), doc.paragraph_count()), (2, 2));
        assert_eq!(doc.lines().len(), 2);
        assert_eq!(
            read_conllu(UD).unwrap()[1].tokens()[0].slice("10 000 рублей"),
            "10 000"
        );
    }

    #[test]
    fn separators_between_forms() {
        let input = "# text = а\u{feff}б \u{200b} в\n1\tа\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
                     2\tб\t_\t_\t_\t_\t_\t_\t_\t_\n3\t\u{200b}\t_\t_\t_\t_\t_\t_\t_\t_\n\
                     4\tв\t_\t_\t_\t_\t_\t_\t_\t_\n\n";
        let doc = &read_conllu(input).unwrap()[0];
        let tokens: Vec<&str> = doc
            .tokens()
            .iter()
            .map(|span| span.slice(doc.text()))
            .collect();
        assert_eq!(tokens, ["а", "б", "\u{200b}", "в"]);
        assert_eq!(doc.to_conllu(), input);
    }

    #[test]
    fn text_without_comment() {
        let input = "1\tА\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n2\t,\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=\\s\\n\n3\tб\t_\t_\t_\t_\t_\t_\t_\t_\n\n";
        let doc = &read_conllu(input).unwrap()[0];
        assert_eq!(doc.text(), "А, \nб");
        assert_eq!(doc.to_conllu(), input);
        // Every token is one word: the maps are the identity and take no memory
        assert!(doc.token_words.is_empty() && doc.word_tokens.is_empty());
        assert_eq!(
            (doc.word_count(), doc.token_words(2), doc.word_token(2)),
            (3, 2..3, 2)
        );
    }

    #[test]
    #[cfg(feature = "ru")]
    fn generated() {
        let text = "Ёж идёт,\nдомой.  Утро\n\n«Кот»";
        let mut doc = crate::Segmenter::new("ru").unwrap().segment(text);
        doc.set_columns(
            vec![(
                Column::Misc,
                vec![
                    None,
                    Some("Gloss=go".to_owned()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
            )],
            None,
        )
        .unwrap();
        assert_eq!(
            doc.to_conllu(),
            "# newdoc\n# newpar\n# sent_id = 1\n# text = Ёж идёт, домой.\n\
             1\tЁж\t_\t_\t_\t_\t_\t_\t_\t_\n\
             2\tидёт\t_\t_\t_\t_\t_\t_\t_\tGloss=go|SpaceAfter=No\n\
             3\t,\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=\\n\n\
             4\tдомой\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
             5\t.\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=\\s\\s\n\n\
             # sent_id = 2\n# text = Утро\n\
             1\tУтро\t_\t_\t_\t_\t_\t_\t_\t_\n\n\
             # newpar\n# sent_id = 3\n# text = «Кот»\n\
             1\t«\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
             2\tКот\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
             3\t»\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
        );
        let back = &read_conllu(&doc.to_conllu()).unwrap()[0];
        let tokens = |doc: &Document| -> Vec<String> {
            doc.tokens()
                .iter()
                .map(|span| span.slice(doc.text()).to_owned())
                .collect()
        };
        assert_eq!(tokens(back), tokens(&doc));
        assert_eq!((back.sentence_count(), back.paragraph_count()), (3, 2));
    }

    #[test]
    fn generated_multiword() {
        let mut doc = read_conllu(UD).unwrap().remove(0);
        doc.verbatim = None;
        let written = doc.to_conllu();
        assert!(
            written.starts_with(
                "# newdoc\n# newpar\n# sent_id = 1\n# text = Vino  del norte, ¿no?\n\
                 1\tVino\tvenir\tVERB\t_\tMood=Ind\t0\troot\t_\tSpacesAfter=\\s\\s\n\
                 2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n\
                 2\tde\tde\tADP\t_\t_\t4\tcase\t4:case\t_\n"
            ),
            "{written}"
        );
        assert!(
            written.ends_with(
                "# newpar\n# sent_id = 2\n# text = Sí.\n\
                 1\tSí\tsí\tINTJ\t_\t_\t0\troot\t_\tSpaceAfter=No\n\
                 2\t.\t.\tPUNCT\t_\t_\t1\tpunct\t_\t_\n\n"
            ),
            "{written}"
        );
    }

    #[test]
    fn blocks_without_words() {
        let multiword = "1-2\tаб\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let empty = "1.1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let word = "1\tв\t_\t_\t_\t_\t_\t_\t_\t_\n";
        for (input, line, message) in [
            (format!("{multiword}\n"), 2, "a sentence without words"),
            (format!("{empty}\n{word}\n"), 2, "a sentence without words"),
            (
                format!("{word}\n{multiword}"),
                3,
                "the text does not end with a blank line",
            ),
            (
                format!("{empty}# x\n{word}\n"),
                2,
                "a comment after token lines",
            ),
        ] {
            assert_eq!(
                read_conllu(&input).unwrap_err(),
                ConlluError {
                    line,
                    message: message.to_owned()
                },
                "{input:?}"
            );
        }
    }

    #[test]
    fn spaces_after_lines() {
        let input = "# text = Ёж идёт.\n1\tЁж\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=\\n\n\
                     2\tидёт\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n3\t.\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=\\n\n\n\
                     # text = Кот\n1\tКот\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=x\n\n";
        let doc = &read_conllu(input).unwrap()[0];
        assert_eq!(doc.text(), "Ёж\nидёт.\nКот");
        let lines: Vec<(&str, Option<LineBreak>)> = doc
            .lines()
            .iter()
            .map(|line| (line.span.slice(doc.text()), line.break_before))
            .collect();
        assert_eq!(
            lines,
            [
                ("Ёж", None),
                ("идёт.", Some(LineBreak::Author)),
                ("Кот", Some(LineBreak::Author))
            ]
        );
        assert_eq!(doc.to_conllu(), input);
    }

    #[test]
    #[cfg(feature = "ru")]
    fn generated_spaces() {
        let text = "а\u{a0}б\u{200b}в\u{2028}г \u{feff}\u{3000}д\r\nе\tж\u{a0}\u{a0}з.\nИ";
        let mut doc = crate::Segmenter::new("ru").unwrap().segment(text);
        let tokens: Vec<&str> = doc.tokens().iter().map(|span| span.slice(text)).collect();
        assert_eq!(tokens, ["а", "б", "в", "г", "д", "е", "ж", "з.", "И"]);
        doc = Document::from_tokens(text.to_owned(), doc.tokens().to_vec(), Some(&[8])).unwrap();
        let misc: Vec<String> = doc
            .to_conllu()
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| line.rsplit('\t').next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            misc,
            [
                "_",
                "SpaceAfter=No",
                "SpacesAfter=\\n",
                "SpacesAfter=\\s\\s",
                "SpacesAfter=\\r\\n",
                "SpacesAfter=\\t",
                "SpacesAfter=\\s\\s",
                "SpacesAfter=\\n",
                "_"
            ]
        );
        assert!(doc.to_conllu().contains("# text = а бв г  д е\tж  з.\n"));
        let back = &read_conllu(&doc.to_conllu()).unwrap()[0];
        assert_eq!((back.sentence_count(), back.lines().len()), (2, 4));
    }

    #[test]
    fn generated_nfc() {
        let text = "Мои\u{306} е\u{308}ж\u{2028}при-\u{2028}мер";
        let tokens = vec![
            Span { start: 0, end: 8 },
            Span { start: 9, end: 15 },
            Span { start: 18, end: 34 },
        ];
        let mut doc = Document::from_tokens(text.to_owned(), tokens, None).unwrap();
        doc.set_columns(
            vec![(
                Column::Lemma,
                vec![Some("мои\u{306}".to_owned()), None, None],
            )],
            None,
        )
        .unwrap();
        let written = doc.to_conllu();
        assert!(written.contains("# text = Мой ёж при- мер\n"), "{written}");
        assert!(written.contains("1\tМой\tмой\t"), "{written}");
        assert!(written.contains("3\tпри- мер\t"), "{written}");
        assert_eq!(doc.text(), text);
    }

    #[test]
    fn document_ids() {
        let doc = read_conllu("1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n\n")
            .unwrap()
            .remove(0);
        let doc =
            Document::from_tokens(doc.text().to_owned(), doc.tokens().to_vec(), None).unwrap();
        assert_eq!(
            doc.to_conllu_with_id("d-1").unwrap(),
            "# newdoc id = d-1\n# newpar\n# sent_id = d-1-1\n# text = а\n1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
        );
        let decomposed = doc.to_conllu_with_id("е\u{308}ж").unwrap();
        assert!(decomposed.starts_with("# newdoc id = ёж\n# newpar\n# sent_id = ёж-1\n"));
        for id in ["", "d 1", "d\t1"] {
            assert_eq!(
                doc.to_conllu_with_id(id).unwrap_err(),
                DocumentError(format!("document ID {id:?} is empty or holds whitespace"))
            );
        }
    }

    #[test]
    fn errors() {
        for (input, line, message) in [
            ("1\tа\n\n", 1, "expected 10 tab-separated columns, got 2"),
            (
                "2\tа\t_\t_\t_\t_\t_\t_\t_\t_\n\n",
                1,
                "expected word 1, got \"2\"",
            ),
            (
                "1\tа\t_\t_\t_\t_\tx\t_\t_\t_\n\n",
                1,
                "cannot read the head \"x\"",
            ),
            (
                "1-2\tаб\t_\t_\t_\t_\t_\t_\t_\t_\n1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n\n",
                3,
                "a multiword token without all its words",
            ),
            (
                "# text = а\n1\tб\t_\t_\t_\t_\t_\t_\t_\t_\n\n",
                2,
                "the form \"б\" does not match the text of the sentence",
            ),
            (
                "# text = а б\n1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n\n",
                1,
                "the text of the sentence holds characters that no token has",
            ),
            (
                "1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n# x\n\n",
                2,
                "a comment after token lines",
            ),
            ("# x\n\n", 2, "a sentence without words"),
            (
                "1\tа\t_\t_\t_\t_\t_\t_\t_\t_\n",
                1,
                "the text does not end with a blank line",
            ),
        ] {
            assert_eq!(
                read_conllu(input).unwrap_err(),
                ConlluError {
                    line,
                    message: message.to_owned()
                },
                "{input:?}"
            );
        }
    }
}

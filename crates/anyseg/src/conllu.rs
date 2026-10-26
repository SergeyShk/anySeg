//! Reading and writing CoNLL-U

use std::error::Error;
use std::fmt;
use std::fmt::Write as _;

use crate::Span;
use crate::document::{Column, Document, Line, Verbatim};
use crate::layout::{is_separator, one_line};

/// A line of CoNLL-U that cannot be read
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConlluError {
    /// The number of the line, from 1
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ConlluError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl Error for ConlluError {}

/// Columns 2-10 of a word line
struct Word<'a> {
    form: &'a str,
    columns: [Option<&'a str>; 7],
    head: Option<u32>,
}

struct Token<'a> {
    form: &'a str,
    /// Columns 2-10 of the line of a multiword token
    multiword: Option<&'a str>,
    misc: Option<&'a str>,
    words: usize,
}

#[derive(Default)]
struct Sentence<'a> {
    comments: Vec<&'a str>,
    tokens: Vec<Token<'a>>,
    words: Vec<Word<'a>>,
    empty: Vec<(usize, String)>,
    /// The number of multiword token and word lines so far
    lines: usize,
    /// The number of the first line
    line: usize,
}

impl Sentence<'_> {
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
    Some(out)
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
    sentence: &mut Sentence<'a>,
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
    sentence.words.push(Word {
        form: fields[1],
        columns: [
            field(fields[2]),
            field(fields[3]),
            field(fields[4]),
            field(fields[5]),
            field(fields[7]),
            field(fields[8]),
            field(fields[9]),
        ],
        head,
    });
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
        }),
    }
    Ok(())
}

/// The documents of a CoNLL-U text: a `# newdoc` comment starts a document and `# newpar` a paragraph. The text
/// of a sentence is its `# text` comment or, without it, the forms of its tokens with the spaces of `SpaceAfter`
/// and `SpacesAfter`; sentences of a paragraph are joined by a space (none after `SpaceAfter=No`) and paragraphs
/// by a blank line. Comments, multiword token lines and empty nodes are kept, so that
/// [`Document::to_conllu`] writes the text back as it was.
///
/// # Errors
///
/// [`ConlluError`] on a line that is not a comment, a blank line or a token line of 10 columns with IDs in order,
/// or on forms that do not match the `# text` of their sentence.
pub fn read_conllu(input: &str) -> Result<Vec<Document>, ConlluError> {
    let mut sentences = Vec::new();
    let mut sentence = Sentence::default();
    let mut multiword = None;
    for (index, line) in input.lines().enumerate() {
        let number = index + 1;
        if line.is_empty() {
            if !sentence.words.is_empty() || !sentence.comments.is_empty() {
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
                sentences.push(std::mem::take(&mut sentence));
            }
        } else if line.starts_with('#') {
            if !sentence.words.is_empty() || !sentence.tokens.is_empty() {
                return Err(ConlluError {
                    line: number,
                    message: "a comment after token lines".to_owned(),
                });
            }
            if sentence.comments.is_empty() {
                sentence.line = number;
            }
            sentence.comments.push(line);
        } else {
            if sentence.comments.is_empty() && sentence.words.is_empty() {
                sentence.line = number;
            }
            parse(&mut sentence, number, line, &mut multiword)?;
        }
    }
    if !sentence.words.is_empty() || !sentence.comments.is_empty() {
        return Err(ConlluError {
            line: input.lines().count(),
            message: "the text does not end with a blank line".to_owned(),
        });
    }
    let mut documents = Vec::new();
    let mut start = 0;
    for index in 1..=sentences.len() {
        if index == sentences.len() || sentences[index].has_comment("newdoc") {
            documents.push(document(&sentences[start..index])?);
            start = index;
        }
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

fn document(sentences: &[Sentence<'_>]) -> Result<Document, ConlluError> {
    let mut text = String::new();
    let mut tokens = Vec::new();
    let mut token_words = vec![0];
    let mut word_tokens = Vec::new();
    let mut forms = Vec::new();
    let mut columns: [Vec<Option<String>>; 7] = Default::default();
    let mut heads = Vec::new();
    let mut sentence_tokens = Vec::new();
    let mut paragraph_sentences = Vec::new();
    let mut verbatim = Verbatim::default();
    for (index, sentence) in sentences.iter().enumerate() {
        let new_paragraph = index == 0 || sentence.has_comment("newpar");
        if index > 0 {
            if new_paragraph {
                text.push_str("\n\n");
            } else if !misc_has(
                sentences[index - 1]
                    .tokens
                    .last()
                    .and_then(|token| token.misc),
                "SpaceAfter=No",
            ) {
                text.push(' ');
            }
        }
        if new_paragraph {
            paragraph_sentences.push(index);
        }
        let start = text.len();
        match sentence.text() {
            Some(sentence_text) => text.push_str(sentence_text),
            None => {
                for (position, token) in sentence.tokens.iter().enumerate() {
                    text.push_str(token.form);
                    if position + 1 < sentence.tokens.len() {
                        match spaces_after(token.misc) {
                            Some(spaces) => text.push_str(&spaces),
                            None if misc_has(token.misc, "SpaceAfter=No") => {}
                            None => text.push(' '),
                        }
                    }
                }
            }
        }
        sentence_tokens.push(tokens.len());
        let mut position = start;
        let mut words = sentence.words.iter();
        for token in &sentence.tokens {
            let span = align(&text, position, token.form).ok_or_else(|| ConlluError {
                line: sentence.line,
                message: format!(
                    "the form {:?} does not match the text of the sentence",
                    token.form
                ),
            })?;
            position = span.end;
            let token_index = tokens.len();
            tokens.push(span);
            if let Some(line) = token.multiword {
                verbatim.multiword.push((token_index, line.to_owned()));
            }
            for word in words.by_ref().take(token.words) {
                let form = (token.multiword.is_some() || word.form != span.slice(&text))
                    .then(|| word.form.to_owned());
                forms.push(form);
                for (column, value) in columns.iter_mut().zip(word.columns) {
                    column.push(value.map(str::to_owned));
                }
                heads.push(word.head);
                word_tokens.push(token_index);
            }
            token_words.push(word_tokens.len());
        }
        if !text[position..].trim().is_empty() {
            return Err(ConlluError {
                line: sentence.line,
                message: "the text of the sentence holds characters that no token has".to_owned(),
            });
        }
        verbatim.comments.push(
            sentence
                .comments
                .iter()
                .map(|&comment| comment.to_owned())
                .collect(),
        );
        verbatim.empty.push(sentence.empty.clone());
    }
    sentence_tokens.push(tokens.len());
    paragraph_sentences.push(sentences.len());
    let lines = (0..paragraph_sentences.len() - 1)
        .map(|paragraph| Line {
            span: Span {
                start: tokens[sentence_tokens[paragraph_sentences[paragraph]]].start,
                end: tokens[sentence_tokens[paragraph_sentences[paragraph + 1]] - 1].end,
            },
            break_before: None,
        })
        .collect();
    let sparse = |values: Vec<Option<String>>| {
        if values.iter().all(Option::is_none) {
            Vec::new()
        } else {
            values
        }
    };
    Ok(Document {
        text,
        tokens,
        token_words,
        word_tokens,
        forms: sparse(forms),
        columns: columns.map(sparse),
        heads: if heads.iter().all(Option::is_none) {
            Vec::new()
        } else {
            heads
        },
        sentence_tokens,
        paragraph_sentences,
        lines,
        verbatim: Some(Box::new(verbatim)),
    })
}

impl Document {
    /// The document in CoNLL-U. A document read from CoNLL-U is written as it was read, with the columns set
    /// since. Otherwise every sentence gets `# sent_id` and `# text` (line breaks turned into spaces), the first one
    /// `# newdoc`, every paragraph `# newpar`, and the space after a token inside a paragraph is `SpaceAfter=No`
    /// when there is none and `SpacesAfter` when it is not one space.
    pub fn to_conllu(&self) -> String {
        let mut out = String::new();
        for sentence in 0..self.sentence_count() {
            self.write_sentence(sentence, &mut out);
        }
        out
    }

    fn write_sentence(&self, index: usize, out: &mut String) {
        let sentence = self.sentence(index);
        let verbatim = self.verbatim.as_deref();
        match verbatim {
            Some(verbatim) => {
                for comment in &verbatim.comments[index] {
                    out.push_str(comment);
                    out.push('\n');
                }
            }
            None => {
                let paragraph = self.sentence_paragraph(index);
                if index == 0 {
                    out.push_str("# newdoc\n");
                }
                if self.paragraph_sentences[paragraph] == index {
                    out.push_str("# newpar\n");
                }
                let _ = writeln!(out, "# sent_id = {}", index + 1);
                let _ = writeln!(
                    out,
                    "# text = {}",
                    one_line(sentence.span.slice(&self.text))
                );
            }
        }
        let empty: &[(usize, String)] = verbatim.map_or(&[], |verbatim| &verbatim.empty[index]);
        let mut empty = empty.iter().peekable();
        let mut lines = 0;
        let mut flush = |lines: usize, out: &mut String| {
            while let Some((_, line)) = empty.next_if(|node| node.0 <= lines) {
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
                            clean(form),
                            layout.as_deref().unwrap_or("_")
                        );
                    }
                }
                out.push('\n');
                lines += 1;
            }
            for word in words.clone() {
                flush(lines, out);
                let misc = self.column(Column::Misc, word);
                let misc = match (&layout, words.len()) {
                    (Some(layout), 1) => Some(merge(misc, layout)),
                    _ => misc.map(str::to_owned),
                };
                let value = |column| self.column(column, word).unwrap_or("_");
                let head = self
                    .head(word)
                    .map_or_else(|| "_".to_owned(), |head| head.to_string());
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    word - sentence.words.start + 1,
                    clean(self.form(word)),
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
        match &self.text[self.tokens[token].end..next.start] {
            "" => Some("SpaceAfter=No".to_owned()),
            " " => None,
            spaces => Some(format!("SpacesAfter={}", escape_spaces(spaces))),
        }
    }
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

/// A form fit for a column of CoNLL-U: tabs and line breaks turned into spaces
fn clean(form: &str) -> std::borrow::Cow<'_, str> {
    if form.contains(['\t', '\n', '\r']) {
        form.replace(['\t', '\n', '\r'], " ").into()
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
    }

    #[test]
    #[cfg(feature = "ru")]
    fn generated() {
        let text = "Ёж идёт,\nдомой.  Утро\n\n«Кот»";
        let mut doc = crate::Segmenter::new("ru").unwrap().segment(text);
        doc.set_columns(
            vec![(
                Column::Misc,
                vec![None, Some("Gloss=go".to_owned()), None, None, None],
            )],
            None,
        )
        .unwrap();
        assert_eq!(
            doc.to_conllu(),
            "# newdoc\n# newpar\n# sent_id = 1\n# text = Ёж идёт, домой.  Утро\n\
             1\tЁж\t_\t_\t_\t_\t_\t_\t_\t_\n\
             2\tидёт,\t_\t_\t_\t_\t_\t_\t_\tGloss=go|SpacesAfter=\\n\n\
             3\tдомой.\t_\t_\t_\t_\t_\t_\t_\tSpacesAfter=\\s\\s\n\
             4\tУтро\t_\t_\t_\t_\t_\t_\t_\t_\n\n\
             # newpar\n# sent_id = 2\n# text = «Кот»\n\
             1\t«Кот»\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
        );
        let back = &read_conllu(&doc.to_conllu()).unwrap()[0];
        let tokens = |doc: &Document| -> Vec<String> {
            doc.tokens()
                .iter()
                .map(|span| span.slice(doc.text()).to_owned())
                .collect()
        };
        assert_eq!(tokens(back), tokens(&doc));
        assert_eq!((back.sentence_count(), back.paragraph_count()), (2, 2));
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
                1,
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

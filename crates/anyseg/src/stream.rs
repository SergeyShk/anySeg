//! Segmentation of a text read in pieces, which gives the sentences of the whole text

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read};

use crate::layout::{self, is_separator};
use crate::{Document, Segmenter};

/// The size of the buffer of the input: complete paragraphs go out in pieces of about this size, and before a read
/// that may wait for input
#[cfg(not(test))]
pub(crate) const BUFFER: usize = 1 << 16;
#[cfg(test)]
pub(crate) const BUFFER: usize = 1 << 9;

/// The longest text without blank lines that is segmented at once: a longer one goes out without its last sentence,
/// and a longer line is cut at a space, so that the memory does not grow with the input. The allocator of macOS does
/// not give back blocks of more than 4 MiB, which the tokens of larger pieces reach, at two bytes a token
#[cfg(not(test))]
pub(crate) const PARAGRAPH: usize = 1 << 18;
#[cfg(test)]
pub(crate) const PARAGRAPH: usize = 1 << 12;

/// A document of a piece of a text read in pieces: complete paragraphs, or the sentences of a paragraph without
/// blank lines that is longer than 256 KiB
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Piece {
    /// The document of the piece
    pub document: Document,
    /// The offset of the text of the document in the input, in bytes after a byte order mark
    pub start: usize,
    /// The first paragraph of the document goes on with the last one of the piece before it
    pub continued: bool,
    /// The same offset in characters
    #[doc(hidden)]
    pub char_start: usize,
}

/// A piece as the cutter gives it: the document of the text segmented, of which the piece is the first sentences and
/// the first bytes. The writer of the command takes the sentences from the document itself, where the space after
/// the last of them is known
pub(crate) struct Cut {
    pub(crate) document: Document,
    pub(crate) sentences: usize,
    keep: usize,
    start: usize,
    char_start: usize,
    pub(crate) continued: bool,
}

impl Cut {
    fn piece(self) -> Piece {
        let document = if self.sentences == self.document.sentence_count()
            && self.keep == self.document.text().len()
        {
            self.document
        } else {
            self.document.truncated(self.sentences, self.keep)
        };
        Piece {
            document,
            start: self.start,
            continued: self.continued,
            char_start: self.char_start,
        }
    }
}

/// Cuts the lines of a text into pieces that are segmented as in the whole text
#[derive(Default)]
pub(crate) struct Cutter {
    /// The text read and not yet segmented
    text: String,
    /// The length of the text up to its last blank line: the paragraphs before it are complete
    ready: usize,
    /// The next piece goes on with a paragraph cut by its length
    continued: bool,
    /// The line read so far is blank: a long line comes in pieces
    blank: bool,
    /// The offset of the text in the input, in bytes and in characters
    start: usize,
    char_start: usize,
    /// Pieces of long lines of separators left out of the text: their position in it, bytes and characters
    dropped: Vec<(usize, usize, usize)>,
    /// The pieces cut and not yet taken
    pub(crate) pieces: VecDeque<Cut>,
}

impl Cutter {
    /// Text at the start of the input that is not read: a byte order mark
    pub(crate) fn skip(&mut self, text: &str) {
        self.start += text.len();
        self.char_start += text.chars().count();
    }

    /// Adds a line with its line break, or a piece of a line longer than PARAGRAPH, `first` if it starts the line
    /// and `ended` if it ends it; complete paragraphs go out as pieces of about BUFFER
    pub(crate) fn line(
        &mut self,
        segment: &dyn Fn(String) -> Document,
        line: &str,
        first: bool,
        ended: bool,
    ) {
        self.blank = (first || self.blank) && blank(line);
        if !self.blank {
            self.text.push_str(line);
            if self.text.len() >= PARAGRAPH || !ended {
                self.cut_long(segment);
            }
        } else if ended || closes_paragraph(&format!("\n{line}")) {
            // A blank line ends the paragraphs before it, and so does a piece of a long line of separators that holds
            // the end of a paragraph
            if blank(&self.text) {
                self.continued = false;
            }
            if self.text.is_empty() {
                self.skip(line);
            } else {
                self.text.push_str(line);
                self.ready = self.text.len();
            }
            if self.ready >= BUFFER {
                self.cut_ready(segment);
            }
        } else if self.text.is_empty() {
            self.skip(line);
        } else {
            self.dropped
                .push((self.text.len(), line.len(), line.chars().count()));
        }
    }

    /// The text up to its last blank line, whose paragraphs are complete
    pub(crate) fn cut_ready(&mut self, segment: &dyn Fn(String) -> Document) {
        if self.ready == 0 {
            return;
        }
        let rest = self.text.split_off(self.ready);
        let ready = std::mem::replace(&mut self.text, rest);
        self.ready = 0;
        let document = segment(ready);
        let (count, length) = (document.sentence_count(), document.text().len());
        self.emit(document, count, length);
        self.continued = false;
    }

    /// The rest of the text, at the end of the input
    pub(crate) fn finish(&mut self, segment: &dyn Fn(String) -> Document) {
        self.ready = self.text.len();
        self.cut_ready(segment);
    }

    /// A text without a blank line that reached PARAGRAPH, without its last sentence, which the next line may go on
    /// with: it stays at the start of the text, and the next piece continues its paragraph. One sentence longer than
    /// PARAGRAPH goes out whole, and the separators after it stay: with the next line they may end the paragraph
    fn cut_long(&mut self, segment: &dyn Fn(String) -> Document) {
        let document = segment(std::mem::take(&mut self.text));
        let count = document.sentence_count();
        let (written, keep) = match count {
            0 => (0, document.text().len()),
            1 => (1, document.sentence(0).span.end),
            _ => (count - 1, document.sentence(count - 1).span.start),
        };
        // The last sentence may itself start a paragraph (after U+2029), and the separators dropped may end one
        let continued = match count {
            0 => self.continued && !closes_paragraph(document.text()),
            1 => true,
            _ => {
                document
                    .paragraph(document.paragraph_count() - 1)
                    .sentences
                    .start
                    < count - 1
            }
        };
        let rest = document.text()[keep..].to_owned();
        self.emit(document, written, keep);
        self.continued = continued;
        self.text = rest;
        self.ready = 0;
    }

    /// The piece of the first `sentences` of a document of the text, whose first `keep` bytes it takes
    fn emit(&mut self, document: Document, sentences: usize, keep: usize) {
        let (start, char_start) = (self.start, self.char_start);
        self.char_start += document.text()[..keep].chars().count();
        self.start += keep;
        self.dropped.retain_mut(|(position, bytes, chars)| {
            if *position <= keep {
                self.start += *bytes;
                self.char_start += *chars;
                return false;
            }
            *position -= keep;
            true
        });
        if sentences == 0 {
            return;
        }
        // What the last piece kept and what was read since may end the paragraph before the first token
        let lead = document
            .tokens()
            .first()
            .map_or("", |token| &document.text()[..token.start]);
        let continued = std::mem::take(&mut self.continued) && !closes_paragraph(lead);
        self.pieces.push_back(Cut {
            document,
            sentences,
            keep,
            start,
            char_start,
            continued,
        });
    }
}

/// A text of nothing but separators
pub(crate) fn blank(text: &str) -> bool {
    text.chars().all(is_separator)
}

/// The separators after a sentence hold the end of its paragraph: U+2029 or a blank line
pub(crate) fn closes_paragraph(separators: &str) -> bool {
    layout::paragraphs(&format!("x{separators}x")).len() > 1
}

/// Reads a line of the input with its line break (LF, CR or CRLF) into `bytes`, or a piece of a line longer than
/// PARAGRAPH, which ends after a space or else at a character, and keeps the rest of it in `carry`; `true` - the
/// line ended, `None` - the input did. Before a read that may wait for input (an empty buffer) `wait` runs. After the
/// end of the input (`end`) nothing is read: in a terminal a line without a line break ends with one Ctrl+D
pub(crate) fn read_line<R: Read, E: From<io::Error>>(
    reader: &mut BufReader<R>,
    bytes: &mut Vec<u8>,
    carry: &mut Vec<u8>,
    end: &mut bool,
    mut wait: impl FnMut() -> Result<(), E>,
) -> Result<Option<bool>, E> {
    bytes.append(carry);
    // A CR ended the buffer: the line feed of CRLF may begin the next one
    let mut after_cr = false;
    while !*end {
        if bytes.len() >= PARAGRAPH && !after_cr {
            let cut = bytes
                .iter()
                .rposition(|&byte| byte == b' ' || byte == b'\t')
                .map(|at| at + 1)
                .or_else(|| (1..bytes.len()).rev().find(|&at| bytes[at] & 0xC0 != 0x80))
                .unwrap_or(bytes.len());
            *carry = bytes.split_off(cut);
            return Ok(Some(false));
        }
        if reader.buffer().is_empty() {
            wait()?;
        }
        let available = match reader.fill_buf() {
            Ok(available) => available,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        };
        if available.is_empty() {
            *end = true;
            break;
        }
        if after_cr {
            if available[0] == b'\n' {
                bytes.push(b'\n');
                reader.consume(1);
            }
            break;
        }
        let (length, done) = match available
            .iter()
            .position(|&byte| byte == b'\n' || byte == b'\r')
        {
            Some(at) if available[at] == b'\r' => match available.get(at + 1) {
                Some(b'\n') => (at + 2, true),
                Some(_) => (at + 1, true),
                None => {
                    after_cr = true;
                    (at + 1, false)
                }
            },
            Some(at) => (at + 1, true),
            None => (available.len(), false),
        };
        bytes.extend_from_slice(&available[..length]);
        reader.consume(length);
        if done {
            break;
        }
    }
    Ok((!bytes.is_empty()).then_some(true))
}

/// The pieces of a text read from a reader, in order: see [`Segmenter::stream`]
pub struct Stream<S, R> {
    segmenter: S,
    reader: BufReader<R>,
    cutter: Cutter,
    bytes: Vec<u8>,
    carry: Vec<u8>,
    /// The input ended
    end: bool,
    /// The reading ended, at the end of the input or at an error
    done: bool,
    /// The number of the current line, from 1, and whether the next read starts a line
    number: usize,
    starts: bool,
}

impl<S: AsRef<Segmenter>, R: Read> Stream<S, R> {
    /// The pieces of the text of a reader in UTF-8, segmented by a segmenter that the stream owns or borrows
    pub fn new(segmenter: S, reader: R) -> Self {
        Stream {
            segmenter,
            reader: BufReader::with_capacity(BUFFER, reader),
            cutter: Cutter::default(),
            bytes: Vec::new(),
            carry: Vec::new(),
            end: false,
            done: false,
            number: 0,
            starts: true,
        }
    }

    /// Reads the next line into the cutter; `false` at the end of the input
    fn read(&mut self) -> io::Result<bool> {
        let segmenter = self.segmenter.as_ref();
        let segment = |text: String| segmenter.segment_string(text);
        let cutter = &mut self.cutter;
        self.bytes.clear();
        let Some(ended) = read_line(
            &mut self.reader,
            &mut self.bytes,
            &mut self.carry,
            &mut self.end,
            || {
                cutter.cut_ready(&segment);
                Ok::<(), io::Error>(())
            },
        )?
        else {
            cutter.finish(&segment);
            return Ok(false);
        };
        let first = std::mem::replace(&mut self.starts, ended);
        self.number += usize::from(first);
        let line = std::str::from_utf8(&self.bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("line {}: not UTF-8", self.number),
            )
        })?;
        let line = match line.strip_prefix('\u{feff}') {
            Some(rest) if self.number == 1 && first => {
                cutter.skip("\u{feff}");
                rest
            }
            _ => line,
        };
        cutter.line(&segment, line, first, ended);
        Ok(true)
    }
}

impl<S: AsRef<Segmenter>, R: Read> Iterator for Stream<S, R> {
    type Item = io::Result<Piece>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(cut) = self.cutter.pieces.pop_front() {
                return Some(Ok(cut.piece()));
            }
            if self.done {
                return None;
            }
            match self.read() {
                Ok(true) => {}
                Ok(false) => self.done = true,
                Err(err) => {
                    self.done = true;
                    return Some(Err(err));
                }
            }
        }
    }
}

#[cfg(all(test, feature = "ru", feature = "es"))]
mod tests {
    use super::*;
    use crate::Span;

    /// The pieces of a text give the tokens, words, sentences and paragraphs of the whole text, at their offsets
    fn same_as_whole(language: &str, text: &str) -> Vec<Piece> {
        same_tokens(language, text, true)
    }

    /// The same, or only the tokens and words if a sentence of the text is longer than PARAGRAPH, which is cut
    fn same_tokens(language: &str, text: &str, sentences_too: bool) -> Vec<Piece> {
        let segmenter = Segmenter::new(language).unwrap();
        let whole = segmenter.segment(text);
        let pieces: Vec<Piece> = segmenter
            .stream(text.as_bytes())
            .collect::<io::Result<_>>()
            .unwrap();
        let (mut tokens, mut sentences, mut words, mut paragraphs) = (vec![], vec![], vec![], 0);
        let shifted = |span: Span, start: usize| (span.start + start, span.end + start);
        for piece in &pieces {
            let doc = &piece.document;
            assert_eq!(
                &text[piece.start..piece.start + doc.text().len()],
                doc.text()
            );
            assert_eq!(text[..piece.start].chars().count(), piece.char_start);
            tokens.extend(doc.tokens().iter().map(|&span| shifted(span, piece.start)));
            sentences.extend(
                (0..doc.sentence_count())
                    .map(|index| shifted(doc.sentence(index).span, piece.start)),
            );
            words.extend((0..doc.word_count()).map(|word| doc.form(word).to_owned()));
            paragraphs += doc.paragraph_count() - usize::from(piece.continued);
        }
        let expected = |doc: &Document| {
            (
                doc.tokens()
                    .iter()
                    .map(|&span| shifted(span, 0))
                    .collect::<Vec<_>>(),
                (0..doc.sentence_count())
                    .map(|index| shifted(doc.sentence(index).span, 0))
                    .collect::<Vec<_>>(),
                (0..doc.word_count())
                    .map(|word| doc.form(word).to_owned())
                    .collect::<Vec<_>>(),
            )
        };
        let (whole_tokens, whole_sentences, whole_words) = expected(&whole);
        let first = |a: &[(usize, usize)], b: &[(usize, usize)]| {
            a.iter()
                .zip(b)
                .position(|(x, y)| x != y)
                .map(|at| (at, a[at], b[at]))
        };
        assert_eq!(
            (tokens.len(), first(&tokens, &whole_tokens)),
            (whole_tokens.len(), None),
            "tokens of {text:.60?}"
        );
        assert!(words == whole_words, "words of {text:.60?}");
        if sentences_too {
            assert_eq!(
                (sentences.len(), first(&sentences, &whole_sentences)),
                (whole_sentences.len(), None),
                "sentences of {text:.60?}"
            );
            assert_eq!(paragraphs, whole.paragraph_count());
        }
        pieces
    }

    #[test]
    fn pieces_are_the_whole_text() {
        let paragraphs = "Кот спит. Ёж идёт.\n\nУтро «настало».\r\n\r\n".repeat(200);
        let pieces = same_as_whole("ru", &paragraphs);
        assert!(pieces.len() > 5 && pieces.iter().all(|piece| !piece.continued));
        same_as_whole("ru", &format!("\u{feff}\n\n  {paragraphs}"));
        same_as_whole(
            "es",
            &"Vengo del mar. Quiero decírselo.\n\nAl fin.\n".repeat(300),
        );
        // A paragraph without blank lines goes out in pieces of whole sentences that continue it
        let long = "Мама мыла раму. Папа читал.\n".repeat(PARAGRAPH / 10);
        let pieces = same_as_whole("ru", &long);
        assert!(pieces.len() > 2 && pieces[1..].iter().all(|piece| piece.continued));
        // And so does a line longer than PARAGRAPH; one sentence longer than it is cut
        same_as_whole("ru", &"Мама мыла раму. ".repeat(PARAGRAPH / 4));
        let pieces = same_tokens("ru", &"мама мыла раму. ".repeat(PARAGRAPH / 4), false);
        assert!(pieces.len() > 1 && pieces[1..].iter().all(|piece| piece.continued));
        same_as_whole("ru", "");
        same_as_whole("ru", " \n\n ");
    }

    #[test]
    fn not_utf8() {
        let segmenter = Segmenter::new("ru").unwrap();
        let mut stream = segmenter.stream(&b"\xd0\x81\xd0\xb6.\n\xff\n"[..]);
        let error = stream.next().unwrap().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "line 2: not UTF-8");
        assert!(stream.next().is_none());
    }

    #[test]
    fn owned_segmenter() {
        let stream = Stream::new(Segmenter::new("ru").unwrap(), &b"\xd0\x81\xd0\xb6.\n"[..]);
        let pieces: Vec<Piece> = stream.collect::<io::Result<_>>().unwrap();
        assert_eq!(pieces[0].document.text(), "Ёж.\n");
    }
}

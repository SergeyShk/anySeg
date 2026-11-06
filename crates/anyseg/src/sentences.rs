//! Sentences: the boundaries inside a paragraph, from its tokens, its lines and the lists of a language

use crate::Span;
use crate::document::{Line, LineBreak};
use crate::lang::{DigitGroups, Tables};
use crate::tokens::{Exceptions, is_alpha, is_emoji, is_mark, is_word};

/// Marks that start a list item at the beginning of a line
const BULLETS: &str = "-‐‒–—―−•·*▪◦‣●■►▸→✓✔";

/// A list item number has at most this many digits; a longer one is a year or an amount
const MAX_ITEM_DIGITS: usize = 3;

/// A block is verse with at least this many rhyming lines, or one pair in a stanza of as many lines
const MIN_RHYMES: usize = 4;

/// How far apart two rhyming lines may stand: AABB, ABAB and ABBA all fit
const RHYME_WINDOW: usize = 3;

/// A token of final marks: «.», «?!», «...», «…»
fn is_final(piece: &str) -> bool {
    piece
        .chars()
        .all(|character| matches!(character, '.' | '!' | '?' | '…'))
}

/// A quote written as an HTML entity: «&quot;», «&#39;&#39;»
fn is_entity(piece: &str) -> bool {
    piece.len() > 2 && piece.starts_with('&') && piece.ends_with(';')
}

/// A closing quote or bracket, which stays with the final mark before it
fn is_closing(piece: &str) -> bool {
    matches!(
        piece,
        "»" | "”" | "“" | "\"" | "'" | "''" | ")" | "]" | "}" | "’" | "›"
    ) || is_entity(piece)
}

fn is_opening(piece: &str) -> bool {
    matches!(
        piece,
        "«" | "„" | "“" | "\"" | "'" | "``" | "(" | "[" | "{" | "‘" | "‹" | "¿" | "¡"
    ) || is_entity(piece)
}

fn is_dash(piece: &str) -> bool {
    piece
        .chars()
        .all(|character| matches!(character, '-' | '‐' | '‒' | '–' | '—' | '―' | '−'))
}

/// An emoji or an emoticon: «😀», «:)», «)))»; decided by the first byte, as it runs for every token
fn is_emoticon(piece: &str, tables: &Tables) -> bool {
    let bytes = piece.as_bytes();
    let face = match bytes.first() {
        Some(b':' | b';' | b'=') => bytes.len() > 1,
        Some(&bracket @ (b')' | b'(')) => bytes.len() > 1 && bytes.iter().all(|&b| b == bracket),
        Some(0xE2 | 0xF0) => piece.chars().next().is_some_and(is_emoji),
        _ => false,
    };
    face || tables.is_listed_emoticon(piece)
}

fn is_stars(piece: &str) -> bool {
    piece.len() >= 3 && piece.bytes().all(|byte| byte == b'*')
}

/// A run of the marks of emphasis in Markdown: «*», «**», «__», «~~»
fn is_emphasis(piece: &str) -> bool {
    let mut bytes = piece.bytes();
    bytes
        .next()
        .is_some_and(|first| matches!(first, b'*' | b'_' | b'~') && bytes.all(|byte| byte == first))
}

/// A mention, which starts a sentence after final marks or an emoticon: «Спасибо! @anna да»
fn is_mention(piece: &str) -> bool {
    piece.len() > 1 && piece.starts_with('@')
}

fn has_word(piece: &str) -> bool {
    let mut chars = piece.chars();
    chars.next().is_some_and(is_word) || chars.any(is_word)
}

fn starts_upper(piece: &str) -> bool {
    piece.chars().next().is_some_and(char::is_uppercase)
}

fn is_number(piece: &str) -> bool {
    piece.starts_with(|c: char| c.is_ascii_digit()) || piece.chars().all(|c| "IVXLCDM".contains(c))
}

/// An integer or a decimal number, which a unit may follow, but not a year, a date or a time: «5», «1,5», «15 000»,
/// and with marks between groups of three digits where the language groups digits so: «1.012,72»
fn is_quantity(piece: &str, groups: DigitGroups) -> bool {
    let year = piece.len() == 4
        && piece.starts_with(['1', '2'])
        && piece.bytes().all(|byte| byte.is_ascii_digit());
    let group_mark = match groups {
        DigitGroups::Space => None,
        DigitGroups::Point => Some('.'),
        DigitGroups::Comma => Some(','),
    };
    let marks = piece
        .char_indices()
        .filter(|&(at, c)| {
            matches!(c, ',' | '.')
                && !(Some(c) == group_mark
                    && piece[at + 1..]
                        .bytes()
                        .take_while(u8::is_ascii_digit)
                        .count()
                        == 3)
        })
        .count();
    piece.starts_with(|c: char| c.is_ascii_digit())
        && piece.chars().all(|c| {
            c.is_ascii_digit() || matches!(c, ',' | '.' | ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}')
        })
        && marks <= 1
        && !year
}

/// A word with the period of an abbreviation in the token: «г.», «т.», «А.»
fn is_abbreviation(piece: &str) -> bool {
    piece.len() > 1
        && piece.ends_with('.')
        && piece[..piece.len() - 1]
            .chars()
            .next_back()
            .is_some_and(|c| is_alpha(c) || is_mark(c))
}

/// The first letter or digit of a line, past the quotes, dashes and bullets before it, is a capital letter; the
/// time of a chat log counts as nothing: «[12:01] Вася», «12:01 Вася»
fn begins_upper(line: &str) -> bool {
    let Some(first) = line.find(is_word) else {
        return false;
    };
    let rest = &line[first..];
    if rest.starts_with(|c: char| c.is_ascii_digit()) {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !(line[..first].ends_with('[') || rest[digits..].starts_with(':')) {
            return false;
        }
    }
    rest.chars()
        .find(|&c| is_alpha(c))
        .is_some_and(char::is_uppercase)
}

/// The last word of a line and its last two letters, unless it starts with a capital letter, as a name does
fn ending(line: &str) -> Option<(&str, [char; 2])> {
    let line = line.trim_end_matches(|c| !is_alpha(c));
    let word = line.rsplit(|c| !is_alpha(c)).next()?;
    let mut letters = word.chars().rev();
    let last = letters.next()?;
    let before = letters.next()?;
    word.starts_with(char::is_lowercase)
        .then_some((word, [before, last]))
}

/// A line that does not end with punctuation: a heading, a line of a post
fn ends_open(line: &str) -> bool {
    line.chars()
        .next_back()
        .is_some_and(|c| is_word(c) || !(c.is_ascii_punctuation() || "«»„“”‘’…—–‒―−".contains(c)))
}

struct Splitter<'a> {
    text: &'a str,
    lines: &'a [Span],
    tokens: &'a [Span],
    tables: &'a Tables,
    /// The first token of every line
    line_tokens: Vec<usize>,
    /// Lines end with a comma before a line with a capital letter, or rhyme
    verse: bool,
    /// Some line has lowercase letters, so a line without them is a heading
    cased: bool,
    /// Some line goes on with a lowercase letter, as in text wrapped at the width of a page
    typeset: bool,
    /// The first token of the current sentence
    sentence: usize,
    /// The first token of the sentence before it
    previous_sentence: Option<usize>,
}

/// A text split into tokens, sentences and paragraphs
#[derive(Default)]
pub(crate) struct Split {
    pub(crate) tokens: Vec<Span>,
    /// The first token of every sentence
    pub(crate) starts: Vec<usize>,
    /// The first sentence of every paragraph
    pub(crate) paragraphs: Vec<usize>,
    /// The lines with how each begins, if kept
    pub(crate) lines: Option<Vec<Line>>,
}

/// Splits the tokens of a paragraph into sentences and appends them to `out`, with the period of an abbreviation
/// that ends a sentence as a token of its own
pub(crate) fn split(
    text: &str,
    lines: &[Span],
    tokens: &[Span],
    tables: &Tables,
    exceptions: &Exceptions,
    out: &mut Split,
) {
    if tokens.is_empty() {
        return;
    }
    let mut splitter = Splitter {
        text,
        lines,
        tokens,
        tables,
        line_tokens: Vec::new(),
        verse: false,
        cased: false,
        typeset: false,
        sentence: 0,
        previous_sentence: None,
    };
    if lines.len() > 1 {
        splitter.read_lines();
    }
    let first_line = out.lines.as_ref().map_or(0, Vec::len);
    if let Some(breaks) = out.lines.as_mut() {
        breaks.extend(lines.iter().enumerate().map(|(index, &span)| Line {
            span,
            break_before: (index > 0).then_some(LineBreak::Author),
        }));
    }
    out.paragraphs.push(out.starts.len());
    out.starts.push(out.tokens.len());
    let mut words = false;
    // Round brackets opened and not closed yet
    let mut open = 0usize;
    let mut next_line = 1;
    let mut index = 0;
    while index < tokens.len() {
        let (last, mut boundary) = splitter.end(index, words, open == 0);
        // At a line break after the part, the rules of lines may start a sentence where the marks do not
        if !boundary && next_line < lines.len() && splitter.line_tokens[next_line] == last + 1 {
            boundary = splitter.line_boundary(next_line - 1);
        }
        for position in index..=last {
            words = words || has_word(splitter.piece(position));
            let span = tokens[position];
            let piece = splitter.piece(position);
            match piece {
                "(" => open += 1,
                ")" => open = open.saturating_sub(1),
                _ => {}
            }
            let ends = position == last && (boundary || last + 1 == tokens.len());
            if ends
                && tables.conventions.final_period_token
                && is_abbreviation(piece)
                && !exceptions.tokens.contains(piece)
            {
                out.tokens.push(Span {
                    start: span.start,
                    end: span.end - 1,
                });
                out.tokens.push(Span {
                    start: span.end - 1,
                    end: span.end,
                });
            } else {
                out.tokens.push(span);
            }
            if next_line < lines.len() && splitter.line_tokens[next_line] == position + 1 {
                let kind = if (position == last && boundary) || !splitter.wraps(next_line - 1) {
                    LineBreak::Author
                } else {
                    LineBreak::Typeset
                };
                if let Some(breaks) = out.lines.as_mut() {
                    breaks[first_line + next_line].break_before = Some(kind);
                }
                next_line += 1;
            }
        }
        if boundary && last + 1 < tokens.len() {
            out.starts.push(out.tokens.len());
            words = false;
            splitter.previous_sentence = Some(splitter.sentence);
            splitter.sentence = last + 1;
        }
        index = last + 1;
    }
}

impl Splitter<'_> {
    fn piece(&self, index: usize) -> &str {
        self.tokens[index].slice(self.text)
    }

    fn line(&self, index: usize) -> &str {
        self.lines[index].slice(self.text)
    }

    /// The token right after the one before it, without a space between
    fn attached(&self, index: usize) -> bool {
        self.tokens[index - 1].end == self.tokens[index].start
    }

    fn read_lines(&mut self) {
        let mut token = 0;
        for line in self.lines {
            while token + 1 < self.tokens.len() && self.tokens[token].start < line.start {
                token += 1;
            }
            self.line_tokens.push(token);
        }
        for pair in self.lines.windows(2) {
            let (before, after) = (pair[0].slice(self.text), pair[1].slice(self.text));
            self.verse |= before.ends_with([',', ';']) && begins_upper(after);
            self.typeset |= after.starts_with(char::is_lowercase);
        }
        if !self.verse && !self.typeset {
            self.verse = self.rhymes();
        }
        self.cased = self.verse
            && self
                .lines
                .iter()
                .any(|line| line.slice(self.text).contains(char::is_lowercase));
    }

    /// Whether the lines rhyme: at least half of them with a line close to them, at least four and with different
    /// endings, as a list of one grammatical form has one; or a stanza of four lines in a metre with a pair two lines
    /// apart, as a pair side by side can be a chance
    fn rhymes(&self) -> bool {
        let count = self.lines.len();
        if count < MIN_RHYMES {
            return false;
        }
        let ends: Vec<Option<(&str, [char; 2])>> = self
            .lines
            .iter()
            .map(|line| ending(line.slice(self.text)))
            .collect();
        let rhymes: Vec<(usize, [char; 2])> = (0..count)
            .filter_map(|index| {
                let (word, letters) = ends[index]?;
                (index.saturating_sub(RHYME_WINDOW)..count.min(index + RHYME_WINDOW + 1))
                    .any(|other| {
                        ends[other].is_some_and(|(other, end)| end == letters && other != word)
                    })
                    .then_some((index, letters))
            })
            .collect();
        rhymes.len() * 2 >= count
            && match rhymes.as_slice() {
                [(first, _), (second, _)] => second - first == 2 && self.metre(),
                [(_, letters), rest @ ..] => {
                    rest.len() + 1 >= MIN_RHYMES && rest.iter().any(|(_, end)| end != letters)
                }
                [] => false,
            }
    }

    /// Whether all lines are within 30 % of the middle length and a mark closes the last one, which the lines of a
    /// post seldom have
    fn metre(&self) -> bool {
        if self
            .lines
            .last()
            .is_some_and(|line| ends_open(line.slice(self.text)))
        {
            return false;
        }
        let mut lengths: Vec<usize> = self
            .lines
            .iter()
            .map(|line| line.slice(self.text).chars().count())
            .collect();
        lengths.sort_unstable();
        let middle = lengths[lengths.len() / 2];
        lengths[0] * 10 >= middle * 7 && lengths[lengths.len() - 1] * 10 <= middle * 13
    }

    /// The last token of the sentence part that starts at `index`, and whether a sentence ends after it. A part is
    /// one token, or a final mark or an abbreviation with the final marks and closing quotes and brackets right
    /// after it; `unpaired` tells that no round bracket is open before it
    fn end(&self, index: usize, words: bool, unpaired: bool) -> (usize, bool) {
        let piece = self.piece(index);
        let count = self.tokens.len();
        if index + 1 == count {
            return (index, false);
        }
        let candidate =
            matches!(piece.as_bytes().last(), Some(b'.' | b'!' | b'?')) || piece.ends_with('…');
        let stars = is_stars(piece) || (piece == "*" && index > 0 && self.piece(index - 1) == "*");
        // A closing bracket that no bracket opened, right after a word, is a smile: «Спасибо) Завтра»; after a
        // number or a word without lowercase letters at the start of a line, it closes a list marker: «II)», «AB)»
        let smile = unpaired
            && piece == ")"
            && index > 0
            && self.attached(index)
            && !(self.line_start(index - 1)
                && !self.piece(index - 1).chars().any(char::is_lowercase))
            && !is_number(self.piece(index - 1))
            && self
                .piece(index - 1)
                .chars()
                .filter(|&c| is_alpha(c))
                .nth(1)
                .is_some();
        let emoticon = smile || is_emoticon(piece, self.tables);
        if !candidate && !stars && !emoticon {
            return (index, false);
        }
        let words = words || has_word(piece);
        if stars {
            // A separator of stars is a sentence of its own: «* * *», «***»
            return (index, starts_upper(self.piece(index + 1)));
        }
        if emoticon {
            // An emoticon after the words of a sentence ends it before a capital letter or a mention, unless it
            // starts a line as a bullet
            if self.line_start(index) {
                return (index, false);
            }
            let next = self.piece(index + 1);
            return (
                index,
                words && (starts_upper(next) || is_mention(next) || matches!(next, "¿" | "¡")),
            );
        }
        if !candidate || self.omission(index) || self.leading_dot(index) {
            return (index, false);
        }
        let mut last = index;
        while last + 1 < count
            && self.attached(last + 1)
            && (is_closing(self.piece(last + 1)) || is_final(self.piece(last + 1)))
        {
            last += 1;
        }
        let marks = (index..=last).any(|position| is_final(self.piece(position)));
        // Emoticons after final marks stay in the sentence: «Ура! :)»
        while marks
            && last + 1 < count
            && is_emoticon(self.piece(last + 1), self.tables)
            && !self.line_start(last + 1)
        {
            last += 1;
        }
        // A dash glued to the mark with a space after it: a hyphen numbers an article, «Artículo 1.- Objeto», and a
        // longer dash closes a line of dialogue with its sentence, «La viuda.-- El niño»
        if self.tables.conventions.closing_dashes
            && last + 2 < count
            && self.attached(last + 1)
            && is_dash(self.piece(last + 1))
            && !self.attached(last + 2)
        {
            if self.piece(last + 1) == "-" {
                return (last + 1, false);
            }
            last += 1;
        }
        if last + 1 == count {
            return (last, false);
        }
        // A source in brackets after a quotation stays in its sentence: «…» (Пушкин)
        let source =
            matches!(self.piece(last), "»" | "”" | "\"" | "''") && self.piece(last + 1) == "(";
        let boundary = if marks {
            words && !source && !self.list_number(index) && self.starts(last + 1, true)
        } else {
            is_abbreviation(piece)
                && self.starts(last + 1, false)
                && (self.ends(index) || self.not_initial(index, last + 1))
                && !self.final_abbreviation_goes_on(index)
        };
        (last, boundary)
    }

    /// Whether the tokens from `index` start a sentence: a capital letter, maybe after an opening quote or bracket,
    /// and after final marks also a digit, a mention or a dash before a capital letter
    fn starts(&self, index: usize, marks: bool) -> bool {
        let opens = |piece: &str| {
            piece.starts_with(|c: char| c.is_uppercase() || (marks && c.is_ascii_digit()))
        };
        let piece = self.piece(index);
        if opens(piece) {
            return true;
        }
        if piece.starts_with(is_alpha) {
            return false;
        }
        // An inverted mark opens a sentence whatever comes after it, unless an ellipsis or a mark glued to it goes on
        if marks
            && matches!(piece, "¿" | "¡")
            && !self.attached(index)
            && !matches!(self.piece(index - 1), "..." | "…")
        {
            return true;
        }
        let next = |offset: usize| {
            self.tokens
                .get(index + offset)
                .map(|span| span.slice(self.text))
        };
        if is_opening(piece) {
            return next(1).is_some_and(opens);
        }
        if !marks {
            return false;
        }
        if is_mention(piece) {
            return true;
        }
        if piece.starts_with(['…', '.']) && piece != "." && is_final(piece) {
            // An ellipsis before a capital letter opens a sentence: «Он вышел. … Тополиный пух»
            return next(1).is_some_and(starts_upper);
        }
        if is_stars(piece) || (piece == "*" && next(1) == Some("*")) {
            return true;
        }
        if is_emphasis(piece) {
            // Emphasis in Markdown or a footnote before a capital letter: «**Итог**», «__Жирный__», «* Нумерация»
            return next(1).is_some_and(starts_upper);
        }
        if is_dash(piece) {
            // Hyphens of a dash may stand apart: «- - Según»
            let mut offset = 1;
            while next(offset).is_some_and(is_dash) {
                offset += 1;
            }
            return match next(offset) {
                Some("¿" | "¡") => true,
                Some(after) if is_opening(after) => next(offset + 1).is_some_and(starts_upper),
                Some(after) => {
                    starts_upper(after)
                        || (self.tables.conventions.closing_dashes
                            && after.starts_with(|c: char| c.is_ascii_digit()))
                }
                None => false,
            };
        }
        false
    }

    /// Whether an abbreviation before a capital letter ends a sentence: one of the language that does («и т. д.»),
    /// or a lowercase one after a number, maybe through other lowercase ones («в 1914 г.», «15 тыс. руб.»)
    fn ends(&self, index: usize) -> bool {
        if self.ends_as_written(index) {
            return true;
        }
        let lowercase = |piece: &str| !piece.chars().any(char::is_uppercase);
        // The abbreviations before this one and the word before them, as the list keys them: «т. д.», «и пр.»
        let mut key = String::new();
        for position in (index.saturating_sub(3)..=index).rev() {
            let piece = self.piece(position);
            let abbreviation = is_abbreviation(piece) && lowercase(piece);
            // A token with inner periods keys as its parts: «т.д.» as «т д», «EE.UU.» as «ee uu»
            let word = if abbreviation
                || (is_abbreviation(piece) && piece[..piece.len() - 1].contains('.'))
            {
                piece[..piece.len() - 1]
                    .split('.')
                    .map(|part| self.tables.key(part))
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                self.tables.key(piece)
            };
            key = if key.is_empty() {
                word
            } else {
                format!("{word} {key}")
            };
            if self.tables.final_abbreviations.contains(&key) {
                return true;
            }
            if !abbreviation {
                break;
            }
        }
        // Back to a number over units: «5 млрд руб.», «60 км/ч.»
        if !lowercase(self.piece(index)) {
            return false;
        }
        for position in (index.saturating_sub(4)..index).rev() {
            let piece = self.piece(position);
            if is_number(piece) {
                return true;
            }
            let word = piece.trim_end_matches('.');
            let unit = piece == "/"
                || self.piece(position + 1) == "/"
                || (lowercase(piece)
                    && word.chars().count() <= 4
                    && word.chars().all(is_alpha)
                    && (word.len() < piece.len()
                        || self.tables.abbreviations.contains(&self.tables.key(word))));
            if !unit {
                return false;
            }
        }
        false
    }

    /// Whether the abbreviations up to this one are one that the language writes with capitals and that ends a
    /// sentence: «44 a. C.», «EE. UU.», unlike the initials «Н. Э.» next to «н. э.»
    fn ends_as_written(&self, index: usize) -> bool {
        let written = &self.tables.final_abbreviations_written;
        if written.is_empty() {
            return false;
        }
        let mut key = String::new();
        for position in (index.saturating_sub(3)..=index).rev() {
            let piece = self.piece(position);
            if !is_abbreviation(piece) {
                return false;
            }
            let parts = piece[..piece.len() - 1].replace('.', " ");
            key = if key.is_empty() {
                parts
            } else {
                format!("{parts} {key}")
            };
            if written.contains(&key) {
                return true;
            }
        }
        false
    }

    /// Whether an abbreviation and the one after it make an abbreviation of the language that ends a sentence, so the
    /// first does not end one after a number: «44 a. C.»
    fn final_abbreviation_goes_on(&self, index: usize) -> bool {
        let tables = self.tables;
        if tables.final_abbreviations.is_empty() {
            return false;
        }
        let part = |position: usize| tables.key(self.piece(position).trim_end_matches('.'));
        index + 1 < self.tokens.len()
            && is_abbreviation(self.piece(index + 1))
            && tables
                .final_abbreviations
                .contains(&format!("{} {}", part(index), part(index + 1)))
    }

    /// Whether a single letter with a period before the capital letter at `next` is not an initial: a letter of
    /// another script before a word of the language, as a variable or a name («через x. Тогда», «Витамин C. Его»), or
    /// a capital letter after a quantity, as a unit («5 А. Напряжение»)
    fn not_initial(&self, index: usize, next: usize) -> bool {
        let mut chars = self.piece(index).chars();
        let (Some(letter), Some('.'), None) = (chars.next(), chars.next(), chars.next()) else {
            return false;
        };
        let script = self.tables.conventions.script;
        let capital = self.tokens[next..]
            .iter()
            .take(2)
            .find_map(|span| span.slice(self.text).chars().find(|&c| is_alpha(c)));
        let previous = index.checked_sub(1).map(|position| self.piece(position));
        let initials =
            previous.is_some_and(|piece| is_abbreviation(piece) && piece.chars().count() == 2);
        let opener = || {
            !self.tables.sentence_openers.is_empty()
                && self.tokens[next..]
                    .iter()
                    .take(2)
                    .map(|span| span.slice(self.text))
                    .find(|piece| piece.starts_with(is_alpha))
                    .is_some_and(|piece| {
                        self.tables
                            .sentence_openers
                            .contains(&self.tables.key(piece))
                    })
        };
        (!initials && !script.contains(letter) && capital.is_some_and(|c| script.contains(c)))
            || (!initials && letter.is_uppercase() && opener())
            || (letter.is_uppercase()
                && previous
                    .is_some_and(|piece| is_quantity(piece, self.tables.conventions.digit_groups))
                && !(index >= 2 && self.piece(index - 2) == ":"))
    }

    /// An ellipsis right after an opening quote or bracket marks an omission, not an end: «"…», «[…]»
    fn omission(&self, index: usize) -> bool {
        index > 0
            && self.piece(index).starts_with(['…', '.'])
            && self.piece(index) != "."
            && self.attached(index)
            && is_opening(self.piece(index - 1))
            && (index == 1 || !self.attached(index - 1))
    }

    /// A period with a space before it and a Latin word right after it starts a name: «.NET», «.htaccess»
    fn leading_dot(&self, index: usize) -> bool {
        index > 0
            && self.piece(index) == "."
            && !self.attached(index)
            && self.attached(index + 1)
            && self
                .piece(index + 1)
                .starts_with(|c: char| c.is_ascii_alphabetic())
    }

    /// A number with a period that marks a list item, not the end of a sentence: at the start of a line, or next
    /// to the number of another item in a line: «План. 1. Прийти. 2. Уйти.»
    fn list_number(&self, index: usize) -> bool {
        index > 0
            && self.piece(index) == "."
            && self.attached(index)
            && is_number(self.piece(index - 1))
            && (index == 1 || self.line_start(index - 1) || self.in_series(index - 1))
    }

    /// Whether a number before a capital letter starts a sentence, with the number next to it in the sentence before
    /// or after, or follows a colon as the first item of a list; the other numbers are headings, dates, answers, counts
    fn in_series(&self, number: usize) -> bool {
        let Some(value) = self.item(number) else {
            return false;
        };
        let start = number == self.sentence;
        (start || self.piece(number - 1) == ":")
            && self.starts(number + 2, false)
            && ((start
                && self
                    .previous_item()
                    .is_some_and(|before| before + 1 == value))
                || self.next_item(number + 2) == Some(value + 1))
    }

    /// The number of the list item in the sentence before this one, at its start or after a colon
    fn previous_item(&self) -> Option<usize> {
        let first = self.previous_sentence?;
        self.item(first).or_else(|| {
            (first..self.sentence)
                .filter(|&index| self.piece(index) == ":")
                .find_map(|colon| self.item(colon + 1))
        })
    }

    /// The number of a list item at a token: up to three digits and a period right after them
    fn item(&self, index: usize) -> Option<usize> {
        let piece = self.piece(index);
        (piece.len() <= MAX_ITEM_DIGITS
            && piece.bytes().all(|byte| byte.is_ascii_digit())
            && self.tokens.get(index + 1)?.slice(self.text) == "."
            && self.attached(index + 1))
        .then(|| piece.parse().ok())
        .flatten()
    }

    /// The number of a list item after the first final mark from a token on, past the closing quotes and brackets
    fn next_item(&self, from: usize) -> Option<usize> {
        let mark = (from..self.tokens.len()).find(|&index| is_final(self.piece(index)))?;
        let next = (mark + 1..self.tokens.len()).find(|&index| !is_closing(self.piece(index)))?;
        self.item(next)
    }

    /// Whether a token starts a line of a paragraph of several lines
    fn line_start(&self, index: usize) -> bool {
        self.line_tokens.binary_search(&index).is_ok()
    }

    /// Whether a line break starts a sentence: before a list item, at a line without words, or after a heading
    fn line_boundary(&self, line: usize) -> bool {
        let first = self.line_tokens[line + 1];
        let after = self.piece(first);
        let marker = after.chars().all(|c| BULLETS.contains(c))
            || after.starts_with(is_emoji)
            || after.contains('\u{20e3}')
            || (first + 1 < self.tokens.len()
                && self.attached(first + 1)
                && matches!(self.piece(first + 1), "." | ")")
                && (is_number(after) || after.chars().count() == 1));
        if marker {
            return true;
        }
        let (before, next) = (self.line(line), self.line(line + 1));
        if !has_word(before) || !has_word(next) {
            return true;
        }
        // A heading stands apart in verse too
        let verse = self.verse && (!self.cased || before.contains(char::is_lowercase));
        !verse && !self.typeset && ends_open(before) && begins_upper(next)
    }

    /// Whether a line break without a sentence boundary is a wrap at the width of the page: outside verse, the next
    /// line goes on with a lowercase letter, or some line of the paragraph does
    fn wraps(&self, line: usize) -> bool {
        !self.verse && (self.typeset || self.line(line + 1).starts_with(char::is_lowercase))
    }
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use crate::{LineBreak, Segmenter};

    fn sentences(text: &str) -> Vec<&str> {
        Segmenter::new("ru")
            .unwrap()
            .sentenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    fn tokens(text: &str) -> Vec<&str> {
        Segmenter::new("ru")
            .unwrap()
            .tokenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    #[test]
    fn final_marks() {
        assert_eq!(
            sentences("Ура!!! Ура?! Ура... Слово – слово… и №1"),
            ["Ура!!!", "Ура?!", "Ура...", "Слово – слово… и №1"]
        );
        assert_eq!(
            sentences("Он пришел.Она ушла.Все спят. В 10.30.Начали"),
            ["Он пришел.", "Она ушла.", "Все спят.", "В 10.30.", "Начали"]
        );
        assert_eq!(
            sentences("Это важно (очень важно!). Он вышел. (Дверь хлопнула.) Все ушли."),
            [
                "Это важно (очень важно!).",
                "Он вышел.",
                "(Дверь хлопнула.)",
                "Все ушли."
            ]
        );
        assert_eq!(
            sentences("Кот спит. . . . Пёс лает! и бегает. 5 котов спят."),
            ["Кот спит. . . .", "Пёс лает! и бегает.", "5 котов спят."]
        );
        assert_eq!(
            sentences("Писал: \"… Мир велик\". Он вышел. … Пух летит […] Да."),
            [
                "Писал: \"… Мир велик\".",
                "Он вышел.",
                "… Пух летит […] Да."
            ]
        );
    }

    #[test]
    fn abbreviations_and_initials() {
        assert_eq!(
            sentences("Купили груши и т. д. Потом ушли. Это г. Москва. В 1914 г. Затем"),
            [
                "Купили груши и т. д.",
                "Потом ушли.",
                "Это г. Москва.",
                "В 1914 г.",
                "Затем"
            ]
        );
        assert_eq!(
            sentences("Цена 15 000 р. Пишите. Едем 60 км/ч. Стоим 5 млрд руб. Всё."),
            [
                "Цена 15 000 р.",
                "Пишите.",
                "Едем 60 км/ч.",
                "Стоим 5 млрд руб.",
                "Всё."
            ]
        );
        assert_eq!(
            sentences("Кот, т. е. зверь, спит. А.С.Пушкин жил в Москве.Учился в Лицее."),
            [
                "Кот, т. е. зверь, спит.",
                "А.С.Пушкин жил в Москве.",
                "Учился в Лицее."
            ]
        );
        assert_eq!(
            sentences("Вуз им. Н. Э. Баумана. Умер в 1224 г. до н. э. Затем"),
            ["Вуз им. Н. Э. Баумана.", "Умер в 1224 г. до н. э.", "Затем"]
        );
        assert_eq!(
            tokens("в 1914 г. Затем, т.д.Потом и т. д.!"),
            [
                "в",
                "1914",
                "г",
                ".",
                "Затем",
                ",",
                "т.",
                "д",
                ".",
                "Потом",
                "и",
                "т.",
                "д.",
                "!"
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            sentences("1. Введение. Текст. 2. Обзор"),
            ["1. Введение.", "Текст.", "2.", "Обзор"]
        );
        assert_eq!(
            sentences("Счёт 2:3. 4 гола забил он."),
            ["Счёт 2:3.", "4 гола забил он."]
        );
    }

    #[test]
    fn numbers_of_list_items() {
        assert_eq!(
            sentences("План. 1. Прийти. 2. Уйти. Всё."),
            ["План.", "1. Прийти.", "2. Уйти.", "Всё."]
        );
        assert_eq!(
            sentences("План. 9. «Да!» 10.Нет. 11. Да?"),
            ["План.", "9. «Да!»", "10.Нет.", "11. Да?"]
        );
        assert_eq!(
            sentences("План. 0. Ноль. 1. Один."),
            ["План.", "0. Ноль.", "1. Один."]
        );
        assert_eq!(
            sentences("План. 1. 2. 3. Конец."),
            ["План.", "1.", "2.", "3. Конец."]
        );
        // Not a series: an answer, a heading far from the next one, years, roman numerals, numbers out of order
        assert_eq!(
            sentences("Сколько? 5. Потом ушли."),
            ["Сколько?", "5.", "Потом ушли."]
        );
        assert_eq!(
            sentences("Война. 1941. Бои. 1942. Мир."),
            ["Война.", "1941.", "Бои.", "1942.", "Мир."]
        );
        assert_eq!(
            sentences("Текст. I. Начало. II. Конец."),
            ["Текст.", "I.", "Начало.", "II.", "Конец."]
        );
        assert_eq!(
            sentences("Текст. 3. Да. 1. Нет."),
            ["Текст.", "3.", "Да.", "1.", "Нет."]
        );
        assert_eq!(
            sentences("Нужно: 1. Прийти. 2. Уйти. 3. Спать."),
            ["Нужно: 1. Прийти.", "2. Уйти.", "3. Спать."]
        );
        assert_eq!(
            sentences("Результаты: 1. Иванов. 2. Петров."),
            ["Результаты: 1. Иванов.", "2. Петров."]
        );
        assert_eq!(
            sentences("Счёт: 1. Потом ушли."),
            ["Счёт: 1.", "Потом ушли."]
        );
        // Counts after labels are not a list
        assert_eq!(
            sentences("Комнат: 2. Этаж: 3. Цена договорная."),
            ["Комнат: 2.", "Этаж: 3.", "Цена договорная."]
        );
        assert_eq!(
            sentences("Ванная: 1. Спальни: 2. Гараж: 3. Сад небольшой."),
            ["Ванная: 1.", "Спальни: 2.", "Гараж: 3.", "Сад небольшой."]
        );
        assert_eq!(
            sentences("Дети: 1. Внуки: 2. Живёт в Москве."),
            ["Дети: 1.", "Внуки: 2.", "Живёт в Москве."]
        );
    }

    #[test]
    fn dialogue_and_quotes() {
        assert_eq!(
            sentences("— Ты придёшь? — спросила она. — Приду. — Придумала! — Она встала."),
            [
                "— Ты придёшь? — спросила она.",
                "— Приду.",
                "— Придумала!",
                "— Она встала."
            ]
        );
        assert_eq!(
            sentences("Она спросила: «Ты придёшь?» Я кивнул. «Стой! — крикнул он. — Не двигайся!»"),
            [
                "Она спросила: «Ты придёшь?»",
                "Я кивнул.",
                "«Стой! — крикнул он.",
                "— Не двигайся!»"
            ]
        );
        assert_eq!(
            sentences("«Не спится!» (Пушкин). Ушёл… Больше не видели. Тишина… и шум."),
            [
                "«Не спится!» (Пушкин).",
                "Ушёл…",
                "Больше не видели.",
                "Тишина… и шум."
            ]
        );
    }

    #[test]
    fn social() {
        assert_eq!(
            sentences(
                "Привет 😀 Как дела? Ура! :) Пойдём. Спасибо! @anna да. Платье)) @anna_k оцени!"
            ),
            [
                "Привет 😀",
                "Как дела?",
                "Ура! :)",
                "Пойдём.",
                "Спасибо!",
                "@anna да.",
                "Платье))",
                "@anna_k оцени!"
            ]
        );
        assert_eq!(
            sentences("Текст. * * * Новая глава. *** Конец."),
            ["Текст.", "* * *", "Новая глава.", "***", "Конец."]
        );
    }

    #[test]
    fn line_breaks() {
        assert_eq!(
            sentences("Глава 1\nВведение\nЭто текст. Он короткий."),
            ["Глава 1", "Введение", "Это текст.", "Он короткий."]
        );
        assert_eq!(
            sentences("Нужно:\n- паспорт\n• СНИЛС\n1) полис\n2. фото\nа) справка"),
            [
                "Нужно:",
                "- паспорт",
                "• СНИЛС",
                "1) полис",
                "2. фото",
                "а) справка"
            ]
        );
        assert_eq!(
            sentences("Кот спал.\r\nПёс ел.\n***\nКот ушел.\n-"),
            ["Кот спал.", "Пёс ел.", "***", "Кот ушел.", "-"]
        );
        assert_eq!(
            sentences(
                "Скажи-ка, дядя, ведь не даром\nМосква, спаленная пожаром,\nФранцузу отдана?\nВедь были"
            ),
            [
                "Скажи-ка, дядя, ведь не даром\nМосква, спаленная пожаром,\nФранцузу отдана?",
                "Ведь были"
            ]
        );
        assert_eq!(
            sentences("Абзац заканчивается\nздесь. Второй\nНачинается тут."),
            ["Абзац заканчивается\nздесь.", "Второй\nНачинается тут."]
        );
    }

    #[test]
    fn verse_without_commas_at_line_ends() {
        let stanza = "Над тихой речкой встал туман\nИ скрыл прибрежные кусты\nОтец уехал в дальний стан\nИ сжёг за собою мосты.";
        assert_eq!(sentences(stanza), [stanza]);
        assert_eq!(
            sentences(
                "Но живу я в далеком скиту\nИ не знаю для счастья границ.\nТишиной провожаю мечту.\nИ мечта воздвигает Царицу."
            ),
            [
                "Но живу я в далеком скиту\nИ не знаю для счастья границ.",
                "Тишиной провожаю мечту.",
                "И мечта воздвигает Царицу."
            ]
        );
        assert_eq!(
            sentences("ГЛАВА XIV\nСУД\nМирская молва —\nМорская волна."),
            ["ГЛАВА XIV", "СУД", "Мирская молва —\nМорская волна."]
        );
        assert_eq!(
            sentences("III\nИ ты стоял,\nВесь в белом"),
            ["III", "И ты стоял,\nВесь в белом"]
        );
        // In a paragraph without lowercase letters, no line stands for a heading
        let capitals = "Я ПОМНЮ ЧУДНОЕ МГНОВЕНЬЕ,\nПЕРЕДО МНОЙ ЯВИЛАСЬ ТЫ\nКАК МИМОЛЁТНОЕ ВИДЕНЬЕ,\nКАК ГЕНИЙ ЧИСТОЙ КРАСОТЫ.";
        assert_eq!(sentences(capitals), [capitals]);
        assert_eq!(
            sentences(
                "THE SOFTWARE IS PROVIDED, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED. IN NO EVENT SHALL THE\nAUTHORS BE LIABLE, WHETHER IN AN ACTION OF CONTRACT,\nTORT OR OTHERWISE."
            ),
            [
                "THE SOFTWARE IS PROVIDED, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED.",
                "IN NO EVENT SHALL THE\nAUTHORS BE LIABLE, WHETHER IN AN ACTION OF CONTRACT,\nTORT OR OTHERWISE."
            ]
        );
    }

    #[test]
    fn lines_that_only_look_like_verse() {
        for lines in [
            ["Иван Иванов", "Петр Петров", "Олег Сидоров", "Егор Козлов"],
            [
                "Иванов 5 руб",
                "Петров 3 руб",
                "Сидоров 2 руб",
                "Козлов 1 руб",
            ],
            ["Мама", "Папа", "Бабушка", "Дедушка."],
            ["Купить молоко", "Купить хлеб", "Купить сыр", "Купить масло"],
            [
                "Спасибо за покупку!",
                "Заказ передан в доставку",
                "Ожидайте звонка курьера.",
                "С уважением, магазин",
            ],
            [
                "Вчера ходили гулять",
                "Сегодня будем читать",
                "Завтра пойдем в кино",
                "Потом ляжем спать",
            ],
            // One grammatical form, with one ending
            [
                "Вчера ходили гулять",
                "Сегодня будем читать",
                "Завтра будем играть",
                "Потом ляжем спать",
            ],
            [
                "Машину помыть",
                "Хлеба купить",
                "Кота покормить",
                "Детей забрать",
            ],
            [
                "Order received",
                "Payment accepted",
                "Parcel shipped",
                "Invoice printed",
            ],
            [
                "Dos tomates",
                "Tres cebollas",
                "Cuatro manzanas",
                "Cinco naranjas",
            ],
            // A pair side by side, or three lines of one form, in lines of one length
            [
                "Оплата картой",
                "Доставка почтой",
                "Склад закрыт",
                "Звоните нам.",
            ],
            [
                "Цена договорная",
                "Мебель новая",
                "Доставка платная",
                "Звоните сейчас.",
            ],
        ] {
            assert_eq!(sentences(&lines.join("\n")), lines);
        }
    }

    #[test]
    fn dashes_without_verse() {
        for lines in [
            [
                "Итоги квартала",
                "-----",
                "Продажи выросли",
                "Расходы упали",
                "Прибыль на месте",
            ],
            [
                "С уважением",
                "Иван Петров",
                "--",
                "Отдел продаж",
                "Телефон на сайте",
            ],
        ] {
            assert_eq!(sentences(&lines.join("\n")), lines);
        }
        assert_eq!(
            sentences("Январь 5\nФевраль -\nМарт 7\nАпрель 6\nМай 8"),
            ["Январь 5", "Февраль -\nМарт 7", "Апрель 6", "Май 8"]
        );
        assert_eq!(
            sentences("Имя -\nИван\nВозраст -\nТридцать лет\nГород -\nМосква"),
            ["Имя -\nИван", "Возраст -\nТридцать лет", "Город -\nМосква"]
        );
        assert_eq!(
            sentences(
                "Сегодня хорошая погода\nИдём гулять в парк -\nБерите детей\nВстречаемся у входа"
            ),
            [
                "Сегодня хорошая погода",
                "Идём гулять в парк -\nБерите детей",
                "Встречаемся у входа"
            ]
        );
    }

    #[test]
    fn line_break_kinds() {
        let kinds = |text: &str| -> Vec<LineBreak> {
            Segmenter::new("ru")
                .unwrap()
                .segment(text)
                .lines()
                .iter()
                .filter_map(|line| line.break_before)
                .collect()
        };
        assert_eq!(
            kinds("Тени становились длин-\nнее. Мы шли\nпо дороге."),
            [LineBreak::Typeset, LineBreak::Typeset]
        );
        assert_eq!(
            kinds("Провели собрание в\nМоскве. Решение\nприняли все и\n«Газпром» тоже."),
            [LineBreak::Typeset; 3]
        );
        assert_eq!(
            kinds("Мы шли\nпо дороге -\nМосква далеко."),
            [LineBreak::Typeset, LineBreak::Typeset]
        );
        assert_eq!(
            kinds("Над тихой речкой встал туман\nИ скрыл кусты\nОтец уехал в стан\nИ сжёг мосты."),
            [LineBreak::Author; 3]
        );
        assert_eq!(
            kinds(
                "Прошёл фестиваль.\nОрганизаторы довольны.\n- Да\nМосква, спаленная пожаром,\nФранцузу"
            ),
            [
                LineBreak::Author,
                LineBreak::Author,
                LineBreak::Author,
                LineBreak::Author
            ]
        );
    }

    #[test]
    fn found_by_the_review() {
        assert_eq!(
            sentences(
                "Родился 5 мая 1950 года в г. Ташкенте. Через 5 лет у акад. Сахарова отняли награды."
            ),
            [
                "Родился 5 мая 1950 года в г. Ташкенте.",
                "Через 5 лет у акад. Сахарова отняли награды."
            ]
        );
        assert_eq!(
            sentences(
                "Объём 300 куб. м. Затем всё. Офис на пр. Мира. Купили соль и пр. Потом ушли."
            ),
            [
                "Объём 300 куб. м.",
                "Затем всё.",
                "Офис на пр. Мира.",
                "Купили соль и пр.",
                "Потом ушли."
            ]
        );
        assert_eq!(
            sentences("Справки по тел. 8 (495) 123-45-67. Нет у них тел. Есть души."),
            [
                "Справки по тел. 8 (495) 123-45-67.",
                "Нет у них тел.",
                "Есть души."
            ]
        );
        assert_eq!(
            tokens("Купили и т. д\u{301}. Потом и т. д\u{301}."),
            [
                "Купили",
                "и",
                "т.",
                "д\u{301}",
                ".",
                "Потом",
                "и",
                "т.",
                "д\u{301}",
                "."
            ]
        );
        assert_eq!(
            sentences("Наши услуги:\n✅ Ремонт квартир\n✅ Покраска стен\n\nСкидки!\n🔥 Успейте"),
            [
                "Наши услуги:",
                "✅ Ремонт квартир",
                "✅ Покраска стен",
                "Скидки!",
                "🔥 Успейте"
            ]
        );
        assert_eq!(
            sentences("Программа\n1\u{fe0f}\u{20e3} Регистрация\n2\u{fe0f}\u{20e3} Доклады"),
            [
                "Программа",
                "1\u{fe0f}\u{20e3} Регистрация",
                "2\u{fe0f}\u{20e3} Доклады"
            ]
        );
    }

    #[test]
    fn letters_and_units() {
        assert_eq!(
            sentences("Через x. Тогда x = 5. Витамин C. Его много. Петра I. У него"),
            [
                "Через x.",
                "Тогда x = 5.",
                "Витамин C.",
                "Его много.",
                "Петра I.",
                "У него"
            ]
        );
        assert_eq!(
            sentences("Ток 5 А. Напряжение 220 В. Мощность. В 29-летнем А. С. Пушкине"),
            [
                "Ток 5 А.",
                "Напряжение 220 В.",
                "Мощность.",
                "В 29-летнем А. С. Пушкине"
            ]
        );
        assert_eq!(tokens("Ток 5 А. Он"), ["Ток", "5", "А", ".", "Он"]);
        // A letter of the language before a Latin word and an initial after a time, a date, a year or a score
        assert_eq!(
            sentences("Архив, т. III, с. 257. См. ч. II и г. London. Ток 1,5 А. Он"),
            [
                "Архив, т. III, с. 257.",
                "См. ч. II и г. London.",
                "Ток 1,5 А.",
                "Он"
            ]
        );
        assert_eq!(
            sentences("В 12:00 В. Путин, 12.05.2009 В. Путин, в 2020 В. Путин, 6:3 А. Зверева."),
            ["В 12:00 В. Путин, 12.05.2009 В. Путин, в 2020 В. Путин, 6:3 А. Зверева."]
        );
    }

    #[test]
    fn posts_and_markdown() {
        assert_eq!(
            sentences("Спасибо) Завтра увидимся) Пока. Он (кот) Спит. Пункты: а) Один"),
            [
                "Спасибо)",
                "Завтра увидимся)",
                "Пока.",
                "Он (кот) Спит.",
                "Пункты: а) Один"
            ]
        );
        assert_eq!(
            sentences("Это важно. **Итог** — всё. *Курсив* тоже. __Жирный__ да. ~~Нет~~ да."),
            [
                "Это важно.",
                "**Итог** — всё.",
                "*Курсив* тоже.",
                "__Жирный__ да.",
                "~~Нет~~ да."
            ]
        );
        assert_eq!(
            sentences("**Примечание:** Это важно. **Итог:** Всё. Текст. * Сноска"),
            [
                "**Примечание:** Это важно.",
                "**Итог:** Всё.",
                "Текст.",
                "* Сноска"
            ]
        );
        assert_eq!(
            sentences("План:\nI) Введение\nII) Основная часть. Пункты: I) Первый; II) Второй"),
            [
                "План:",
                "I) Введение",
                "II) Основная часть.",
                "Пункты: I) Первый; II) Второй"
            ]
        );
        assert_eq!(sentences("Люблю тебя :** Пока"), ["Люблю тебя :**", "Пока"]);
        assert_eq!(
            sentences("Привет)\nКак дела?\nAB) Оба"),
            ["Привет)", "Как дела?", "AB) Оба"]
        );
        assert_eq!(
            sentences("Также .NET и Node.js. Он ушёл .Она пришла"),
            ["Также .NET и Node.js.", "Он ушёл .", "Она пришла"]
        );
    }

    #[test]
    fn found_by_the_qa() {
        assert_eq!(
            sentences("Согласно абз. 2 п. 1 и подп. 3 прил. 1 к закону. Это 28 окт. 2026 г. Всё."),
            [
                "Согласно абз. 2 п. 1 и подп. 3 прил. 1 к закону.",
                "Это 28 окт. 2026 г.",
                "Всё."
            ]
        );
        let custom = Segmenter::new("ru").unwrap().with_tokens(["т.д."]).unwrap();
        let text = "Купили хлеб и т.д. Потом ушли.";
        let spans: Vec<&str> = custom
            .sentenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect();
        assert_eq!(spans, ["Купили хлеб и т.д.", "Потом ушли."]);
        let custom = Segmenter::new("ru").unwrap().with_tokens(["Т.Д."]).unwrap();
        let text = "Купили хлеб и Т.Д. Потом ушли.";
        assert_eq!(custom.sentenize(text).len(), 2);
        assert_eq!(
            sentences("[12:01] Вася: привет\n[12:02] Петя: здарова"),
            ["[12:01] Вася: привет", "[12:02] Петя: здарова"]
        );
        assert_eq!(
            sentences("12:01 Вася: привет\n12:02 Петя: здарова"),
            ["12:01 Вася: привет", "12:02 Петя: здарова"]
        );
        assert_eq!(
            sentences("Федеральный закон от\n27.07.2006 № 152-ФЗ вступил в силу."),
            ["Федеральный закон от\n27.07.2006 № 152-ФЗ вступил в силу."]
        );
    }

    #[test]
    fn long_runs() {
        // Linear scans: a quadratic one takes seconds on each of these
        for text in [
            "Кот спит. ".repeat(20_000),
            "1. ".repeat(20_000),
            "a\n".repeat(20_000),
            "- пункт\n".repeat(20_000),
            "* ".repeat(40_000),
            "т. ".repeat(40_000),
            "😀 ".repeat(40_000),
            "@a ".repeat(40_000),
            "в 1914 г. ".repeat(20_000),
            "1. А. 2. Б. ".repeat(20_000),
            format!("1. А {}", "б ".repeat(40_000)),
            "кот ест\nпёс пьёт\n".repeat(20_000),
            format!("Текст: {}", "1. А: 2. Б. ".repeat(20_000)),
            format!(
                "а {}: 1. Дом. 2. Сад{}",
                "б : ".repeat(20_000),
                ": 2. Сад".repeat(20_000)
            ),
        ] {
            let spans = Segmenter::new("ru").unwrap().sentenize(&text);
            assert!(!spans.is_empty());
        }
    }
}

#[cfg(all(test, feature = "es"))]
mod spanish {
    use crate::Segmenter;

    fn sentences(text: &str) -> Vec<&str> {
        Segmenter::new("es")
            .unwrap()
            .sentenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    fn whole(text: &str) {
        assert_eq!(sentences(text), [text]);
    }

    #[test]
    fn inverted_marks() {
        assert_eq!(
            sentences("Hola. ¿Qué tal? ¡Bien! ¿o no? ¡ahora!"),
            ["Hola.", "¿Qué tal?", "¡Bien!", "¿o no?", "¡ahora!"]
        );
        whole("Si vienes, ¿me avisas?");
        whole("¡Ay! me duele.");
        whole("Bueno... ¿qué dices?");
        whole("Bueno...¿qué dices?");
        assert_eq!(sentences("Fin.¿Qué?"), ["Fin.", "¿Qué?"]);
        assert_eq!(sentences("Ganamos 🎉 ¡Vamos!"), ["Ganamos 🎉", "¡Vamos!"]);
        assert_eq!(sentences("Jajaja :D ¿Y tú?"), ["Jajaja :D", "¿Y tú?"]);
    }

    #[test]
    fn dialogue() {
        assert_eq!(
            sentences("—Hola —dijo—. ¿Qué tal? —Bien."),
            ["—Hola —dijo—.", "¿Qué tal?", "—Bien."]
        );
        whole("—¿Vienes? —preguntó María.");
        assert_eq!(
            sentences("¿Por qué? - - Según Piqué, no. - - ¿Cómo?"),
            ["¿Por qué?", "- - Según Piqué, no.", "- - ¿Cómo?"]
        );
        assert_eq!(sentences("¿Cuánto? —5 euros."), ["¿Cuánto?", "—5 euros."]);
        assert_eq!(sentences("Era tarde. —¿qué?"), ["Era tarde.", "—¿qué?"]);
    }

    #[test]
    fn dashes_glued_to_marks() {
        assert_eq!(
            sentences("Está muy baja.--Tiene fiebre."),
            ["Está muy baja.", "--Tiene fiebre."]
        );
        whole("Dijo que sí.--dijo él.");
        assert_eq!(
            sentences("La viuda.-- El niño llegó."),
            ["La viuda.--", "El niño llegó."]
        );
        assert_eq!(
            sentences("Para asombro de Mahoma.--\nOtra lengua clama."),
            ["Para asombro de Mahoma.--", "Otra lengua clama."]
        );
        whole("¡Está lejos!--\n  al ángel de la muerte.");
        whole("El Sr.--\nGarcía vino.");
        whole("IV.--\nDe los fines.");
        whole("Artículo 1.- Objeto del reglamento.");
        whole("PRIMERO.- Aprobar el plan.");
        whole("EE.UU.-China firmaron.");
    }

    #[test]
    fn abbreviations() {
        assert_eq!(
            sentences("El Sr. García y la Dra. López llegaron. Vino Dña. María."),
            [
                "El Sr. García y la Dra. López llegaron.",
                "Vino Dña. María."
            ]
        );
        assert_eq!(
            sentences("Pesas, etc. Además, no. Es el mar. Luego."),
            ["Pesas, etc.", "Además, no.", "Es el mar.", "Luego."]
        );
        assert_eq!(
            sentences("Llegó a las 3 p. m. Comimos. Murió en 44 a. C. Nació en 10 d. C. Luego."),
            [
                "Llegó a las 3 p. m.",
                "Comimos.",
                "Murió en 44 a. C.",
                "Nació en 10 d. C.",
                "Luego."
            ]
        );
        whole("Nació en el 44 a. C. en Roma.");
        assert_eq!(
            sentences("Trabaja en EE.UU. Su hermano no."),
            ["Trabaja en EE.UU.", "Su hermano no."]
        );
        assert_eq!(
            sentences("Mide 5 cm. Pesa 3 kg."),
            ["Mide 5 cm.", "Pesa 3 kg."]
        );
        whole("Véase p. 45 y pág. 7.");
        whole("Frutas, p. ej. Manzanas.");
    }

    #[test]
    fn initials_and_words_after_them() {
        whole("Vino J. L. Borges.");
        whole("Lo dijo M. De la Cruz.");
        assert_eq!(
            sentences("El rey Juan Carlos I. Su hijo reina."),
            ["El rey Juan Carlos I.", "Su hijo reina."]
        );
        assert_eq!(
            sentences("Tomo vitamina C. Es buena."),
            ["Tomo vitamina C.", "Es buena."]
        );
        // Initials stay initials before such a word, as «J. L. Es» has one more letter before it
        whole("Vino J. L. Es.");
    }
}

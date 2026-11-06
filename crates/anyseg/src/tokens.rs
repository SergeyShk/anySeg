//! Tokens: a scan of the text by classes of characters, with the lists of a language

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use unicode_normalization::char::{compose, is_combining_mark};

use crate::Span;
use crate::lang::{DigitGroups, Hyphens, Tables};
use crate::layout::is_separator;

fn char_at(text: &str, index: usize) -> Option<char> {
    text.get(index..)?.chars().next()
}

/// A letter: ASCII, Latin-1 and the Russian alphabet skip the tables of Unicode, which take most of the time
pub(crate) fn is_alpha(character: char) -> bool {
    match character {
        'a'..='z' | 'A'..='Z' | 'А'..='я' | 'Ё' | 'ё' => true,
        '\u{c0}'..='\u{ff}' => !matches!(character, '\u{d7}' | '\u{f7}'),
        _ if character.is_ascii() => false,
        _ => character.is_alphabetic(),
    }
}

/// A letter or a digit, as [`char::is_alphanumeric`] decides, with the shortcuts of [`is_alpha`]
pub(crate) fn is_word(character: char) -> bool {
    match character {
        'a'..='z' | 'A'..='Z' | '0'..='9' | 'А'..='я' | 'Ё' | 'ё' => true,
        '\u{c0}'..='\u{ff}' => !matches!(character, '\u{d7}' | '\u{f7}'),
        _ if character.is_ascii() => false,
        _ => character.is_alphanumeric(),
    }
}

pub(crate) fn is_mark(character: char) -> bool {
    is_combining_mark(character)
}

/// Characters that join the letters of a word without being letters: soft hyphen and zero width (non-)joiner
fn is_joiner(character: char) -> bool {
    matches!(character, '\u{ad}' | '\u{200c}' | '\u{200d}')
}

pub(crate) fn is_emoji(character: char) -> bool {
    matches!(
        character as u32,
        0x1F000..=0x1FAFF | 0x203C | 0x2049 | 0x2600..=0x27BF | 0x2B05..=0x2B07 | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55
            | 0x231A..=0x231B | 0x23E9..=0x23F3 | 0x23F8..=0x23FA
    )
}

/// Characters inside an emoji: variation selectors, zero width joiner, tags and the keycap
fn is_emoji_part(character: char) -> bool {
    matches!(
        character as u32,
        0xFE0E..=0xFE0F | 0x200D | 0xE0020..=0xE007F | 0x20E3
    )
}

fn is_roman(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| "IVXLCDM".contains(character))
}

/// A word with a first capital letter and lowercase letters after it: the start of a sentence after a missing space,
/// and no top-level domain: «casa.De pronto»
fn capitalized(text: &str) -> bool {
    text.starts_with(char::is_uppercase) && text.contains(char::is_lowercase)
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn is_letters(text: &str) -> bool {
    text.chars().any(is_alpha)
        && text
            .chars()
            .all(|character| is_alpha(character) || is_mark(character) || is_joiner(character))
}

/// How two runs of a word or a number are joined
#[derive(Clone, Copy, PartialEq, Eq)]
enum Joint {
    Hyphen,
    Apostrophe,
    /// A point between digits: «1.5», «12.10.2026»
    Point,
    /// A comma between digits: «4,5»
    Comma,
    /// A colon between digits: «12:30»
    Colon,
    Underscore,
    /// A space between groups of digits: «3 000»
    Thousands,
    /// A mark that the language keeps inside a token: «km/h»
    Inner,
}

impl Joint {
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The lists of the user: more abbreviations and strings that are one token each
#[derive(Clone, Debug, Default)]
pub(crate) struct Exceptions {
    /// Keys of words that keep a period after them
    pub(crate) abbreviations: BTreeSet<String>,
    /// Strings, as written, that are one token
    pub(crate) tokens: BTreeSet<String>,
    /// The same strings for a lookup by a slice of the text
    lookup: HashSet<String>,
    /// The lengths of the strings that start with a character, longest first
    lengths: HashMap<char, Vec<usize>>,
}

impl Exceptions {
    pub(crate) fn add_token(&mut self, token: &str) {
        let Some(first) = token.chars().next() else {
            return;
        };
        if self.tokens.insert(token.to_owned()) {
            self.lookup.insert(token.to_owned());
            let lengths = self.lengths.entry(first).or_default();
            if let Err(position) = lengths.binary_search_by(|length| token.len().cmp(length)) {
                lengths.insert(position, token.len());
            }
        }
    }
}

impl PartialEq for Exceptions {
    fn eq(&self, other: &Self) -> bool {
        self.abbreviations == other.abbreviations && self.tokens == other.tokens
    }
}

impl Eq for Exceptions {}

impl Hash for Exceptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.abbreviations.hash(state);
        self.tokens.hash(state);
    }
}

struct Scanner<'a> {
    text: &'a str,
    end: usize,
    tables: &'a Tables,
    exceptions: &'a Exceptions,
    out: &'a mut Vec<Span>,
    /// Round brackets opened and not closed yet in the paragraph
    open: usize,
    /// The labels last read for an e-mail address or a domain: their end and their last period
    chain: (usize, Option<usize>),
    /// The end of the labels where no token can start an e-mail address or a domain
    not_web: usize,
    /// The end of the labels last read for a domain
    domain_chain: usize,
}

/// The tokens of a paragraph, appended to `out`
pub(crate) fn tokenize(
    text: &str,
    span: Span,
    tables: &Tables,
    exceptions: &Exceptions,
    out: &mut Vec<Span>,
) {
    let mut scanner = Scanner {
        text,
        end: span.end,
        tables,
        exceptions,
        out,
        open: 0,
        chain: (0, None),
        not_web: 0,
        domain_chain: 0,
    };
    let mut index = span.start;
    while let Some(character) = scanner.get(index) {
        if is_separator(character) {
            index += character.len_utf8();
        } else {
            let end = scanner.token(index, character);
            index = scanner.marks_after(end);
        }
    }
}

impl Scanner<'_> {
    fn get(&self, index: usize) -> Option<char> {
        (index < self.end)
            .then(|| char_at(self.text, index))
            .flatten()
    }

    fn push(&mut self, start: usize, end: usize) -> usize {
        self.out.push(Span { start, end });
        end
    }

    fn starts_with(&self, index: usize, prefix: &str) -> bool {
        self.text[index..self.end]
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    }

    /// Emits the token at `index` (or several) and returns the position after them
    fn token(&mut self, index: usize, character: char) -> usize {
        if !self.exceptions.tokens.is_empty()
            && let Some(end) = self.exception(index, character)
        {
            return self.push(index, end);
        }
        if matches!(character, 'h' | 'H' | 'f' | 'F' | 'w' | 'W')
            && ["http://", "https://", "ftp://", "www."]
                .iter()
                .any(|prefix| self.starts_with(index, prefix))
        {
            let end = self.url(index);
            return self.push(index, end);
        }
        if is_word(character) || (is_mark(character) && self.marks_start_word(index)) {
            // Only a word followed by «@» or by a dot or a dash before a letter or a digit can start an e-mail
            // address or a domain
            let run = self.superscript_end(index, self.run(index));
            let web = match self.get(run) {
                Some('@') => true,
                Some(next @ ('.' | '-' | '_' | '+' | '%')) => {
                    self.get(run + next.len_utf8()).is_some_and(is_word)
                }
                _ => false,
            };
            if !web || index < self.not_web {
                return self.compound(index, run);
            }
            // Every start inside the same labels reads them to the same end: read them once
            if index >= self.chain.0 {
                let end = self.labels(index, ".-_+%");
                let period = self.text[index..end]
                    .rfind('.')
                    .map(|offset| index + offset);
                self.chain = (end, period);
            }
            let (labels, period) = self.chain;
            let end = match self.get(labels) {
                Some('@') => self.email(labels).ok_or(labels),
                _ if period.is_some_and(|period| period > index) => self.domain(index),
                _ => Err(labels),
            };
            return match end {
                Ok(end) => self.push(index, end),
                // The later starts in the labels that failed see a part of them and fail as well
                Err(failed) => {
                    self.not_web = failed;
                    self.compound(index, run)
                }
            };
        }
        if let Some(end) = self.emoticon(index) {
            return self.push(index, end);
        }
        let next = index + character.len_utf8();
        match character {
            '-' | '\u{2212}'
                if self.tables.conventions.signed_numbers
                    && self.get(next).is_some_and(|c| c.is_ascii_digit())
                    && self.text[..index].chars().next_back().is_none_or(|c| {
                        c.is_whitespace() || matches!(c, '(' | '[' | '«' | '"')
                    }) =>
            {
                let first = self.out.len();
                let run = self.run(next);
                let end = self.compound(next, run);
                self.out[first].start = index;
                end
            }
            '@' | '#' if self.get(next).is_some_and(|c| is_word(c) || c == '_') => {
                let end = self.handle(next, character == '@');
                self.push(index, end)
            }
            '+' if self.get(next).is_some_and(|c| c.is_ascii_digit()) => {
                match self.digit_chain(next) {
                    Some(end) => self.push(index, end),
                    None => self.push(index, next),
                }
            }
            '&' if self.entity(index).is_some() => {
                let mut end = index;
                while let Some(next) = self.entity(end) {
                    end = next;
                }
                self.push(index, end)
            }
            _ if is_emoji(character) => {
                let end = self.emoji(index);
                self.push(index, end)
            }
            _ => self.punctuation(index, character),
        }
    }

    /// Extends the token that ends at `end` over the variation selectors, the keycap and the combining marks
    /// after it («▶️», «#️⃣», «°Ć»), unless the marks do not compose with it and a word follows them: «[͡ие]»
    fn marks_after(&mut self, end: usize) -> usize {
        let mut stop = end;
        // Most tokens end before a space or ASCII punctuation: a byte tells it without the tables
        while self
            .text
            .as_bytes()
            .get(stop)
            .is_some_and(|byte| !byte.is_ascii())
            && let Some(c) = self.get(stop)
        {
            if c != '\u{200d}' && is_emoji_part(c) {
                stop += c.len_utf8();
            } else if is_mark(c) {
                let composes = self.text[..stop]
                    .chars()
                    .next_back()
                    .is_some_and(|base| compose(base, c).is_some());
                if !composes && self.marks_start_word(stop) {
                    break;
                }
                stop = self.marks_end(stop);
            } else {
                break;
            }
        }
        if stop > end
            && let Some(last) = self.out.last_mut()
        {
            last.end = stop;
        }
        stop
    }

    fn marks_end(&self, start: usize) -> usize {
        let mut end = start;
        while let Some(c) = self.get(end).filter(|&c| is_mark(c)) {
            end += c.len_utf8();
        }
        end
    }

    /// Whether the combining marks at `start` come before a letter or a digit and so begin a word
    fn marks_start_word(&self, start: usize) -> bool {
        self.get(self.marks_end(start)).is_some_and(is_word)
    }

    /// The end of the longest token of the user at `index` that no letter or digit adjoins
    fn exception(&self, index: usize, character: char) -> Option<usize> {
        let word = |c: char| is_word(c) || is_mark(c);
        if self.text[..index].chars().next_back().is_some_and(word) {
            return None;
        }
        let rest = &self.text[index..self.end];
        self.exceptions
            .lengths
            .get(&character)?
            .iter()
            .find(|&&length| {
                rest.get(..length)
                    .is_some_and(|piece| self.exceptions.lookup.contains(piece))
                    && !self.get(index + length).is_some_and(word)
            })
            .map(|length| index + length)
    }

    /// The end of a URL: up to whitespace or a quote, without the punctuation that ends a sentence and a
    /// closing bracket it did not open
    fn url(&self, start: usize) -> usize {
        let mut end = start;
        // Brackets in the URL: «(», «)», «[», «]»
        let mut brackets = [0usize; 4];
        while let Some(character) = self.get(end) {
            if is_separator(character) || "\"«»<>".contains(character) {
                break;
            }
            if let Some(kind) = "()[]".find(character) {
                brackets[kind] += 1;
            }
            end += character.len_utf8();
        }
        while let Some(last) = self.text[start..end].chars().next_back() {
            let unbalanced = match last {
                ')' => brackets[1] > brackets[0],
                ']' => brackets[3] > brackets[2],
                _ => false,
            };
            if !(unbalanced || ".,;:!?'’".contains(last)) {
                break;
            }
            if unbalanced {
                brackets[if last == ')' { 1 } else { 3 }] -= 1;
            }
            end -= last.len_utf8();
        }
        end
    }

    /// A run of letters, digits and dots, dashes and underscores between them, as in domains and e-mail
    fn labels(&self, start: usize, extra: &str) -> usize {
        let mut end = start;
        while let Some(character) = self.get(end) {
            let inner = extra.contains(character)
                && self.get(end + character.len_utf8()).is_some_and(is_word);
            if is_word(character) || is_mark(character) || inner {
                end += character.len_utf8();
            } else {
                break;
            }
        }
        end
    }

    /// The end of a host name in the labels from `start` to `end`: after the last label that `top` accepts,
    /// so that a sentence glued to it after a period («example.ru.Регистрация») is left out
    fn host(&self, start: usize, end: usize, top: impl Fn(&str) -> bool) -> Option<usize> {
        let mut labels = self.text[start..end]
            .split('.')
            .scan(start, |position, label| {
                let at = *position;
                *position += label.len() + 1;
                Some((at, label))
            })
            .skip(1)
            .peekable();
        while let Some((at, label)) = labels.next() {
            let ends = labels
                .peek()
                .is_none_or(|(_, next)| next.starts_with(char::is_uppercase));
            if ends && top(label) {
                return Some(at + label.len());
            }
        }
        None
    }

    /// The end of an e-mail address whose «@» is at `at`
    fn email(&self, at: usize) -> Option<usize> {
        let end = self.labels(at + 1, ".-");
        self.host(at + 1, end, |top| {
            top.chars().count() >= 2 && top.chars().all(is_alpha)
        })
    }

    /// A bare domain («example.ru», «Газета.ru») with its port and path, or a file name («robots.txt»);
    /// otherwise the end of the labels read
    fn domain(&mut self, start: usize) -> Result<usize, usize> {
        // Every start inside the same labels reads them to the same end: read them once
        if start >= self.domain_chain {
            self.domain_chain = self.labels(start, ".-_");
        }
        let labels = self.domain_chain;
        let mut end = self
            .host(start, labels, |top| {
                self.tables.web_suffixes.contains(&self.tables.key(top))
                    && (self.tables.conventions.capital_domains || !capitalized(top))
            })
            .ok_or(labels)?;
        if self.get(end) == Some(':') && self.get(end + 1).is_some_and(|c| c.is_ascii_digit()) {
            end += 1;
            while self.get(end).is_some_and(|c| c.is_ascii_digit()) {
                end += 1;
            }
        }
        if self.get(end) == Some('/') {
            end = self.url(end);
        }
        Ok(end)
    }

    /// A mention after «@» or a hashtag after «#»
    fn handle(&self, start: usize, mention: bool) -> usize {
        let mut end = start;
        while let Some(character) = self.get(end) {
            let inner = (if mention { ".-" } else { "-" }).contains(character)
                && self.get(end + 1).is_some_and(|c| is_word(c) || c == '_');
            if is_word(character) || character == '_' || is_mark(character) || inner {
                end += character.len_utf8();
            } else {
                break;
            }
        }
        end
    }

    /// An emoticon: one of the lists or a face of eyes, a nose and a run of one mouth character
    fn emoticon(&self, start: usize) -> Option<usize> {
        if self.tables.starts_emoticon(self.text.as_bytes()[start])
            && let Some(emoticon) = self
                .tables
                .emoticons
                .iter()
                .find(|emoticon| self.text[start..self.end].starts_with(emoticon.as_str()))
        {
            return Some(start + emoticon.len());
        }
        let mut index = start;
        let eyes = self.get(index)?;
        if !":;=".contains(eyes) {
            return None;
        }
        index += 1;
        if matches!(self.get(index), Some('-' | '\'')) {
            index += 1;
        }
        let mouth = self.get(index).filter(|c| ")(DPpOo*|/3][".contains(*c))?;
        // After a number or a word a digit is a score or an equation, and so is a letter after «=»: «2:3»,
        // «n=3», «F=P/S»
        let after_word = self.text[..start].chars().next_back().is_some_and(is_word);
        if after_word && (mouth.is_ascii_digit() || (eyes == '=' && mouth.is_alphabetic())) {
            return None;
        }
        let mouth_start = index;
        while self.get(index) == Some(mouth) {
            index += 1;
        }
        // Stars after a colon at a word are bold in Markdown: «**Итог:**»; after a space, a kiss: «:**»
        if mouth == '*' && index - mouth_start > 1 && after_word {
            return None;
        }
        let letter = is_word(mouth);
        (!(letter && self.get(index).is_some_and(is_word))).then_some(index)
    }

    /// The end of an HTML entity at `start`: «&quot;», «&#39;», «&#x27;»
    fn entity(&self, start: usize) -> Option<usize> {
        let rest = self.text[start..self.end].strip_prefix('&')?;
        let (length, valid) = match rest.strip_prefix('#') {
            Some(number) => match number.strip_prefix(['x', 'X']) {
                Some(hex) => {
                    let digits = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
                    (2 + digits, digits > 0)
                }
                None => {
                    let digits = number.bytes().take_while(u8::is_ascii_digit).count();
                    (1 + digits, digits > 0)
                }
            },
            None => {
                let letters = rest.bytes().take_while(u8::is_ascii_alphanumeric).count();
                (letters, letters > 0)
            }
        };
        (valid && rest[length..].starts_with(';')).then_some(start + length + 2)
    }

    /// A run of emoji without separators between them
    fn emoji(&self, start: usize) -> usize {
        let mut end = start;
        while let Some(character) = self.get(end) {
            let joined = character == '\u{200d}' && self.get(end + 3).is_some_and(is_emoji);
            if is_emoji(character)
                || (is_emoji_part(character) && (character != '\u{200d}' || joined))
            {
                end += character.len_utf8();
            } else {
                break;
            }
        }
        end
    }

    /// Groups of digits joined by hyphens, three or more of them: a phone number, an ISBN, an ISO date
    fn digit_chain(&self, start: usize) -> Option<usize> {
        let mut end = start;
        let mut groups = 0;
        loop {
            let group = end;
            while self.get(end).is_some_and(|c| c.is_ascii_digit()) {
                end += 1;
            }
            if end == group {
                return None;
            }
            groups += 1;
            if self.get(end) == Some('-') && self.get(end + 1).is_some_and(|c| c.is_ascii_digit()) {
                end += 1;
            } else {
                break;
            }
        }
        (groups >= 3 && !self.get(end).is_some_and(is_word)).then_some(end)
    }

    /// A run of letters and digits with the combining marks and joiners inside it
    fn run(&self, start: usize) -> usize {
        let rest = &self.text[start..self.end];
        let mut chars = rest.char_indices().peekable();
        while let Some((offset, character)) = chars.next() {
            if is_word(character) || is_mark(character) {
                continue;
            }
            let inner = is_joiner(character)
                && chars.peek().is_some_and(|&(_, c)| is_word(c) || is_mark(c));
            if !inner {
                return start + offset;
            }
        }
        self.end
    }

    /// The joint after the run `last`, if the text goes on with the next run of the same word or number;
    /// `kinds` are the bits of the joints since the last hyphen, those of the current number
    fn joint(&self, last: Span, kinds: u8) -> Option<(Joint, usize)> {
        let piece = last.slice(self.text);
        let character = self.get(last.end)?;
        let next = last.end + character.len_utf8();
        let after = self.get(next)?;
        let digits = is_digits(piece);
        let only = |kind: Joint| kinds & !kind.bit() == 0;
        let groups = self.tables.conventions.digit_groups;
        let joint = match character {
            '-' | '\u{2010}' | '\u{2011}' if is_word(after) => Joint::Hyphen,
            '\'' | '\u{2019}' if is_alpha(after) && !digits => Joint::Apostrophe,
            '_' if is_word(after) => Joint::Underscore,
            // A range with an en dash, where a range is one token
            '\u{2013}'
                if digits && after.is_ascii_digit() && !self.tables.conventions.digit_ranges =>
            {
                Joint::Hyphen
            }
            mark if is_word(after) && self.tables.conventions.inner_marks.contains(&mark) => {
                Joint::Inner
            }
            // A version: «v1.2.3»
            '.' if (digits
                || (piece.starts_with(is_alpha)
                    && piece.ends_with(|c: char| c.is_ascii_digit())))
                && after.is_ascii_digit()
                && (only(Joint::Point) || (groups == DigitGroups::Comma && only(Joint::Comma))) =>
            {
                Joint::Point
            }
            // A comma ends groups of three digits or is the only mark: «1.012,72», «4,5», but «3.5,4.5»
            ',' if digits
                && after.is_ascii_digit()
                && match groups {
                    DigitGroups::Space => only(Joint::Thousands),
                    DigitGroups::Point => only(Joint::Point) && (kinds == 0 || piece.len() == 3),
                    DigitGroups::Comma => only(Joint::Comma) && self.digits_end(next) - next == 3,
                } =>
            {
                Joint::Comma
            }
            ':' if digits && piece.len() <= 2 && only(Joint::Colon) => {
                let end = self.digits_end(next);
                if end - next != 2 || self.get(end).is_some_and(is_word) {
                    return None;
                }
                Joint::Colon
            }
            ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}'
                if digits
                    && groups == DigitGroups::Space
                    && only(Joint::Thousands)
                    && (if kinds == 0 {
                        piece.len() <= 3
                    } else {
                        piece.len() == 3
                    }) =>
            {
                let end = self.digits_end(next);
                if end - next != 3 || self.get(end).is_some_and(is_word) {
                    return None;
                }
                Joint::Thousands
            }
            _ => return None,
        };
        Some((joint, next))
    }

    fn digits_end(&self, start: usize) -> usize {
        let mut end = start;
        while self.get(end).is_some_and(|c| c.is_ascii_digit()) {
            end += 1;
        }
        end
    }

    /// Whether a hyphen between two runs is a token of its own; `chains` are the numbers of groups in the
    /// chains of digits joined by hyphens that the runs belong to
    fn splits(&self, runs: &[Span], joints: &[Joint], chains: &[usize], index: usize) -> bool {
        let left = runs[index].slice(self.text);
        let right = runs[index + 1].slice(self.text);
        let conventions = &self.tables.conventions;
        if is_digits(left) && is_digits(right) {
            // A range «2-3», but not a chain of three groups or more: a phone number, an ISO date
            return conventions.digit_ranges && chains[index] < 3;
        }
        if is_roman(left) && is_roman(right) {
            return conventions.roman_ranges;
        }
        match conventions.hyphens {
            Hyphens::Whole => false,
            Hyphens::Split => {
                let left_key = self.tables.key(left);
                !self.tables.prefixes.contains(&left_key)
                    && !self.listed_whole(runs, joints, index, &left_key, &self.tables.key(right))
            }
            Hyphens::Inflected => self.inflected_parts(runs, joints, index),
        }
    }

    /// Whether the word with the hyphen after the run `index` stays whole by the list: the two runs around the
    /// hyphen, whose keys are given, or all the runs joined by hyphens. A chain longer than any word of the list is not
    /// read: «a-a-a-…»
    fn listed_whole(
        &self,
        runs: &[Span],
        joints: &[Joint],
        index: usize,
        left_key: &str,
        right_key: &str,
    ) -> bool {
        let tables = self.tables;
        if tables.whole.contains(&format!("{left_key}-{right_key}")) {
            return true;
        }
        let limit = tables.whole_parts;
        if limit <= 2 {
            return false;
        }
        let hyphen = |joint: usize| joints.get(joint) == Some(&Joint::Hyphen);
        let mut first = index;
        while first > 0 && hyphen(first - 1) && index + 2 - first <= limit {
            first -= 1;
        }
        let mut last = index + 1;
        while hyphen(last) && last + 1 - first < limit {
            last += 1;
        }
        if (first > 0 && hyphen(first - 1)) || hyphen(last) {
            return false;
        }
        let keys: Vec<String> = runs[first..=last]
            .iter()
            .map(|run| tables.key(run.slice(self.text)))
            .collect();
        tables.whole.contains(&keys.join("-"))
    }

    /// Whether a hyphen is a token of its own in a language whose words split where their parts inflect
    fn inflected_parts(&self, runs: &[Span], joints: &[Joint], index: usize) -> bool {
        let left = runs[index].slice(self.text);
        let right = runs[index + 1].slice(self.text);
        let tables = self.tables;
        let (left_key, right_key) = (tables.key(left), tables.key(right));
        // A name with an inflected first part and a linker between hyphens: «Ростова-на-Дону»
        let linker = |run: usize| {
            tables
                .linkers
                .contains(&tables.key(runs[run].slice(self.text)))
        };
        if (index + 1 < joints.len() && joints[index + 1] == Joint::Hyphen && linker(index + 1))
            || (index > 0 && joints[index - 1] == Joint::Hyphen && linker(index))
        {
            return true;
        }
        if self.listed_whole(runs, joints, index, &left_key, &right_key) {
            return false;
        }
        // A particle closes the word: «де» inside «Рио-де-Жанейро» is not one
        let closes = index + 1 == joints.len() || joints[index + 1] != Joint::Hyphen;
        if closes && tables.particles.contains(&right_key) {
            return !(right_key == tables.conventions.indefinite_particle
                && tables.indefinite.contains(&left_key));
        }
        if !is_letters(left) {
            return false;
        }
        if is_digits(right) {
            // A word with an inflected first part and a number splits («Формулы-1», «Марса-96»), an
            // abbreviation stays whole («Ту-154», «COVID-19»)
            return left.chars().filter(|&c| is_alpha(c)).count() >= 3
                && left.chars().any(char::is_lowercase)
                && !tables.prefixes.contains(&left_key);
        }
        if tables.numerals.contains(&right_key) {
            return true;
        }
        // Both parts inflect when they end with the same letter of an inflected form: «языку-предку»; a
        // repeated word («еле-еле») and a first part that does not inflect («мини-саунами») stay whole
        let ending = |word: &str| word.chars().next_back().map(String::from);
        is_letters(right)
            && left_key != right_key
            && left_key.chars().count() >= 3
            && right_key.chars().count() >= 3
            && !tables.prefixes.contains(&left_key)
            && ending(&left_key).is_some_and(|end| tables.endings.contains(&end))
            && ending(&left_key) == ending(&right_key)
    }

    /// A word or a number with the runs joined to it, split at the hyphens that are tokens of their own; a
    /// period after an abbreviation or an initial stays in the token
    fn compound(&mut self, start: usize, end: usize) -> usize {
        // Most words have no joints: one token, with the period of an abbreviation
        let digits = is_digits(&self.text[start..end]);
        let joins = self.get(end).is_some_and(|character| {
            matches!(
                character,
                '-' | '\u{2010}' | '\u{2011}' | '\'' | '\u{2019}' | '_'
            ) || (digits
                && matches!(
                    character,
                    '.' | ',' | ':' | ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}'
                ))
                || (character == '.' && self.text[..end].ends_with(|c: char| c.is_ascii_digit()))
                || (digits && character == '\u{2013}')
                || self.tables.conventions.inner_marks.contains(&character)
        });
        if digits && let Some(stop) = self.ordinal_end(end) {
            return self.push(start, stop);
        }
        if !joins {
            let end = if let Some(stop) = self.dotted_abbreviation(start, end) {
                stop
            } else if self.abbreviation(start, end) {
                end + 1
            } else if digits {
                self.percent_ending(end)
            } else {
                self.dotted_name(start, end)
            };
            return self.push(start, end);
        }
        let mut runs = vec![Span { start, end }];
        let mut joints = Vec::new();
        let mut kinds = 0;
        while let Some((joint, next)) = self.joint(runs[runs.len() - 1], kinds) {
            kinds = if joint == Joint::Hyphen {
                0
            } else {
                kinds | joint.bit()
            };
            joints.push(joint);
            runs.push(Span {
                start: next,
                end: self.run(next),
            });
        }
        let mut chains = vec![1; runs.len()];
        let mut chain = 0;
        for index in 1..=runs.len() {
            let joined = index < runs.len()
                && joints[index - 1] == Joint::Hyphen
                && is_digits(runs[index - 1].slice(self.text))
                && is_digits(runs[index].slice(self.text));
            if !joined {
                chains[chain..index].fill(index - chain);
                chain = index;
            }
        }
        let phone = self.phone_groups(&runs, &joints, &chains);
        let mut first = start;
        for index in 0..joints.len() {
            if phone[index] {
                self.push(first, runs[index].end);
                first = runs[index + 1].start;
            } else if joints[index] == Joint::Hyphen && self.splits(&runs, &joints, &chains, index)
            {
                self.push(first, runs[index].end);
                self.push(runs[index].end, runs[index + 1].start);
                first = runs[index + 1].start;
            }
        }
        let last = runs[runs.len() - 1];
        let mut end = last.end;
        if let Some(stop) = self.dotted_abbreviation(last.start, last.end) {
            end = stop;
        } else if first == last.start && self.abbreviation(first, end) {
            end += 1;
        } else if is_digits(last.slice(self.text)) {
            end = self.percent_ending(end);
        }
        self.push(first, end)
    }

    /// The end of an ordinal number whose digits end at `end`, with its period and ending: «3.º»
    fn ordinal_end(&self, end: usize) -> Option<usize> {
        let endings = &self.tables.conventions.ordinal_endings;
        if endings.is_empty() || self.get(end) != Some('.') {
            return None;
        }
        let stop = self.run(end + 1);
        endings
            .iter()
            .any(|ending| ending == &self.text[end + 1..stop])
            .then_some(stop)
    }

    /// The end of a run of letters and digits up to a superscript digit, or of that digit, where it is a token
    #[inline]
    fn superscript_end(&self, start: usize, end: usize) -> usize {
        if !self.tables.conventions.split_superscripts {
            return end;
        }
        match self.text[start..end].find(['\u{b2}', '\u{b3}']) {
            Some(0) => start + 2,
            Some(offset) => start + offset,
            None => end,
        }
    }

    /// The end of an abbreviation with periods inside it, in a language that keeps it in one token: «EE.UU.», «U.S.»,
    /// «Ph.D.»; its parts are letters, one or two of them
    #[inline]
    fn dotted_abbreviation(&self, start: usize, end: usize) -> Option<usize> {
        // It runs for every word: most have no period after them, and many languages split at the periods
        if self.tables.conventions.split_dotted
            || end >= self.end
            || self.text.as_bytes()[end] != b'.'
        {
            return None;
        }
        self.dotted_parts(start, end)
    }

    #[cold]
    fn dotted_parts(&self, start: usize, end: usize) -> Option<usize> {
        let short = |piece: &str| is_letters(piece) && piece.chars().count() <= 2;
        if !short(&self.text[start..end]) {
            return None;
        }
        let mut stop = end;
        let mut part = start;
        while self.get(stop) == Some('.') && self.get(stop + 1).is_some_and(is_alpha) {
            let next = self.run(stop + 1);
            let piece = &self.text[stop + 1..next];
            if !short(piece) || self.glued(&self.text[part..stop], piece, next) {
                break;
            }
            part = stop + 1;
            stop = next;
        }
        if stop == end {
            return None;
        }
        let period = self.get(stop) == Some('.') && self.get(stop + 1) != Some('.');
        Some(if period { stop + 1 } else { stop })
    }

    /// Whether the part `piece` that ends at `end` starts a sentence glued to the part `before`, and not an
    /// abbreviation: «it.He», «vi.Yo», «no.Y luego», «EE.UU.La»; but «Ph.D», «M.Sc.», «p.ej.», «s.XX», «ee.uu.»
    fn glued(&self, before: &str, piece: &str, end: usize) -> bool {
        let mut letters = piece.chars();
        let capitalized = letters.next().is_some_and(char::is_uppercase)
            && letters.next().is_some_and(char::is_lowercase);
        let period = self.get(end) == Some('.');
        if capitalized && !period {
            return true;
        }
        if !before.ends_with(char::is_lowercase) {
            return false;
        }
        capitalized
            || (before.chars().count() == 2 && before.chars().all(char::is_lowercase) && !period)
    }

    /// The end of a name of the language joined to the next word by a period: «Яндекс.Маркете»
    fn dotted_name(&self, start: usize, end: usize) -> usize {
        if end < self.end
            && self.text.as_bytes()[end] == b'.'
            && self.get(end + 1).is_some_and(char::is_uppercase)
            && self
                .tables
                .dotted_names
                .contains(&self.tables.key(&self.text[start..end]))
        {
            let next = self.run(end + 1);
            if is_letters(&self.text[end + 1..next]) {
                return next;
            }
        }
        end
    }

    /// The end of a number with a percent sign and an ending after a hyphen, which is one word: «20%-ный»
    fn percent_ending(&self, end: usize) -> usize {
        if self.tables.conventions.percent_endings
            && self.get(end) == Some('%')
            && self.get(end + 1) == Some('-')
            && self.get(end + 2).is_some_and(char::is_lowercase)
        {
            // Endings are short («-ный», «-го»); a longer word is a word after a dash: «50%-только»
            let ending = self.run(end + 2);
            let letters = &self.text[end + 2..ending];
            if is_letters(letters) && letters.chars().count() <= 4 {
                return ending;
            }
        }
        end
    }

    /// The spaces between groups of digits that are not a number with thousands but a phone number: groups before a
    /// chain of three groups or more or before a group of another length, «+7 495 123-45-67», «8 800 555 35 35»
    fn phone_groups(&self, runs: &[Span], joints: &[Joint], chains: &[usize]) -> Vec<bool> {
        let mut phone = vec![false; joints.len()];
        let mut index = 0;
        while index < joints.len() {
            if joints[index] != Joint::Thousands {
                index += 1;
                continue;
            }
            let first = index;
            while index < joints.len() && joints[index] == Joint::Thousands {
                index += 1;
            }
            let after = if index < joints.len() {
                joints[index] == Joint::Hyphen && chains[index] >= 3
            } else {
                // Numbers side by side have more digits than a phone number
                let (shortest, longest) = self.tables.conventions.phone_digits.unwrap_or((1, 0));
                let space = |c: char| matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}');
                let mut digits: usize = runs[first..=index]
                    .iter()
                    .map(|run| run.end - run.start)
                    .sum();
                let mut end = runs[index].end;
                while digits <= longest
                    && self.get(end).is_some_and(space)
                    && self.get(end + 1).is_some_and(|c| c.is_ascii_digit())
                {
                    let group = self.digits_end(end + 1);
                    digits += group - end - 1;
                    end = group;
                }
                end > runs[index].end
                    && (shortest..=longest).contains(&digits)
                    && !self.get(end).is_some_and(|c| is_word(c) || c == '-')
            };
            if after {
                phone[first..index].fill(true);
            }
        }
        phone
    }

    /// Whether a period follows a word that keeps it: an abbreviation of the language or a single letter
    fn abbreviation(&self, start: usize, end: usize) -> bool {
        if self.get(end) != Some('.') || self.get(end + 1) == Some('.') {
            return false;
        }
        let word = &self.text[start..end];
        if !is_letters(word) {
            return false;
        }
        let word_key = self.tables.key(word);
        if self.exceptions.abbreviations.contains(&word_key) {
            return true;
        }
        if word_key.chars().count() == 1 {
            // An uppercase letter is an initial; a lowercase one is an abbreviation unless it is a word
            return word.chars().any(char::is_uppercase)
                || !self.tables.not_abbreviations.contains(&word_key);
        }
        self.tables.abbreviations.contains(&word_key)
            && !self.tables.capital_not_abbreviations.contains(word)
    }

    /// Punctuation and other symbols: runs of final marks, of hyphens and of some symbols are one token,
    /// as are runs of brackets that are emoticons
    fn punctuation(&mut self, start: usize, character: char) -> usize {
        let run_of = |set: &str| {
            let mut end = start;
            while let Some(c) = self.get(end).filter(|c| set.contains(*c)) {
                end += c.len_utf8();
            }
            end
        };
        let next = start + character.len_utf8();
        match character {
            '.' | '!' | '?' | '…' => {
                let end = if self.tables.conventions.mixed_final_marks {
                    run_of(".!?…")
                } else {
                    run_of(&character.to_string())
                };
                self.push(start, end)
            }
            '-' | '*' | '№' | '=' | '~' | '_' | '+' | '#' | '`' | '\'' => {
                let end = run_of(&character.to_string());
                self.push(start, end)
            }
            '(' | ')' => {
                let end = run_of(&character.to_string());
                let length = end - start;
                // A run of «(» opens brackets before a word and is an emoticon otherwise; a run of «)» closes
                // the open brackets one by one, and the rest of it is an emoticon
                let singles = if character == '(' {
                    if length >= 2 && !self.get(end).is_some_and(is_word) {
                        0
                    } else {
                        length
                    }
                } else if length <= self.open + 1 {
                    length
                } else {
                    self.open
                };
                for offset in 0..singles {
                    self.push(start + offset, start + offset + 1);
                }
                if character == '(' {
                    self.open += singles;
                } else {
                    self.open = self.open.saturating_sub(singles);
                }
                if singles < length {
                    self.push(start + singles, end);
                }
                end
            }
            '°' if self
                .get(next)
                .is_some_and(|c| self.tables.conventions.degree_letters.contains(c)) =>
            {
                let letter = next + self.get(next).map_or(0, char::len_utf8);
                if self.get(letter).is_some_and(is_word) {
                    self.push(start, next)
                } else {
                    self.push(start, letter)
                }
            }
            _ => self.push(start, next),
        }
    }
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use super::{is_alpha, is_word};

    #[test]
    fn character_shortcuts() {
        for character in (0..=0x10ffff).filter_map(char::from_u32) {
            assert_eq!(
                is_alpha(character),
                character.is_alphabetic(),
                "{character:?}"
            );
            assert_eq!(
                is_word(character),
                character.is_alphanumeric(),
                "{character:?}"
            );
        }
    }

    fn tokens(text: &str) -> Vec<&str> {
        crate::Segmenter::new("ru")
            .unwrap()
            .tokenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    #[test]
    fn words_and_punctuation() {
        assert_eq!(
            tokens("Мама мыла раму, а «кот» (рыжий) спал..."),
            [
                "Мама",
                "мыла",
                "раму",
                ",",
                "а",
                "«",
                "кот",
                "»",
                "(",
                "рыжий",
                ")",
                "спал",
                "..."
            ]
        );
        assert_eq!(
            tokens("Что?! Да!!! Ну?.. Так…"),
            ["Что", "?!", "Да", "!!!", "Ну", "?..", "Так", "…"]
        );
        assert_eq!(
            tokens("а -- б — в – г"),
            ["а", "--", "б", "—", "в", "–", "г"]
        );
        assert_eq!(
            tokens("``Кот&#39;&#39; &quot;пёс&quot; a&b &#x27;"),
            [
                "``",
                "Кот",
                "&#39;&#39;",
                "&quot;",
                "пёс",
                "&quot;",
                "a",
                "&",
                "b",
                "&#x27;"
            ]
        );
        assert_eq!(
            tokens("Ку\u{301}рица До\u{ad}ма"),
            ["Ку\u{301}рица", "До\u{ad}ма"]
        );
        assert_eq!(tokens("\u{ad} кот \u{ad}"), ["кот"]);
        assert_eq!(
            tokens("О'Нил д’Артаньян 'да'"),
            ["О'Нил", "д’Артаньян", "'", "да", "'"]
        );
    }

    #[test]
    fn abbreviations_and_initials() {
        assert_eq!(
            tokens("В 1914 г. в Москве, т. е. в 2 тыс. км от А.С. Пушкина, и т.д. и т.п."),
            [
                "В",
                "1914",
                "г.",
                "в",
                "Москве",
                ",",
                "т.",
                "е.",
                "в",
                "2",
                "тыс.",
                "км",
                "от",
                "А.",
                "С.",
                "Пушкина",
                ",",
                "и",
                "т.",
                "д.",
                "и",
                "т.",
                "п",
                "."
            ]
        );
        assert_eq!(
            tokens("Это я. Это кот."),
            ["Это", "я", ".", "Это", "кот", "."]
        );
        assert_eq!(tokens("г.Москва"), ["г.", "Москва"]);
        assert_eq!(
            tokens("85 000 руб. в месяц"),
            ["85 000", "руб.", "в", "месяц"]
        );
    }

    #[test]
    fn hyphens() {
        assert_eq!(
            tokens(
                "кто-то кое-как из-за по-русски во-первых северо-западный Wi-Fi VIP-зал чуть-чуть"
            ),
            [
                "кто-то",
                "кое-как",
                "из-за",
                "по-русски",
                "во-первых",
                "северо-западный",
                "Wi-Fi",
                "VIP-зал",
                "чуть-чуть"
            ]
        );
        assert_eq!(
            tokens("Наконец-то скажи-ка всё-таки пришёл-таки какого-то"),
            [
                "Наконец",
                "-",
                "то",
                "скажи",
                "-",
                "ка",
                "всё-таки",
                "пришёл",
                "-",
                "таки",
                "какого-то"
            ]
        );
        assert_eq!(
            tokens("два-три полтора-двух год-два 5-летний 90-х Ту-154 COVID-19"),
            [
                "два",
                "-",
                "три",
                "полтора",
                "-",
                "двух",
                "год",
                "-",
                "два",
                "5-летний",
                "90-х",
                "Ту-154",
                "COVID-19"
            ]
        );
        assert_eq!(
            tokens("2-3 1941-1945 XIX-XX двух- и"),
            [
                "2", "-", "3", "1941", "-", "1945", "XIX", "-", "XX", "двух", "-", "и"
            ]
        );
    }

    #[test]
    fn found_by_the_dirty_set() {
        assert_eq!(
            tokens(
                "Он чтó-то сказал, Домá. Дж. К. Роулинг, robots.txt, Ростова-на-Дону, «Формулы-1», Ту-154"
            ),
            [
                "Он",
                "чтó-то",
                "сказал",
                ",",
                "Домá",
                ".",
                "Дж.",
                "К.",
                "Роулинг",
                ",",
                "robots.txt",
                ",",
                "Ростова",
                "-",
                "на",
                "-",
                "Дону",
                ",",
                "«",
                "Формулы",
                "-",
                "1",
                "»",
                ",",
                "Ту-154"
            ]
        );
        assert_eq!(
            tokens("hr@example.ru.Жду сайт example.ru.Регистрация ``Привет''"),
            [
                "hr@example.ru",
                ".",
                "Жду",
                "сайт",
                "example.ru",
                ".",
                "Регистрация",
                "``",
                "Привет",
                "''"
            ]
        );
    }

    #[test]
    fn inflected_compounds() {
        assert_eq!(
            tokens(
                "учеными-энтузиастами языку-предку городе-саде еле-еле мини-саунами кое-какие ноу-хау"
            ),
            [
                "учеными",
                "-",
                "энтузиастами",
                "языку",
                "-",
                "предку",
                "городе",
                "-",
                "саде",
                "еле-еле",
                "мини-саунами",
                "кое-какие",
                "ноу-хау"
            ]
        );
        assert_eq!(
            tokens("Я. Иванов, прямо-таки #ростов-на-дону ‼\u{fe0f}"),
            [
                "Я.",
                "Иванов",
                ",",
                "прямо-таки",
                "#ростов-на-дону",
                "‼\u{fe0f}"
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            tokens(
                "4,5 1.5 12.10.2026 12:30 9:00 3:0 1 000 000 руб 2 000,5 10% $5 5$ №5 №№ 27° 20 °С"
            ),
            [
                "4,5",
                "1.5",
                "12.10.2026",
                "12:30",
                "9:00",
                "3",
                ":",
                "0",
                "1 000 000",
                "руб",
                "2 000,5",
                "10",
                "%",
                "$",
                "5",
                "5",
                "$",
                "№",
                "5",
                "№№",
                "27",
                "°",
                "20",
                "°С"
            ]
        );
        assert_eq!(
            tokens("1,2,3 +7-900-000-00-00 978-5-000-00000-0 2026-10-25 +5 −5 1/3 км/ч"),
            [
                "1,2",
                ",",
                "3",
                "+7-900-000-00-00",
                "978-5-000-00000-0",
                "2026-10-25",
                "+",
                "5",
                "−",
                "5",
                "1",
                "/",
                "3",
                "км",
                "/",
                "ч"
            ]
        );
        assert_eq!(tokens("в 3 4 и 2 0001"), ["в", "3", "4", "и", "2", "0001"]);
    }

    #[test]
    fn web() {
        assert_eq!(
            tokens(
                "См. https://example.com/a?b=1&c=(2). Или (www.example.ru/x) и Газета.ru, сайт.рф."
            ),
            [
                "См.",
                "https://example.com/a?b=1&c=(2)",
                ".",
                "Или",
                "(",
                "www.example.ru/x",
                ")",
                "и",
                "Газета.ru",
                ",",
                "сайт.рф",
                "."
            ]
        );
        assert_eq!(
            tokens("Пишите ivan.petrov@example.com, @ivan_p! #море #доброе_утро @screened-12."),
            [
                "Пишите",
                "ivan.petrov@example.com",
                ",",
                "@ivan_p",
                "!",
                "#море",
                "#доброе_утро",
                "@screened-12",
                "."
            ]
        );
    }

    #[test]
    fn emoji_and_emoticons() {
        assert_eq!(
            tokens("Отлично👍 🎉🎂 👍🏽 👨\u{200d}👩\u{200d}👧 ☀\u{fe0f}"),
            [
                "Отлично",
                "👍",
                "🎉🎂",
                "👍🏽",
                "👨\u{200d}👩\u{200d}👧",
                "☀\u{fe0f}"
            ]
        );
        assert_eq!(
            tokens("Спасибо :) Ура:-))) дождь((( ^_^ :D (см. рис. 1)) ок)"),
            [
                "Спасибо",
                ":)",
                "Ура",
                ":-)))",
                "дождь",
                "(((",
                "^_^",
                ":D",
                "(",
                "см.",
                "рис.",
                "1",
                ")",
                ")",
                "ок",
                ")"
            ]
        );
    }

    #[test]
    fn final_abbreviation() {
        assert_eq!(
            tokens("Это было в 1914 г."),
            ["Это", "было", "в", "1914", "г", "."]
        );
        assert_eq!(tokens("Пушкин А.С."), ["Пушкин", "А.", "С", "."]);
    }

    #[test]
    fn numbers_after_a_hyphen() {
        assert_eq!(
            tokens("2,5-3,5 млн, 1.5-2.5, с 10:00-18:00, 10.10.2026-12.10.2026, 1 000-2 000 руб"),
            [
                "2,5",
                "-",
                "3,5",
                "млн",
                ",",
                "1.5",
                "-",
                "2.5",
                ",",
                "с",
                "10:00",
                "-",
                "18:00",
                ",",
                "10.10.2026",
                "-",
                "12.10.2026",
                ",",
                "1 000",
                "-",
                "2 000",
                "руб"
            ]
        );
    }

    #[test]
    fn found_by_the_review() {
        assert_eq!(
            tokens("со счётом 2:3, n=3, Спасибо :3"),
            [
                "со",
                "счётом",
                "2",
                ":",
                "3",
                ",",
                "n",
                "=",
                "3",
                ",",
                "Спасибо",
                ":3"
            ]
        );
        assert_eq!(
            tokens("Файл IMG_1234.jpg, report_2026.pdf и my-file.txt"),
            [
                "Файл",
                "IMG_1234.jpg",
                ",",
                "report_2026.pdf",
                "и",
                "my-file.txt"
            ]
        );
        assert_eq!(
            tokens("при 20°C тепла, 20°C. При 20°F в тени, 20°С тепла"),
            [
                "при",
                "20",
                "°C",
                "тепла",
                ",",
                "20",
                "°C",
                ".",
                "При",
                "20",
                "°F",
                "в",
                "тени",
                ",",
                "20",
                "°С",
                "тепла"
            ]
        );
        assert_eq!(
            tokens("▶\u{fe0f} Смотрите ©\u{fe0f} 2026 #\u{fe0f}\u{20e3}"),
            [
                "▶\u{fe0f}",
                "Смотрите",
                "©\u{fe0f}",
                "2026",
                "#\u{fe0f}\u{20e3}"
            ]
        );
        assert_eq!(
            tokens("Рио-де-Жанейро, Па-де-Кале, Вася-то, мол-де, Отчего-то"),
            [
                "Рио-де-Жанейро",
                ",",
                "Па-де-Кале",
                ",",
                "Вася",
                "-",
                "то",
                ",",
                "мол",
                "-",
                "де",
                ",",
                "Отчего-то"
            ]
        );
        assert_eq!(
            tokens("гласный [\u{361}ие], ▶\u{fe0f}Смотрите"),
            [
                "гласный",
                "[",
                "\u{361}ие",
                "]",
                ",",
                "▶\u{fe0f}",
                "Смотрите"
            ]
        );
        assert_eq!(
            tokens("((а)) (да))) Ок)))"),
            ["(", "(", "а", ")", ")", "(", "да", ")", "))", "Ок", ")))"]
        );
    }

    #[test]
    fn separators_and_combining_marks() {
        assert_eq!(tokens("a\u{1f}b\u{0}c"), ["a", "b", "c"]);
        assert_eq!(
            tokens("Ура 😀\u{301} да °C\u{301}Б"),
            ["Ура", "😀\u{301}", "да", "°C\u{301}", "Б"]
        );
    }

    #[test]
    fn technical_text() {
        assert_eq!(
            tokens("Целую :** и :*** да"),
            ["Целую", ":**", "и", ":***", "да"]
        );
        assert_eq!(
            tokens("F=P/S, x=O(n) и Пока:P"),
            [
                "F", "=", "P", "/", "S", ",", "x", "=", "O", "(", "n", ")", "и", "Пока", ":P"
            ]
        );
        assert_eq!(
            tokens("main.rs, config.yaml, книга.fb2, v1.2.3 и **Итог:** да"),
            [
                "main.rs",
                ",",
                "config.yaml",
                ",",
                "книга.fb2",
                ",",
                "v1.2.3",
                "и",
                "**",
                "Итог",
                ":",
                "**",
                "да"
            ]
        );
        assert_eq!(
            tokens("на Яндекс.Маркете, Лента.ру и Яндекс. Потом"),
            [
                "на",
                "Яндекс.Маркете",
                ",",
                "Лента.ру",
                "и",
                "Яндекс",
                ".",
                "Потом"
            ]
        );
    }

    #[test]
    fn phones_and_percents() {
        assert_eq!(
            tokens("+7 495 123-45-67, 8 800 555 35 35, 1 000-1 500 руб"),
            [
                "+",
                "7",
                "495",
                "123-45-67",
                ",",
                "8",
                "800",
                "555",
                "35",
                "35",
                ",",
                "1 000",
                "-",
                "1 500",
                "руб"
            ]
        );
        assert_eq!(
            tokens("+7 912 345 67 89, 8 495 123 4567; 12 345 11 234 9 876; 1 250 12,5"),
            [
                "+", "7", "912", "345", "67", "89", ",", "8", "495", "123", "4567", ";", "12 345",
                "11 234", "9 876", ";", "1 250", "12,5"
            ]
        );
        assert_eq!(
            tokens("Скидка 50%-только сегодня"),
            ["Скидка", "50", "%", "-", "только", "сегодня"]
        );
        assert_eq!(
            tokens("20%-ный, 0,5%-го, 1 000%-ная и 15%-20%"),
            [
                "20%-ный",
                ",",
                "0,5%-го",
                ",",
                "1 000%-ная",
                "и",
                "15",
                "%",
                "-",
                "20",
                "%"
            ]
        );
    }

    #[test]
    fn long_chains() {
        // Linear scans: a quadratic one takes seconds on each of these
        for text in [
            "a.".repeat(40_000),
            "а.".repeat(20_000),
            "a+".repeat(40_000),
            "a+b.".repeat(20_000),
            format!("http://x.ru/{}", ")".repeat(40_000)),
            format!("{}1", "1.".repeat(40_000)),
            format!("1{}", " 000".repeat(40_000)),
            format!("{}1", "1-".repeat(40_000)),
            format!("{}a", "(".repeat(40_000)),
            format!("{}a{}", "(".repeat(20_000), ")".repeat(40_000)),
            "Сайт.рф.".repeat(20_000),
            "B.ru.".repeat(20_000),
            format!("[{}a", "\u{361}".repeat(40_000)),
            "\u{301}".repeat(40_000),
        ] {
            assert_eq!(
                tokens(&text).concat().replace(' ', ""),
                text.replace(' ', "")
            );
        }
    }
}

#[cfg(all(test, feature = "es"))]
mod spanish {
    use crate::Segmenter;

    fn tokens(text: &str) -> Vec<&str> {
        Segmenter::new("es")
            .unwrap()
            .tokenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    #[test]
    fn inner_marks() {
        for token in [
            "km/h",
            "y/o",
            "15/01/2011",
            "1993/99",
            "Reynard/Honda",
            "I+D",
            "1990-1995",
            "1990–1995",
            "XIX-XX",
            "franco-alemán",
        ] {
            assert_eq!(tokens(&format!("Es {token} hoy")), ["Es", token, "hoy"]);
        }
        assert_eq!(tokens("a / b + c"), ["a", "/", "b", "+", "c"]);
        assert_eq!(tokens("a/ b"), ["a", "/", "b"]);
    }

    #[test]
    fn signs_of_numbers() {
        assert_eq!(tokens("de -5 grados"), ["de", "-5", "grados"]);
        assert_eq!(tokens("-5,5 grados"), ["-5,5", "grados"]);
        assert_eq!(tokens("(−7)"), ["(", "−7", ")"]);
        assert_eq!(tokens("6-4"), ["6-4"]);
        assert_eq!(tokens("sí-5"), ["sí-5"]);
        assert_eq!(tokens("- 5"), ["-", "5"]);
        assert_eq!(tokens("a.-5"), ["a.", "-", "5"]);
    }

    #[test]
    fn ordinals() {
        for token in ["3.º", "2.ª", "3.er", "1.os", "1.as", "1º", "2ª"] {
            assert_eq!(tokens(&format!("el {token} piso")), ["el", token, "piso"]);
        }
        assert_eq!(tokens("el 3.a piso"), ["el", "3", ".", "a", "piso"]);
        assert_eq!(tokens("el 3.ºx"), ["el", "3", ".", "ºx"]);
    }

    #[test]
    fn superscripts() {
        assert_eq!(
            tokens("10 km² y 5 cm³"),
            ["10", "km", "²", "y", "5", "cm", "³"]
        );
        assert_eq!(tokens("² km"), ["²", "km"]);
    }

    #[test]
    fn domains_with_a_capital_letter() {
        assert_eq!(tokens("casa.De pronto"), ["casa", ".", "De", "pronto"]);
        assert_eq!(
            tokens("www.ejemplo.es y ejemplo.es"),
            ["www.ejemplo.es", "y", "ejemplo.es"]
        );
        assert_eq!(tokens("EJEMPLO.ES"), ["EJEMPLO.ES"]);
    }

    #[test]
    fn abbreviations() {
        assert_eq!(tokens("El Sr. Pérez"), ["El", "Sr.", "Pérez"]);
        assert_eq!(tokens("Cap. 3, pp. 12"), ["Cap.", "3", ",", "pp.", "12"]);
        assert_eq!(tokens("Es del PP."), ["Es", "del", "PP", "."]);
        assert_eq!(
            tokens("EE. UU. o EE.UU. hoy"),
            ["EE.", "UU.", "o", "EE.UU.", "hoy"]
        );
        assert_eq!(tokens("Vino, etc., y"), ["Vino", ",", "etc.", ",", "y"]);
        assert_eq!(
            tokens("Vino, etc. Luego"),
            ["Vino", ",", "etc", ".", "Luego"]
        );
        assert_eq!(tokens("Sí y. No"), ["Sí", "y", ".", "No"]);
    }
}

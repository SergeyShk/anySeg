//! Tokens: a scan of the text by classes of characters, with the lists of a language

use std::collections::BTreeSet;
use std::ops::Bound;

use unicode_normalization::char::is_combining_mark;

use crate::Span;
use crate::lang::Tables;
use crate::layout::is_separator;

/// Top-level domains and file extensions that make a name one token: «example.ru», «Газета.ru», «robots.txt»
const SUFFIXES: &[&str] = &[
    "ru", "рф", "su", "com", "org", "net", "info", "biz", "edu", "gov", "int", "io", "ai", "me",
    "tv", "fm", "pro", "online", "site", "store", "app", "dev", "by", "ua", "kz", "uz", "kg", "am",
    "ge", "az", "md", "lv", "lt", "ee", "de", "fr", "uk", "us", "eu", "it", "es", "pl", "cz", "cn",
    "jp", "txt", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "csv", "json", "xml", "html",
    "htm", "jpg", "jpeg", "png", "gif", "svg", "zip", "rar", "exe", "py", "js", "mp3", "mp4",
    "avi",
];

/// Emoticons that are not a face of eyes and a mouth
const EMOTICONS: &[&str] = &["^_^", "^^", "-_-", "o_O", "O_o", "о_О", "О_о", "<3"];

fn char_at(text: &str, index: usize) -> Option<char> {
    text.get(index..)?.chars().next()
}

fn is_mark(character: char) -> bool {
    is_combining_mark(character)
}

/// Characters that join the letters of a word without being letters: soft hyphen and zero width (non-)joiner
fn is_joiner(character: char) -> bool {
    matches!(character, '\u{ad}' | '\u{200c}' | '\u{200d}')
}

fn is_emoji(character: char) -> bool {
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

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn is_letters(text: &str) -> bool {
    text.chars().any(char::is_alphabetic)
        && text.chars().all(|character| {
            character.is_alphabetic() || is_mark(character) || is_joiner(character)
        })
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
}

/// The lists of the user: more abbreviations and strings that are one token each
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Exceptions {
    /// Keys of words that keep a period after them
    pub(crate) abbreviations: BTreeSet<String>,
    /// Strings, as written, that are one token
    pub(crate) tokens: BTreeSet<String>,
}

struct Scanner<'a> {
    text: &'a str,
    end: usize,
    tables: &'a Tables,
    exceptions: &'a Exceptions,
    out: &'a mut Vec<Span>,
    /// Round brackets opened and not closed yet in the paragraph
    open: usize,
}

/// The tokens of a paragraph, appended to `out`
pub(crate) fn tokenize(
    text: &str,
    span: Span,
    tables: &Tables,
    exceptions: &Exceptions,
    out: &mut Vec<Span>,
) {
    let first = out.len();
    let mut scanner = Scanner {
        text,
        end: span.end,
        tables,
        exceptions,
        out,
        open: 0,
    };
    let mut index = span.start;
    while let Some(character) = scanner.get(index) {
        if is_separator(character) {
            index += character.len_utf8();
        } else {
            index = scanner.token(index, character);
        }
    }
    // A period after an abbreviation that ends a paragraph ends a sentence: it is a token of its own
    if out.len() > first
        && let Some(last) = out.last().copied()
    {
        let piece = last.slice(text);
        if piece.len() > 1
            && piece.ends_with('.')
            && !exceptions.tokens.contains(piece)
            && piece[..piece.len() - 1]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_alphabetic() || is_mark(character))
        {
            out.pop();
            out.push(Span {
                start: last.start,
                end: last.end - 1,
            });
            out.push(Span {
                start: last.end - 1,
                end: last.end,
            });
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
        if ["http://", "https://", "ftp://", "www."]
            .iter()
            .any(|prefix| self.starts_with(index, prefix))
        {
            let end = self.url(index);
            return self.push(index, end);
        }
        if character.is_alphanumeric() {
            // Only a word followed by «@» or by a dot or a dash before a letter or a digit can start an e-mail
            // address or a domain
            let run = self.run(index);
            let web = match self.get(run) {
                Some('@') => true,
                Some(next @ ('.' | '-' | '_' | '+' | '%')) => self
                    .get(run + next.len_utf8())
                    .is_some_and(char::is_alphanumeric),
                _ => false,
            };
            if !web {
                return self.compound(index, run);
            }
            let labels = self.labels(index, ".-_+%");
            let end = match self.get(labels) {
                Some('@') => self.email(index),
                _ if self.text[index..labels].contains('.') => self.domain(index),
                _ => None,
            };
            return match end {
                Some(end) => self.push(index, end),
                None => self.compound(index, run),
            };
        }
        if let Some(end) = self.emoticon(index) {
            return self.push(index, end);
        }
        let next = index + character.len_utf8();
        match character {
            '@' | '#'
                if self
                    .get(next)
                    .is_some_and(|c| c.is_alphanumeric() || c == '_') =>
            {
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

    /// The end of the longest token of the user at `index` that no letter or digit adjoins
    fn exception(&self, index: usize, character: char) -> Option<usize> {
        let word = |c: char| c.is_alphanumeric() || is_mark(c);
        if self.text[..index].chars().next_back().is_some_and(word) {
            return None;
        }
        let rest = &self.text[index..self.end];
        let first = &rest[..character.len_utf8()];
        self.exceptions
            .tokens
            .range::<str, _>((Bound::Included(first), Bound::Unbounded))
            .take_while(|token| token.starts_with(first))
            .filter(|token| {
                rest.starts_with(token.as_str()) && !self.get(index + token.len()).is_some_and(word)
            })
            .map(|token| index + token.len())
            .max()
    }

    /// The end of a URL: up to whitespace or a quote, without the punctuation that ends a sentence and a
    /// closing bracket it did not open
    fn url(&self, start: usize) -> usize {
        let mut end = start;
        while let Some(character) = self.get(end) {
            if is_separator(character) || "\"«»<>".contains(character) {
                break;
            }
            end += character.len_utf8();
        }
        loop {
            let piece = &self.text[start..end];
            let Some(last) = piece.chars().next_back() else {
                return end;
            };
            let unbalanced = |open: char, close: char| {
                last == close && piece.matches(close).count() > piece.matches(open).count()
            };
            if ".,;:!?'’".contains(last) || unbalanced('(', ')') || unbalanced('[', ']') {
                end -= last.len_utf8();
            } else {
                return end;
            }
        }
    }

    /// A run of letters, digits and dots, dashes and underscores between them, as in domains and e-mail
    fn labels(&self, start: usize, extra: &str) -> usize {
        let mut end = start;
        while let Some(character) = self.get(end) {
            let inner = extra.contains(character)
                && self
                    .get(end + character.len_utf8())
                    .is_some_and(char::is_alphanumeric);
            if character.is_alphanumeric() || is_mark(character) || inner {
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
        let labels: Vec<(usize, &str)> = self.text[start..end]
            .split('.')
            .scan(start, |position, label| {
                let at = *position;
                *position += label.len() + 1;
                Some((at, label))
            })
            .collect();
        for index in 1..labels.len() {
            let (at, label) = labels[index];
            let last = index + 1 == labels.len();
            let glued = labels
                .get(index + 1)
                .is_some_and(|(_, next)| next.starts_with(char::is_uppercase));
            if top(label) && (last || glued) {
                return Some(at + label.len());
            }
        }
        None
    }

    fn email(&self, start: usize) -> Option<usize> {
        let at = self.labels(start, ".-_+%");
        if self.get(at) != Some('@') {
            return None;
        }
        let end = self.labels(at + 1, ".-");
        self.host(at + 1, end, |top| {
            top.chars().count() >= 2 && top.chars().all(char::is_alphabetic)
        })
    }

    /// A bare domain («example.ru», «Газета.ru») with its port and path, or a file name («robots.txt»)
    fn domain(&self, start: usize) -> Option<usize> {
        let end = self.labels(start, ".-");
        let mut end = self.host(start, end, |top| {
            SUFFIXES.contains(&self.tables.key(top).as_str())
        })?;
        if self.get(end) == Some(':') && self.get(end + 1).is_some_and(|c| c.is_ascii_digit()) {
            end += 1;
            while self.get(end).is_some_and(|c| c.is_ascii_digit()) {
                end += 1;
            }
        }
        if self.get(end) == Some('/') {
            end = self.url(end);
        }
        Some(end)
    }

    /// A mention after «@» or a hashtag after «#»
    fn handle(&self, start: usize, mention: bool) -> usize {
        let mut end = start;
        while let Some(character) = self.get(end) {
            let inner = (if mention { ".-" } else { "-" }).contains(character)
                && self
                    .get(end + 1)
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if character.is_alphanumeric() || character == '_' || is_mark(character) || inner {
                end += character.len_utf8();
            } else {
                break;
            }
        }
        end
    }

    /// An emoticon: one of [`EMOTICONS`] or a face of eyes, a nose and a run of one mouth character
    fn emoticon(&self, start: usize) -> Option<usize> {
        if let Some(emoticon) = EMOTICONS
            .iter()
            .find(|emoticon| self.text[start..self.end].starts_with(**emoticon))
        {
            return Some(start + emoticon.len());
        }
        let mut index = start;
        if !":;=".contains(self.get(index)?) {
            return None;
        }
        index += 1;
        if matches!(self.get(index), Some('-' | '\'')) {
            index += 1;
        }
        let mouth = self.get(index).filter(|c| ")(DPpOo*|/3][".contains(*c))?;
        while self.get(index) == Some(mouth) {
            index += 1;
        }
        let letter = mouth.is_alphanumeric();
        (!(letter && self.get(index).is_some_and(char::is_alphanumeric))).then_some(index)
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
        (groups >= 3 && !self.get(end).is_some_and(char::is_alphanumeric)).then_some(end)
    }

    /// A run of letters and digits with the combining marks and joiners inside it
    fn run(&self, start: usize) -> usize {
        let rest = &self.text[start..self.end];
        let mut chars = rest.char_indices().peekable();
        while let Some((offset, character)) = chars.next() {
            if character.is_alphanumeric() || is_mark(character) {
                continue;
            }
            let inner = is_joiner(character)
                && chars
                    .peek()
                    .is_some_and(|&(_, c)| c.is_alphanumeric() || is_mark(c));
            if !inner {
                return start + offset;
            }
        }
        self.end
    }

    /// The joint after a run, if the text goes on with the next run of the same word or number
    fn joint(&self, runs: &[Span], joints: &[Joint]) -> Option<(Joint, usize)> {
        let last = runs[runs.len() - 1];
        let piece = last.slice(self.text);
        let character = self.get(last.end)?;
        let next = last.end + character.len_utf8();
        let after = self.get(next)?;
        let digits = is_digits(piece);
        let numbers = |kinds: &[Joint]| joints.iter().all(|joint| kinds.contains(joint));
        let joint = match character {
            '-' | '\u{2010}' | '\u{2011}' if after.is_alphanumeric() => Joint::Hyphen,
            '\'' | '\u{2019}' if after.is_alphabetic() && !digits => Joint::Apostrophe,
            '_' if after.is_alphanumeric() => Joint::Underscore,
            '.' if digits && after.is_ascii_digit() && numbers(&[Joint::Point]) => Joint::Point,
            ',' if digits
                && after.is_ascii_digit()
                && numbers(&[Joint::Thousands])
                && !joints.contains(&Joint::Comma) =>
            {
                Joint::Comma
            }
            ':' if digits && piece.len() <= 2 && numbers(&[Joint::Colon]) => {
                let end = self.digits_end(next);
                if end - next != 2 || self.get(end).is_some_and(char::is_alphanumeric) {
                    return None;
                }
                Joint::Colon
            }
            ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}'
                if digits
                    && numbers(&[Joint::Thousands])
                    && (if joints.is_empty() {
                        piece.len() <= 3
                    } else {
                        piece.len() == 3
                    }) =>
            {
                let end = self.digits_end(next);
                if end - next != 3 || self.get(end).is_some_and(char::is_alphanumeric) {
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

    /// Whether a hyphen between two runs is a token of its own
    fn splits(&self, runs: &[Span], joints: &[Joint], index: usize) -> bool {
        let left = runs[index].slice(self.text);
        let right = runs[index + 1].slice(self.text);
        if is_digits(left) && is_digits(right) {
            // A range «2-3», but not a chain of three groups or more: a phone number, an ISO date
            let mut first = index;
            while first > 0
                && joints[first - 1] == Joint::Hyphen
                && is_digits(runs[first - 1].slice(self.text))
            {
                first -= 1;
            }
            let mut last = index + 1;
            while last < joints.len()
                && joints[last] == Joint::Hyphen
                && is_digits(runs[last + 1].slice(self.text))
            {
                last += 1;
            }
            return last - first < 2;
        }
        if is_roman(left) && is_roman(right) {
            return true;
        }
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
        if tables.whole.contains(&format!("{left_key}-{right_key}")) {
            return false;
        }
        if tables.particles.contains(&right_key) {
            return !(right_key == "то" && tables.indefinite.contains(&left_key));
        }
        if !is_letters(left) {
            return false;
        }
        if is_digits(right) {
            // A word with an inflected first part and a number splits («Формулы-1», «Марса-96»), an
            // abbreviation stays whole («Ту-154», «COVID-19»)
            return left.chars().filter(|c| c.is_alphabetic()).count() >= 3
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
        });
        if !joins {
            let end = if self.abbreviation(start, end) {
                end + 1
            } else {
                end
            };
            return self.push(start, end);
        }
        let mut runs = vec![Span { start, end }];
        let mut joints = Vec::new();
        while let Some((joint, next)) = self.joint(&runs, &joints) {
            joints.push(joint);
            runs.push(Span {
                start: next,
                end: self.run(next),
            });
        }
        let mut first = start;
        for index in 0..joints.len() {
            if joints[index] == Joint::Hyphen && self.splits(&runs, &joints, index) {
                self.push(first, runs[index].end);
                self.push(runs[index].end, runs[index + 1].start);
                first = runs[index + 1].start;
            }
        }
        let mut end = runs[runs.len() - 1].end;
        if first == runs[runs.len() - 1].start && self.abbreviation(first, end) {
            end += 1;
        }
        self.push(first, end)
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
                let end = run_of(".!?…");
                self.push(start, end)
            }
            '-' | '*' | '№' | '=' | '~' | '_' | '+' | '#' | '`' | '\'' => {
                let end = run_of(&character.to_string());
                self.push(start, end)
            }
            '(' | ')' => {
                let end = run_of(&character.to_string());
                let length = end - start;
                let emoticon = length >= 2
                    && match character {
                        ')' => self.open == 0,
                        _ => !self
                            .get(end)
                            .is_some_and(|c| c.is_alphanumeric() || c == '('),
                    };
                if emoticon {
                    return self.push(start, end);
                }
                if character == '(' {
                    self.open += 1;
                } else {
                    self.open = self.open.saturating_sub(1);
                }
                self.push(start, next)
            }
            '°' if self.get(next).is_some_and(|c| matches!(c, 'C' | 'С' | 'F'))
                && !self
                    .get(next + 'С'.len_utf8().min(2))
                    .is_some_and(char::is_alphanumeric) =>
            {
                let letter = self.get(next).map_or(0, char::len_utf8);
                self.push(start, next + letter)
            }
            _ => self.push(start, next),
        }
    }
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use super::*;
    use crate::Language;
    use crate::lang::tables;

    fn tokens(text: &str) -> Vec<&str> {
        let mut out = Vec::new();
        tokenize(
            text,
            Span {
                start: 0,
                end: text.len(),
            },
            tables(Language::Russian),
            &Exceptions::default(),
            &mut out,
        );
        out.iter().map(|span| span.slice(text)).collect()
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
}

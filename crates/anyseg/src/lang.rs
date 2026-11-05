//! The data of the languages: conventions and lists in text files built into the binary, read once per language

#![cfg_attr(
    not(any(feature = "ru", feature = "es")),
    allow(dead_code, unused_imports)
)]

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;

use crate::Language;

const COMMON_WEB_SUFFIXES: &str = include_str!("../data/common/web_suffixes.txt");
const COMMON_EMOTICONS: &str = include_str!("../data/common/emoticons.txt");

/// How a language cuts a word at a hyphen
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Hyphens {
    /// Never: «sociedad-estado»
    #[default]
    Whole,
    /// Always, except after the prefixes and in the words of the lists: «al-Qaeda», but «e-mail»
    Split,
    /// When both parts inflect or a particle or a numeral follows: «учеными-энтузиастами», «Вася-то», «два-три»
    Inflected,
}

/// How a language groups the digits of a long number
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DigitGroups {
    /// «3 000», «3 000,5»
    Space,
    /// «1.012,72»
    #[default]
    Point,
    /// «1,000,000.50»
    Comma,
}

/// The alphabet of a language
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Script {
    #[default]
    Latin,
    Cyrillic,
}

impl Script {
    /// Whether a letter belongs to the alphabet
    pub(crate) fn contains(self, letter: char) -> bool {
        match self {
            Script::Latin => letter.is_ascii_alphabetic() || matches!(letter, '\u{c0}'..='\u{24f}'),
            Script::Cyrillic => matches!(letter, '\u{400}'..='\u{52f}'),
        }
    }
}

/// What a language decides about the cuts that are not lists of words
#[derive(Debug)]
pub(crate) struct Conventions {
    pub(crate) hyphens: Hyphens,
    /// The particle after a hyphen that the words of [`Tables::indefinite`] keep
    pub(crate) indefinite_particle: String,
    /// Two groups of digits at a hyphen are a range of three tokens: «2-3»
    pub(crate) digit_ranges: bool,
    /// Two Roman numbers at a hyphen are a range of three tokens: «XIX-XX»
    pub(crate) roman_ranges: bool,
    /// A run of different final marks is one token: «?!»; otherwise a token for each mark: «?», «.»
    pub(crate) mixed_final_marks: bool,
    /// An abbreviation with periods inside it is a token for each part: «т.», «е.»; otherwise one: «EE.UU.»
    pub(crate) split_dotted: bool,
    /// The period of an abbreviation at the end of a sentence is a token of its own: «г», «.»
    pub(crate) final_period_token: bool,
    /// A number with a percent sign and an ending after a hyphen is one word: «20%-ный»
    pub(crate) percent_endings: bool,
    pub(crate) digit_groups: DigitGroups,
    /// The numbers of digits that make groups separated by spaces a phone number
    pub(crate) phone_digits: Option<(usize, usize)>,
    pub(crate) script: Script,
    /// Marks that are not part of a word in the lists, after canonical decomposition
    fold_marks: Vec<char>,
    /// Letters that make one token with a degree sign
    pub(crate) degree_letters: String,
    /// Marks that a verb loses when enclitics join it, after canonical decomposition
    pub(crate) enclitic_host_marks: Vec<char>,
    /// Letters of which a verb that takes enclitics has one before its ending; none: any verb
    pub(crate) enclitic_stem_letters: Vec<char>,
    /// A token with a first capital letter after a word or a comma is a name, which stays whole
    pub(crate) capital_after_word_whole: bool,
}

impl Default for Conventions {
    fn default() -> Self {
        Conventions {
            hyphens: Hyphens::default(),
            indefinite_particle: String::new(),
            digit_ranges: false,
            roman_ranges: false,
            mixed_final_marks: true,
            split_dotted: false,
            final_period_token: true,
            percent_endings: false,
            digit_groups: DigitGroups::default(),
            phone_digits: None,
            script: Script::default(),
            fold_marks: Vec::new(),
            degree_letters: "CF".to_owned(),
            enclitic_host_marks: Vec::new(),
            enclitic_stem_letters: Vec::new(),
            capital_after_word_whole: false,
        }
    }
}

fn wrong<T>(line: &str) -> T {
    panic!("wrong value of the convention {line:?}")
}

impl Conventions {
    /// Whether the keys of the lists keep all marks
    pub(crate) fn folds_nothing(&self) -> bool {
        self.fold_marks.is_empty()
    }

    /// The conventions in a file of `key = value` lines; what a file leaves out takes the default. The files are
    /// part of the build, and a test reads each of them
    fn read(text: &str) -> Self {
        let mut conventions = Conventions::default();
        for line in lines(text) {
            let (key, value) = line
                .split_once('=')
                .map(|(key, value)| (key.trim(), value.trim()))
                .unwrap_or_else(|| panic!("no `=` in the convention {line:?}"));
            let flag = |yes: &str, no: &str| {
                if value == yes {
                    true
                } else if value == no {
                    false
                } else {
                    wrong(line)
                }
            };
            match key {
                "hyphens" => {
                    conventions.hyphens = match value {
                        "whole" => Hyphens::Whole,
                        "split" => Hyphens::Split,
                        "inflected" => Hyphens::Inflected,
                        _ => wrong(line),
                    }
                }
                "indefinite_particle" => conventions.indefinite_particle = value.to_owned(),
                "digit_ranges" => conventions.digit_ranges = flag("split", "whole"),
                "roman_ranges" => conventions.roman_ranges = flag("split", "whole"),
                "final_marks" => conventions.mixed_final_marks = flag("mixed", "same"),
                "dotted" => conventions.split_dotted = flag("split", "whole"),
                "final_period" => conventions.final_period_token = flag("token", "inside"),
                "percent_endings" => conventions.percent_endings = flag("yes", "no"),
                "digit_groups" => {
                    conventions.digit_groups = match value {
                        "space" => DigitGroups::Space,
                        "point" => DigitGroups::Point,
                        "comma" => DigitGroups::Comma,
                        _ => wrong(line),
                    }
                }
                "phone_digits" => {
                    conventions.phone_digits = (value != "none").then(|| {
                        value
                            .split_once('-')
                            .and_then(|(low, high)| Some((low.parse().ok()?, high.parse().ok()?)))
                            .unwrap_or_else(|| wrong(line))
                    });
                }
                "script" => {
                    conventions.script = match value {
                        "latin" => Script::Latin,
                        "cyrillic" => Script::Cyrillic,
                        _ => wrong(line),
                    }
                }
                "fold_marks" | "enclitic_host_marks" => {
                    let marks = value
                        .split_whitespace()
                        .map(|mark| {
                            mark.strip_prefix("U+")
                                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                                .and_then(char::from_u32)
                                .unwrap_or_else(|| wrong(line))
                        })
                        .collect();
                    if key == "fold_marks" {
                        conventions.fold_marks = marks;
                    } else {
                        conventions.enclitic_host_marks = marks;
                    }
                }
                "degree_letters" => conventions.degree_letters = value.to_owned(),
                "enclitic_stem_letters" => {
                    conventions.enclitic_stem_letters = value
                        .split_whitespace()
                        .filter_map(|letter| letter.chars().next())
                        .collect();
                }
                "capital_after_word" => {
                    conventions.capital_after_word_whole = flag("whole", "words");
                }
                _ => panic!("unknown convention {line:?}"),
            }
        }
        conventions
    }
}

/// The words of a token that holds several
#[derive(Debug)]
pub(crate) struct Multiword {
    pub(crate) words: Vec<String>,
    /// The token may be a name: it stays whole in capitals after a word as well
    pub(crate) may_be_name: bool,
}

/// The files of a language; what a language has no use for stays empty
#[derive(Clone, Copy, Default)]
struct Files {
    conventions: &'static str,
    abbreviations: &'static str,
    not_abbreviations: &'static str,
    particles: &'static str,
    indefinite: &'static str,
    whole: &'static str,
    numerals: &'static str,
    prefixes: &'static str,
    endings: &'static str,
    linkers: &'static str,
    final_abbreviations: &'static str,
    dotted_names: &'static str,
    homoglyphs: &'static str,
    web_suffixes: &'static str,
    emoticons: &'static str,
    multiword: &'static str,
    multiword_names: &'static str,
    multiword_exceptions: &'static str,
    enclitics: &'static str,
    enclitic_hosts: &'static str,
    enclitic_exceptions: &'static str,
}

/// The conventions and the lists of a language that the mechanism consults
#[derive(Debug)]
pub(crate) struct Tables {
    pub(crate) conventions: Conventions,
    /// Words that keep a period after them inside the token
    pub(crate) abbreviations: HashSet<String>,
    /// Lowercase single letters that never take such a period
    pub(crate) not_abbreviations: HashSet<String>,
    /// Particles after a hyphen that are tokens of their own
    pub(crate) particles: HashSet<String>,
    /// Words that keep the particle «-то» of an indefinite word
    pub(crate) indefinite: HashSet<String>,
    /// Hyphenated words that stay whole whatever the rules say
    pub(crate) whole: HashSet<String>,
    /// The number of parts of the longest word of `whole`
    pub(crate) whole_parts: usize,
    /// Numerals: two of them, or a word and a numeral, split at a hyphen
    pub(crate) numerals: HashSet<String>,
    /// First parts that do not inflect: their hyphenated words stay whole
    pub(crate) prefixes: HashSet<String>,
    /// Letters that end inflected forms
    pub(crate) endings: HashSet<String>,
    /// Words between two hyphens that split a name at both: «Ростова-на-Дону»
    pub(crate) linkers: HashSet<String>,
    /// Abbreviations, as the keys of their words joined by spaces, that end a sentence before a capital letter
    pub(crate) final_abbreviations: HashSet<String>,
    /// Names joined to the next word by a period: «Яндекс.Маркет»
    pub(crate) dotted_names: HashSet<String>,
    /// Latin letters that stand for letters of the language in its words
    homoglyphs: HashMap<char, char>,
    /// Top-level domains and file extensions that make a name with a period one token
    pub(crate) web_suffixes: HashSet<String>,
    /// Emoticons that are not a face of eyes and a mouth
    pub(crate) emoticons: Vec<String>,
    /// Whether an emoticon starts with a byte
    emoticon_starts: [bool; 256],
    /// The lengths of the emoticons in bytes, as bits
    emoticon_lengths: u64,
    /// The tokens that hold several words
    pub(crate) multiword: HashMap<String, Multiword>,
    /// Tokens of `multiword` that stay whole, and the words after which they do
    pub(crate) multiword_exceptions: HashMap<String, Vec<String>>,
    /// Pronouns that join the end of a verb, the longest first
    pub(crate) enclitics: Vec<String>,
    /// What a verb that takes enclitics ends with after a stem letter, without the marks it loses
    pub(crate) enclitic_endings: Vec<String>,
    /// The same with a mark of those: the verb keeps it
    pub(crate) enclitic_marked_endings: Vec<String>,
    /// What no verb that takes enclitics ends with
    pub(crate) enclitic_refused_endings: Vec<String>,
    /// Whole verbs that take enclitics
    pub(crate) enclitic_verbs: HashSet<String>,
    /// Words that look like a verb with enclitics
    pub(crate) enclitic_exceptions: HashSet<String>,
}

/// A word lowercase, without the marks of the language and soft hyphens
fn fold(word: &str, marks: &[char]) -> String {
    if word.chars().all(plain) {
        return word.chars().flat_map(char::to_lowercase).collect();
    }
    normalized_fold(word, marks)
}

/// Words of ASCII and the Russian alphabet fold without normalization, unless a language drops the marks of «й», «ё»
fn plain(character: char) -> bool {
    character.is_ascii() || matches!(character, 'А'..='я' | 'Ё' | 'ё')
}

fn normalized_fold(word: &str, marks: &[char]) -> String {
    word.nfd()
        .filter(|character| *character != '\u{ad}' && !marks.contains(character))
        .nfc()
        .flat_map(char::to_lowercase)
        .collect()
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

impl Tables {
    fn new(files: Files) -> Self {
        let conventions = Conventions::read(files.conventions);
        let marks = conventions.fold_marks.clone();
        let list = |text: &str| -> HashSet<String> {
            lines(text).map(|line| fold(line, &marks)).collect()
        };
        let emoticons: Vec<String> = lines(COMMON_EMOTICONS)
            .chain(lines(files.emoticons))
            .map(str::to_owned)
            .collect();
        let mut emoticon_starts = [false; 256];
        let mut emoticon_lengths = 0;
        for emoticon in &emoticons {
            emoticon_starts[usize::from(emoticon.as_bytes()[0])] = true;
            emoticon_lengths |= 1 << emoticon.len();
        }
        let whole = list(files.whole);
        let mut enclitics: Vec<String> = lines(files.enclitics).map(str::to_owned).collect();
        enclitics.sort_by_key(|enclitic| std::cmp::Reverse(enclitic.len()));
        let hosts = || lines(files.enclitic_hosts);
        let marked = |ending: &&str| {
            ending
                .nfd()
                .any(|character| conventions.enclitic_host_marks.contains(&character))
        };
        let endings: Vec<&str> = hosts().filter_map(|host| host.strip_prefix('-')).collect();
        let mut multiword_exceptions: HashMap<String, Vec<String>> = HashMap::new();
        for line in lines(files.multiword_exceptions) {
            if let Some((word, token)) = line.split_once(char::is_whitespace) {
                multiword_exceptions
                    .entry(fold(token.trim(), &marks))
                    .or_default()
                    .push(fold(word, &marks));
            }
        }
        let table = |text: &'static str, may_be_name: bool| {
            let marks = &marks;
            lines(text).map(move |line| {
                let mut parts = line.split_whitespace().map(str::to_owned);
                let token = parts.next().unwrap_or_default();
                let words = parts.collect();
                (fold(&token, marks), Multiword { words, may_be_name })
            })
        };
        Tables {
            multiword_exceptions,
            multiword: table(files.multiword, false)
                .chain(table(files.multiword_names, true))
                .collect(),
            enclitics,
            enclitic_endings: endings
                .iter()
                .filter(|ending| !marked(ending))
                .map(|ending| (*ending).to_owned())
                .collect(),
            enclitic_marked_endings: endings
                .iter()
                .filter(|ending| marked(ending))
                .map(|ending| (*ending).to_owned())
                .collect(),
            enclitic_refused_endings: hosts()
                .filter_map(|host| host.strip_prefix('!'))
                .map(str::to_owned)
                .collect(),
            enclitic_verbs: hosts()
                .filter(|host| !host.starts_with(['-', '!']))
                .map(str::to_owned)
                .collect(),
            enclitic_exceptions: list(files.enclitic_exceptions),
            abbreviations: list(files.abbreviations),
            not_abbreviations: list(files.not_abbreviations),
            particles: list(files.particles),
            indefinite: list(files.indefinite),
            whole_parts: whole
                .iter()
                .map(|word| word.matches('-').count() + 1)
                .max()
                .unwrap_or(0),
            whole,
            numerals: list(files.numerals),
            prefixes: list(files.prefixes),
            endings: list(files.endings),
            linkers: list(files.linkers),
            final_abbreviations: lines(files.final_abbreviations)
                .map(|line| {
                    line.split(|c: char| c == '.' || c.is_whitespace())
                        .filter(|word| !word.is_empty())
                        .map(|word| fold(word, &marks))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect(),
            dotted_names: list(files.dotted_names),
            homoglyphs: lines(files.homoglyphs)
                .filter_map(|line| {
                    let mut chars = line
                        .split_whitespace()
                        .filter_map(|part| part.chars().next());
                    Some((chars.next()?, chars.next()?))
                })
                .collect(),
            web_suffixes: list(COMMON_WEB_SUFFIXES)
                .into_iter()
                .chain(list(files.web_suffixes))
                .collect(),
            emoticons,
            emoticon_starts,
            emoticon_lengths,
            conventions,
        }
    }

    /// Whether an emoticon of the lists can start with this byte
    pub(crate) fn starts_emoticon(&self, byte: u8) -> bool {
        self.emoticon_starts[usize::from(byte)]
    }

    /// Whether a token is an emoticon of the lists; decided by its first byte and its length first, as it runs for
    /// every token
    #[inline]
    pub(crate) fn is_listed_emoticon(&self, piece: &str) -> bool {
        let bytes = piece.as_bytes();
        bytes
            .first()
            .is_some_and(|&first| self.emoticon_starts[usize::from(first)])
            && bytes.len() < 64
            && self.emoticon_lengths >> bytes.len() & 1 == 1
            && self.emoticons.iter().any(|emoticon| emoticon == piece)
    }

    /// The key of a word in the lists: lowercase, without the marks of the language and soft hyphens, and, in a word
    /// with letters other than Latin ones, with the Latin letters that stand for letters of the language replaced
    pub(crate) fn key(&self, word: &str) -> String {
        let folded = fold(word, &self.conventions.fold_marks);
        if self.homoglyphs.is_empty()
            || !folded
                .chars()
                .any(|character| character.is_alphabetic() && !character.is_ascii())
        {
            return folded;
        }
        folded
            .chars()
            .map(|character| *self.homoglyphs.get(&character).unwrap_or(&character))
            .collect()
    }
}

/// The tables of a language, read on the first call
pub(crate) fn tables(language: Language) -> &'static Tables {
    match language {
        #[cfg(feature = "ru")]
        Language::Russian => {
            static RUSSIAN: OnceLock<Tables> = OnceLock::new();
            RUSSIAN.get_or_init(|| {
                Tables::new(Files {
                    conventions: include_str!("../data/ru/conventions.txt"),
                    abbreviations: include_str!("../data/ru/abbreviations.txt"),
                    not_abbreviations: include_str!("../data/ru/not_abbreviations.txt"),
                    particles: include_str!("../data/ru/hyphen_particles.txt"),
                    indefinite: include_str!("../data/ru/indefinite.txt"),
                    whole: include_str!("../data/ru/hyphen_whole.txt"),
                    numerals: include_str!("../data/ru/numerals.txt"),
                    prefixes: include_str!("../data/ru/hyphen_prefixes.txt"),
                    endings: include_str!("../data/ru/inflection_endings.txt"),
                    linkers: include_str!("../data/ru/hyphen_linkers.txt"),
                    final_abbreviations: include_str!("../data/ru/final_abbreviations.txt"),
                    dotted_names: include_str!("../data/ru/dotted_names.txt"),
                    homoglyphs: include_str!("../data/ru/homoglyphs.txt"),
                    web_suffixes: include_str!("../data/ru/web_suffixes.txt"),
                    emoticons: include_str!("../data/ru/emoticons.txt"),
                    ..Files::default()
                })
            })
        }
        #[cfg(feature = "es")]
        Language::Spanish => {
            static SPANISH: OnceLock<Tables> = OnceLock::new();
            SPANISH.get_or_init(|| {
                Tables::new(Files {
                    conventions: include_str!("../data/es/conventions.txt"),
                    multiword: include_str!("../data/es/multiword.txt"),
                    multiword_names: include_str!("../data/es/multiword_names.txt"),
                    multiword_exceptions: include_str!("../data/es/multiword_exceptions.txt"),
                    enclitics: include_str!("../data/es/enclitics.txt"),
                    enclitic_hosts: include_str!("../data/es/enclitic_hosts.txt"),
                    enclitic_exceptions: include_str!("../data/es/enclitic_exceptions.txt"),
                    ..Files::default()
                })
            })
        }
    }
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        let tables = tables(Language::Russian);
        assert_eq!(tables.key("Чтó"), "что");
        assert_eq!(tables.key("Ку\u{301}\u{ad}рица"), "курица");
        assert_eq!(tables.key("Ёлка й"), "ёлка й");
        assert_eq!(tables.key("Com"), "com");
        assert!(tables.final_abbreviations.contains("т д"));
    }

    #[test]
    fn plain_words_fold_without_normalization() {
        let characters: Vec<char> = (0..=0x10ffff)
            .filter_map(char::from_u32)
            .filter(|&character| plain(character))
            .collect();
        for &language in Language::ALL {
            let marks = &tables(language).conventions.fold_marks;
            for &first in &characters {
                let word = first.to_string();
                assert_eq!(
                    fold(&word, marks),
                    normalized_fold(&word, marks),
                    "{word:?}"
                );
                for &second in &characters {
                    let word = format!("{first}{second}");
                    assert_eq!(
                        fold(&word, marks),
                        normalized_fold(&word, marks),
                        "{word:?}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod conventions {
    use super::*;
    use crate::tokens::Exceptions;

    fn language(conventions: &'static str) -> Tables {
        Tables::new(Files {
            conventions,
            ..Files::default()
        })
    }

    fn tokens<'a>(tables: &Tables, text: &'a str) -> Vec<&'a str> {
        crate::split_text(text, tables, &Exceptions::default(), false)
            .tokens
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    fn sentence_count(tables: &Tables, text: &str) -> usize {
        crate::split_text(text, tables, &Exceptions::default(), false)
            .starts
            .len()
    }

    #[test]
    fn hyphens() {
        assert_eq!(
            tokens(&language(""), "sociedad-estado Wi-Fi"),
            ["sociedad-estado", "Wi-Fi"]
        );
        let tables = Tables::new(Files {
            conventions: "hyphens = split",
            prefixes: "e\nnon",
            whole: "wi-fi",
            ..Files::default()
        });
        assert_eq!(
            tokens(&tables, "al-Qaeda e-mail Wi-Fi non-human year-old"),
            [
                "al",
                "-",
                "Qaeda",
                "e-mail",
                "Wi-Fi",
                "non-human",
                "year",
                "-",
                "old"
            ]
        );
    }

    #[test]
    fn whole_words_of_several_parts() {
        let whole = "mother-in-law\nsars-cov-2\nwi-fi";
        for conventions in ["hyphens = split", "hyphens = inflected"] {
            let tables = Tables::new(Files {
                conventions,
                whole,
                numerals: "in\nlaw\ncov",
                ..Files::default()
            });
            assert_eq!(
                tokens(&tables, "mother-in-law SARS-CoV-2 Wi-Fi"),
                ["mother-in-law", "SARS-CoV-2", "Wi-Fi"],
                "{conventions}"
            );
        }
        let tables = Tables::new(Files {
            conventions: "hyphens = split",
            whole,
            ..Files::default()
        });
        // A part of a listed word and a chain around one are not the word
        assert_eq!(
            tokens(&tables, "in-law step-mother-in-law"),
            [
                "in", "-", "law", "step", "-", "mother", "-", "in", "-", "law"
            ]
        );
        // A linear scan: a quadratic one takes seconds
        let long = format!("{}a", "a-".repeat(40_000));
        assert_eq!(tokens(&tables, &long).concat(), long);
    }

    #[test]
    fn ranges() {
        assert_eq!(
            tokens(&language(""), "6-4 1.5-2.5 713-654-0365 XIX-XX"),
            ["6-4", "1.5-2.5", "713-654-0365", "XIX-XX"]
        );
        assert_eq!(
            tokens(
                &language("digit_ranges = split\nroman_ranges = split"),
                "6-4 1.5-2.5 713-654-0365 XIX-XX"
            ),
            [
                "6",
                "-",
                "4",
                "1.5",
                "-",
                "2.5",
                "713-654-0365",
                "XIX",
                "-",
                "XX"
            ]
        );
    }

    #[test]
    fn final_marks() {
        assert_eq!(
            tokens(&language(""), "Ya?! Qué?."),
            ["Ya", "?!", "Qué", "?."]
        );
        let tables = language("final_marks = same");
        let text = "Ya?! Qué?. No!!! Sí... Bien";
        assert_eq!(
            tokens(&tables, text),
            [
                "Ya", "?", "!", "Qué", "?", ".", "No", "!!!", "Sí", "...", "Bien"
            ]
        );
        assert_eq!(sentence_count(&tables, text), 5);
    }

    #[test]
    fn dotted_abbreviations() {
        assert_eq!(
            tokens(
                &language(""),
                "EE.UU. y S.A., Ph.D. en a.m. o E.E.UU y A.B... Fin.la"
            ),
            [
                "EE.UU.", "y", "S.A.", ",", "Ph.D.", "en", "a.m.", "o", "E.E.UU", "y", "A.B",
                "...", "Fin", ".", "la"
            ]
        );
        assert_eq!(
            tokens(&language("dotted = split"), "S.A. y"),
            ["S.", "A.", "y"]
        );
        // A sentence glued to a short word is not an abbreviation
        assert_eq!(
            tokens(&language(""), "Lo vi.Yo no, it.He, no.Y, de.la"),
            [
                "Lo", "vi", ".", "Yo", "no", ",", "it", ".", "He", ",", "no", ".", "Y", ",", "de",
                ".", "la"
            ]
        );
        assert_eq!(sentence_count(&language(""), "Lo vi.Yo no"), 2);
        assert_eq!(
            tokens(&language(""), "EE.UU.La casa, U.S.He said"),
            ["EE.UU.", "La", "casa", ",", "U.S.", "He", "said"]
        );
        // A part with capitals, a final period and a part that is no word are an abbreviation
        assert_eq!(
            tokens(
                &language(""),
                "a Ph.D in math, Mr.T y ee.uu. o cc.oo. s.XX x"
            ),
            [
                "a", "Ph.D", "in", "math", ",", "Mr.T", "y", "ee.uu.", "o", "cc.oo.", "s.XX", "x"
            ]
        );
        assert_eq!(sentence_count(&language(""), "a Ph.D in math"), 1);
        assert_eq!(
            tokens(&language(""), "M.Sc. p.ej. d.C. LL.D. K.O. x"),
            ["M.Sc.", "p.ej.", "d.C.", "LL.D.", "K.O.", "x"]
        );
        // After a hyphen, in one token or after the token of the hyphen
        assert_eq!(
            tokens(&language(""), "Un grupo pro-EE.UU. llegó"),
            ["Un", "grupo", "pro-EE.UU.", "llegó"]
        );
        assert_eq!(
            sentence_count(&language(""), "Un grupo pro-EE.UU. llegó"),
            1
        );
        assert_eq!(
            tokens(&language("hyphens = split"), "anti-U.S. policy"),
            ["anti", "-", "U.S.", "policy"]
        );
        // A linear scan: a quadratic one takes seconds
        let long = "a.".repeat(40_000);
        assert_eq!(tokens(&language(""), &long).concat(), long);
    }

    #[test]
    fn final_period() {
        let text = "Vino de EE.UU. Fue a EE.UU.";
        assert_eq!(
            tokens(&language(""), text),
            ["Vino", "de", "EE.UU.", "Fue", "a", "EE.UU", "."]
        );
        assert_eq!(
            tokens(&language("final_period = inside"), text),
            ["Vino", "de", "EE.UU.", "Fue", "a", "EE.UU."]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            tokens(&language(""), "1.012,72 16.877.402,99 1,5,3 3 000 20%-ish"),
            [
                "1.012,72",
                "16.877.402,99",
                "1,5",
                ",",
                "3",
                "3",
                "000",
                "20",
                "%",
                "-",
                "ish"
            ]
        );
        assert_eq!(
            tokens(&language("digit_groups = space"), "3 000,5 1.012,72"),
            ["3 000,5", "1.012", ",", "72"]
        );
        assert_eq!(
            tokens(
                &language("digit_groups = comma"),
                "1,000,000.50 3 000 1.5.2"
            ),
            ["1,000,000.50", "3", "000", "1.5.2"]
        );
        assert_eq!(
            tokens(&language("percent_endings = yes"), "20%-ish"),
            ["20%-ish"]
        );
        // Commas between numbers and a comma after a group that is not of three digits are not groups
        assert_eq!(
            tokens(&language("digit_groups = comma"), "18,19,20 7,5 1,000,5"),
            ["18", ",", "19", ",", "20", "7", ",", "5", "1,000", ",", "5"]
        );
        assert_eq!(
            tokens(&language(""), "3.5,4.5 12.05,7 1.012.345,6"),
            ["3.5", ",", "4.5", "12.05", ",", "7", "1.012.345,6"]
        );
    }

    #[test]
    fn phones() {
        let text = "8 800 555 35 35";
        assert_eq!(
            tokens(&language("digit_groups = space"), text),
            ["8 800 555", "35", "35"]
        );
        assert_eq!(
            tokens(
                &language("digit_groups = space\nphone_digits = 10-11"),
                text
            ),
            ["8", "800", "555", "35", "35"]
        );
        assert_eq!(
            tokens(&language("digit_groups = space\nphone_digits = none"), text),
            ["8 800 555", "35", "35"]
        );
    }

    #[test]
    fn units_after_numbers() {
        let count =
            |conventions: &'static str, text: &str| sentence_count(&language(conventions), text);
        assert_eq!(count("digit_groups = space", "Hay 1 000 000 A. Luego"), 2);
        assert_eq!(count("", "Hay 1012,72 A. Luego"), 2);
        assert_eq!(count("", "Hay 1.012,72 A. Luego"), 2);
        assert_eq!(count("", "Hay 1.000.000 A. Luego"), 2);
        assert_eq!(count("digit_groups = comma", "Hay 1,000,000 A. Luego"), 2);
        assert_eq!(count("digit_groups = comma", "Hay 1,000.50 A. Luego"), 2);
        // A date is not a quantity: the letter after it is an initial
        assert_eq!(count("", "El 12.05.2009 A. Luego"), 1);
        assert_eq!(count("digit_groups = space", "El 12.05.2009 A. Luego"), 1);
    }

    #[test]
    fn final_abbreviations_with_capitals() {
        let tables = Tables::new(Files {
            final_abbreviations: "EE.UU.\na.m.",
            ..Files::default()
        });
        assert_eq!(sentence_count(&tables, "Vino a.m. Fue"), 2);
        assert_eq!(sentence_count(&tables, "Vino de EE.UU. Fue"), 2);
        assert_eq!(sentence_count(&language(""), "Vino de EE.UU. Fue"), 1);
    }

    #[test]
    fn scripts() {
        let latin = language("");
        assert_eq!(sentence_count(&latin, "Vitamina β. Entonces"), 2);
        assert_eq!(sentence_count(&latin, "A través de x. Entonces"), 1);
        let cyrillic = language("script = cyrillic");
        assert_eq!(sentence_count(&cyrillic, "через x. Тогда"), 2);
        assert_eq!(sentence_count(&latin, "через x. Тогда"), 1);
    }

    #[test]
    fn marks_of_the_keys() {
        assert_eq!(language("").key("Más\u{ad}"), "más");
        assert_eq!(language("").key("Ma\u{301}s"), "más");
        assert_eq!(language("fold_marks = U+0301").key("Más"), "mas");
    }

    #[test]
    fn degrees() {
        assert_eq!(
            tokens(&language(""), "20°C 20°С"),
            ["20", "°C", "20", "°", "С"]
        );
        assert_eq!(
            tokens(&language("degree_letters = CFС"), "20°С"),
            ["20", "°С"]
        );
    }

    #[test]
    fn lists_of_the_language() {
        let neutral = language("");
        assert_eq!(
            tokens(&neutral, "example.com сайт.рф"),
            ["example.com", "сайт", ".", "рф"]
        );
        assert_eq!(sentence_count(&neutral, "Hola <3 Adiós"), 2);
        assert_eq!(sentence_count(&neutral, "Hola о_О Adiós"), 1);
        let tables = Tables::new(Files {
            web_suffixes: "рф",
            emoticons: "о_О",
            ..Files::default()
        });
        assert_eq!(tokens(&tables, "сайт.рф"), ["сайт.рф"]);
        assert_eq!(sentence_count(&tables, "Hola о_О Adiós"), 2);
        // Whatever byte an emoticon of the lists starts with
        let tables = Tables::new(Files {
            emoticons: "(^_^)\n>_<\n⌐■_■",
            ..Files::default()
        });
        for text in ["Hola >_< Adiós", "Hola (^_^) Adiós", "Hola ⌐■_■ Adiós"] {
            assert_eq!(sentence_count(&tables, text), 2, "{text}");
        }
    }

    #[test]
    fn multiword_tokens_are_data() {
        use crate::words::expand;
        let words = |list: &[&str]| Some(list.iter().map(|&word| word.to_owned()).collect());
        let tables = Tables::new(Files {
            multiword: "i'm i 'm\n",
            enclitics: "ся\n",
            enclitic_hosts: "-ть\n",
            ..Files::default()
        });
        assert_eq!(
            expand(&tables, "мыться", None, || None),
            words(&["мыть", "ся"])
        );
        assert_eq!(
            expand(&tables, "I'm", Some("word"), || None),
            words(&["I", "'m"])
        );
        assert_eq!(
            expand(&tables, "Мыться", Some("word"), || None),
            words(&["Мыть", "ся"])
        );
        let tables = Tables::new(Files {
            conventions: "capital_after_word = whole\nenclitic_stem_letters = ы\n",
            enclitics: "ся\n",
            enclitic_hosts: "-ть\n!кать\n",
            multiword_exceptions: "et i'm\n",
            multiword: "i'm i 'm\n",
            ..Files::default()
        });
        assert_eq!(expand(&tables, "I'm", Some("word"), || None), None);
        assert_eq!(expand(&tables, "I'm", Some(","), || None), None);
        assert_eq!(
            expand(&tables, "I'm", Some("«"), || None),
            words(&["I", "'m"])
        );
        assert_eq!(expand(&tables, "i'm", Some("ET"), || None), None);
        assert_eq!(
            expand(&tables, "мыться", None, || None),
            words(&["мыть", "ся"])
        );
        assert_eq!(expand(&tables, "мыкаться", None, || None), None);
        assert_eq!(expand(&tables, "мчаться", None, || None), None);
    }

    #[test]
    #[should_panic(expected = "unknown convention")]
    fn unknown_convention() {
        language("hyphen = whole");
    }

    #[test]
    #[should_panic(expected = "wrong value")]
    fn wrong_value() {
        language("hyphens = sometimes");
    }

    #[test]
    #[should_panic(expected = "no `=`")]
    fn no_value() {
        language("hyphens");
    }

    #[test]
    fn languages_read() {
        for &language in Language::ALL {
            tables(language);
        }
    }
}

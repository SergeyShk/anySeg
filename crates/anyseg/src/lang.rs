//! The data of the languages: lists in text files built into the binary, read once per language

#![cfg_attr(not(feature = "ru"), allow(dead_code, unused_imports))]

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;

use crate::Language;

/// The lists of a language that the tokenizer consults
#[derive(Debug, Default)]
pub(crate) struct Tables {
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
    /// Latin letters that stand for letters of the language in its words
    homoglyphs: HashMap<char, char>,
}

/// A word lowercase, without stress marks (U+0300, U+0301) and soft hyphens
fn fold(word: &str) -> String {
    if word.chars().all(plain) {
        return word.chars().flat_map(char::to_lowercase).collect();
    }
    normalized_fold(word)
}

/// ASCII and the Russian alphabet have no marks to drop: words of them fold without normalization
fn plain(character: char) -> bool {
    character.is_ascii() || matches!(character, 'А'..='я' | 'Ё' | 'ё')
}

fn normalized_fold(word: &str) -> String {
    word.nfd()
        .filter(|&character| !matches!(character, '\u{300}' | '\u{301}' | '\u{ad}'))
        .nfc()
        .flat_map(char::to_lowercase)
        .collect()
}

fn list(text: &str) -> HashSet<String> {
    lines(text).map(fold).collect()
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

impl Tables {
    fn new(files: [&str; 11]) -> Self {
        let [
            abbreviations,
            not_abbreviations,
            particles,
            indefinite,
            whole,
            numerals,
            prefixes,
            endings,
            linkers,
            final_abbreviations,
            homoglyphs,
        ] = files;
        Tables {
            abbreviations: list(abbreviations),
            not_abbreviations: list(not_abbreviations),
            particles: list(particles),
            indefinite: list(indefinite),
            whole: list(whole),
            numerals: list(numerals),
            prefixes: list(prefixes),
            endings: list(endings),
            linkers: list(linkers),
            final_abbreviations: lines(final_abbreviations)
                .map(|line| {
                    line.split(|c: char| c == '.' || c.is_whitespace())
                        .filter(|word| !word.is_empty())
                        .map(fold)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect(),
            homoglyphs: lines(homoglyphs)
                .filter_map(|line| {
                    let mut chars = line
                        .split_whitespace()
                        .filter_map(|part| part.chars().next());
                    Some((chars.next()?, chars.next()?))
                })
                .collect(),
        }
    }

    /// The key of a word in the lists: lowercase, without stress marks and soft hyphens, and, in a word with
    /// letters other than Latin ones, with the Latin letters that stand for letters of the language replaced
    pub(crate) fn key(&self, word: &str) -> String {
        let folded = fold(word);
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
                Tables::new([
                    include_str!("../data/ru/abbreviations.txt"),
                    include_str!("../data/ru/not_abbreviations.txt"),
                    include_str!("../data/ru/hyphen_particles.txt"),
                    include_str!("../data/ru/indefinite.txt"),
                    include_str!("../data/ru/hyphen_whole.txt"),
                    include_str!("../data/ru/numerals.txt"),
                    include_str!("../data/ru/hyphen_prefixes.txt"),
                    include_str!("../data/ru/inflection_endings.txt"),
                    include_str!("../data/ru/hyphen_linkers.txt"),
                    include_str!("../data/ru/final_abbreviations.txt"),
                    include_str!("../data/ru/homoglyphs.txt"),
                ])
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
        for &first in &characters {
            let word = first.to_string();
            assert_eq!(fold(&word), normalized_fold(&word), "{word:?}");
            for &second in &characters {
                let word = format!("{first}{second}");
                assert_eq!(fold(&word), normalized_fold(&word), "{word:?}");
            }
        }
    }
}

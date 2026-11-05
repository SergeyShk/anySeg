//! Multiword tokens: the words of a token that holds several, from the lists of a language

use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

use crate::lang::Tables;

/// Whether a language has tokens of several words
pub(crate) fn has_multiword(tables: &Tables) -> bool {
    !tables.multiword.is_empty() || !tables.enclitics.is_empty()
}

/// The words of a token that holds several, in the case of the token. `after_word` tells that a word stands before it
/// in its sentence: a token with a first capital letter there is a name, «Al Gore», «Bonaparte». Most tokens hold one
/// word and a lowercase one is its own key, so it runs without a copy for every token
pub(crate) fn expand(
    tables: &Tables,
    token: &str,
    after_word: impl FnOnce() -> bool,
) -> Option<Vec<String>> {
    let upper = token.starts_with(char::is_uppercase);
    if !upper && !token.starts_with(char::is_lowercase) {
        return None;
    }
    let capitals = upper && !token.contains(char::is_lowercase);
    if upper && !capitals && after_word() {
        return None;
    }
    let key = key(tables, token);
    let words = match tables.multiword.get(key.as_ref()) {
        Some(words) => words.clone(),
        None => enclitic_words(tables, &key)?,
    };
    Some(
        words
            .into_iter()
            .enumerate()
            .map(|(index, word)| cased(word, index, upper, capitals))
            .collect(),
    )
}

/// The key of a token in the lists: the token itself if it is lowercase letters of Latin-1, which are in NFC
fn key<'a>(tables: &Tables, token: &'a str) -> Cow<'a, str> {
    if tables.conventions.folds_nothing()
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || matches!(c, '\u{df}'..='\u{ff}') && c != '\u{f7}')
    {
        Cow::Borrowed(token)
    } else {
        Cow::Owned(tables.key(token))
    }
}

/// A word in the case of its token: capitals in all the words, a first capital letter in the first
fn cased(word: String, index: usize, upper: bool, capitals: bool) -> String {
    if capitals {
        return word.to_uppercase();
    }
    if !upper || index > 0 {
        return word;
    }
    let mut letters = word.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => word,
    }
}

/// A verb and one or two pronouns that joined its end. The pronouns have no marks, so a final «é» is no «e» of «te»:
/// «desperté»; the verb loses the marks that it took for them
fn enclitic_words(tables: &Tables, key: &str) -> Option<Vec<String>> {
    let peel = |word: &str| -> Option<usize> {
        let found = tables
            .enclitics
            .iter()
            .find(|enclitic| word.ends_with(enclitic.as_str()))?;
        Some(word.len() - found.len())
    };
    let last = peel(key)?;
    let words = if let Some(verb) = host(tables, &key[..last]) {
        vec![verb.into_owned(), key[last..].to_owned()]
    } else {
        let first = peel(&key[..last])?;
        let verb = host(tables, &key[..first])?;
        vec![
            verb.into_owned(),
            key[first..last].to_owned(),
            key[last..].to_owned(),
        ]
    };
    (!tables.enclitic_exceptions.contains(key) && key.chars().all(char::is_alphabetic))
        .then_some(words)
}

/// The verb before enclitics, without the marks that it took for them: a whole verb of the list or an ending after
/// a vowel
fn host<'a>(tables: &Tables, word: &'a str) -> Option<Cow<'a, str>> {
    let marks = &tables.conventions.enclitic_host_marks;
    let verb = if word.is_ascii() {
        Cow::Borrowed(word)
    } else {
        Cow::Owned(
            word.nfd()
                .filter(|character| !marks.contains(character))
                .nfc()
                .collect(),
        )
    };
    let takes = tables.enclitic_verbs.contains(verb.as_ref())
        || tables.enclitic_endings.iter().any(|ending| {
            verb.strip_suffix(ending.as_str())
                .is_some_and(|stem| stem.contains(['a', 'e', 'i', 'o', 'u', 'y']))
        });
    takes.then_some(verb)
}

#[cfg(all(test, feature = "es"))]
mod tests {
    use crate::{Column, Document, Segmenter, read_conllu};

    fn spanish(text: &str) -> Document {
        Segmenter::new("es").unwrap().segment(text)
    }

    /// The tokens of several words with their words
    fn multiword(text: &str) -> Vec<(String, Vec<String>)> {
        let doc = spanish(text);
        (0..doc.tokens().len())
            .filter(|&token| doc.token_words(token).len() > 1)
            .map(|token| {
                (
                    doc.tokens()[token].slice(doc.text()).to_owned(),
                    doc.token_words(token)
                        .map(|word| doc.form(word).to_owned())
                        .collect(),
                )
            })
            .collect()
    }

    fn expanded(text: &str) -> Vec<Vec<String>> {
        multiword(text)
            .into_iter()
            .map(|(_, words)| words)
            .collect()
    }

    #[test]
    fn contractions() {
        assert_eq!(
            expanded("Vengo del mercado y voy al cine."),
            [["de", "el"], ["a", "el"]]
        );
        assert_eq!(expanded("Al llegar"), [["A", "el"]]);
        assert_eq!(expanded("Del mercado"), [["De", "el"]]);
        assert_eq!(expanded("DEL NORTE AL SUR"), [["DE", "EL"], ["A", "EL"]]);
    }

    #[test]
    fn names_stay_whole() {
        for text in ["Vino Al Gore.", "Lo dijo Del Bosque.", "Vino Bonaparte."] {
            assert_eq!(multiword(text), []);
        }
        assert_eq!(expanded("Dijo: Al llegar"), [["A", "el"]]);
        assert_eq!(expanded("Fin. Del mercado"), [["De", "el"]]);
    }

    #[test]
    fn verbs_with_pronouns() {
        for (text, words) in [
            ("Quiero comprarlo", vec!["comprar", "lo"]),
            ("Voy a decírselo", vec!["decir", "se", "lo"]),
            ("Voy a decir\u{301}selo", vec!["decir", "se", "lo"]),
            ("Está diciéndole", vec!["diciendo", "le"]),
            ("Sigue dándonosla", vec!["dando", "nos", "la"]),
            ("Hay que verse", vec!["ver", "se"]),
            ("Va a irse", vec!["ir", "se"]),
            ("Hacerlo", vec!["Hacer", "lo"]),
            ("HACERLO", vec!["HACER", "LO"]),
            ("Va a compr\u{ad}arlo", vec!["comprar", "lo"]),
        ] {
            assert_eq!(expanded(text), [words], "{text}");
        }
    }

    #[test]
    fn listed_forms() {
        for (text, words) in [
            ("Dámelo", vec!["Da", "me", "lo"]),
            ("Véase", vec!["Vea", "se"]),
            ("hazlo", vec!["haz", "lo"]),
            ("Va a reírse", vec!["reír", "se"]),
        ] {
            assert_eq!(expanded(text), [words], "{text}");
        }
    }

    #[test]
    fn words_that_only_look_so() {
        for text in [
            "La parte fuerte de la muerte.",
            "Los gobiernos firman en firme.",
            "El título de la película.",
            "Me desperté y acerté.",
            "Carlos y Marla.",
            "El co-arte y 3-lo.",
        ] {
            assert_eq!(multiword(text), [], "{text}");
        }
    }

    #[test]
    fn listed_tokens_stay_whole() {
        let segmenter = Segmenter::new("es").unwrap().with_tokens(["del"]).unwrap();
        assert_eq!(segmenter.segment("Vengo del mar y al mar").word_count(), 7);
    }

    #[test]
    fn surface_tokens_are_kept() {
        let segmenter = Segmenter::new("es").unwrap();
        let text = "Vengo del mar, voy al cine.";
        let doc = segmenter.segment(text);
        assert_eq!(doc.tokens(), segmenter.tokenize(text));
        assert_eq!((doc.tokens().len(), doc.word_count()), (8, 10));
        assert_eq!(doc.token_words(1), 1..3);
        assert_eq!(doc.word_token(2), 1);
        assert_eq!(doc.word_id(2), 3);
        assert_eq!(doc.sentence(0).words, 0..10);
        assert_eq!(doc.form(0), "Vengo");
        assert_eq!(doc.form(1), "de");
    }

    #[test]
    fn conllu_with_ranges() {
        let text = "Vengo del mar. Al llegar, voy.";
        let doc = spanish(text);
        let conllu = doc.to_conllu();
        assert_eq!(
            conllu,
            "# newdoc\n# newpar\n# sent_id = 1\n# text = Vengo del mar.\n\
             1\tVengo\t_\t_\t_\t_\t_\t_\t_\t_\n\
             2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n\
             2\tde\t_\t_\t_\t_\t_\t_\t_\t_\n\
             3\tel\t_\t_\t_\t_\t_\t_\t_\t_\n\
             4\tmar\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
             5\t.\t_\t_\t_\t_\t_\t_\t_\t_\n\n\
             # sent_id = 2\n# text = Al llegar, voy.\n\
             1-2\tAl\t_\t_\t_\t_\t_\t_\t_\t_\n\
             1\tA\t_\t_\t_\t_\t_\t_\t_\t_\n\
             2\tel\t_\t_\t_\t_\t_\t_\t_\t_\n\
             3\tllegar\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
             4\t,\t_\t_\t_\t_\t_\t_\t_\t_\n\
             5\tvoy\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
             6\t.\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
        );
        let read = read_conllu(&conllu).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].text(), text);
        assert_eq!(read[0].word_count(), doc.word_count());
        assert_eq!(read[0].tokens(), doc.tokens());
        assert_eq!(read[0].form(1), "de");
    }

    #[test]
    fn multiword_after_a_space_or_not() {
        let conllu = spanish("Vengo del, al").to_conllu();
        assert!(
            conllu.contains("2-3\tdel\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n"),
            "{conllu}"
        );
    }

    #[test]
    fn columns_are_per_word() {
        let mut doc = spanish("Vengo del mar");
        let upos: Vec<Option<&str>> = vec![Some("VERB"), Some("ADP"), Some("DET"), Some("NOUN")];
        doc.set_columns([(Column::Upos, upos)], None::<Vec<Option<u32>>>)
            .unwrap();
        assert_eq!(doc.column(Column::Upos, 2), Some("DET"));
        let wrong: Vec<Option<&str>> = vec![Some("VERB"); 3];
        assert!(
            doc.set_columns([(Column::Upos, wrong)], None::<Vec<Option<u32>>>)
                .is_err()
        );
    }

    #[test]
    fn state_keeps_the_words() {
        let doc = spanish("Vengo del mar. Quiero decírselo.");
        let state = doc.to_state();
        assert_eq!(
            Document::from_state(doc.text().to_owned(), &state).unwrap(),
            doc
        );
    }

    #[cfg(feature = "ru")]
    #[test]
    fn languages_without_multiword_tokens() {
        let doc = Segmenter::new("ru").unwrap().segment("Del al comprarlo");
        assert_eq!(doc.word_count(), doc.tokens().len());
    }
}

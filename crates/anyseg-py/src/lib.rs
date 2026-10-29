//! Python module `anyseg._anyseg`

use std::hash::{Hash, Hasher};
use std::sync::{PoisonError, RwLock, RwLockReadGuard};

use anyseg::Column;
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PySlice, PyString, PyTuple, PyType};

const COLUMNS: &str = "form, lemma, upos, xpos, feats, head, deprel, deps, misc";

fn repr(py: Python<'_>, text: &str) -> PyResult<String> {
    Ok(PyString::new(py, text).repr()?.to_string())
}

/// Byte offsets of spans to character offsets, in one pass over the text. The spans go in order
/// and do not overlap
fn char_offsets(text: &str, spans: impl IntoIterator<Item = anyseg::Span>) -> Vec<(usize, usize)> {
    let ascii = text.is_ascii();
    let (mut byte, mut char) = (0, 0);
    let mut position = |offset: usize| {
        if ascii {
            return offset;
        }
        char += text[byte..offset].chars().count();
        byte = offset;
        char
    };
    spans
        .into_iter()
        .map(|span| (position(span.start), position(span.end)))
        .collect()
}

/// Piece of a text with its position
///
/// Example:
///     >>> from anyseg import Span
///     >>> text = "Ёж идёт"
///     >>> span = Span(3, 7, "идёт")
///     >>> span, text[span.start:span.end] == span.text
///     (Span(start=3, end=7, text='идёт'), True)
///
/// Description:
///     Positions are offsets in the characters of a Python string, as in its slices. Spans are
///     immutable, compare by all three attributes and can be pickled
///
/// Arguments:
///     start (int): Position of the first character
///     end (int): Position after the last character
///     text (str): The piece of the text
#[pyclass(frozen, eq, hash, module = "anyseg")]
#[derive(PartialEq, Eq, Hash)]
struct Span {
    #[pyo3(get)]
    start: usize,
    #[pyo3(get)]
    end: usize,
    #[pyo3(get)]
    text: String,
}

#[pymethods]
impl Span {
    #[new]
    fn new(start: usize, end: usize, text: String) -> Self {
        Span { start, end, text }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Span(start={}, end={}, text={})",
            self.start,
            self.end,
            repr(py, &self.text)?
        ))
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> (Bound<'py, PyType>, (usize, usize, String)) {
        let span = slf.get();
        (slf.get_type(), (span.start, span.end, span.text.clone()))
    }
}

fn spans(text: &str, spans: Vec<anyseg::Span>) -> Vec<Span> {
    char_offsets(text, spans.iter().copied())
        .into_iter()
        .zip(spans)
        .map(|((start, end), span)| Span {
            start,
            end,
            text: span.slice(text).to_owned(),
        })
        .collect()
}

/// Segmenter of texts in one language
///
/// Example:
///     >>> from anyseg import Segmenter
///     >>> segmenter = Segmenter("ru")
///     >>> segmenter.tokenize("Ёж идёт")
///     [Span(start=0, end=2, text='Ёж'), Span(start=3, end=7, text='идёт')]
///     >>> [span.text for span in segmenter.sentenize("Ёж идёт. Кот спит в 1914 г. Затем")]
///     ['Ёж идёт.', 'Кот спит в 1914 г.', 'Затем']
///     >>> segmenter = Segmenter("ru", abbreviations=["сущ."], tokens=["C++"])
///     >>> [span.text for span in segmenter.tokenize("сущ. C++")]
///     ['сущ.', 'C++']
///
/// Description:
///     Tokens and sentences are spans of the text, which is never changed. A segmenter is
///     immutable, can be shared between threads and pickled, and equals the segmenters of the
///     same language and lists
///
/// Arguments:
///     lang (str): ISO 639-1 code of the language: "ru"
///     abbreviations (Iterable[str] | None): More words that keep a period after them, given with
///         the period or without it, in any case
///     tokens (Iterable[str] | None): Strings that are one token each: they match as written where
///         a token would start and no letter or digit adjoins them; of several, the longest
///
/// Methods:
///     tokenize: Tokens of a text
///     sentenize: Sentences of a text
///     segment: Document of a text
///
/// Raises:
///     ValueError: If the language is not supported, an abbreviation is not a word of letters or a
///         token is empty, has whitespace or starts or ends with an invisible separator
///     TypeError: If abbreviations or tokens is a string or holds something else
#[pyclass(frozen, eq, hash, module = "anyseg")]
#[derive(PartialEq, Eq, Hash)]
struct Segmenter {
    inner: anyseg::Segmenter,
}

/// The strings of an iterable that is not a string itself
fn strings(value: Option<&Bound<'_, PyAny>>, name: &str) -> PyResult<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_instance_of::<PyString>() {
        return Err(PyTypeError::new_err(format!(
            "{name} must be an iterable of strings, not a string"
        )));
    }
    value
        .try_iter()?
        .map(|item| item?.extract::<String>())
        .collect()
}

#[pymethods]
impl Segmenter {
    #[new]
    #[pyo3(signature = (lang, *, abbreviations = None, tokens = None))]
    fn new(
        py: Python<'_>,
        lang: &str,
        abbreviations: Option<Bound<'_, PyAny>>,
        tokens: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let Ok(inner) = anyseg::Segmenter::new(lang) else {
            let codes: Vec<&str> = anyseg::Language::ALL
                .iter()
                .map(|language| language.code())
                .collect();
            return Err(PyValueError::new_err(format!(
                "unsupported language {}, expected one of: {}",
                repr(py, lang)?,
                codes.join(", ")
            )));
        };
        let abbreviations = strings(abbreviations.as_ref(), "abbreviations")?;
        let tokens = strings(tokens.as_ref(), "tokens")?;
        let inner = inner
            .with_abbreviations(abbreviations)
            .and_then(|inner| inner.with_tokens(tokens));
        match inner {
            Ok(inner) => Ok(Segmenter { inner }),
            Err(anyseg::InvalidEntry::Abbreviation(word)) => Err(PyValueError::new_err(format!(
                "abbreviation {} is not a word of letters with an optional final period",
                repr(py, &word)?
            ))),
            Err(anyseg::InvalidEntry::Token(token)) => Err(PyValueError::new_err(format!(
                "token {} is empty, has whitespace or starts or ends with an invisible separator",
                repr(py, &token)?
            ))),
            Err(error) => Err(PyValueError::new_err(error.to_string())),
        }
    }

    /// ISO 639-1 code of the language
    #[getter]
    fn lang(&self) -> &'static str {
        self.inner.language().code()
    }

    /// Abbreviations added to the language, sorted: lowercase, without the period and stress marks
    #[getter]
    fn abbreviations<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.inner.abbreviations())
    }

    /// Strings that are one token each, sorted
    #[getter]
    fn tokens<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.inner.tokens())
    }

    /// Tokens of a text
    ///
    /// Description:
    ///     Whitespace and the invisible separators (zero width space, word joiner, byte order
    ///     mark, and a soft hyphen and zero width (non-)joiner outside a word) separate tokens and
    ///     are in none. Words keep their combining marks and soft hyphens; hyphenated words,
    ///     numbers with decimal commas and points, dates, times and digit groups, URLs, e-mail
    ///     addresses, file names, mentions, hashtags, emoticons and runs of emoji are one token
    ///     each, as are runs of final marks ("?!", "..."). Abbreviations and initials keep their
    ///     period, except at the end of a sentence. Particles after a hyphen ("Вася-то"), ranges
    ///     ("2-3"), approximate numbers ("два-три") and compounds whose first part inflects
    ///     ("Ростова-на-Дону", "Формулы-1", "учеными-энтузиастами") split at the hyphen
    ///
    /// Arguments:
    ///     text (str): Text
    ///
    /// Returns:
    ///     list[Span]: Tokens in order
    ///
    /// Raises:
    ///     UnicodeEncodeError: If the text contains a lone surrogate
    fn tokenize(&self, text: &str) -> Vec<Span> {
        spans(text, self.inner.tokenize(text))
    }

    /// Sentences of a text
    ///
    /// Description:
    ///     A sentence runs from the start of its first token to the end of its last. A paragraph,
    ///     the part of the text between blank lines, starts a sentence; inside it a sentence ends
    ///     at final marks (".", "?!", "...") with the closing quotes and brackets after them, when
    ///     a capital letter, a digit, or a dash or an opening quote before a capital letter follows.
    ///     An abbreviation ends a sentence before a capital letter when the language lists it
    ///     ("и т. д.") or it follows a number ("в 1914 г."); initials do not. A line break starts a
    ///     sentence before a list item or a line of dialogue, at a line without words and after a
    ///     line without final punctuation before a capital letter, except in verse and in text
    ///     wrapped at the width of the page (a line of the paragraph goes on with a lowercase
    ///     letter)
    ///
    /// Arguments:
    ///     text (str): Text
    ///
    /// Returns:
    ///     list[Span]: Sentences in order
    ///
    /// Raises:
    ///     UnicodeEncodeError: If the text contains a lone surrogate
    fn sentenize(&self, text: &str) -> Vec<Span> {
        spans(text, self.inner.sentenize(text))
    }

    /// Document of a text
    ///
    /// Description:
    ///     Tokens as tokenize gives them, one word per token, sentences as sentenize gives them,
    ///     paragraphs and their lines. A line that goes on with a lowercase letter outside verse
    ///     and does not start a sentence is a wrap at the width of the page ("typeset"), other
    ///     lines are breaks of the author
    ///
    /// Arguments:
    ///     text (str): Text
    ///
    /// Returns:
    ///     Document: Segmented text
    ///
    /// Raises:
    ///     UnicodeEncodeError: If the text contains a lone surrogate
    fn segment(&self, py: Python<'_>, text: Bound<'_, PyString>) -> PyResult<Document> {
        let inner = self.inner.segment(text.to_str()?);
        Ok(Document::new(py, inner, Some(text.unbind())))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut out = format!("Segmenter({}", repr(py, self.lang())?);
        for (name, list) in [
            ("abbreviations", self.abbreviations(py)?),
            ("tokens", self.tokens(py)?),
        ] {
            if !list.is_empty() {
                out.push_str(&format!(", {name}={}", list.repr()?));
            }
        }
        out.push(')');
        Ok(out)
    }

    fn __reduce__<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyTuple>)> {
        let py = slf.py();
        let segmenter = slf.get();
        let lists = PyDict::new(py);
        lists.set_item("abbreviations", segmenter.abbreviations(py)?)?;
        lists.set_item("tokens", segmenter.tokens(py)?)?;
        let new = py.import("copyreg")?.getattr("__newobj_ex__")?;
        let arguments = (slf.get_type(), (segmenter.lang(),), lists).into_pyobject(py)?;
        Ok((new, arguments))
    }
}

/// Segmented text: tokens, words, sentences, paragraphs and lines
///
/// Example:
///     >>> from anyseg import Document, Segmenter
///     >>> doc = Segmenter("ru").segment("Ёж идёт домой\n\nКот спит")
///     >>> doc.tokens[1], doc.sentences[1].text
///     (Token(start=3, end=7, text='идёт'), 'Кот спит')
///     >>> doc.set_columns(upos=["NOUN", "VERB", "ADV", "NOUN", "VERB"], head=[2, 0, 2, 2, 0])
///     >>> doc.words[1]
///     Word(id=2, form='идёт', upos='VERB', head=0)
///     >>> doc = Document.from_tokens("Ёж идёт\n\nдомой", [0, 3, 9], [2, 7, 14])
///     >>> [sentence.text for sentence in doc.sentences]
///     ['Ёж идёт', 'домой']
///
/// Description:
///     The text is never changed: tokens are its spans, and positions are offsets in the
///     characters of the Python string. A token holds one word or, for a multiword token,
///     several; words have the columns of CoNLL-U, empty until set. Sentences hold whole tokens,
///     paragraphs (the parts of the text between blank lines) hold whole sentences, and lines are
///     those of the paragraphs. Tokens, words and the other parts are views of the document;
///     set_columns is its only change, and a document can be shared between threads
///
/// Methods:
///     column: Values of a column of the words
///     set_columns: Set columns of the words
///     from_tokens: Document of the tokens of a text
///     to_conllu: Document in CoNLL-U
#[pyclass(frozen, module = "anyseg")]
struct Document {
    inner: RwLock<anyseg::Document>,
    text: Py<PyString>,
    /// Character offsets of the tokens and the lines
    tokens: Vec<(usize, usize)>,
    lines: Vec<(usize, usize)>,
}

impl Document {
    fn new(py: Python<'_>, inner: anyseg::Document, text: Option<Py<PyString>>) -> Self {
        let tokens = char_offsets(inner.text(), inner.tokens().iter().copied());
        let lines = char_offsets(inner.text(), inner.lines().iter().map(|line| line.span));
        let text = text.unwrap_or_else(|| PyString::new(py, inner.text()).unbind());
        Document {
            inner: RwLock::new(inner),
            text,
            tokens,
            lines,
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, anyseg::Document> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn cover(&self, tokens: &std::ops::Range<usize>) -> (usize, usize) {
        (self.tokens[tokens.start].0, self.tokens[tokens.end - 1].1)
    }
}

#[pymethods]
impl Document {
    #[getter]
    fn text(&self, py: Python<'_>) -> Py<PyString> {
        self.text.clone_ref(py)
    }

    #[getter]
    fn tokens(slf: &Bound<'_, Self>) -> Tokens {
        let end = slf.get().tokens.len();
        Tokens {
            doc: slf.clone().unbind(),
            start: 0,
            end,
        }
    }

    #[getter]
    fn words(slf: &Bound<'_, Self>) -> Words {
        let end = slf.get().read().word_count();
        Words {
            doc: slf.clone().unbind(),
            start: 0,
            end,
        }
    }

    #[getter]
    fn sentences(slf: &Bound<'_, Self>) -> Sentences {
        let end = slf.get().read().sentence_count();
        Sentences {
            doc: slf.clone().unbind(),
            start: 0,
            end,
        }
    }

    #[getter]
    fn paragraphs(slf: &Bound<'_, Self>) -> Paragraphs {
        let end = slf.get().read().paragraph_count();
        Paragraphs {
            doc: slf.clone().unbind(),
            start: 0,
            end,
        }
    }

    #[getter]
    fn lines(slf: &Bound<'_, Self>) -> Lines {
        let end = slf.get().lines.len();
        Lines {
            doc: slf.clone().unbind(),
            start: 0,
            end,
        }
    }

    /// Values of a column of the words
    ///
    /// Arguments:
    ///     name (str): Column of CoNLL-U: form, lemma, upos, xpos, feats, head, deprel, deps
    ///         or misc
    ///
    /// Returns:
    ///     list[str | None] | list[int | None]: One value per word, None for an empty one; heads
    ///         are numbers
    ///
    /// Raises:
    ///     ValueError: If there is no such column
    fn column<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyList>> {
        let inner = self.read();
        let words = 0..inner.word_count();
        match name {
            "form" => PyList::new(py, words.map(|word| inner.form(word))),
            "head" => PyList::new(py, words.map(|word| inner.head(word))),
            _ => match Column::from_name(name) {
                Some(column) => PyList::new(py, words.map(|word| inner.column(column, word))),
                None => Err(PyValueError::new_err(format!(
                    "unknown column {}, expected one of: {COLUMNS}",
                    repr(py, name)?
                ))),
            },
        }
    }

    /// Set columns of the words
    ///
    /// Description:
    ///     All columns are set at once, or none if a value is wrong. None empties a value; the
    ///     forms come from the tokens and cannot be set
    ///
    /// Arguments:
    ///     **columns (list[str | None] | list[int | None]): One value per word for every column:
    ///         lemma, upos, xpos, feats, head, deprel, deps or misc; a head is 0 or the ID of a
    ///         word of its sentence
    ///
    /// Raises:
    ///     ValueError: If there is no such column, a list has not one value per word, a value is
    ///         empty, a lemma or MISC holds a tab or a line break or another value holds
    ///         whitespace, or a head is out of its sentence
    ///     TypeError: If a value is neither a string nor None, or a head is not an integer
    #[pyo3(signature = (**columns))]
    fn set_columns(&self, py: Python<'_>, columns: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let mut values = Vec::new();
        let mut heads = None;
        for (name, column) in columns.into_iter().flatten() {
            let name: String = name.extract()?;
            if name == "head" {
                let mut converted = Vec::new();
                for (word, head) in column
                    .extract::<Vec<Option<i64>>>()?
                    .into_iter()
                    .enumerate()
                {
                    converted.push(match head {
                        None => None,
                        Some(head) => Some(u32::try_from(head).map_err(|_| {
                            PyValueError::new_err(format!(
                                "head of word {word} is {head}, not 0 or a word ID"
                            ))
                        })?),
                    });
                }
                heads = Some(converted);
            } else if let Some(column_name) = Column::from_name(&name) {
                values.push((column_name, column.extract::<Vec<Option<String>>>()?));
            } else {
                return Err(PyValueError::new_err(format!(
                    "{} cannot be set, expected: {}",
                    repr(py, &name)?,
                    COLUMNS.replace("form, ", "")
                )));
            }
        }
        self.inner
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .set_columns(values, heads)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Document of the tokens of a text
    ///
    /// Description:
    ///     A token holds one word. Paragraphs are the parts of the text between blank lines, and
    ///     every paragraph starts a sentence; without sentence starts a paragraph is one sentence
    ///
    /// Arguments:
    ///     text (str): Text
    ///     starts (list[int]): Positions of the first characters of the tokens
    ///     ends (list[int]): Positions after their last characters
    ///     sentence_starts (list[int] | None): Indices of the tokens that start sentences
    ///
    /// Returns:
    ///     Document: Document of the tokens
    ///
    /// Raises:
    ///     ValueError: If the lists differ in length, a token is empty, out of the text, overlaps
    ///         the token before it, starts or ends with whitespace or runs over a paragraph
    ///         boundary, the text between the tokens holds anything but whitespace, or the
    ///         sentence starts are not increasing token indices
    #[staticmethod]
    #[pyo3(signature = (text, starts, ends, *, sentence_starts=None))]
    fn from_tokens(
        py: Python<'_>,
        text: Bound<'_, PyString>,
        starts: Vec<isize>,
        ends: Vec<isize>,
        sentence_starts: Option<Vec<usize>>,
    ) -> PyResult<Document> {
        if starts.len() != ends.len() {
            return Err(PyValueError::new_err(format!(
                "{} starts and {} ends",
                starts.len(),
                ends.len()
            )));
        }
        let string = text.to_str()?;
        let length = string.chars().count() as isize;
        let mut previous = 0;
        for (index, (&start, &end)) in starts.iter().zip(&ends).enumerate() {
            let message = if start < 0 {
                "starts before the text"
            } else if start >= end {
                "is empty"
            } else if end > length {
                "ends after the text"
            } else if start < previous {
                "overlaps or precedes the token before it"
            } else {
                previous = end;
                continue;
            };
            return Err(PyValueError::new_err(format!("token {index}: {message}")));
        }
        let mut offsets = string
            .char_indices()
            .map(|(offset, _)| offset)
            .chain([string.len()]);
        let (mut char, mut byte) = (0, offsets.next().unwrap_or(0));
        let mut to_byte = |position: usize| {
            while char < position {
                byte = offsets.next().unwrap_or(string.len());
                char += 1;
            }
            byte
        };
        let tokens = starts
            .iter()
            .zip(&ends)
            .map(|(&start, &end)| anyseg::Span {
                start: to_byte(start as usize),
                end: to_byte(end as usize),
            })
            .collect();
        let inner =
            anyseg::Document::from_tokens(string.to_owned(), tokens, sentence_starts.as_deref())
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Document::new(py, inner, Some(text.unbind())))
    }

    /// Document in CoNLL-U
    ///
    /// Description:
    ///     A document read from CoNLL-U is written as it was read, with the columns set since.
    ///     Otherwise every sentence gets the comments sent_id and text, the first one newdoc and
    ///     every paragraph newpar, and the space after a token inside a paragraph goes to MISC:
    ///     SpaceAfter=No when there is none and SpacesAfter when it is not one space. There the
    ///     invisible separators are left out, line breaks other than CR and LF become LF and
    ///     whitespace other than tabs becomes a space; the comment text is the tokens with these
    ///     spaces, line breaks turned into spaces. Forms, the comment text and column values are in
    ///     Unicode NFC, as CoNLL-U requires; the document keeps its text
    ///
    /// Arguments:
    ///     doc_id (str | None): ID of the document: newdoc gets it and sent_id becomes ID-N (the ID
    ///         in NFC), so that sentences of several documents in one file have distinct IDs; a
    ///         document read from CoNLL-U keeps its comments
    ///
    /// Returns:
    ///     str: Sentences in CoNLL-U, each followed by a blank line
    ///
    /// Raises:
    ///     ValueError: If the ID is empty or holds whitespace
    #[pyo3(signature = (*, doc_id=None))]
    fn to_conllu(&self, doc_id: Option<&str>) -> PyResult<String> {
        let inner = self.read();
        match doc_id {
            None => Ok(inner.to_conllu()),
            Some(id) => inner
                .to_conllu_with_id(id)
                .map_err(|error| PyValueError::new_err(error.to_string())),
        }
    }

    fn __repr__(&self) -> String {
        let inner = self.read();
        format!(
            "Document(tokens={}, words={}, sentences={}, paragraphs={})",
            inner.tokens().len(),
            inner.word_count(),
            inner.sentence_count(),
            inner.paragraph_count()
        )
    }
}

/// Views compare equal when they show the same part of the same document
macro_rules! view {
    ($view:ident) => {
        impl PartialEq for $view {
            fn eq(&self, other: &Self) -> bool {
                self.doc.as_ptr() == other.doc.as_ptr() && self.index == other.index
            }
        }

        impl Eq for $view {}

        impl Hash for $view {
            fn hash<H: Hasher>(&self, state: &mut H) {
                (self.doc.as_ptr() as usize).hash(state);
                self.index.hash(state);
            }
        }
    };
}

macro_rules! sequence {
    ($name:ident, $view:ident, $what:literal) => {
        #[doc = concat!("Sequence of the ", $what, " of a document or of a part of it")]
        #[pyclass(frozen, sequence, module = "anyseg")]
        struct $name {
            doc: Py<Document>,
            start: usize,
            end: usize,
        }

        #[pymethods]
        impl $name {
            fn __len__(&self) -> usize {
                self.end - self.start
            }

            fn __getitem__<'py>(
                &self,
                py: Python<'py>,
                key: &Bound<'py, PyAny>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let length = (self.end - self.start) as isize;
                if let Ok(slice) = key.cast::<PySlice>() {
                    let indices = slice.indices(length)?;
                    if indices.step == 1 {
                        let start = self.start + indices.start as usize;
                        let part = $name {
                            doc: self.doc.clone_ref(py),
                            start,
                            end: start + indices.slicelength,
                        };
                        return Ok(Bound::new(py, part)?.into_any());
                    }
                    let views = (0..indices.slicelength).map(|position| $view {
                        doc: self.doc.clone_ref(py),
                        index: (self.start as isize
                            + indices.start
                            + position as isize * indices.step) as usize,
                    });
                    return Ok(PyList::new(py, views)?.into_any());
                }
                let index: isize = key.extract()?;
                let position = if index < 0 { index + length } else { index };
                if !(0..length).contains(&position) {
                    return Err(PyIndexError::new_err(concat!($what, " index out of range")));
                }
                let view = $view {
                    doc: self.doc.clone_ref(py),
                    index: self.start + position as usize,
                };
                Ok(Bound::new(py, view)?.into_any())
            }

            fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
                self.list(py)?.try_iter()
            }

            fn __contains__(&self, value: &Bound<'_, PyAny>) -> bool {
                self.position(value).is_some()
            }

            /// Position of a view in the sequence
            #[pyo3(signature = (value, start=0, stop=isize::MAX, /))]
            fn index(
                &self,
                value: &Bound<'_, PyAny>,
                start: isize,
                stop: isize,
            ) -> PyResult<usize> {
                let length = (self.end - self.start) as isize;
                let bound = |index: isize| {
                    if index < 0 {
                        (index + length).max(0)
                    } else {
                        index.min(length)
                    }
                };
                match self.position(value) {
                    Some(position)
                        if (bound(start)..bound(stop)).contains(&(position as isize)) =>
                    {
                        Ok(position)
                    }
                    _ => Err(PyValueError::new_err(concat!(
                        "the value is not among the ",
                        $what
                    ))),
                }
            }

            /// Number of times a view is in the sequence: 0 or 1
            #[pyo3(signature = (value, /))]
            fn count(&self, value: &Bound<'_, PyAny>) -> usize {
                usize::from(self.position(value).is_some())
            }

            fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
                Ok(format!("{}({})", stringify!($name), self.list(py)?.repr()?))
            }
        }

        impl $name {
            /// The position of a view of the same document in the sequence
            fn position(&self, value: &Bound<'_, PyAny>) -> Option<usize> {
                let view = value.cast::<$view>().ok()?.get();
                (view.doc.as_ptr() == self.doc.as_ptr()
                    && (self.start..self.end).contains(&view.index))
                .then(|| view.index - self.start)
            }

            fn list<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
                PyList::new(
                    py,
                    (self.start..self.end).map(|index| $view {
                        doc: self.doc.clone_ref(py),
                        index,
                    }),
                )
            }
        }
    };
}

sequence!(Tokens, Token, "tokens");
sequence!(Words, Word, "words");
sequence!(Sentences, Sentence, "sentences");
sequence!(Paragraphs, Paragraph, "paragraphs");
sequence!(Lines, Line, "lines");

/// Token of a document: a span of its text
///
/// Example:
///     >>> from anyseg import Segmenter
///     >>> token = Segmenter("ru").segment("Ёж  идёт").tokens[0]
///     >>> token, token.spaces_after, token.words[0].form
///     (Token(start=0, end=2, text='Ёж'), '  ', 'Ёж')
///
/// Description:
///     A token holds one word or, for a multiword token, several. Tokens compare equal when they
///     are the same token of the same document
#[pyclass(frozen, eq, hash, module = "anyseg")]
struct Token {
    doc: Py<Document>,
    index: usize,
}

view!(Token);

#[pymethods]
impl Token {
    /// Position of the token in the document, from 0
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Position of the first character
    #[getter]
    fn start(&self) -> usize {
        self.doc.get().tokens[self.index].0
    }

    /// Position after the last character
    #[getter]
    fn end(&self) -> usize {
        self.doc.get().tokens[self.index].1
    }

    #[getter]
    fn text<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        let inner = self.doc.get().read();
        PyString::new(py, inner.tokens()[self.index].slice(inner.text()))
    }

    /// Text between the token and the next one, or the end of the text
    #[getter]
    fn spaces_after<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        let inner = self.doc.get().read();
        let tokens = inner.tokens();
        let end = tokens
            .get(self.index + 1)
            .map_or(inner.text().len(), |next| next.start);
        PyString::new(py, &inner.text()[tokens[self.index].end..end])
    }

    #[getter]
    fn words(&self, py: Python<'_>) -> Words {
        let words = self.doc.get().read().token_words(self.index);
        Words {
            doc: self.doc.clone_ref(py),
            start: words.start,
            end: words.end,
        }
    }

    #[getter]
    fn sentence(&self, py: Python<'_>) -> Sentence {
        Sentence {
            doc: self.doc.clone_ref(py),
            index: self.doc.get().read().token_sentence(self.index),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Token(start={}, end={}, text={})",
            self.start(),
            self.end(),
            self.text(py).repr()?
        ))
    }
}

/// Word of a document with its columns of CoNLL-U
///
/// Example:
///     >>> from anyseg import read_conllu
///     >>> conllu = "1\tdel\tde\tADP\t_\t_\t0\troot\t_\t_\n\n"
///     >>> word = read_conllu(conllu)[0].words[0]
///     >>> word, word.lemma, word.feats
///     (Word(id=1, form='del', lemma='de', upos='ADP', head=0, deprel='root'), 'de', None)
///
/// Description:
///     Empty columns are None. The form of a word is the text of its token, except for the words
///     of a multiword token. Words compare equal when they are the same word of the same document
#[pyclass(frozen, eq, hash, module = "anyseg")]
struct Word {
    doc: Py<Document>,
    index: usize,
}

view!(Word);

impl Word {
    fn value<'py>(&self, py: Python<'py>, column: Column) -> Option<Bound<'py, PyString>> {
        let inner = self.doc.get().read();
        inner
            .column(column, self.index)
            .map(|value| PyString::new(py, value))
    }
}

#[pymethods]
impl Word {
    /// Position of the word in the document, from 0
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// ID of the word in its sentence, from 1
    #[getter]
    fn id(&self) -> usize {
        self.doc.get().read().word_id(self.index)
    }

    #[getter]
    fn form<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        PyString::new(py, self.doc.get().read().form(self.index))
    }

    #[getter]
    fn lemma<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Lemma)
    }

    #[getter]
    fn upos<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Upos)
    }

    #[getter]
    fn xpos<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Xpos)
    }

    #[getter]
    fn feats<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Feats)
    }

    /// ID of the head word in the sentence, 0 for the root
    #[getter]
    fn head(&self) -> Option<u32> {
        self.doc.get().read().head(self.index)
    }

    #[getter]
    fn deprel<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Deprel)
    }

    #[getter]
    fn deps<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Deps)
    }

    #[getter]
    fn misc<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyString>> {
        self.value(py, Column::Misc)
    }

    #[getter]
    fn token(&self, py: Python<'_>) -> Token {
        Token {
            doc: self.doc.clone_ref(py),
            index: self.doc.get().read().word_token(self.index),
        }
    }

    #[getter]
    fn sentence(&self, py: Python<'_>) -> Sentence {
        Sentence {
            doc: self.doc.clone_ref(py),
            index: self.doc.get().read().word_sentence(self.index),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let inner = self.doc.get().read();
        let mut out = format!(
            "Word(id={}, form={}",
            inner.word_id(self.index),
            repr(py, inner.form(self.index))?
        );
        for column in [Column::Lemma, Column::Upos, Column::Xpos, Column::Feats] {
            if let Some(value) = inner.column(column, self.index) {
                out.push_str(&format!(", {}={}", column.name(), repr(py, value)?));
            }
        }
        if let Some(head) = inner.head(self.index) {
            out.push_str(&format!(", head={head}"));
        }
        for column in [Column::Deprel, Column::Deps, Column::Misc] {
            if let Some(value) = inner.column(column, self.index) {
                out.push_str(&format!(", {}={}", column.name(), repr(py, value)?));
            }
        }
        out.push(')');
        Ok(out)
    }
}

/// Sentence of a document: from the start of its first token to the end of its last one
///
/// Example:
///     >>> from anyseg import Segmenter
///     >>> sentence = Segmenter("ru").segment("Ёж идёт\n\nКот спит").sentences[1]
///     >>> sentence, [token.text for token in sentence.tokens]
///     (Sentence(start=9, end=17, text='Кот спит'), ['Кот', 'спит'])
///
/// Description:
///     A sentence holds whole tokens. Sentences compare equal when they are the same sentence of
///     the same document
#[pyclass(frozen, eq, hash, module = "anyseg")]
struct Sentence {
    doc: Py<Document>,
    index: usize,
}

view!(Sentence);

#[pymethods]
impl Sentence {
    /// Position of the sentence in the document, from 0
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Position of the first character
    #[getter]
    fn start(&self) -> usize {
        let doc = self.doc.get();
        doc.cover(&doc.read().sentence(self.index).tokens).0
    }

    /// Position after the last character
    #[getter]
    fn end(&self) -> usize {
        let doc = self.doc.get();
        doc.cover(&doc.read().sentence(self.index).tokens).1
    }

    #[getter]
    fn text<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        let inner = self.doc.get().read();
        PyString::new(py, inner.sentence(self.index).span.slice(inner.text()))
    }

    #[getter]
    fn tokens(&self, py: Python<'_>) -> Tokens {
        let tokens = self.doc.get().read().sentence(self.index).tokens;
        Tokens {
            doc: self.doc.clone_ref(py),
            start: tokens.start,
            end: tokens.end,
        }
    }

    #[getter]
    fn words(&self, py: Python<'_>) -> Words {
        let words = self.doc.get().read().sentence(self.index).words;
        Words {
            doc: self.doc.clone_ref(py),
            start: words.start,
            end: words.end,
        }
    }

    #[getter]
    fn paragraph(&self, py: Python<'_>) -> Paragraph {
        Paragraph {
            doc: self.doc.clone_ref(py),
            index: self.doc.get().read().sentence_paragraph(self.index),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Sentence(start={}, end={}, text={})",
            self.start(),
            self.end(),
            self.text(py).repr()?
        ))
    }
}

/// Paragraph of a document: a part of its text between blank lines
///
/// Example:
///     >>> from anyseg import Segmenter
///     >>> paragraph = Segmenter("ru").segment("Ёж идёт\n\nКот спит").paragraphs[1]
///     >>> paragraph, len(paragraph.sentences)
///     (Paragraph(start=9, end=17, text='Кот спит'), 1)
///
/// Description:
///     A paragraph holds whole sentences and goes from the start of its first token to the end
///     of its last one. Paragraphs compare equal when they are the same paragraph of the same
///     document
#[pyclass(frozen, eq, hash, module = "anyseg")]
struct Paragraph {
    doc: Py<Document>,
    index: usize,
}

view!(Paragraph);

#[pymethods]
impl Paragraph {
    /// Position of the paragraph in the document, from 0
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Position of the first character
    #[getter]
    fn start(&self) -> usize {
        let doc = self.doc.get();
        doc.cover(&doc.read().paragraph(self.index).tokens).0
    }

    /// Position after the last character
    #[getter]
    fn end(&self) -> usize {
        let doc = self.doc.get();
        doc.cover(&doc.read().paragraph(self.index).tokens).1
    }

    #[getter]
    fn text<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        let inner = self.doc.get().read();
        PyString::new(py, inner.paragraph(self.index).span.slice(inner.text()))
    }

    #[getter]
    fn sentences(&self, py: Python<'_>) -> Sentences {
        let sentences = self.doc.get().read().paragraph(self.index).sentences;
        Sentences {
            doc: self.doc.clone_ref(py),
            start: sentences.start,
            end: sentences.end,
        }
    }

    #[getter]
    fn tokens(&self, py: Python<'_>) -> Tokens {
        let tokens = self.doc.get().read().paragraph(self.index).tokens;
        Tokens {
            doc: self.doc.clone_ref(py),
            start: tokens.start,
            end: tokens.end,
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Paragraph(start={}, end={}, text={})",
            self.start(),
            self.end(),
            self.text(py).repr()?
        ))
    }
}

/// Line of a paragraph of a document
///
/// Example:
///     >>> from anyseg import Segmenter
///     >>> Segmenter("ru").segment("Утро\nНочь").lines[1]
///     Line(start=5, end=9, text='Ночь', break_before='author')
///
/// Description:
///     A line goes without the whitespace around it. Its break_before is None for the first line
///     of a paragraph, "author" for a break the author made (a verse line, a list item, a
///     paragraph per line) and "typeset" for a wrap at the width of the page, as in text extracted
///     from PDF. Lines compare equal when they are the same line of the same document
#[pyclass(frozen, eq, hash, module = "anyseg")]
struct Line {
    doc: Py<Document>,
    index: usize,
}

view!(Line);

#[pymethods]
impl Line {
    /// Position of the line in the document, from 0
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Position of the first character
    #[getter]
    fn start(&self) -> usize {
        self.doc.get().lines[self.index].0
    }

    /// Position after the last character
    #[getter]
    fn end(&self) -> usize {
        self.doc.get().lines[self.index].1
    }

    #[getter]
    fn text<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        let inner = self.doc.get().read();
        PyString::new(py, inner.lines()[self.index].span.slice(inner.text()))
    }

    /// How the line begins: None for the first line of a paragraph, "author" or "typeset"
    #[getter]
    fn break_before(&self) -> Option<&'static str> {
        self.doc.get().read().lines()[self.index]
            .break_before
            .map(anyseg::LineBreak::name)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let break_before = match self.break_before() {
            Some(name) => format!("'{name}'"),
            None => "None".to_owned(),
        };
        Ok(format!(
            "Line(start={}, end={}, text={}, break_before={break_before})",
            self.start(),
            self.end(),
            self.text(py).repr()?
        ))
    }
}

/// Documents of a text in CoNLL-U, described by `anyseg.read_conllu`
#[pyfunction]
fn read_conllu(py: Python<'_>, text: &str) -> PyResult<Vec<Document>> {
    let documents =
        anyseg::read_conllu(text).map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok(documents
        .into_iter()
        .map(|document| Document::new(py, document, None))
        .collect())
}

#[pymodule(gil_used = false)]
fn _anyseg(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Span>()?;
    m.add_class::<Segmenter>()?;
    m.add_class::<Document>()?;
    m.add_class::<Token>()?;
    m.add_class::<Word>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;
    m.add_class::<Line>()?;
    m.add_class::<Tokens>()?;
    m.add_class::<Words>()?;
    m.add_class::<Sentences>()?;
    m.add_class::<Paragraphs>()?;
    m.add_class::<Lines>()?;
    m.add_function(wrap_pyfunction!(read_conllu, m)?)?;
    m.add(
        "__version__",
        env!("CARGO_PKG_VERSION").replace("-dev.", ".dev"),
    )?;
    Ok(())
}

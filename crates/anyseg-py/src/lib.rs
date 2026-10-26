//! Python module `anyseg._anyseg`

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;

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
        let text = PyString::new(py, &self.text).repr()?;
        Ok(format!(
            "Span(start={}, end={}, text={text})",
            self.start, self.end
        ))
    }

    fn __getnewargs__(&self) -> (usize, usize, String) {
        (self.start, self.end, self.text.clone())
    }
}

/// Spans of a text in byte offsets to spans in character offsets, in one pass over the text.
/// The spans go in order and do not overlap
fn spans(text: &str, spans: Vec<anyseg::Span>) -> Vec<Span> {
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
        .map(|span| Span {
            start: position(span.start),
            end: position(span.end),
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
///     >>> segmenter.sentenize(" Ёж идёт ")
///     [Span(start=1, end=8, text='Ёж идёт')]
///
/// Description:
///     Tokens and sentences are spans of the text, which is never changed. A segmenter is
///     immutable and can be shared between threads and pickled
///
/// Arguments:
///     lang (str): ISO 639-1 code of the language: "ru"
///
/// Methods:
///     tokenize: Tokens of a text
///     sentenize: Sentences of a text
///
/// Raises:
///     ValueError: If the language is not supported
#[pyclass(frozen, module = "anyseg")]
struct Segmenter {
    inner: anyseg::Segmenter,
}

#[pymethods]
impl Segmenter {
    #[new]
    fn new(py: Python<'_>, lang: &str) -> PyResult<Self> {
        match anyseg::Segmenter::new(lang) {
            Ok(inner) => Ok(Segmenter { inner }),
            Err(_) => {
                let codes: Vec<&str> = anyseg::Language::ALL
                    .iter()
                    .map(|language| language.code())
                    .collect();
                Err(PyValueError::new_err(format!(
                    "unsupported language {}, expected one of: {}",
                    PyString::new(py, lang).repr()?,
                    codes.join(", ")
                )))
            }
        }
    }

    /// ISO 639-1 code of the language
    #[getter]
    fn lang(&self) -> &'static str {
        self.inner.language().code()
    }

    /// Tokens of a text
    ///
    /// Description:
    ///     Tokens are the runs of characters between whitespace
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
    ///     The sentence is the text without the whitespace around it; a blank text has none
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

    fn __repr__(&self) -> String {
        format!("Segmenter('{}')", self.lang())
    }

    fn __getnewargs__(&self) -> (&'static str,) {
        (self.lang(),)
    }
}

#[pymodule(gil_used = false)]
fn _anyseg(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Span>()?;
    m.add_class::<Segmenter>()?;
    m.add(
        "__version__",
        env!("CARGO_PKG_VERSION").replace("-dev.", ".dev"),
    )?;
    Ok(())
}

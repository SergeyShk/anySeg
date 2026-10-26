"""Fast segmentation of text into tokens and sentences with positions in the source text"""

from functools import cache

from ._anyseg import (
    Document,
    Line,
    Lines,
    Paragraph,
    Paragraphs,
    Segmenter,
    Sentence,
    Sentences,
    Span,
    Token,
    Tokens,
    Word,
    Words,
    __version__,
    read_conllu,
)

__all__ = [
    "Document",
    "Line",
    "Lines",
    "Paragraph",
    "Paragraphs",
    "Segmenter",
    "Sentence",
    "Sentences",
    "Span",
    "Token",
    "Tokens",
    "Word",
    "Words",
    "__version__",
    "read_conllu",
    "segment",
    "sentenize",
    "tokenize",
]


@cache
def _segmenter(lang: str) -> Segmenter:
    return Segmenter(lang)


def tokenize(text: str, *, lang: str) -> list[Span]:
    """
    Tokens of a text

    Example:
        >>> from anyseg import tokenize
        >>> [span.text for span in tokenize("Мама мыла раму", lang="ru")]
        ['Мама', 'мыла', 'раму']

    Description:
        The same as Segmenter(lang).tokenize(text), with one segmenter per language

    Arguments:
        text (str): Text
        lang (str): ISO 639-1 code of the language: "ru"

    Returns:
        list[Span]: Tokens in order

    Raises:
        ValueError: If the language is not supported
        UnicodeEncodeError: If the text contains a lone surrogate
    """
    return _segmenter(lang).tokenize(text)


def sentenize(text: str, *, lang: str) -> list[Span]:
    """
    Sentences of a text

    Example:
        >>> from anyseg import sentenize
        >>> [span.text for span in sentenize(" Мама мыла раму ", lang="ru")]
        ['Мама мыла раму']

    Description:
        The same as Segmenter(lang).sentenize(text), with one segmenter per language

    Arguments:
        text (str): Text
        lang (str): ISO 639-1 code of the language: "ru"

    Returns:
        list[Span]: Sentences in order

    Raises:
        ValueError: If the language is not supported
        UnicodeEncodeError: If the text contains a lone surrogate
    """
    return _segmenter(lang).sentenize(text)


def segment(text: str, *, lang: str) -> Document:
    """
    Document of a text

    Example:
        >>> from anyseg import segment
        >>> doc = segment("Мама мыла раму", lang="ru")
        >>> [word.form for word in doc.words]
        ['Мама', 'мыла', 'раму']

    Description:
        The same as Segmenter(lang).segment(text), with one segmenter per language

    Arguments:
        text (str): Text
        lang (str): ISO 639-1 code of the language: "ru"

    Returns:
        Document: Segmented text

    Raises:
        ValueError: If the language is not supported
        UnicodeEncodeError: If the text contains a lone surrogate
    """
    return _segmenter(lang).segment(text)

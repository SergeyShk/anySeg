"""Fast segmentation of text into tokens and sentences with positions in the source text"""

from collections.abc import Sequence
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
    read_conllu as _read_conllu,
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

for _sequence in (Tokens, Words, Sentences, Paragraphs, Lines):
    Sequence.register(_sequence)


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
        >>> [span.text for span in sentenize(" Мама мыла раму. Кот спит ", lang="ru")]
        ['Мама мыла раму.', 'Кот спит']

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


def read_conllu(text: str) -> list[Document]:
    r"""
    Documents of a text in CoNLL-U

    Example:
        >>> from anyseg import read_conllu
        >>> conllu = "# text = Ёж идёт\n1\tЁж\t_\t_\t_\t_\t_\t_\t_\t_\n2\tидёт\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
        >>> docs = read_conllu(conllu)
        >>> list(docs[0].tokens), docs[0].to_conllu() == conllu
        ([Token(start=0, end=2, text='Ёж'), Token(start=3, end=7, text='идёт')], True)

    Description:
        The comment newdoc starts a document and newpar a paragraph. The text of a sentence is its
        comment text or, without it, the forms of its tokens with the spaces of SpaceAfter and
        SpacesAfter; sentences of a paragraph are joined by a space (none after SpaceAfter=No),
        paragraphs by a blank line. Comments, multiword token lines and empty nodes are kept, so
        that to_conllu writes the text back as it was, with LF line ends and heads as plain
        numbers. A byte order mark at the start is skipped

    Arguments:
        text (str): Text in CoNLL-U

    Returns:
        list[Document]: Documents in order

    Raises:
        ValueError: If a line is neither a comment, a blank line nor a token line of 10 columns,
            an ID or a head cannot be read, IDs are out of order, a multiword token lacks some of
            its words, a comment follows the token lines of a sentence, a sentence has no words,
            forms do not match the text of their sentence or the text holds characters that no
            token has, or the text does not end with a blank line
    """
    return _read_conllu(text)

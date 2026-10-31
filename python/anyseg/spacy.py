"""
spaCy pipelines with the tokens and sentences of anySeg

Example:
    >>> import spacy
    >>> tokenizer = {"@tokenizers": "anyseg.Tokenizer.v1"}
    >>> nlp = spacy.blank("ru", config={"nlp": {"tokenizer": tokenizer}})
    >>> _ = nlp.add_pipe("anyseg_sentencizer")
    >>> doc = nlp("В 1914 г. цена была 3 000 руб. Затем выросла")
    >>> [token.text for token in doc]
    ['В', '1914', 'г.', 'цена', 'была', '3 000', 'руб', '.', 'Затем', 'выросла']
    >>> [sentence.text for sentence in doc.sents]
    ['В 1914 г. цена была 3 000 руб.', 'Затем выросла']

Description:
    The tokenizer anyseg.Tokenizer.v1 makes the tokens of a pipeline, and the component
    anyseg_sentencizer sets the sentence starts of a document before a parser, which keeps them.
    After installing anyseg[spacy] both are found by name, without importing anyseg. to_doc and
    from_doc convert a document of anySeg to a spaCy document and back
"""

import unicodedata
from collections.abc import Callable, Iterable
from typing import Any

import numpy
from spacy.attrs import IDX, LENGTH, SENT_START
from spacy.language import Language
from spacy.pipeline.senter import senter_score
from spacy.tokens import Doc
from spacy.training import Example
from spacy.util import registry
from spacy.vocab import Vocab

from ._anyseg import Document, Segmenter, Span, Token

__all__ = ["Sentencizer", "Tokenizer", "from_doc", "to_doc"]

# What separates the tokens of anySeg and is in none of them
SEPARATORS = "".join(
    character
    for character in map(chr, range(0x3001))
    if character.isspace() or unicodedata.category(character) == "Cc"
) + ("\u00ad\u200b\u200c\u200d\u2060\ufeff")


@registry.tokenizers("anyseg.Tokenizer.v1")
def create_tokenizer(
    lang: str | None = None,
    abbreviations: list[str] | None = None,
    tokens: list[str] | None = None,
) -> Callable[[Language], "Tokenizer"]:
    """
    Tokenizer anyseg.Tokenizer.v1 of a pipeline: the @tokenizers of the section [nlp.tokenizer]

    Arguments:
        lang (str | None): ISO 639-1 code of the language, None - the language of the pipeline
        abbreviations (list[str] | None): More abbreviations, as for Segmenter
        tokens (list[str] | None): Strings that are one token each, as for Segmenter

    Returns:
        Callable[[Language], Tokenizer]: Function that makes the tokenizer of a pipeline
    """

    def make(nlp: Language) -> Tokenizer:
        language = (nlp.lang if lang is None else lang) or ""
        return Tokenizer(nlp.vocab, language, abbreviations=abbreviations, tokens=tokens)

    return make


class Tokenizer:
    r"""
    Tokenizer of a spaCy pipeline with the tokens of anySeg

    Example:
        >>> from spacy.vocab import Vocab
        >>> tokenizer = Tokenizer(Vocab(), "ru")
        >>> [(token.text, token.whitespace_) for token in tokenizer("Кот\n\nспит  тут")]
        [('Кот', ''), ('\n\n', ''), ('спит', ' '), (' ', ''), ('тут', '')]

    Description:
        The text of a document is the text given. One space after a token is its whitespace,
        and the rest of the text between tokens is a token of its own, as spaCy keeps
        whitespace. In a pipeline the tokenizer is set by its name in the configuration, so that
        a saved pipeline loads with it

    Arguments:
        vocab (Vocab): Vocabulary of the pipeline
        lang (str): ISO 639-1 code of the language: "ru"
        abbreviations (Iterable[str] | None): More abbreviations, as for Segmenter
        tokens (Iterable[str] | None): Strings that are one token each, as for Segmenter

    Raises:
        ValueError: If the language is not supported or a list is wrong, as for Segmenter
        TypeError: If abbreviations or tokens is a string or holds something else
    """

    def __init__(
        self,
        vocab: Vocab,
        lang: str,
        *,
        abbreviations: Iterable[str] | None = None,
        tokens: Iterable[str] | None = None,
    ) -> None:
        self.vocab = vocab
        self.segmenter = Segmenter(lang, abbreviations=abbreviations, tokens=tokens)

    def __call__(self, text: str) -> Doc:
        """
        Document of a text

        Arguments:
            text (str): Text

        Returns:
            Doc: Document

        Raises:
            UnicodeEncodeError: If the text contains a lone surrogate
        """
        words, spaces, _ = layout(text, self.segmenter.tokenize(text))
        return Doc(self.vocab, words=words, spaces=spaces)

    def __repr__(self) -> str:
        return arguments("Tokenizer", self.segmenter)

    # The settings are in the configuration of the pipeline: nothing to save
    def to_bytes(self, **kwargs: object) -> bytes:
        return b""

    def from_bytes(self, data: bytes, **kwargs: object) -> "Tokenizer":
        return self

    def to_disk(self, path: object, **kwargs: object) -> None:
        return None

    def from_disk(self, path: object, **kwargs: object) -> "Tokenizer":
        return self


# Scores of sentences, as the sentencizer of spaCy gives them
Scorer = Callable[..., dict[str, Any]]


@Language.factory(
    "anyseg_sentencizer",
    default_config={
        "lang": None,
        "abbreviations": None,
        "tokens": None,
        "overwrite": False,
        "scorer": {"@scorers": "spacy.senter_scorer.v1"},
    },
    assigns=["token.is_sent_start", "doc.sents"],
    default_score_weights={"sents_f": 1.0, "sents_p": 0.0, "sents_r": 0.0},
)
def create_sentencizer(
    nlp: Language,
    name: str,
    lang: str | None,
    abbreviations: list[str] | None,
    tokens: list[str] | None,
    overwrite: bool,
    scorer: Scorer | None,
) -> "Sentencizer":
    """
    Component anyseg_sentencizer of a pipeline: nlp.add_pipe("anyseg_sentencizer", config={...})

    Arguments:
        nlp (Language): Pipeline
        name (str): Name of the component in the pipeline
        lang (str | None): ISO 639-1 code of the language, None - the language of the pipeline
        abbreviations (list[str] | None): More abbreviations, as for Segmenter
        tokens (list[str] | None): Strings that are one token each, as for Segmenter
        overwrite (bool): Replace the sentence starts set before
        scorer (Scorer | None): Scores of the sentences in an evaluation

    Returns:
        Sentencizer: Component
    """
    language = (nlp.lang if lang is None else lang) or ""
    return Sentencizer(
        language, abbreviations=abbreviations, tokens=tokens, overwrite=overwrite, scorer=scorer
    )


class Sentencizer:
    r"""
    Component of a spaCy pipeline that sets the sentence starts of anySeg

    Example:
        >>> import spacy
        >>> nlp = spacy.blank("ru")
        >>> _ = nlp.add_pipe("anyseg_sentencizer")
        >>> [sentence.text for sentence in nlp("Кот спит в 1914 г. Затем\n\nУтро").sents]
        ['Кот спит в 1914 г.', 'Затем\n\n', 'Утро']

    Description:
        The text of a document is split into sentences by anySeg, whatever the tokens. A sentence
        starts at the token where its first character is, and the whitespace after a sentence
        stays in it. Before a parser the component fixes the sentences, and the parser keeps them

    Arguments:
        lang (str): ISO 639-1 code of the language: "ru"
        abbreviations (Iterable[str] | None): More abbreviations, as for Segmenter
        tokens (Iterable[str] | None): Strings that are one token each, as for Segmenter
        overwrite (bool): Replace the sentence starts set before; False - set only the tokens
            whose start is unknown
        scorer (Scorer | None): Scores of the sentences in an evaluation: precision, recall and F1
            of sentences, as for the sentencizer of spaCy

    Raises:
        ValueError: If the language is not supported or a list is wrong, as for Segmenter
        TypeError: If abbreviations or tokens is a string or holds something else
    """

    def __init__(
        self,
        lang: str,
        *,
        abbreviations: Iterable[str] | None = None,
        tokens: Iterable[str] | None = None,
        overwrite: bool = False,
        scorer: Scorer | None = senter_score,
    ) -> None:
        self.segmenter = Segmenter(lang, abbreviations=abbreviations, tokens=tokens)
        self.overwrite = overwrite
        self.scorer = scorer

    def __call__(self, doc: Doc) -> Doc:
        """
        Sentence starts of a document

        Arguments:
            doc (Doc): Document

        Returns:
            Doc: The same document

        Raises:
            UnicodeEncodeError: If the text contains a lone surrogate
        """
        if not len(doc):
            return doc
        sentences = self.segmenter.sentenize(doc.text)
        edges = doc.to_array([IDX, LENGTH])
        ends = edges[:, 0] + edges[:, 1]
        starts = numpy.array([span.start for span in sentences[1:]], dtype=numpy.uint64)
        values = numpy.full(len(doc), -1, dtype=numpy.int64)
        values[0] = 1
        # The token that ends after the first character of a sentence holds it
        values[numpy.searchsorted(ends, starts, side="right")] = 1
        if not self.overwrite:
            known = doc.to_array([SENT_START]).view(numpy.int64)
            values = numpy.where(known != 0, known, values)
        doc.from_array([SENT_START], values.view(numpy.uint64))  # type: ignore[arg-type]
        return doc

    def score(self, examples: Iterable[Example], **kwargs: Any) -> dict[str, Any]:
        """
        Scores of the sentences of examples: sents_p, sents_r and sents_f

        Arguments:
            examples (Iterable[Example]): Examples with the sentences of the component
            kwargs (Any): Arguments of the scorer

        Returns:
            dict[str, Any]: Scores, none without a scorer
        """
        return self.scorer(examples, **kwargs) if self.scorer is not None else {}

    def __repr__(self) -> str:
        return arguments("Sentencizer", self.segmenter, overwrite=self.overwrite)


def arguments(name: str, segmenter: Segmenter, **flags: bool) -> str:
    """
    Representation of a tokenizer or a component by its settings that differ from the defaults

    Arguments:
        name (str): Name of the class
        segmenter (Segmenter): Segmenter of the settings
        flags (bool): More settings, False by default

    Returns:
        str: Representation
    """
    settings = [repr(segmenter.lang)]
    lists = {"abbreviations": segmenter.abbreviations, "tokens": segmenter.tokens}
    settings += [f"{key}={value!r}" for key, value in {**lists, **flags}.items() if value]
    return f"{name}({', '.join(settings)})"


def to_doc(document: Document, vocab: Vocab) -> Doc:
    """
    spaCy document of a document

    Example:
        >>> from spacy.vocab import Vocab
        >>> from anyseg import Segmenter
        >>> document = Segmenter("ru").segment("Ёж идёт.  Кот спит")
        >>> document.set_columns(upos=["NOUN", "VERB", "PUNCT", "NOUN", "VERB"])
        >>> doc = to_doc(document, Vocab())
        >>> [(token.text, token.pos_) for token in doc]
        [('Ёж', 'NOUN'), ('идёт', 'VERB'), ('.', 'PUNCT'), (' ', 'SPACE'), ('Кот', 'NOUN'), ('спит', 'VERB')]
        >>> [sentence.text for sentence in doc.sents]
        ['Ёж идёт.  ', 'Кот спит']

    Description:
        The text of the spaCy document is the text of the document, with whitespace between the
        tokens as the tokenizer makes it. Sentences come from the document, and lemmas, parts of
        speech, tags and features from its columns where set. Heads and relations are set when
        every word has a head: the relation root becomes ROOT, a word without a relation gets dep
        (a root - ROOT), and a whitespace token depends on the token before it with dep. A
        whitespace token has its text for a lemma and SPACE for a part of speech

    Arguments:
        document (Document): Document
        vocab (Vocab): Vocabulary of the pipeline

    Returns:
        Doc: spaCy document

    Raises:
        ValueError: If the document has a multiword token or a part of speech unknown to spaCy
    """
    tokens = document.tokens
    if len(document.words) != len(tokens):
        raise ValueError("a document with multiword tokens cannot be converted")
    words, spaces, indices = layout(document.text, tokens)
    sentence_starts: list[bool | int | None] = [not index for index in range(len(words))]
    for sentence in document.sentences[1:]:
        sentence_starts[indices[sentence.tokens[0].index]] = True
    heads = document.column("head")
    parse = dependencies(document, len(words), indices) if heads and None not in heads else None
    return Doc(
        vocab,
        words=words,
        spaces=spaces,
        sent_starts=sentence_starts,
        lemmas=filled(document.column("lemma"), indices, list(words)),
        pos=filled(document.column("upos"), indices, ["SPACE"] * len(words)),
        tags=filled(document.column("xpos"), indices, [""] * len(words)),
        morphs=filled(document.column("feats"), indices, [""] * len(words)),
        heads=parse[0] if parse else None,
        deps=parse[1] if parse else None,
    )


def filled(values: list[str | None], indices: list[int], words: list[str]) -> list[str] | None:
    """
    Values of the words of a spaCy document from a column of a document

    Arguments:
        values (list[str | None]): Values of the column
        indices (list[int]): Index of every token of the document among the words
        words (list[str]): Values of the whitespace tokens for all the words, filled in place

    Returns:
        list[str] | None: Values of the words, None if the column is empty
    """
    if all(value is None for value in values):
        return None
    for index, value in zip(indices, values, strict=True):
        words[index] = value or ""
    return words


def dependencies(
    document: Document, count: int, indices: list[int]
) -> tuple[list[int], list[str]]:
    """
    Heads and relations of a spaCy document of a document whose words all have heads

    Arguments:
        document (Document): Document
        count (int): Number of the words of the spaCy document
        indices (list[int]): Index of every token of the document among the words

    Returns:
        tuple[list[int], list[str]]: Heads and relations of the words
    """
    # A whitespace token depends on the token before it, at the start of the text - on the next
    heads = [index - 1 if index else index + 1 for index in range(count)]
    relations = ["dep"] * count
    for sentence in document.sentences:
        first = sentence.tokens[0].index
        for word in sentence.words:
            index = indices[word.index]
            head = word.head
            heads[index] = indices[first + head - 1] if head else index
            relation = word.deprel
            if relation == "root" or (relation is None and not head):
                relations[index] = "ROOT"
            else:
                relations[index] = relation or "dep"
    return heads, relations


def from_doc(doc: Doc) -> Document:
    r"""
    Document of a spaCy document

    Example:
        >>> import spacy
        >>> nlp = spacy.blank("ru")
        >>> _ = nlp.add_pipe("sentencizer")
        >>> document = from_doc(nlp("Ёж идёт.\n Кот спит"))
        >>> document.tokens[3], [sentence.text for sentence in document.sentences]
        (Token(start=10, end=13, text='Кот'), ['Ёж идёт.', 'Кот спит'])

    Description:
        Tokens lose the whitespace and invisible separators at their edges, and tokens of nothing
        else are dropped. A sentence of the spaCy document starts at its first token left, and a
        document without sentences is a sentence per paragraph; every paragraph starts a sentence.
        Lemmas, parts of speech, tags, features, heads and relations become the columns lemma,
        upos, xpos, feats, head and deprel, empty values None; a token without a head has neither.
        The relation ROOT becomes root, and a word whose head is a dropped token depends on its
        head. A sentence has one root: the root of the parse or, if the sentence lacks it, the
        first word whose head is in another sentence; the other such words depend on the root
        with the relation dep

    Arguments:
        doc (Doc): spaCy document

    Returns:
        Document: Document

    Raises:
        ValueError: If a token runs over a blank line or a value cannot be in its column, as for
            Document.set_columns
    """
    starts: list[int] = []
    ends: list[int] = []
    kept: list[int] = []
    positions = [-1] * len(doc)
    sentence_starts: list[int] = []
    marks = (
        doc.to_array([SENT_START]).view(numpy.int64) if doc.has_annotation("SENT_START") else None
    )
    pending = True
    for token in doc:
        if marks is not None and marks[token.i] == 1:
            pending = True
        text = token.text
        rest = text.lstrip(SEPARATORS)
        if not rest:
            continue
        start = token.idx + len(text) - len(rest)
        positions[token.i] = len(kept)
        if pending:
            sentence_starts.append(len(kept))
            pending = False
        starts.append(start)
        ends.append(start + len(rest.rstrip(SEPARATORS)))
        kept.append(token.i)
    document = Document.from_tokens(
        doc.text, starts, ends, sentence_starts=sentence_starts if marks is not None else None
    )
    columns: dict[str, list[Any]] = {}
    for column, attribute, value in (
        ("lemma", "LEMMA", lambda token: token.lemma_),
        ("upos", "POS", lambda token: token.pos_),
        ("xpos", "TAG", lambda token: token.tag_),
        ("feats", "MORPH", lambda token: str(token.morph)),
    ):
        if doc.has_annotation(attribute):
            columns[column] = [value(doc[index]) or None for index in kept]
    if doc.has_annotation("DEP"):
        columns["head"], columns["deprel"] = relations(doc, document, kept, positions)
    if columns:
        document.set_columns(**columns)
    return document


def relations(
    doc: Doc, document: Document, kept: list[int], positions: list[int]
) -> tuple[list[int | None], list[str | None]]:
    """
    Columns head and deprel of a document from the parse of its spaCy document

    Arguments:
        doc (Doc): spaCy document
        document (Document): Document of its tokens left
        kept (list[int]): Index in the spaCy document of every token of the document
        positions (list[int]): Index in the document of every token of the spaCy document, -1
            for a dropped one

    Returns:
        tuple[list[int | None], list[str | None]]: Columns head and deprel
    """
    first = [0] * len(kept)
    for sentence in document.sentences:
        tokens = sentence.tokens
        start = tokens[0].index
        for index in range(start, start + len(tokens)):
            first[index] = start
    heads: list[int | None] = []
    deprels: list[str | None] = []
    roots: dict[int, int] = {}
    loose: list[int] = []
    for position, index in enumerate(kept):
        token = doc[index]
        if not token.has_head():
            heads.append(None)
            deprels.append(None)
            continue
        head = token.head
        # A chain of dropped heads ends at a token left or at the root
        for _ in range(len(doc)):
            if positions[head.i] >= 0 or head.head.i == head.i:
                break
            head = head.head
        target = positions[head.i]
        if head.i == index or target < 0 or first[target] != first[position]:
            if head.i == index:
                roots.setdefault(first[position], position)
            loose.append(position)
            heads.append(0)
            deprels.append("root")
        else:
            heads.append(target - first[position] + 1)
            deprels.append(token.dep_ or None)
    # One root per sentence: the root of the parse, else the first word whose head is outside
    for position in loose:
        root = roots.setdefault(first[position], position)
        if root != position:
            heads[position] = root - first[position] + 1
            deprels[position] = "dep"
    return heads, deprels


def layout(text: str, spans: Iterable[Span | Token]) -> tuple[list[str], list[bool], list[int]]:
    """
    Words and spaces of a spaCy document of the spans of a text

    Description:
        One space after a span is its whitespace, and the rest of the text between spans is a
        word of its own, as spaCy keeps whitespace

    Arguments:
        text (str): Text
        spans (Iterable[Span | Token]): Spans of the text in order, without overlaps

    Returns:
        tuple[list[str], list[bool], list[int]]: Words, spaces and the index of every span among
            the words
    """
    words: list[str] = []
    spaces: list[bool] = []
    indices: list[int] = []
    position = 0
    for span in spans:
        start = span.start
        if start > position:
            gap = text[position:start]
            if words and gap[0] == " ":
                spaces[-1] = True
                gap = gap[1:]
            if gap:
                words.append(gap)
                spaces.append(False)
        indices.append(len(words))
        words.append(span.text)
        spaces.append(False)
        position = span.end
    if position < len(text):
        gap = text[position:]
        if words and gap[0] == " ":
            spaces[-1] = True
            gap = gap[1:]
        if gap:
            words.append(gap)
            spaces.append(False)
    return words, spaces, indices

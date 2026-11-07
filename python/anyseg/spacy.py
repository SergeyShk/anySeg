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
    from_doc convert a document of anySeg to a spaCy document and back. The tokens of a spaCy
    document are those of the text; the words of a multiword token are in its extension
    token._.anyseg_words
"""

import re
import unicodedata
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from typing import Any, Literal

try:
    import spacy  # noqa: F401
except ModuleNotFoundError as error:
    # A broken spaCy fails with its own error
    if error.name != "spacy":
        raise
    raise ImportError('anyseg.spacy needs spaCy: pip install "anyseg[spacy]"') from error

import numpy
from spacy.attrs import IDX, IS_SPACE, LENGTH, SENT_START
from spacy.language import Language
from spacy.pipeline.senter import senter_score
from spacy.tokens import Doc, Span, Token
from spacy.training import Example
from spacy.util import registry
from spacy.vocab import Vocab

from ._anyseg import Document, Segmenter

__all__ = ["Sentencizer", "Tokenizer", "from_doc", "to_doc"]

# What separates the tokens of anySeg and is in none of them
SEPARATORS = "".join(
    character
    for character in map(chr, range(0x3001))
    if character.isspace() or unicodedata.category(character) == "Cc"
) + ("\u00ad\u200b\u200c\u200d\u2060\ufeff")

# What spaCy cannot keep: its strings are in UTF-8
SURROGATES = re.compile(r"[\ud800-\udfff]")

# What a column of a word cannot hold, as in a lemma of tokens merged over a line break
LINE_BREAKS = re.compile(r"[\t\r\n]")

# The columns of a word kept in the extension of its multiword token, besides its form
WORD_COLUMNS = ("lemma", "upos", "xpos", "feats", "head", "deprel", "deps", "misc")


def words_key(start: int, end: int) -> tuple[str, str, int, int]:
    """
    Key of the words of a token in the user data of a spaCy document

    Description:
        Spans of spaCy keep their extensions under such a key, with both ends. Documents shift it
        when they are joined or cut, and a token that retokenizing changes no longer finds it

    Arguments:
        start (int): Position of the first character of the token
        end (int): Position after its last character

    Returns:
        tuple[str, str, int, int]: Key
    """
    return ("._.", "anyseg_words", start, end)


def get_words(token: Token) -> Any:
    """
    Value of the extension token._.anyseg_words

    Arguments:
        token (Token): Token

    Returns:
        Any: Words of the multiword token, None for a token of one word
    """
    data: dict[Any, Any] = token.doc.user_data
    return data.get(words_key(token.idx, token.idx + len(token)))


def set_words(token: Token, value: Any) -> None:
    """
    Sets the value of the extension token._.anyseg_words

    Arguments:
        token (Token): Token
        value (Any): Words of the multiword token, None for a token of one word
    """
    data: dict[Any, Any] = token.doc.user_data
    key = words_key(token.idx, token.idx + len(token))
    if value is None:
        data.pop(key, None)
    else:
        data[key] = value


# Forced: the module may be imported again, and the values are kept in the documents anyway
Token.set_extension("anyseg_words", getter=get_words, setter=set_words, force=True)
# Doc.to_json writes such keys as those of spans, and Doc.from_json sets them on spans
Span.set_extension("anyseg_words", default=None, force=True)


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
        The text of a document is the text given, except that a lone surrogate, which spaCy
        cannot keep, is U+FFFD. One space after a token is its whitespace, and the rest of the
        text between tokens is a token of its own with is_space, as spaCy keeps whitespace. The
        words of a multiword token are in token._.anyseg_words: a list of dictionaries with their
        forms, [{"form": "de"}, {"form": "el"}]; a token of one word has None there, as has a
        token that retokenizing changed. In a pipeline the tokenizer is set by its name in the
        configuration, so that a saved pipeline loads with it

    Arguments:
        vocab (Vocab): Vocabulary of the pipeline
        lang (str): ISO 639-1 code of the language: "ru", "es"
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
        vocab.lex_attr_getters[IS_SPACE] = is_gap  # type: ignore[attr-defined]

    def __call__(self, text: str) -> Doc:
        """
        Document of a text

        Arguments:
            text (str): Text

        Returns:
            Doc: Document
        """
        return self.make_doc(self.segmenter.segment(text))

    def pipe(self, texts: Iterable[str], batch_size: int = 1000) -> Iterator[Doc]:
        """
        Documents of texts, segmented in batches without the GIL as Segmenter.pipe does

        Arguments:
            texts (Iterable[str]): Texts
            batch_size (int): Number of texts in a batch

        Returns:
            Iterator[Doc]: Documents in order

        Raises:
            ValueError: If batch_size is less than 1
            TypeError: If texts is a string or not iterable, or a text is not a string
        """
        for document in self.segmenter.pipe(texts, batch_size=batch_size):
            yield self.make_doc(document)

    def make_doc(self, document: Document) -> Doc:
        """
        spaCy document of the tokens and the multiword tokens of a document

        Arguments:
            document (Document): Document

        Returns:
            Doc: spaCy document
        """
        tokens = document.tokens
        words, spaces, _ = layout(self.vocab, document.text, tokens.starts, tokens.ends)
        doc = Doc(self.vocab, words=words, spaces=spaces)
        keep_words(doc, document)
        return doc

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
        lang (str): ISO 639-1 code of the language: "ru", "es"
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
        tokens as the tokenizer makes it and U+FFFD for a lone surrogate. Sentences come from the document, and lemmas, parts of
        speech, tags and features from its columns where set. Heads and relations are set when
        every word has a head, and the heads of every sentence must form a tree with one root:
        the relation root becomes ROOT, a word without a relation gets dep
        (a root - ROOT), and a whitespace token depends on the token before it with dep. A
        whitespace token has its text for a lemma and SPACE for a part of speech. The words of a
        multiword token go to token._.anyseg_words with their columns, and main marks the one that
        gave the token its values: its word closest to the root, or its first word without heads.
        The vocabulary takes the rule of anySeg for is_space, so that a pickled document keeps it:
        a tokenizer of spaCy on that vocabulary then marks invisible separators as whitespace too

    Arguments:
        document (Document): Document
        vocab (Vocab): Vocabulary of the pipeline

    Returns:
        Doc: spaCy document

    Raises:
        ValueError: If a token takes a part of speech unknown to spaCy, features that are not
            pairs of a name and a value, or the heads of a sentence do not form a tree with one root
    """
    tokens = document.tokens
    vocab.lex_attr_getters[IS_SPACE] = is_gap  # type: ignore[attr-defined]
    words, spaces, indices = layout(vocab, document.text, tokens.starts, tokens.ends)
    sentence_starts: list[bool | int | None] = [not index for index in range(len(words))]
    for sentence in document.sentences[1:]:
        sentence_starts[indices[sentence.tokens[0].index]] = True
    heads = document.column("head")
    parse = dependencies(document, len(words), indices) if heads and None not in heads else None
    multiword = len(document.words) != len(tokens)
    main = parse[2] if parse else None

    def column(name: Literal["lemma", "upos", "xpos", "feats"]) -> list[str | None]:
        nonlocal main
        values = document.column(name)
        if not multiword:
            return values
        if not any(values):
            return [None] * len(tokens)
        if main is None:
            main = [token.words[0].index for token in tokens]
        return [values[word] for word in main]

    feats = column("feats")
    try:
        doc = Doc(
            vocab,
            words=words,
            spaces=spaces,
            sent_starts=sentence_starts,
            lemmas=filled(column("lemma"), indices, list(words)),
            pos=filled(column("upos"), indices, ["SPACE"] * len(words)),
            tags=filled(column("xpos"), indices, [""] * len(words)),
            morphs=filled(feats, indices, [""] * len(words)),
            heads=parse[0] if parse else None,
            deps=parse[1] if parse else None,
        )
    except ValueError as error:
        check_features(feats, error)
        raise
    keep_words(doc, document, main)
    return doc


def check_features(values: list[str | None], error: ValueError) -> None:
    """
    Says which features spaCy could not read, if they are not pairs of a name and a value

    Arguments:
        values (list[str | None]): Features of every token
        error (ValueError): What spaCy raised

    Raises:
        ValueError: If some features are not such pairs
    """
    for token, value in enumerate(values):
        if value and value != "_" and any(pair.count("=") != 1 for pair in value.split("|")):
            raise ValueError(
                f"features of token {token} are not name=value pairs: {value!r}"
            ) from error


def keep_words(doc: Doc, document: Document, main: list[int] | None = None) -> None:
    """
    Words of the multiword tokens of a document in the extension of the tokens of a spaCy document

    Arguments:
        doc (Doc): spaCy document of the same text
        document (Document): Document
        main (list[int] | None): Index of the word of every token that gave it its values, if any
    """
    data: dict[Any, Any] = doc.user_data
    for token in document.multiword_tokens:
        count = len(token.sentence.words)
        data[words_key(token.start, token.end)] = [
            {
                "form": word.form,
                **{
                    column: value
                    for column in WORD_COLUMNS
                    if (value := getattr(word, column)) is not None
                },
                **({"sentence_words": count} if word.head is not None else {}),
                **({"main": True} if main and word.index == main[token.index] else {}),
            }
            for word in token.words
        ]


def filled(values: list[str | None], indices: list[int], words: list[str]) -> list[str] | None:
    """
    Values of the words of a spaCy document from the values of the tokens of a document

    Arguments:
        values (list[str | None]): Value of every token
        indices (list[int]): Index of every token of the document among the words
        words (list[str]): Values of the whitespace tokens for all the words, filled in place

    Returns:
        list[str] | None: Values of the words, None if no token has a value
    """
    if all(value is None for value in values):
        return None
    for index, value in zip(indices, values, strict=True):
        words[index] = value or ""
    return words


def dependencies(
    document: Document, count: int, indices: list[int]
) -> tuple[list[int], list[str], list[int]]:
    """
    Heads and relations of a spaCy document of a document whose words all have heads

    Description:
        A token takes the head and the relation of its word closest to the root, the first of
        such words. Every word above it is outside the token, and the head of a token is closer to
        the root than the token, so the tokens form a tree when the words do

    Arguments:
        document (Document): Document
        count (int): Number of the words of the spaCy document
        indices (list[int]): Index of every token of the document among the words

    Returns:
        tuple[list[int], list[str], list[int]]: Heads and relations of the words, and the index of
            the word of every token that gave them

    Raises:
        ValueError: If the heads of a sentence do not form a tree with one root
    """
    # A whitespace token depends on the token before it, at the start of the text - on the next
    heads = [index - 1 if index else index + 1 for index in range(count)]
    relations = ["dep"] * count
    main = [0] * len(document.tokens)
    multiword = len(document.words) != len(document.tokens)
    for sentence in document.sentences:
        words = sentence.words
        ids = [word.head or 0 for word in words]
        if not tree(ids):
            raise ValueError(
                f"the heads of sentence {sentence.index} do not form a tree with one root"
            )
        first = words[0].index
        holders: Sequence[int]
        if multiword:
            holders = [word.token.index for word in words]
            distances = depths(ids)
            givers = (
                (
                    token.index,
                    min(token.words, key=lambda word: distances[word.index - first]),
                )
                for token in sentence.tokens
            )
        else:
            holders = range(first, first + len(words))
            givers = ((word.index, word) for word in words)
        for token, word in givers:
            main[token] = word.index
            index = indices[token]
            head = word.head
            heads[index] = indices[holders[head - 1]] if head else index
            relation = word.deprel
            if relation == "root" or (relation is None and not head):
                relations[index] = "ROOT"
            else:
                relations[index] = relation or "dep"
    return heads, relations, main


def depths(heads: list[int]) -> list[int]:
    """
    Distance of every word of a sentence from the root

    Arguments:
        heads (list[int]): Head of every word, its ID from 1 or 0 for the root, forming a tree

    Returns:
        list[int]: Number of heads above every word
    """
    known = [-1] * len(heads)
    for start in range(len(heads)):
        path = []
        word = start
        while known[word] < 0 and heads[word]:
            path.append(word)
            word = heads[word] - 1
        depth = max(known[word], 0)
        known[word] = depth
        for seen in reversed(path):
            depth += 1
            known[seen] = depth
    return known


def tree(heads: list[int]) -> bool:
    """
    Whether the heads of the words of a sentence form a tree: one root and no cycles

    Arguments:
        heads (list[int]): Head of every word, its ID from 1 or 0 for the root

    Returns:
        bool: The heads form a tree
    """
    # A head read from CoNLL-U may point past its sentence
    if heads.count(0) != 1 or max(heads) > len(heads):
        return False
    # 0 - not seen, 1 - on the current path, 2 - reaches the root
    state = [0] * len(heads)
    for start in range(len(heads)):
        path = []
        word = start
        while state[word] == 0:
            state[word] = 1
            path.append(word)
            if heads[word] == 0:
                break
            word = heads[word] - 1
        if state[word] == 1 and heads[word] != 0:
            return False
        for seen in path:
            state[seen] = 2
    return True


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
        upos, xpos, feats, head and deprel, empty values None, the features in the order of UD; a
        token without a head has neither.
        The relation ROOT becomes root, and a word whose head is a dropped token depends on its
        head. A sentence has one root: the root of the parse or, if the sentence lacks it, the
        first word whose head is in another sentence; the other such words depend on the root
        with the relation dep.
        A line break or a tab inside a lemma, which merged tokens may have, becomes a space, with
        the spaces around it, and at its edge goes away.
        A token with token._.anyseg_words is a multiword token. Its words keep the columns they
        have there, except the word marked main, which takes the value of the token, as a pipeline
        may have changed it; so an edit of that word in the extension gives way to the token.
        Without a mark, the first word takes the value of the token for a column none of them has.
        Heads there are kept if the parse of the sentence still agrees with them: every word has a
        head and the number of words of its sentence in sentence_words, the head of the token is
        the head of one of its words, which then stands for the token, the marked one if it can,
        and the words form a tree. Otherwise the first word takes the head and the relation of the
        token, and the other words depend on it with the relation dep. A parse knows only tokens,
        so a word of one token that depends on another word of a multiword token than the one that
        stands for it moves to that one

    Arguments:
        doc (Doc): spaCy document

    Returns:
        Document: Document

    Raises:
        ValueError: If a token runs over a blank line, the extension is not a list of words with
            their forms or holds a form that cannot be in a document, or a value cannot be in its
            column, as for Document.set_columns
        TypeError: If a value of the extension is of a wrong type, as for Document.set_columns
    """
    starts: list[int] = []
    ends: list[int] = []
    kept: list[int] = []
    multiword: dict[int, list[dict[str, Any]]] = {}
    given = given_words(doc)
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
        if given:
            value = given.get((token.idx, token.idx + len(text)))
            if value is not None:
                multiword[len(kept)] = checked_words(value, len(kept))
        kept.append(token.i)
    document = Document.from_tokens(
        doc.text,
        starts,
        ends,
        sentence_starts=sentence_starts if marks is not None else None,
        words={token: [word["form"] for word in words] for token, words in multiword.items()},
    )
    columns: dict[str, list[Any]] = {}
    for column, attribute, value in (
        ("lemma", "LEMMA", lambda token: token.lemma_),
        ("upos", "POS", lambda token: token.pos_),
        ("xpos", "TAG", lambda token: token.tag_),
        ("feats", "MORPH", lambda token: features(str(token.morph))),
    ):
        if doc.has_annotation(attribute):
            columns[column] = [value(doc[index]) or None for index in kept]
    lemmas = columns.get("lemma")
    if lemmas and LINE_BREAKS.search("".join(filter(None, lemmas))):
        columns["lemma"] = [
            (one_line(lemma) or None) if lemma and LINE_BREAKS.search(lemma) else lemma
            for lemma in lemmas
        ]
    if doc.has_annotation("DEP"):
        columns["head"], columns["deprel"] = relations(doc, document, kept, positions)
    if multiword:
        columns = words_columns(document, columns, multiword)
    if columns:
        document.set_columns(**columns)
    return document


def one_line(value: str) -> str:
    """
    A value of a column with a space in place of its line breaks and tabs and the spaces around them

    Arguments:
        value (str): Value

    Returns:
        str: Value on one line
    """
    return " ".join(filter(None, (part.strip() for part in LINE_BREAKS.split(value))))


def given_words(doc: Doc) -> dict[tuple[int, int], Any]:
    """
    Values of the extension in a spaCy document: one pass over its user data, as a token is slow

    Arguments:
        doc (Doc): spaCy document

    Returns:
        dict[tuple[int, int], Any]: Values by the ends of their tokens
    """
    data: dict[Any, Any] = doc.user_data
    return {
        key[2:]: value
        for key, value in data.items()
        if isinstance(key, tuple) and len(key) == 4 and key[:2] == ("._.", "anyseg_words")
    }


def checked_words(value: Any, token: int) -> list[dict[str, Any]]:
    """
    Words of a multiword token from the value of its extension

    Arguments:
        value (Any): Value of the extension
        token (int): Index of the token in the document

    Returns:
        list[dict[str, Any]]: Words

    Raises:
        ValueError: If the value is not a list or a tuple of mappings with a form that is a string
    """
    if not isinstance(value, list | tuple) or not all(
        isinstance(word, Mapping) and isinstance(word.get("form"), str) for word in value
    ):
        raise ValueError(
            f"anyseg_words of token {token} must be a list of words with their forms, "
            f"such as [{{'form': 'de'}}, {{'form': 'el'}}]"
        )
    return list(value)


def words_columns(
    document: Document, columns: dict[str, list[Any]], multiword: dict[int, list[dict[str, Any]]]
) -> dict[str, list[Any]]:
    """
    Columns of the words of a document from the columns of its tokens and its multiword tokens

    Arguments:
        document (Document): Document with its multiword tokens
        columns (dict[str, list[Any]]): Value of every token by column, heads as IDs among tokens
        multiword (dict[int, list[dict[str, Any]]]): Words of the multiword tokens by the token

    Returns:
        dict[str, list[Any]]: Value of every word by column
    """
    count = len(document.tokens)
    result: dict[str, list[Any]] = {}
    present = {name for words in multiword.values() for word in words for name in word}
    for column in WORD_COLUMNS:
        if column in ("head", "deprel") or (column not in columns and column not in present):
            continue
        values = columns.get(column, [None] * count)
        out: list[Any] = []
        for position, value in enumerate(values):
            given = multiword.get(position)
            if given is None:
                out.append(value)
                continue
            own = [word.get(column) for word in given]
            main = main_word(given)
            # The token gave its value to the word it came from, as a pipeline may have changed it
            if main is not None and value is not None:
                own[main] = value
            elif all(word is None for word in own):
                own[0] = value
            out.extend(own)
        result[column] = out
    if "head" in columns:
        result["head"], result["deprel"] = word_relations(
            document, columns["head"], columns["deprel"], multiword
        )
    return result


def word_relations(
    document: Document,
    heads: list[int | None],
    deprels: list[str | None],
    multiword: dict[int, list[dict[str, Any]]],
) -> tuple[list[int | None], list[str | None]]:
    """
    Columns head and deprel of the words of a document from those of its tokens

    Arguments:
        document (Document): Document with its multiword tokens
        heads (list[int | None]): Head of every token, its position in the sentence from 1 or 0
        deprels (list[str | None]): Relation of every token
        multiword (dict[int, list[dict[str, Any]]]): Words of the multiword tokens by the token

    Returns:
        tuple[list[int | None], list[str | None]]: Head and relation of every word
    """
    word_heads: list[int | None] = []
    word_deprels: list[str | None] = []
    for sentence in document.sentences:
        tokens = sentence.tokens
        start = tokens[0].index
        span = range(start, start + len(tokens))
        if not any(token in multiword for token in span):
            word_heads.extend(heads[token] for token in span)
            word_deprels.extend(deprels[token] for token in span)
            continue
        # The ID of the first word of every token
        ids = {}
        words = 1
        for token in span:
            ids[token] = words
            words += len(multiword[token]) if token in multiword else 1
        ids[span.stop] = words
        kept = given_relations(span, ids, heads, deprels, multiword)
        kept = kept or first_word_relations(span, ids, heads, deprels)
        word_heads.extend(kept[0])
        word_deprels.extend(kept[1])
    return word_heads, word_deprels


def main_word(words: list[dict[str, Any]]) -> int | None:
    """
    Position of the word of a multiword token that gave the token its values, if it is marked

    Arguments:
        words (list[dict[str, Any]]): Words of the token

    Returns:
        int | None: Position among the words, None without a mark
    """
    return next((offset for offset, word in enumerate(words) if word.get("main")), None)


def given_relations(
    span: range,
    ids: dict[int, int],
    heads: list[int | None],
    deprels: list[str | None],
    multiword: dict[int, list[dict[str, Any]]],
) -> tuple[list[int | None], list[str | None]] | None:
    """
    Heads and relations of the words of a sentence, if the extension still agrees with its parse

    Arguments:
        span (range): Tokens of the sentence
        ids (dict[int, int]): ID of the first word of every token, and after the last token
        heads (list[int | None]): Head of every token, its position in the sentence from 1 or 0
        deprels (list[str | None]): Relation of every token
        multiword (dict[int, list[dict[str, Any]]]): Words of the multiword tokens by the token

    Returns:
        tuple[list[int | None], list[str | None]] | None: Head and relation of every word, None if
            the parse changed since the extension was written
    """
    owners = [token for token in span for _ in range(ids[token + 1] - ids[token])]
    standing = dict.fromkeys(span, 0)
    for token in span:
        given = multiword.get(token)
        if given is None:
            continue
        head = heads[token]
        if head is None or not all(
            type(word.get("head")) is int
            and 0 <= word["head"] <= len(owners)
            and word.get("sentence_words") == len(owners)
            for word in given
        ):
            return None
        target = span.start + head - 1 if head else None
        found = [
            offset
            for offset, word in enumerate(given)
            if (owners[word["head"] - 1] if word["head"] else None) == target
        ]
        if not found:
            return None
        main = main_word(given)
        # Without a mark, words may share a head token: the relation of the token tells which one gave its values
        standing[token] = (
            main
            if main in found
            else next(
                (
                    offset
                    for offset in found
                    if (given[offset].get("deprel") or "dep") == deprels[token]
                ),
                found[0],
            )
        )
    word_heads: list[int | None] = []
    word_deprels: list[str | None] = []
    for token in span:
        head = heads[token]
        if head is None:
            return None
        given = multiword.get(token)
        if given is None:
            target = span.start + head - 1
            word_heads.append(ids[target] + standing[target] if head else 0)
            word_deprels.append(deprels[token])
            continue
        word_heads.extend(word["head"] for word in given)
        word_deprels.extend(
            deprels[token] if offset == standing[token] else word.get("deprel")
            for offset, word in enumerate(given)
        )
    return (word_heads, word_deprels) if tree([head or 0 for head in word_heads]) else None


def first_word_relations(
    span: range,
    ids: dict[int, int],
    heads: list[int | None],
    deprels: list[str | None],
) -> tuple[list[int | None], list[str | None]]:
    """
    Heads and relations of the words of a sentence from the parse of its tokens alone: the first
    word of a token takes them, and the other words depend on it with the relation dep

    Arguments:
        span (range): Tokens of the sentence
        ids (dict[int, int]): ID of the first word of every token, and after the last token
        heads (list[int | None]): Head of every token, its position in the sentence from 1 or 0
        deprels (list[str | None]): Relation of every token

    Returns:
        tuple[list[int | None], list[str | None]]: Head and relation of every word
    """
    word_heads: list[int | None] = []
    word_deprels: list[str | None] = []
    for token in span:
        head = heads[token]
        word_heads.append(ids[span.start + head - 1] if head else head)
        word_deprels.append(deprels[token])
        others = ids[token + 1] - ids[token] - 1
        word_heads.extend([None if head is None else ids[token]] * others)
        word_deprels.extend([None if head is None else "dep"] * others)
    return word_heads, word_deprels


def features(morph: str) -> str:
    """
    FEATS of UD from the features of spaCy: sorted by name regardless of case, as UD orders them

    Arguments:
        morph (str): Features of a spaCy token

    Returns:
        str: Features
    """
    return "|".join(sorted(morph.split("|"), key=lambda feature: feature.split("=")[0].lower()))


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


def is_gap(string: str) -> bool:
    """
    Rule of a vocabulary for is_space: a word of nothing but what separates the tokens of anySeg

    Description:
        The rule goes with the vocabulary, so that the lexemes it makes in a worker of nlp.pipe,
        in a pickle or from a DocBin have is_space too

    Arguments:
        string (str): Word

    Returns:
        bool: The word is whitespace
    """
    return bool(string) and all(character in SEPARATORS for character in string)


def layout(
    vocab: Vocab, text: str, starts: list[int], ends: list[int]
) -> tuple[list[str], list[bool], list[int]]:
    """
    Words and spaces of a spaCy document of the tokens of a text

    Description:
        One space after a token is its whitespace, and the rest of the text between tokens is a
        word of its own with is_space, as spaCy keeps whitespace. A lone surrogate is U+FFFD, of
        the same length

    Arguments:
        vocab (Vocab): Vocabulary of the pipeline
        text (str): Text
        starts (list[int]): Positions of the first characters of the tokens, in order
        ends (list[int]): Positions after their last characters

    Returns:
        tuple[list[str], list[bool], list[int]]: Words, spaces and the index of every token among
            the words
    """
    try:
        text.encode()
    except UnicodeEncodeError:
        text = SURROGATES.sub("\ufffd", text)
    words: list[str] = []
    spaces: list[bool] = []
    indices: list[int] = []

    def add_gap(gap: str) -> None:
        if words and gap[0] == " ":
            spaces[-1] = True
            gap = gap[1:]
        if gap:
            # Invisible separators are no whitespace to spaCy: the components after the tokenizer
            # would take them for words. A lexeme made before the rule of is_gap is set by hand
            vocab[gap].is_space = True
            words.append(gap)
            spaces.append(False)

    position = 0
    for start, end in zip(starts, ends, strict=True):
        if start > position:
            add_gap(text[position:start])
        indices.append(len(words))
        words.append(text[start:end])
        spaces.append(False)
        position = end
    if position < len(text):
        add_gap(text[position:])
    return words, spaces, indices

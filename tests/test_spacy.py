"""spaCy pipelines with the tokens and sentences of anySeg"""

import importlib.metadata
import pickle
import random

import pytest

from anyseg import Document, Segmenter, read_conllu

spacy = pytest.importorskip("spacy")

from spacy.tokens import Doc  # noqa: E402
from spacy.util import get_words_and_spaces  # noqa: E402
from spacy.vocab import Vocab  # noqa: E402

from anyseg.spacy import SEPARATORS, Sentencizer, Tokenizer, from_doc, to_doc  # noqa: E402

TOKENIZER = {"nlp": {"tokenizer": {"@tokenizers": "anyseg.Tokenizer.v1"}}}
TEXT = "  Ёж идёт.\u200bКот спит в 1914 г. Затем\n\n Утро \ufeff\n"


@pytest.fixture(scope="module")
def nlp():
    nlp = spacy.blank("ru", config=TOKENIZER)
    nlp.add_pipe("anyseg_sentencizer")
    return nlp


def layout(doc):
    return [(token.text, token.whitespace_) for token in doc]


def sentences(doc):
    return [sentence.text for sentence in doc.sents]


def test_entry_points():
    tokenizers = importlib.metadata.entry_points(group="spacy_tokenizers")
    factories = importlib.metadata.entry_points(group="spacy_factories")
    assert [point.value for point in tokenizers if point.name == "anyseg.Tokenizer.v1"] == [
        "anyseg.spacy:create_tokenizer"
    ]
    assert [point.value for point in factories if point.name == "anyseg_sentencizer"] == [
        "anyseg.spacy:create_sentencizer"
    ]


def test_tokenizer(nlp):
    doc = nlp.make_doc(TEXT)
    assert isinstance(nlp.tokenizer, Tokenizer)
    assert doc.text == TEXT
    assert layout(doc) == [
        *(("  ", ""), ("Ёж", " "), ("идёт", ""), (".", ""), ("\u200b", ""), ("Кот", " ")),
        *(("спит", " "), ("в", " "), ("1914", " "), ("г", ""), (".", " "), ("Затем", "")),
        *(("\n\n ", ""), ("Утро", " "), ("\ufeff\n", "")),
    ]
    assert [token.text for token in doc if token.text.strip(SEPARATORS)] == [
        span.text for span in Segmenter("ru").tokenize(TEXT)
    ]
    assert layout(nlp.make_doc("")) == []
    assert layout(nlp.make_doc(" ")) == [(" ", "")]
    assert layout(nlp.make_doc("Ёж  ")) == [("Ёж", " "), (" ", "")]


def test_tokenizer_whitespace_as_spacy():
    # One space after a token is its whitespace, the rest of the gap is a token
    segmenter = Segmenter("ru")
    tokenizer = Tokenizer(Vocab(), "ru")
    pieces = ["Ёж", "идёт", ".", "1914", " ", "  ", "\n", "\t", "\u00a0", "\u3000"]
    generator = random.Random(7)
    for _ in range(500):
        text = "".join(generator.choices(pieces, k=generator.randrange(12)))
        words = [span.text for span in segmenter.tokenize(text)]
        doc = tokenizer(text)
        assert doc.text == text
        expected = get_words_and_spaces(words, text) if words else ([text], [False])
        assert ([token.text for token in doc], [bool(token.whitespace_) for token in doc]) == (
            expected if text else ([], [])
        ), repr(text)


def test_tokenizer_settings():
    nlp = spacy.blank(
        "ru", config={"nlp": {"tokenizer": {"tokens": ["C++"], **TOKENIZER["nlp"]["tokenizer"]}}}
    )
    assert [token.text for token in nlp("Язык C++")] == ["Язык", "C++"]
    custom = {"@tokenizers": "anyseg.Tokenizer.v1", "lang": "ru", "abbreviations": ["сущ"]}
    nlp = spacy.blank("xx", config={"nlp": {"tokenizer": custom}})
    assert [token.text for token in nlp("Это сущ. Это")] == ["Это", "сущ.", "Это"]
    assert nlp.tokenizer.segmenter == Segmenter("ru", abbreviations=["сущ"])
    assert repr(nlp.tokenizer) == "Tokenizer('ru', abbreviations=('сущ',))"
    with pytest.raises(ValueError, match="unsupported language 'en'"):
        spacy.blank("en", config=TOKENIZER)
    with pytest.raises(TypeError, match="not a string"):
        Tokenizer(Vocab(), "ru", tokens="C++")


def test_sentencizer(nlp):
    doc = nlp(TEXT)
    assert sentences(doc) == [
        "  Ёж идёт.\u200b",
        "Кот спит в 1914 г.",
        "Затем\n\n ",
        "Утро \ufeff\n",
    ]
    assert [token.is_sent_start for token in doc] == [
        True, *[False] * 4, True, *[False] * 5, True, False, True, False,
    ]  # fmt: skip
    assert nlp.get_pipe_meta("anyseg_sentencizer").assigns == ["token.is_sent_start", "doc.sents"]
    assert len(nlp("")) == 0


def test_sentencizer_with_spacy_tokens():
    nlp = spacy.blank("ru")
    nlp.add_pipe("anyseg_sentencizer")
    # A sentence that starts inside a token starts at that token
    doc = nlp("Пишите на ivan@mail.ru!Ответим. Ок")
    assert [token.text for token in doc][2] == "ivan@mail.ru!Ответим"
    assert sentences(doc) == ["Пишите на", "ivan@mail.ru!Ответим.", "Ок"]


def test_sentencizer_keeps_starts():
    nlp = spacy.blank("ru", config=TOKENIZER)

    @spacy.Language.component("test_starts")
    def starts(doc):
        doc[2].is_sent_start = True
        doc[4].is_sent_start = False
        return doc

    nlp.add_pipe("test_starts")
    nlp.add_pipe("anyseg_sentencizer")
    assert sentences(nlp("Ёж идёт домой. Кот спит")) == ["Ёж идёт", "домой. Кот спит"]
    nlp.replace_pipe("anyseg_sentencizer", "anyseg_sentencizer", config={"overwrite": True})
    assert sentences(nlp("Ёж идёт домой. Кот спит")) == ["Ёж идёт домой.", "Кот спит"]
    sentencizer = Sentencizer("ru", tokens=["т.н."], overwrite=True)
    assert repr(sentencizer) == "Sentencizer('ru', tokens=('т.н.',), overwrite=True)"
    assert repr(nlp.get_pipe("anyseg_sentencizer")) == "Sentencizer('ru', overwrite=True)"
    doc = sentencizer(nlp.make_doc("Это т.н. Кот"))
    assert sentences(doc) == ["Это т.н. Кот"]


def test_settings_in_configuration(tmp_path):
    tokenizer = {"tokens": ["C++"], "abbreviations": ["сущ"], **TOKENIZER["nlp"]["tokenizer"]}
    nlp = spacy.blank("ru", config={"nlp": {"tokenizer": tokenizer}})
    nlp.add_pipe("anyseg_sentencizer", config={"abbreviations": ["сущ"]})
    text = "Язык C++ - сущ. Это"
    expected = layout(nlp(text)), sentences(nlp(text))
    assert expected == (
        [("Язык", " "), ("C++", " "), ("-", " "), ("сущ.", " "), ("Это", "")],
        ["Язык C++ - сущ. Это"],
    )
    nlp.to_disk(tmp_path)
    loaded = spacy.load(tmp_path)
    assert (layout(loaded(text)), sentences(loaded(text))) == expected
    restored = spacy.blank("ru", config=nlp.config).from_bytes(nlp.to_bytes())
    assert (layout(restored(text)), sentences(restored(text))) == expected
    copy = pickle.loads(pickle.dumps(nlp))
    assert (layout(copy(text)), sentences(copy(text))) == expected


def test_tokenizer_of_a_saved_pipeline(tmp_path):
    spacy.blank("ru").to_disk(tmp_path)
    nlp = spacy.load(tmp_path, config=TOKENIZER)
    assert [token.text for token in nlp("В 1914 г. цена 3 000 руб.")] == [
        *("В", "1914", "г.", "цена", "3 000", "руб", "."),
    ]


def test_to_doc():
    document = Segmenter("ru").segment("  Ёж идёт.\u200bКот спит\n")
    document.set_columns(
        lemma=["ёж", "идти", ".", "кот", "спать"],
        upos=["NOUN", "VERB", "PUNCT", "NOUN", None],
        feats=["Case=Nom", None, None, None, None],
        head=[2, 0, 2, 2, 0],
        deprel=["nsubj", "root", "punct", None, "root"],
    )
    doc = to_doc(document, Vocab())
    assert doc.text == document.text
    assert layout(doc) == [
        *(("  ", ""), ("Ёж", " "), ("идёт", ""), (".", ""), ("\u200b", "")),
        *(("Кот", " "), ("спит", ""), ("\n", "")),
    ]
    assert sentences(doc) == ["  Ёж идёт.\u200b", "Кот спит\n"]
    assert [(token.lemma_, token.pos_) for token in doc] == [
        *(("  ", "SPACE"), ("ёж", "NOUN"), ("идти", "VERB"), (".", "PUNCT")),
        *(("\u200b", "SPACE"), ("кот", "NOUN"), ("спать", ""), ("\n", "SPACE")),
    ]
    assert [str(token.morph) for token in doc] == ["", "Case=Nom", *[""] * 6]
    assert not doc.has_annotation("TAG")
    assert [(token.head.i, token.dep_) for token in doc] == [
        *((1, "dep"), (2, "nsubj"), (2, "ROOT"), (2, "punct"), (3, "dep")),
        *((6, "dep"), (6, "ROOT"), (6, "dep")),
    ]


def test_to_doc_without_columns():
    document = Segmenter("ru").segment("Ёж идёт. Кот")
    document.set_columns(head=[2, 0, 2, None])
    doc = to_doc(document, Vocab())
    assert layout(doc) == [("Ёж", " "), ("идёт", ""), (".", " "), ("Кот", "")]
    assert sentences(doc) == ["Ёж идёт.", "Кот"]
    assert not any(doc.has_annotation(name) for name in ("LEMMA", "POS", "TAG", "MORPH", "DEP"))
    assert len(to_doc(Segmenter("ru").segment(""), Vocab())) == 0
    assert layout(to_doc(Segmenter("ru").segment(" \n"), Vocab())) == [(" \n", "")]


def test_to_doc_of_multiword_tokens():
    conllu = "# text = del\n1-2\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n1\tde\t_\t_\t_\t_\t_\t_\t_\t_\n2\tel\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
    with pytest.raises(ValueError, match="multiword tokens"):
        to_doc(read_conllu(conllu)[0], Vocab())


def test_from_doc(nlp):
    doc = nlp(TEXT)
    document = from_doc(doc)
    assert isinstance(document, Document)
    assert document.text == TEXT
    assert [token.text for token in document.tokens] == [
        span.text for span in Segmenter("ru").tokenize(TEXT)
    ]
    assert [sentence.text for sentence in document.sentences] == [
        span.text for span in Segmenter("ru").sentenize(TEXT)
    ]
    assert all(value is None for value in document.column("upos"))
    assert layout(to_doc(document, nlp.vocab)) == layout(doc)
    assert sentences(to_doc(document, nlp.vocab)) == sentences(doc)


def test_from_doc_trims_tokens():
    vocab = Vocab()
    words = ["\ufeffЁж", "\u00a0", "идёт\u200b", "\u2060", "\n\n", "Кот\u00ad"]
    doc = Doc(vocab, words=words, spaces=[True, False, False, False, False, False])
    document = from_doc(doc)
    assert [(token.start, token.text) for token in document.tokens] == [
        (1, "Ёж"),
        (5, "идёт"),
        (13, "Кот"),
    ]
    assert [sentence.text for sentence in document.sentences] == ["Ёж \u00a0идёт", "Кот"]
    with pytest.raises(ValueError, match="paragraph boundary"):
        from_doc(Doc(vocab, words=["Ёж\n\nидёт"]))


def test_from_doc_sentences():
    vocab = Vocab()
    words = ["Ёж", "идёт", " ", "Кот", "спит", "\n", "Утро"]
    starts = [True, False, True, False, False, True, False]
    doc = Doc(
        vocab,
        words=words,
        spaces=[True, False, False, True, False, False, False],
        sent_starts=starts,
    )
    document = from_doc(doc)
    # A sentence of a whitespace token starts at the next token left
    assert [sentence.text for sentence in document.sentences] == ["Ёж идёт", "Кот спит", "Утро"]
    unmarked = Doc(vocab, words=words, spaces=[True, False, False, True, False, False, False])
    assert [sentence.text for sentence in from_doc(unmarked).sentences] == [
        "Ёж идёт Кот спит\nУтро"
    ]


def test_from_doc_columns():
    vocab = Vocab()
    words = ["Заголовок", "\n\n", "Мама", "мыла", "раму", ".", "\n"]
    doc = Doc(
        vocab,
        words=words,
        spaces=[False, False, True, True, False, False, False],
        lemmas=["заголовок", "\n\n", "мама", "мыть", "рама", ".", "\n"],
        pos=["NOUN", "SPACE", "NOUN", "VERB", "NOUN", "PUNCT", "SPACE"],
        tags=["NN", "_SP", "NN", "VB", "", "PU", "_SP"],
        morphs=["Case=Nom", "", "Case=Nom", "", "Case=Acc", "", ""],
        # The head of the title is in the next paragraph, the head of "мыла" is whitespace
        heads=[3, 0, 3, 1, 3, 3, 5],
        deps=["nsubj", "dep", "nsubj", "ROOT", "obj", "punct", "dep"],
    )
    document = from_doc(doc)
    assert [sentence.text for sentence in document.sentences] == ["Заголовок", "Мама мыла раму."]
    assert document.column("lemma") == ["заголовок", "мама", "мыть", "рама", "."]
    assert document.column("upos") == ["NOUN", "NOUN", "VERB", "NOUN", "PUNCT"]
    assert document.column("xpos") == ["NN", "NN", "VB", None, "PU"]
    assert document.column("feats") == ["Case=Nom", "Case=Nom", None, "Case=Acc", None]
    assert document.column("head") == [0, 2, 0, 2, 2]
    assert document.column("deprel") == ["root", "nsubj", "root", "obj", "punct"]


def test_from_doc_head_chains():
    vocab = Vocab()
    words = ["Ёж", " ", "\n", "идёт", "\t"]
    spaces = [False] * 5
    # The heads of "Ёж" go round the whitespace tokens, the head of "идёт" ends at "Ёж"
    doc = Doc(
        vocab, words=words, spaces=spaces, heads=[1, 2, 1, 4, 0], deps=["x", "y", "z", "w", "v"]
    )
    assert from_doc(doc).column("head") == [0, 1]
    assert from_doc(doc).column("deprel") == ["root", "w"]
    doc = Doc(
        vocab, words=words, spaces=spaces, heads=[3, 3, 3, 1, 3], deps=["x", "y", "z", "w", "v"]
    )
    assert from_doc(doc).column("head") == [2, 0]
    assert from_doc(doc).column("deprel") == ["x", "root"]


def test_separators():
    # The separators are what the segmenter leaves out of tokens
    segmenter = Segmenter("ru")
    characters = (chr(code) for code in range(0x10000) if not 0xD800 <= code < 0xE000)
    assert {character for character in characters if not segmenter.tokenize(character)} == set(
        SEPARATORS
    )

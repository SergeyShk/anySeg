"""spaCy pipelines with the tokens and sentences of anySeg"""

import importlib.metadata
import pickle
import random

import pytest

from anyseg import Document, Segmenter, read_conllu

spacy = pytest.importorskip("spacy")

from spacy.tokens import Doc, DocBin  # noqa: E402
from spacy.training import Example  # noqa: E402
from spacy.util import get_words_and_spaces  # noqa: E402
from spacy.vocab import Vocab  # noqa: E402

from anyseg.spacy import (  # noqa: E402
    SEPARATORS,
    Sentencizer,
    Tokenizer,
    from_doc,
    to_doc,
    word_relations,
)

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


def test_tokenizer_pipe(nlp):
    texts = [TEXT, "", "Кот спит", " "]
    assert [layout(doc) for doc in nlp.tokenizer.pipe(texts, batch_size=2)] == [
        layout(nlp.make_doc(text)) for text in texts
    ]
    with pytest.raises(TypeError, match="not a string"):
        list(nlp.tokenizer.pipe("Кот"))


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


@pytest.mark.parametrize(
    "gap", ["\u200b\u200b", "\ufeff", "\u2060 ", " \u00ad", "\u200c\u200d", "\x01", " \u200b\n"]
)
def test_tokenizer_invisible_separators_are_whitespace(gap):
    text = f"из{gap}стали"
    tokenizer = Tokenizer(Vocab(), "ru")
    for doc in (
        tokenizer(text),
        next(tokenizer.pipe([text])),
        to_doc(Segmenter("ru").segment(text), Vocab()),
    ):
        assert doc.text == text
        assert [token.is_space for token in doc] == [False, True, False]
        assert doc[1].text.isspace() or not doc[1].is_alpha


def test_invisible_separators_stay_whitespace_out_of_the_process():
    nlp = spacy.blank("ru", config=TOKENIZER)
    doc = nlp("из\u200bстали")
    expected = [False, True, False]
    assert [token.is_space for token in pickle.loads(pickle.dumps(doc))] == expected
    other = spacy.blank("ru", config=TOKENIZER)
    restored = next(DocBin(docs=[doc]).get_docs(other.vocab))
    assert [token.is_space for token in restored] == expected
    # The rule goes with a pickled vocabulary, and the lexeme made before it is marked by hand
    vocab = Vocab()
    assert not vocab["\u2060"].is_space
    assert not to_doc(Segmenter("ru").segment("а"), vocab)[0].is_space
    assert [
        token.is_space for token in to_doc(Segmenter("ru").segment("а\u2060б"), vocab)
    ] == expected
    assert pickle.loads(pickle.dumps(vocab))["\u200b"].is_space


def test_tokenizer_spaces_stay_whitespace():
    tokenizer = Tokenizer(Vocab(), "ru")
    assert [token.is_space for token in tokenizer("Ёж\n\n идёт  тут")] == [
        False,
        True,
        False,
        True,
        False,
    ]
    # The tokens of a text are not marked as whitespace
    assert not tokenizer("a\u200bb")[0].is_space
    assert not tokenizer.vocab["a"].is_space


def test_lone_surrogates(nlp):
    high, low = chr(0xD83D), chr(0xDE00)
    text = f"Ёж {high} идёт. {high}{low} Кот{low}\u200b спит"
    kept = text.replace(high, "\ufffd").replace(low, "\ufffd")
    assert nlp(text).text == kept
    assert [token.text for token in nlp(text)] == [token.text for token in nlp(kept)]
    assert [sentence.text for sentence in nlp(text).sents] == [
        sentence.text for sentence in nlp(kept).sents
    ]
    assert next(nlp.tokenizer.pipe([text])).text == kept
    document = Segmenter("ru").segment(text)
    doc = to_doc(document, Vocab())
    assert doc.text == kept
    assert [token.idx for token in doc if not token.is_space] == document.tokens.starts
    assert from_doc(doc).text == kept


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


def test_sentencizer_scores(nlp):
    reference = Doc(
        nlp.vocab,
        words=["Ёж", "идёт", ".", "Кот", "спит"],
        spaces=[True, False, True, True, False],
        sent_starts=[True, False, False, True, False],
    )
    scores = nlp.evaluate([Example(nlp.make_doc(reference.text), reference)])
    assert (scores["sents_p"], scores["sents_r"], scores["sents_f"]) == (1.0, 1.0, 1.0)
    weights = {"sents_f": 1.0, "sents_p": 0.0, "sents_r": 0.0}
    assert nlp.get_pipe_meta("anyseg_sentencizer").default_score_weights == weights
    assert nlp.config["components"]["anyseg_sentencizer"]["scorer"] == {
        "@scorers": "spacy.senter_scorer.v1"
    }
    assert Sentencizer("ru", scorer=None).score([]) == {}


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


MULTIWORD = (
    "# text = Vengo del mar y decírselo.\n"
    "1\tVengo\tvenir\tVERB\t_\t_\t0\troot\t_\t_\n"
    "2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n"
    "2\tde\tde\tADP\t_\t_\t4\tcase\t_\t_\n"
    "3\tel\tel\tDET\t_\tGender=Masc\t4\tdet\t_\t_\n"
    "4\tmar\tmar\tNOUN\t_\t_\t1\tobl\t_\t_\n"
    "5\ty\ty\tCCONJ\t_\t_\t6\tcc\t_\t_\n"
    "6-8\tdecírselo\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n"
    "6\tdecir\tdecir\tVERB\t_\t_\t1\tconj\t_\t_\n"
    "7\tse\tél\tPRON\t_\t_\t6\tiobj\t_\t_\n"
    "8\tlo\tél\tPRON\t_\t_\t6\tobj\t_\t_\n"
    "9\t.\t.\tPUNCT\t_\t_\t1\tpunct\t_\t_\n\n"
)


def test_to_doc_of_multiword_tokens():
    doc = to_doc(read_conllu(MULTIWORD)[0], Vocab())
    assert [token.text for token in doc] == ["Vengo", "del", "mar", "y", "decírselo", "."]
    assert doc.text == "Vengo del mar y decírselo."
    assert [token.lemma_ for token in doc] == ["venir", "de", "mar", "y", "decir", "."]
    assert [token.pos_ for token in doc] == ["VERB", "ADP", "NOUN", "CCONJ", "VERB", "PUNCT"]
    assert [token.head.text for token in doc] == [
        "Vengo",
        "mar",
        "Vengo",
        "decírselo",
        "Vengo",
        "Vengo",
    ]
    assert [token.dep_ for token in doc] == ["ROOT", "case", "obl", "cc", "conj", "punct"]
    assert doc[0]._.anyseg_words is None
    assert doc[1]._.anyseg_words == [
        {
            "form": "de",
            "lemma": "de",
            "upos": "ADP",
            "head": 4,
            "deprel": "case",
            "sentence_words": 9,
            "main": True,
        },
        {
            "form": "el",
            "lemma": "el",
            "upos": "DET",
            "feats": "Gender=Masc",
            "head": 4,
            "deprel": "det",
            "sentence_words": 9,
        },
    ]
    assert [word["form"] for word in doc[4]._.anyseg_words] == ["decir", "se", "lo"]


def test_multiword_tokens_there_and_back():
    document = read_conllu(MULTIWORD)[0]
    back = from_doc(to_doc(document, Vocab()))
    assert back.text == document.text
    assert [token.text for token in back.tokens] == [token.text for token in document.tokens]
    for column in ("form", "lemma", "upos", "feats", "head", "deprel"):
        assert back.column(column) == document.column(column), column


def test_tokens_of_a_multiword_token_stay_a_tree():
    # The words of a token sit apart in the tree: its first word depends on a word under its second
    conllu = (
        "# text = ab c d\n"
        "1-2\tab\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "1\ta\t_\t_\t_\t_\t3\tx\t_\t_\n"
        "2\tb\t_\t_\t_\t_\t4\ty\t_\t_\n"
        "3\tc\t_\t_\t_\t_\t2\tz\t_\t_\n"
        "4\td\t_\t_\t_\t_\t0\troot\t_\t_\n\n"
    )
    doc = to_doc(read_conllu(conllu)[0], Vocab())
    assert [token.head.text for token in doc] == ["d", "ab", "d"]
    assert [token.dep_ for token in doc] == ["y", "z", "ROOT"]


def test_to_doc_of_a_segmented_multiword_token():
    document = Segmenter("es").segment("Vengo del mar")
    doc = to_doc(document, Vocab())
    assert [token.text for token in doc] == ["Vengo", "del", "mar"]
    assert doc[1]._.anyseg_words == [{"form": "de"}, {"form": "el"}]
    assert from_doc(doc).column("form") == ["Vengo", "de", "el", "mar"]


def test_tokenizer_of_multiword_tokens():
    nlp = spacy.blank("es", config=TOKENIZER)
    nlp.add_pipe("anyseg_sentencizer")
    text = "Vengo  del mar. Quiero decírselo"
    doc = nlp(text)
    assert doc.text == text
    assert [token.text for token in doc] == [
        "Vengo",
        " ",
        "del",
        "mar",
        ".",
        "Quiero",
        "decírselo",
    ]
    assert doc[2]._.anyseg_words == [{"form": "de"}, {"form": "el"}]
    assert doc[3]._.anyseg_words is None
    assert doc[6]._.anyseg_words == [{"form": "decir"}, {"form": "se"}, {"form": "lo"}]
    assert [sentence.text for sentence in doc.sents] == ["Vengo  del mar.", "Quiero decírselo"]
    assert [d[1]._.anyseg_words for d in nlp.tokenizer.pipe(["Voy al mar"])] == [
        [{"form": "a"}, {"form": "el"}]
    ]
    document = from_doc(doc)
    assert document.column("form") == [
        "Vengo",
        "de",
        "el",
        "mar",
        ".",
        "Quiero",
        "decir",
        "se",
        "lo",
    ]
    assert [len(sentence.words) for sentence in document.sentences] == [5, 4]


def test_multiword_tokens_after_a_pipeline():
    nlp = spacy.blank("es", config=TOKENIZER)
    doc = nlp("Vengo del mar")
    doc[1].lemma_ = "del"
    doc[1].pos_ = "ADP"
    doc[0].head = doc[2]
    doc[1].head = doc[2]
    doc[0].dep_, doc[1].dep_, doc[2].dep_ = "x", "case", "ROOT"
    document = from_doc(doc)
    assert document.column("lemma")[1:3] == ["del", None]
    assert document.column("upos")[1:3] == ["ADP", None]
    # Heads count words: «mar» is the fourth, and the other word depends on the first of its token
    assert document.column("head") == [4, 4, 2, 0]
    assert document.column("deprel") == ["x", "case", "dep", "root"]
    assert to_doc(document, nlp.vocab).has_annotation("DEP")


def test_multiword_tokens_are_kept_with_the_document():
    nlp = spacy.blank("es", config=TOKENIZER)
    doc = nlp("Vengo del mar")
    words = [{"form": "de"}, {"form": "el"}]
    assert Doc(nlp.vocab).from_bytes(doc.to_bytes())[1]._.anyseg_words == words
    assert pickle.loads(pickle.dumps(doc))[1]._.anyseg_words == words
    batch = DocBin(store_user_data=True)
    batch.add(doc)
    assert list(next(iter(batch.get_docs(nlp.vocab)))[1]._.anyseg_words) == words
    assert from_doc(next(iter(batch.get_docs(nlp.vocab)))).column("form")[1:3] == ["de", "el"]


def test_wrong_multiword_tokens_in_a_doc():
    doc = spacy.blank("es")("Vengo del mar")
    doc[1]._.anyseg_words = [{"form": "de"}]
    with pytest.raises(ValueError, match="fewer than two forms"):
        from_doc(doc)
    doc[1]._.anyseg_words = [{"form": "de"}, {"form": "e\tl"}]
    with pytest.raises(ValueError, match="tab or a line break"):
        from_doc(doc)
    for value in (
        [{"lemma": "de"}, {"form": "el"}],
        ["de", "el"],
        "de el",
        {"form": "de"},
        5,
        [{"form": 1}, {"form": 2}],
    ):
        doc[1]._.anyseg_words = value
        with pytest.raises(ValueError, match="anyseg_words of token 1 must be a list"):
            from_doc(doc)


def conllu_of(text, words, multiword=None):
    lines = [f"# text = {text}"]
    for index, (form, head) in enumerate(words, 1):
        if index == multiword:
            lines.append(f"{index}-{index + 1}\t{text.split()[index - 1]}" + "\t_" * 8)
        lines.append(f"{index}\t{form}\t_\t_\t_\t_\t{head}\tx\t_\t_")
    return "\n".join(lines) + "\n\n"


def test_words_of_a_token_stand_for_it_there_and_back():
    conllu = (
        "# text = ab c d\n"
        "1-2\tab\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "1\ta\t_\t_\t_\t_\t3\tx\t_\t_\n"
        "2\tb\t_\t_\t_\t_\t4\ty\t_\t_\n"
        "3\tc\t_\t_\t_\t_\t2\tz\t_\t_\n"
        "4\td\t_\t_\t_\t_\t0\troot\t_\t_\n\n"
    )
    document = read_conllu(conllu)[0]
    back = from_doc(to_doc(document, Vocab()))
    assert back.column("head") == [3, 4, 2, 0]
    assert back.column("deprel") == ["x", "y", "z", "root"]
    assert to_doc(back, Vocab()).has_annotation("DEP")


def test_words_keep_the_word_that_gave_the_values():
    conllu = (
        "# text = al del x\n"
        "1-2\tal\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "1\ta\t_\t_\t_\t_\t0\troot\t_\t_\n"
        "2\tel\t_\t_\t_\t_\t5\tdep\t_\t_\n"
        "3-4\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "3\tde\t_\t_\t_\t_\t2\tdep\t_\t_\n"
        "4\tel\t_\t_\t_\t_\t1\tdep\t_\t_\n"
        "5\tx\t_\t_\t_\t_\t4\tdep\t_\t_\n\n"
    )
    document = read_conllu(conllu)[0]
    assert from_doc(to_doc(document, Vocab())).column("head") == [0, 5, 2, 1, 4]


def test_values_of_a_token_changed_after_to_doc():
    doc = to_doc(read_conllu(MULTIWORD)[0], Vocab())
    doc[1].pos_ = "NOUN"
    doc[1].lemma_ = "del"
    back = from_doc(doc)
    assert back.column("upos")[1:3] == ["NOUN", "DET"]
    assert back.column("lemma")[1:3] == ["del", "el"]


def test_columns_of_words_without_heads():
    document = Segmenter("es").segment("Vengo del mar")
    document.set_columns(lemma=["venir", "de", "el", "mar"])
    doc = to_doc(document, Vocab())
    assert [token.lemma_ for token in doc] == ["venir", "de", "mar"]
    assert from_doc(doc).column("lemma") == ["venir", "de", "el", "mar"]
    doc[1].lemma_ = "del"
    assert from_doc(doc).column("lemma") == ["venir", "del", "el", "mar"]


def test_edit_of_the_marked_word_gives_way_to_its_token():
    doc = to_doc(read_conllu(MULTIWORD)[0], Vocab())
    for word in doc[1]._.anyseg_words:
        word["lemma"] = word["lemma"].upper()
    assert from_doc(doc).column("lemma")[1:3] == ["de", "EL"]


def test_words_through_json():
    nlp = spacy.blank("es", config=TOKENIZER)
    data = nlp("Vengo del mar").to_json(underscore=["anyseg_words"])
    restored = Doc(nlp.vocab).from_json(data)
    assert from_doc(restored).column("form") == ["Vengo", "de", "el", "mar"]


def test_features_that_spacy_cannot_read():
    for feats, token, value in ((["Foo", "Case=Nom"], 0, "Foo"), ([None, "a=b=c"], 1, "a=b=c")):
        document = Segmenter("es").segment("a b")
        document.set_columns(feats=feats)
        with pytest.raises(
            ValueError, match=f"features of token {token} are not name=value pairs: '{value}'"
        ):
            to_doc(document, Vocab())


def test_part_of_speech_that_spacy_cannot_read():
    document = Segmenter("es").segment("a b")
    document.set_columns(upos=["BAD", None], feats=["Case=Nom", None])
    with pytest.raises(ValueError, match="BAD"):
        to_doc(document, Vocab())


def test_features_that_spacy_can_read():
    # The empty value of CoNLL-U, and a word that gives its token nothing
    for feats, expected in (
        (["_", None, None], [None, None, None]),
        ([None, None, "Foo"], [None, None, "Foo"]),
    ):
        document = Segmenter("es").segment("x al")
        document.set_columns(feats=feats)
        assert from_doc(to_doc(document, Vocab())).column("feats") == expected


@pytest.mark.parametrize("mark", ["\n", "\t", " \r\n "])
def test_lemma_of_merged_tokens(mark):
    doc = Doc(Vocab(), words=[f"Nueva{mark}York", "vino"], lemmas=[f"Nueva{mark}York", "venir"])
    assert from_doc(doc).column("lemma") == ["Nueva York", "venir"]
    doc = Doc(Vocab(), words=["a", "b", "c"], lemmas=[f"{mark}a", mark, " c"])
    assert from_doc(doc).column("lemma") == ["a", None, " c"]


def test_parse_changed_after_to_doc():
    conllu = conllu_of("del mar hoy", [("de", 3), ("el", 3), ("mar", 4), ("hoy", 0)], 1)
    doc = to_doc(read_conllu(conllu)[0], Vocab())
    doc[1].head = doc[0]
    doc[0].head = doc[2]
    document = from_doc(doc)
    # The heads of the extension would make a cycle: «mar» depends on «de» now
    assert [(word.form, word.head) for word in document.words] == [
        ("de", 4),
        ("el", 1),
        ("mar", 1),
        ("hoy", 0),
    ]
    assert to_doc(document, Vocab()).has_annotation("DEP")


def test_heads_of_the_extension_that_make_no_tree():
    doc = spacy.blank("es")("ab c d")
    doc[0].head = doc[2]
    doc[1].head = doc[0]
    doc[0].dep_, doc[1].dep_, doc[2].dep_ = "x", "y", "ROOT"
    count = {"sentence_words": 4}
    doc[0]._.anyseg_words = [{"form": "a", "head": 3, **count}, {"form": "b", "head": 2, **count}]
    # No word of the token has its head in the head of the token, so the first word takes it
    assert from_doc(doc).column("head") == [4, 1, 1, 0]
    # The head of «b» agrees with the token, and «a» depends on itself: the words form no tree
    doc[0]._.anyseg_words = [{"form": "a", "head": 1, **count}, {"form": "b", "head": 4, **count}]
    assert from_doc(doc).column("head") == [4, 1, 1, 0]
    # The same with a word that agrees: they stand for the token and form a tree
    doc[0]._.anyseg_words = [{"form": "a", "head": 3, **count}, {"form": "b", "head": 4, **count}]
    assert from_doc(doc).column("head") == [3, 4, 2, 0]


@pytest.mark.parametrize(
    "word",
    [
        {"form": "x"},
        {"form": "x", "head": "x", "sentence_words": 4},
        {"form": "x", "head": 1.5, "sentence_words": 4},
        {"form": "x", "head": True, "sentence_words": 4},
        {"form": "x", "head": -1, "sentence_words": 4},
        {"form": "x", "head": 9, "sentence_words": 4},
        {"form": "x", "head": None, "sentence_words": 4},
        {"form": "x", "head": 4},
        {"form": "x", "head": 4, "sentence_words": 7},
    ],
)
@pytest.mark.parametrize("first", [True, False])
def test_wrong_heads_in_the_extension_are_absent(word, first):
    doc = spacy.blank("es")("ab c d")
    doc[0].head = doc[2]
    doc[1].head = doc[0]
    doc[0].dep_, doc[1].dep_, doc[2].dep_ = "x", "y", "ROOT"
    right = {"form": "y", "head": 4, "sentence_words": 4}
    doc[0]._.anyseg_words = [word, right] if first else [right, word]
    assert from_doc(doc).column("head") == [4, 1, 1, 0]


def test_a_word_without_a_head_in_the_extension():
    conllu = (
        "# text = Vengo del mar\n"
        "1\tVengo\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "2\tde\t_\t_\t_\t_\t4\tcase\t_\t_\n"
        "3\tel\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "4\tmar\t_\t_\t_\t_\t_\t_\t_\t_\n\n"
    )
    doc = to_doc(read_conllu(conllu)[0], Vocab())
    doc[1].head, doc[1].dep_ = doc[2], "case"
    doc[2].head, doc[2].dep_ = doc[0], "obl"
    doc[0].dep_ = "ROOT"
    assert from_doc(doc).column("head") == [0, 4, 2, 1]


def test_a_token_without_a_head_next_to_the_extension():
    doc = spacy.blank("es")("Vengo del mar")
    count = {"sentence_words": 4}
    doc[1]._.anyseg_words = [
        {"form": "de", "head": 4, **count},
        {"form": "el", "head": 4, **count},
    ]
    doc[1].head, doc[1].dep_ = doc[2], "case"
    doc[2].dep_ = "ROOT"
    assert from_doc(doc).column("head") == [None, 4, 2, 0]


def test_the_relation_of_a_token_tells_its_word():
    conllu = (
        "# text = ab cd\n"
        "1-2\tab\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "1\ta\t_\t_\t_\t_\t4\tx\t_\t_\n"
        "2\tb\t_\t_\t_\t_\t3\ty\t_\t_\n"
        "3-4\tcd\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "3\tc\t_\t_\t_\t_\t0\troot\t_\t_\n"
        "4\td\t_\t_\t_\t_\t3\tz\t_\t_\n\n"
    )
    document = read_conllu(conllu)[0]
    back = from_doc(to_doc(document, Vocab()))
    assert back.column("deprel") == ["x", "y", "root", "z"]
    assert back.column("head") == [4, 3, 0, 3]


def test_the_word_without_a_relation_stands_for_its_token():
    conllu = (
        "# text = ab cd\n"
        "1-2\tab\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "1\ta\t_\t_\t_\t_\t4\tb\t_\t_\n"
        "2\tb\t_\t_\t_\t_\t3\t_\t_\t_\n"
        "3-4\tcd\t_\t_\t_\t_\t_\t_\t_\t_\n"
        "3\tc\t_\t_\t_\t_\t0\troot\t_\t_\n"
        "4\td\t_\t_\t_\t_\t3\tz\t_\t_\n\n"
    )
    back = from_doc(to_doc(read_conllu(conllu)[0], Vocab()))
    assert back.column("deprel") == ["b", "dep", "root", "z"]


def test_a_merge_elsewhere_in_the_sentence():
    lines = ["# text = Quiero a mi madre darlo hoy"]
    for index, (form, head, relation) in enumerate(
        [
            ("Quiero", 0, "root"),
            ("a", 4, "case"),
            ("mi", 4, "det"),
            ("madre", 1, "obl"),
            ("dar", 1, "xcomp"),
            ("lo", 5, "obj"),
            ("hoy", 5, "advmod"),
        ],
        1,
    ):
        if index == 5:
            lines.append("5-6\tdarlo" + "\t_" * 8)
        lines.append(f"{index}\t{form}\t_\t_\t_\t_\t{head}\t{relation}\t_\t_")
    doc = to_doc(read_conllu("\n".join(lines) + "\n\n")[0], Vocab())
    with doc.retokenize() as retokenizer:
        retokenizer.merge(doc[1:4])
    assert [(word.form, word.head) for word in from_doc(doc).words] == [
        ("Quiero", 0),
        ("a mi madre", 1),
        ("dar", 1),
        ("lo", 3),
        ("hoy", 3),
    ]


def test_a_value_set_on_a_token_that_retokenizing_changed():
    nlp = spacy.blank("es", config=TOKENIZER)
    doc = nlp("Vengo del mar")
    with doc.retokenize() as retokenizer:
        retokenizer.split(doc[1], ["de", "l"], heads=[(doc[1], 1), doc[2]])
    doc[1]._.anyseg_words = [{"form": "d"}, {"form": "e"}]
    with doc.retokenize() as retokenizer:
        retokenizer.merge(doc[1:3])
    assert doc[1]._.anyseg_words == [{"form": "de"}, {"form": "el"}]
    assert from_doc(doc).column("form") == ["Vengo", "de", "el", "mar"]


def test_sentences_changed_after_to_doc():
    conllu = conllu_of(
        "Hoy voy. Al mar llego tarde ya.",
        [
            ("Hoy", 2),
            ("voy", 0),
            (".", 2),
            ("A", 6),
            ("el", 6),
            ("mar", 7),
            ("llego", 2),
            ("tarde", 7),
            ("ya", 7),
            (".", 7),
        ],
    ).replace("4\tA\t", "4-5\tAl" + "\t_" * 8 + "\n4\tA\t")
    nlp = spacy.blank("es")
    sentencizer = nlp.add_pipe("anyseg_sentencizer", config={"overwrite": True})
    doc = sentencizer(to_doc(read_conllu(conllu)[0], Vocab()))
    document = from_doc(doc)
    assert len(document.sentences) == 2
    assert [(word.form, word.head) for word in document.sentences[1].words] == [
        ("A", 3),
        ("el", 1),
        ("mar", 4),
        ("llego", 0),
        ("tarde", 4),
        ("ya", 4),
        (".", 4),
    ]


def test_extension_does_not_follow_retokenizing():
    nlp = spacy.blank("es", config=TOKENIZER)
    doc = nlp("Vengo del mar")
    with doc.retokenize() as retokenizer:
        retokenizer.merge(doc[1:3])
    assert doc[1]._.anyseg_words is None
    assert from_doc(doc).column("form") == ["Vengo", "del mar"]
    doc = nlp("Vengo del mar")
    with doc.retokenize() as retokenizer:
        retokenizer.split(doc[1], ["de", "l"], heads=[(doc[1], 1), doc[2]])
    assert from_doc(doc).column("form") == ["Vengo", "de", "l", "mar"]
    doc = nlp("Vengo del mar")
    with doc.retokenize() as retokenizer:
        retokenizer.merge(doc[0:2])
    assert from_doc(doc).column("form") == ["Vengo del", "mar"]


def test_extension_can_be_set_and_cleared():
    doc = spacy.blank("es")("Vengo del mar")
    assert doc[1]._.anyseg_words is None
    doc[1]._.anyseg_words = [{"form": "de"}, {"form": "el"}]
    assert from_doc(doc).column("form") == ["Vengo", "de", "el", "mar"]
    doc[1]._.anyseg_words = None
    assert doc[1]._.anyseg_words is None
    assert not doc.user_data
    assert from_doc(doc).column("form") == ["Vengo", "del", "mar"]
    doc.user_data["other"] = 1
    assert from_doc(doc).column("form") == ["Vengo", "del", "mar"]


def test_words_of_a_doc_without_a_parse():
    document = read_conllu(MULTIWORD)[0]
    doc = to_doc(document, Vocab())
    fresh = Doc(
        doc.vocab,
        words=[token.text for token in doc],
        spaces=[bool(token.whitespace_) for token in doc],
    )
    fresh.user_data.update(doc.user_data)
    back = from_doc(fresh)
    assert back.column("form") == document.column("form")
    assert back.column("lemma")[1:3] == ["de", "el"]
    assert back.column("head") == [None] * len(back.words)


def test_some_tokens_without_a_head():
    doc = spacy.blank("es")("Vengo del mar")
    doc[1]._.anyseg_words = [{"form": "de", "head": 4}, {"form": "el", "head": 4}]
    columns = {"head": [None, None, 0], "deprel": [None, None, "root"]}
    document = Document.from_tokens(doc.text, [0, 6, 10], [5, 9, 13], words={1: ["de", "el"]})
    heads, deprels = word_relations(
        document, columns["head"], columns["deprel"], {1: doc[1]._.anyseg_words}
    )
    assert heads == [None, None, None, 0]
    assert deprels == [None, None, None, "root"]
    words = [{"form": "de", "head": 4}, {"form": "el", "head": 4}]
    heads, deprels = word_relations(document, [None, 3, 0], [None, "x", "root"], {1: words})
    assert heads == [None, 4, 2, 0]
    assert deprels == [None, "x", "dep", "root"]


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


def test_from_doc_features_in_the_order_of_ud():
    # spaCy sorts the features with their case, UD without it
    doc = Doc(Vocab(), words=["5"], morphs=["Number=Sing|NumForm=Digit|Case=Nom"])
    assert str(doc[0].morph) == "Case=Nom|NumForm=Digit|Number=Sing"
    assert from_doc(doc).column("feats") == ["Case=Nom|Number=Sing|NumForm=Digit"]


def test_to_doc_of_heads_that_are_not_a_tree():
    for heads in ([0, 0, 2], [2, 3, 1], [2, 1, 0]):
        document = Segmenter("ru").segment("Ёж идёт домой")
        document.set_columns(head=heads)
        with pytest.raises(ValueError, match="sentence 0 do not form a tree"):
            to_doc(document, Vocab())
    # CoNLL-U may hold a head past its sentence
    conllu = "1\tЁж\t_\t_\t_\t_\t5\tx\t_\t_\n2\tидёт\t_\t_\t_\t_\t0\troot\t_\t_\n\n"
    with pytest.raises(ValueError, match="do not form a tree"):
        to_doc(read_conllu(conllu)[0], Vocab())


def test_from_doc_one_root():
    vocab = Vocab()
    # A parser that joins a heading with the next paragraph, both ways
    words = ["Глава", "первая", "\n\n", "Мама", "мыла", "раму", "."]
    doc = Doc(
        vocab,
        words=words,
        spaces=[True, False, False, True, True, False, False],
        heads=[4, 3, 1, 0, 4, 4, 4],
        deps=["nsubj", "amod", "dep", "appos", "ROOT", "obj", "punct"],
    )
    document = from_doc(doc)
    assert [sentence.text for sentence in document.sentences] == [
        "Глава первая",
        "Мама мыла раму.",
    ]
    assert document.column("head") == [0, 1, 2, 0, 2, 2]
    assert document.column("deprel") == ["root", "dep", "dep", "root", "obj", "punct"]
    assert sentences(to_doc(document, vocab)) == ["Глава первая\n\n", "Мама мыла раму."]
    # The root of the parse is whitespace, a token has no head
    doc = Doc(
        vocab,
        words=["Ёж", "\n", "идёт", "тут"],
        spaces=[False, False, True, False],
        heads=[1, 1, 1, 3],
        deps=["nsubj", "ROOT", "conj", ""],
    )
    assert from_doc(doc).column("head") == [0, 1, None]
    assert from_doc(doc).column("deprel") == ["root", "dep", None]


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

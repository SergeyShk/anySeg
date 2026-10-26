import pickle
import random
from concurrent.futures import ThreadPoolExecutor

import pytest

from anyseg import (
    Document,
    Lines,
    Paragraphs,
    Segmenter,
    Sentences,
    Tokens,
    Words,
    read_conllu,
    segment,
)

# Two paragraphs, a line break inside the first one, an emoji, a byte order mark and two spaces
TEXT = "Ёж идёт домой.\nКот спит 😀\n\n﻿Утро.  Ночь"
TOKENS = [
    (0, 2, "Ёж"),
    (3, 7, "идёт"),
    (8, 14, "домой."),
    (15, 18, "Кот"),
    (19, 23, "спит"),
    (24, 25, "😀"),
    (28, 33, "Утро."),
    (35, 39, "Ночь"),
]


def conllu(*sentences):
    """CoNLL-U of sentences given as lists of comment lines and token lines with columns in tuples"""
    lines = []
    for sentence in sentences:
        lines.extend("\t".join(line) if isinstance(line, tuple) else line for line in sentence)
        lines.append("")
    return "\n".join(lines) + "\n"


UD = conllu(
    [
        "# newdoc id = d1",
        "# sent_id = 1",
        "# text = Vino  del norte.",
        ("1", "Vino", "venir", "VERB", "_", "Mood=Ind", "0", "root", "_", "_"),
        ("2-3", "del", "_", "_", "_", "_", "_", "_", "_", "_"),
        ("2", "de", "de", "ADP", "_", "_", "4", "case", "4:case", "_"),
        ("3", "el", "el", "DET", "_", "_", "4", "det", "4:det", "_"),
        ("3.1", "vino", "venir", "VERB", "_", "_", "_", "_", "0:root", "_"),
        ("4", "norte", "norte", "NOUN", "_", "_", "1", "obl", "1:obl", "SpaceAfter=No"),
        ("5", ".", ".", "PUNCT", "_", "_", "1", "punct", "_", "_"),
    ],
    [
        "# newpar",
        "# sent_id = 2",
        ("1", "Sí", "sí", "INTJ", "_", "_", "0", "root", "_", "SpaceAfter=No"),
        ("2", ".", ".", "PUNCT", "_", "_", "1", "punct", "_", "_"),
    ],
    [
        "# newdoc",
        "# text = 10 000 рублей",
        ("1", "10 000", "10 000", "NUM", "_", "_", "2", "nummod", "_", "_"),
        ("2", "рублей", "рубль", "NOUN", "_", "_", "0", "root", "_", "_"),
    ],
)


@pytest.fixture
def doc():
    return Segmenter("ru").segment(TEXT)


def test_tokens(doc):
    assert doc.text is TEXT
    assert [(token.start, token.end, token.text) for token in doc.tokens] == TOKENS
    assert [token.index for token in doc.tokens] == list(range(8))
    assert [token.spaces_after for token in doc.tokens] == [
        " ",
        " ",
        "\n",
        " ",
        " ",
        "\n\n﻿",
        "  ",
        "",
    ]
    assert repr(doc.tokens[0]) == "Token(start=0, end=2, text='Ёж')"
    assert repr(doc) == "Document(tokens=8, words=8, sentences=2, paragraphs=2)"


def test_words(doc):
    assert [word.form for word in doc.words] == [text for _, _, text in TOKENS]
    assert [word.id for word in doc.words] == [1, 2, 3, 4, 5, 6, 1, 2]
    word = doc.words[6]
    assert word.token == doc.tokens[6]
    assert list(doc.tokens[6].words) == [word]
    assert word.sentence == doc.sentences[1] == doc.tokens[6].sentence
    assert (word.lemma, word.upos, word.xpos, word.feats, word.head) == (None,) * 5
    assert (word.deprel, word.deps, word.misc) == (None,) * 3
    assert repr(word) == "Word(id=1, form='Утро.')"


def test_sentences_and_paragraphs(doc):
    assert [(s.start, s.end, s.text) for s in doc.sentences] == [
        (0, 25, "Ёж идёт домой.\nКот спит 😀"),
        (28, 39, "Утро.  Ночь"),
    ]
    sentence = doc.sentences[1]
    assert list(sentence.tokens) == list(doc.tokens[6:])
    assert list(sentence.words) == list(doc.words[6:])
    assert sentence.paragraph == doc.paragraphs[1]
    assert repr(sentence) == "Sentence(start=28, end=39, text='Утро.  Ночь')"
    paragraph = doc.paragraphs[0]
    assert (paragraph.start, paragraph.end, paragraph.index) == (0, 25, 0)
    assert list(paragraph.sentences) == [doc.sentences[0]]
    assert len(paragraph.tokens) == 6
    assert repr(doc.paragraphs[1]) == "Paragraph(start=28, end=39, text='Утро.  Ночь')"


def test_lines(doc):
    assert [(line.start, line.end, line.text, line.break_before) for line in doc.lines] == [
        (0, 14, "Ёж идёт домой.", None),
        (15, 25, "Кот спит 😀", "author"),
        (28, 39, "Утро.  Ночь", None),
    ]
    assert (
        repr(doc.lines[1]) == "Line(start=15, end=25, text='Кот спит 😀', break_before='author')"
    )


def test_sequences(doc):
    tokens = doc.tokens
    assert isinstance(tokens, Tokens)
    assert isinstance(doc.words, Words)
    assert isinstance(doc.sentences, Sentences)
    assert isinstance(doc.paragraphs, Paragraphs)
    assert isinstance(doc.lines, Lines)
    assert len(tokens) == 8
    assert tokens[-1] == tokens[7]
    middle = tokens[2:5]
    assert isinstance(middle, Tokens)
    assert [token.text for token in middle] == ["домой.", "Кот", "спит"]
    assert [token.text for token in middle[1:]] == ["Кот", "спит"]
    assert middle[-1] == tokens[4]
    assert [token.index for token in tokens[::3]] == [0, 3, 6]
    assert [token.index for token in tokens[::-2]] == [7, 5, 3, 1]
    assert len(tokens[5:2]) == 0
    assert repr(doc.sentences[1].tokens) == (
        "Tokens([Token(start=28, end=33, text='Утро.'), Token(start=35, end=39, text='Ночь')])"
    )
    for index in (8, -9):
        with pytest.raises(IndexError, match="tokens index out of range"):
            tokens[index]
    with pytest.raises(TypeError):
        tokens["0"]


def test_views_compare_by_document(doc):
    other = Segmenter("ru").segment(TEXT)
    assert doc.tokens[0] == doc.tokens[0]
    assert doc.tokens[0] != doc.tokens[1]
    assert doc.tokens[0] != other.tokens[0]
    assert doc.words[0] != doc.tokens[0]
    assert len({doc.tokens[0], doc.tokens[0], doc.tokens[1], other.tokens[0]}) == 3
    assert len({doc.lines[0], doc.lines[0]}) == len({doc.paragraphs[0], doc.paragraphs[0]}) == 1


def test_columns(doc):
    assert doc.column("form") == [text for _, _, text in TOKENS]
    assert doc.column("lemma") == doc.column("head") == [None] * 8
    doc.set_columns(
        upos=["NOUN", "VERB", "ADV", "NOUN", "VERB", "SYM", "NOUN", "NOUN"],
        head=[2, 0, 2, 2, 0, 5, 0, 1],
        lemma=("ёж", None, None, None, None, None, None, None),
    )
    assert doc.column("upos")[:2] == ["NOUN", "VERB"]
    assert doc.column("head")[-1] == 1
    assert repr(doc.words[0]) == "Word(id=1, form='Ёж', lemma='ёж', upos='NOUN', head=2)"
    doc.set_columns(lemma=[None] * 8)
    assert doc.words[0].lemma is None
    with pytest.raises(ValueError, match=r"^unknown column 'id', expected one of: form, lemma"):
        doc.column("id")


@pytest.mark.parametrize(
    ("columns", "error", "message"),
    [
        ({"upos": ["X"]}, ValueError, "upos has 1 values for 8 words"),
        (
            {"feats": [""] * 8},
            ValueError,
            "feats of word 0 is empty or holds a tab or a line break",
        ),
        (
            {"misc": ["a\tb"] * 8},
            ValueError,
            "misc of word 0 is empty or holds a tab or a line break",
        ),
        ({"head": [7] * 8}, ValueError, "head of word 0 is 7, but its sentence has 6 words"),
        ({"head": [-1] * 8}, ValueError, "head of word 0 is -1, not 0 or a word ID"),
        ({"form": ["x"] * 8}, ValueError, "'form' cannot be set, expected: lemma, upos"),
        ({"lemma": [1] * 8}, TypeError, None),
        ({"head": ["1"] * 8}, TypeError, None),
        ({"upos": "NOUN"}, TypeError, None),
    ],
)
def test_wrong_columns(doc, columns, error, message):
    with pytest.raises(error, match=message):
        doc.set_columns(**{"upos": ["X"] * 8, **columns})
    assert doc.column("upos") == [None] * 8


def test_from_tokens():
    text = "😀 Ёж\n\nкот идёт"
    doc = Document.from_tokens(text, [0, 2, 6, 10], [1, 4, 9, 14], sentence_starts=[1])
    assert doc.text is text
    assert [token.text for token in doc.tokens] == ["😀", "Ёж", "кот", "идёт"]
    assert [sentence.text for sentence in doc.sentences] == ["😀", "Ёж", "кот идёт"]
    assert [len(paragraph.sentences) for paragraph in doc.paragraphs] == [2, 1]
    doc = Document.from_tokens(text, (0, 2, 6, 10), (1, 4, 9, 14))
    assert [sentence.text for sentence in doc.sentences] == ["😀 Ёж", "кот идёт"]
    assert len(Document.from_tokens(" ", [], []).sentences) == 0


@pytest.mark.parametrize(
    ("text", "starts", "ends", "sentence_starts", "message"),
    [
        ("ab", [0], [1, 2], None, "1 starts and 2 ends"),
        ("ab", [1], [1], None, "token 0: is empty"),
        ("ab", [0], [3], None, "token 0: ends after the text"),
        ("ab", [0, 0], [1, 2], None, "token 1: overlaps or precedes the token before it"),
        (" ab", [0], [2], None, "token 0: starts or ends with whitespace"),
        ("a\n\nb", [0], [4], None, "token 0: runs over a paragraph boundary"),
        ("a b", [0, 2], [1, 3], [2], "sentence starts must be increasing token indices"),
        ("a b", [0, 2], [1, 3], [1, 1], "sentence starts must be increasing token indices"),
    ],
)
def test_wrong_tokens(text, starts, ends, sentence_starts, message):
    with pytest.raises(ValueError, match=f"^{message}$"):
        Document.from_tokens(text, starts, ends, sentence_starts=sentence_starts)


def test_generated_conllu(doc):
    doc.set_columns(upos=["NOUN", "VERB", "ADV", "NOUN", "VERB", "SYM", "NOUN", "NOUN"])
    assert doc.to_conllu() == conllu(
        [
            "# newdoc",
            "# newpar",
            "# sent_id = 1",
            "# text = Ёж идёт домой. Кот спит 😀",
            ("1", "Ёж", "_", "NOUN", "_", "_", "_", "_", "_", "_"),
            ("2", "идёт", "_", "VERB", "_", "_", "_", "_", "_", "_"),
            ("3", "домой.", "_", "ADV", "_", "_", "_", "_", "_", "SpacesAfter=\\n"),
            ("4", "Кот", "_", "NOUN", "_", "_", "_", "_", "_", "_"),
            ("5", "спит", "_", "VERB", "_", "_", "_", "_", "_", "_"),
            ("6", "😀", "_", "SYM", "_", "_", "_", "_", "_", "_"),
        ],
        [
            "# newpar",
            "# sent_id = 2",
            "# text = Утро.  Ночь",
            ("1", "Утро.", "_", "NOUN", "_", "_", "_", "_", "_", "SpacesAfter=\\s\\s"),
            ("2", "Ночь", "_", "NOUN", "_", "_", "_", "_", "_", "_"),
        ],
    )
    (back,) = read_conllu(doc.to_conllu())
    assert [token.text for token in back.tokens] == [text for _, _, text in TOKENS]
    assert back.column("upos") == doc.column("upos")
    assert [len(paragraph.sentences) for paragraph in back.paragraphs] == [1, 1]


def test_read_conllu():
    docs = read_conllu(UD)
    assert len(docs) == 2
    assert "".join(doc.to_conllu() for doc in docs) == UD
    doc = docs[0]
    assert doc.text == "Vino  del norte.\n\nSí."
    token = doc.tokens[1]
    assert (token.start, token.end, token.text, token.spaces_after) == (6, 9, "del", " ")
    assert [word.form for word in token.words] == ["de", "el"]
    assert [word.id for word in doc.words] == [1, 2, 3, 4, 5, 1, 2]
    word = doc.words[2]
    assert (word.lemma, word.upos, word.head, word.deprel, word.deps, word.token) == (
        "el",
        "DET",
        4,
        "det",
        "4:det",
        token,
    )
    assert doc.words[3].misc == "SpaceAfter=No"
    assert [sentence.text for sentence in doc.sentences] == ["Vino  del norte.", "Sí."]
    assert len(doc.paragraphs) == len(doc.lines) == 2
    assert docs[1].tokens[0].text == "10 000"
    doc.set_columns(upos=["AUX", *doc.column("upos")[1:]])
    assert (
        doc.to_conllu()
        == UD.split("\n\n# newdoc\n")[0].replace("VERB\t_\tMood", "AUX\t_\tMood", 1) + "\n\n"
    )


@pytest.mark.parametrize(
    ("text", "message"),
    [
        ("1\ta\n\n", "line 1: expected 10 tab-separated columns, got 2"),
        (conllu([("2", "a", *["_"] * 8)]), "line 1: expected word 1, got '2'"),
        (conllu(["# text = a", ("1", "b", *["_"] * 8)]), "line 1: the form 'b' does not match"),
        ("1\ta\t_\t_\t_\t_\t_\t_\t_\t_\n", "line 1: the text does not end with a blank line"),
    ],
)
def test_wrong_conllu(text, message):
    with pytest.raises(ValueError, match=message.replace("'", '"')):
        read_conllu(text)


def test_random_texts():
    segmenter = Segmenter("ru")
    rng = random.Random(1)
    alphabet = "ab Ёж́😀.\n\n\r\t   ﻿​"
    for _ in range(300):
        text = "".join(rng.choices(alphabet, k=rng.randrange(60)))
        doc = segmenter.segment(text)
        spans = [(span.start, span.end) for span in segmenter.tokenize(text)]
        assert [(token.start, token.end) for token in doc.tokens] == spans
        sentences = [(span.start, span.end) for span in segmenter.sentenize(text)]
        assert [(sentence.start, sentence.end) for sentence in doc.sentences] == sentences
        assert len(doc.words) == len(doc.tokens)
        for paragraph in doc.paragraphs:
            lines = [line for line in doc.lines if paragraph.start <= line.start < paragraph.end]
            assert lines[0].break_before is None
            assert (lines[0].start, lines[-1].end) == (paragraph.start, paragraph.end)
            assert all(line.break_before == "author" for line in lines[1:])
        for token in doc.tokens:
            assert any(line.start <= token.start and token.end <= line.end for line in doc.lines)
        if doc.tokens:
            (back,) = read_conllu(doc.to_conllu())
            assert [token.text for token in back.tokens] == [token.text for token in doc.tokens]


def test_threads():
    doc = segment(" ".join(f"слово{number}" for number in range(2000)), lang="ru")
    upos = [["NOUN"] * 2000, ["VERB"] * 2000]

    def work(number):
        if number % 5 == 0:
            doc.set_columns(upos=upos[number % 2])
            return None
        values = {word.upos for word in doc.words[::97]}
        assert len(values) <= 2
        return [token.text for token in doc.tokens[number : number + 3]]

    with ThreadPoolExecutor(8) as pool:
        results = list(pool.map(work, range(200)))
    assert results[1] == ["слово1", "слово2", "слово3"]


def test_document_is_not_picklable(doc):
    with pytest.raises(TypeError):
        pickle.dumps(doc)


def test_lone_surrogate():
    with pytest.raises(UnicodeEncodeError):
        segment("a \ud800", lang="ru")
    with pytest.raises(UnicodeEncodeError):
        Document.from_tokens("a \ud800", [0], [1])

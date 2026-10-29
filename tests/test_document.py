import gc
import pickle
import random
import subprocess
import sys
import unicodedata
from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor

import pytest

from anyseg import (
    Document,
    Lines,
    Paragraphs,
    Segmenter,
    Sentences,
    Span,
    Tokens,
    Words,
    read_conllu,
    segment,
)

# Two paragraphs, a line break inside the first one, an emoji, a byte order mark and two spaces
TEXT = "Ёж идёт домой\nкот спит 😀\n\n\ufeffУтро  Ночь"
TOKENS = [
    (0, 2, "Ёж"),
    (3, 7, "идёт"),
    (8, 13, "домой"),
    (14, 17, "кот"),
    (18, 22, "спит"),
    (23, 24, "😀"),
    (27, 31, "Утро"),
    (33, 37, "Ночь"),
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
        "\n\n\ufeff",
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
    assert repr(word) == "Word(id=1, form='Утро')"


def test_sentences_and_paragraphs(doc):
    assert [(s.start, s.end, s.text) for s in doc.sentences] == [
        (0, 24, "Ёж идёт домой\nкот спит 😀"),
        (27, 37, "Утро  Ночь"),
    ]
    sentence = doc.sentences[1]
    assert list(sentence.tokens) == list(doc.tokens[6:])
    assert list(sentence.words) == list(doc.words[6:])
    assert sentence.paragraph == doc.paragraphs[1]
    assert repr(sentence) == "Sentence(start=27, end=37, text='Утро  Ночь')"
    paragraph = doc.paragraphs[0]
    assert (paragraph.start, paragraph.end, paragraph.index) == (0, 24, 0)
    assert list(paragraph.sentences) == [doc.sentences[0]]
    assert len(paragraph.tokens) == 6
    assert repr(doc.paragraphs[1]) == "Paragraph(start=27, end=37, text='Утро  Ночь')"


def test_lines(doc):
    assert [(line.start, line.end, line.text, line.break_before) for line in doc.lines] == [
        (0, 13, "Ёж идёт домой", None),
        (14, 24, "кот спит 😀", "typeset"),
        (27, 37, "Утро  Ночь", None),
    ]
    assert (
        repr(doc.lines[1]) == "Line(start=14, end=24, text='кот спит 😀', break_before='typeset')"
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
    assert [token.text for token in middle] == ["домой", "кот", "спит"]
    assert [token.text for token in middle[1:]] == ["кот", "спит"]
    assert middle[-1] == tokens[4]
    assert [token.index for token in tokens[::3]] == [0, 3, 6]
    assert [token.index for token in tokens[::-2]] == [7, 5, 3, 1]
    assert len(tokens[5:2]) == 0
    assert isinstance(tokens, Sequence)
    assert [token.index for token in reversed(middle)] == [4, 3, 2]
    assert tokens.index(tokens[3]) == 3
    assert middle.index(tokens[3]) == 1
    assert tokens.index(tokens[3], -5, -4) == 3
    assert (tokens.count(tokens[0]), middle.count(tokens[0])) == (1, 0)
    assert tokens[0] in tokens
    assert tokens[0] not in middle
    assert "Ёж" not in tokens
    assert Segmenter("ru").segment(TEXT).tokens[0] not in tokens
    for args in [(tokens[0], 1), (tokens[5], 0, 5), (doc.words[0],)]:
        with pytest.raises(ValueError, match="the value is not among the tokens"):
            tokens.index(*args)
    assert repr(doc.sentences[1].tokens) == (
        "Tokens([Token(start=27, end=31, text='Утро'), Token(start=33, end=37, text='Ночь')])"
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
        ({"feats": [""] * 8}, ValueError, "feats of word 0 is empty or holds whitespace"),
        (
            {"misc": ["a\tb"] * 8},
            ValueError,
            "misc of word 0 is empty or holds a tab or a line break",
        ),
        ({"upos": ["NO UN"] * 8}, ValueError, "upos of word 0 is empty or holds whitespace"),
        (
            {"lemma": ["a\nb"] * 8},
            ValueError,
            "lemma of word 0 is empty or holds a tab or a line break",
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
        ("ab", [-1], [1], None, "token 0: starts before the text"),
        ("a b", [2], [3], None, "token 0: the text before it holds characters that no token has"),
        (
            "a b",
            [0],
            [1],
            None,
            "the text after the last token holds characters that no token has",
        ),
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
            "# text = Ёж идёт домой кот спит 😀",
            ("1", "Ёж", "_", "NOUN", "_", "_", "_", "_", "_", "_"),
            ("2", "идёт", "_", "VERB", "_", "_", "_", "_", "_", "_"),
            ("3", "домой", "_", "ADV", "_", "_", "_", "_", "_", "SpacesAfter=\\n"),
            ("4", "кот", "_", "NOUN", "_", "_", "_", "_", "_", "_"),
            ("5", "спит", "_", "VERB", "_", "_", "_", "_", "_", "_"),
            ("6", "😀", "_", "SYM", "_", "_", "_", "_", "_", "_"),
        ],
        [
            "# newpar",
            "# sent_id = 2",
            "# text = Утро  Ночь",
            ("1", "Утро", "_", "NOUN", "_", "_", "_", "_", "_", "SpacesAfter=\\s\\s"),
            ("2", "Ночь", "_", "NOUN", "_", "_", "_", "_", "_", "_"),
        ],
    )
    (back,) = read_conllu(doc.to_conllu())
    assert [token.text for token in back.tokens] == [text for _, _, text in TOKENS]
    assert back.column("upos") == doc.column("upos")
    assert [len(paragraph.sentences) for paragraph in back.paragraphs] == [1, 1]


def test_document_id(doc):
    lines = doc.to_conllu(doc_id="d1").splitlines()
    assert [line for line in lines if line.startswith(("# newdoc", "# sent_id"))] == [
        "# newdoc id = d1",
        "# sent_id = d1-1",
        "# sent_id = d1-2",
    ]
    assert doc.to_conllu(doc_id="е\u0308ж").startswith("# newdoc id = ёж\n")
    for doc_id in ["", "d 1"]:
        with pytest.raises(ValueError, match="is empty or holds whitespace"):
            doc.to_conllu(doc_id=doc_id)
    (read,) = read_conllu(UD.split("# newdoc\n")[0])
    assert read.to_conllu(doc_id="x") == read.to_conllu()


def test_conllu_in_nfc():
    text = "Мои\u0306 е\u0308ж"
    doc = segment(text, lang="ru")
    doc.set_columns(lemma=["мои\u0306", None], misc=["Gloss=my own", None])
    written = doc.to_conllu()
    assert "# text = Мой ёж\n" in written
    assert "1\tМой\tмой\t_\t_\t_\t_\t_\t_\tGloss=my own\n" in written
    assert doc.text is text
    (read,) = read_conllu(UD.split("# newdoc\n")[0])
    read.set_columns(misc=["MWE=a b", *read.column("misc")[1:]])
    assert read.words[0].misc == "MWE=a b"


def test_spaces_in_conllu():
    written = segment("а\u00a0б\u200bв\u2028г", lang="ru").to_conllu()
    assert "# text = а бв г\n" in written
    assert [line.split("\t")[9] for line in written.splitlines() if line[:1].isdigit()] == [
        "_",
        "SpaceAfter=No",
        "SpacesAfter=\\n",
        "_",
    ]
    doc = Document.from_tokens(
        "Ёж идёт.\nКот спит.", [0, 3, 9, 13], [2, 8, 12, 18], sentence_starts=[2]
    )
    (back,) = read_conllu(doc.to_conllu())
    assert back.text == doc.text
    assert [(line.text, line.break_before) for line in back.lines] == [
        ("Ёж идёт.", None),
        ("Кот спит.", "author"),
    ]


def test_read_conllu():
    docs = read_conllu(UD)
    assert len(docs) == 2
    assert "".join(doc.to_conllu() for doc in docs) == UD
    assert [doc.text for doc in read_conllu("\ufeff" + UD)] == [doc.text for doc in docs]
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
        (conllu(["# text = a", ("1", "b", *["_"] * 8)]), "line 2: the form 'b' does not match"),
        (conllu([("1-2", "ab", *["_"] * 8)]), "line 2: a sentence without words"),
        ("1\ta\t_\t_\t_\t_\t_\t_\t_\t_\n", "line 1: the text does not end with a blank line"),
    ],
)
def test_wrong_conllu(text, message):
    with pytest.raises(ValueError, match=message.replace("'", '"')):
        read_conllu(text)


def test_random_texts():
    segmenter = Segmenter("ru")
    rng = random.Random(1)
    alphabet = "ab Ёж\u0301😀.\n\n\r\t\u00a0\u2028\u2029\ufeff\u200b"
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
            assert all(line.break_before in ("author", "typeset") for line in lines[1:])
        for token in doc.tokens:
            assert any(line.start <= token.start and token.end <= line.end for line in doc.lines)
        if doc.tokens:
            (back,) = read_conllu(doc.to_conllu())
            forms = [unicodedata.normalize("NFC", token.text) for token in doc.tokens]
            assert [token.text for token in back.tokens] == forms


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


@pytest.mark.parametrize("protocol", range(pickle.HIGHEST_PROTOCOL + 1))
def test_document_is_not_picklable(doc, protocol):
    for value in (doc, doc.tokens, doc.tokens[0], doc.words[0], doc.lines, iter(doc.sentences)):
        with pytest.raises(
            TypeError, match=f"cannot pickle 'anyseg.{type(value).__name__}' object"
        ):
            pickle.dumps(value, protocol)


def test_lone_surrogate():
    for text in ("a \ud800", "a " * 5000 + "\ud800"):
        with pytest.raises(UnicodeEncodeError):
            segment(text, lang="ru")
    with pytest.raises(UnicodeEncodeError):
        Document.from_tokens("a \ud800", [0], [1])


def test_not_tracked_by_the_collector(doc):
    sentence = doc.sentences[0]
    values = [
        *Segmenter("ru").tokenize("Ёж идёт"),
        doc.tokens,
        doc.tokens[1:],
        *doc.tokens[::2],
        *doc.words,
        sentence,
        sentence.tokens,
        sentence.paragraph,
        doc.words[0].token,
        iter(doc.lines),
    ]
    assert not any(gc.is_tracked(value) for value in values)


def test_lazy_iteration(doc):
    tokens = iter(doc.tokens)
    assert iter(tokens) is tokens
    assert next(tokens).text == "Ёж"
    assert [token.text for token in tokens] == [text for _, _, text in TOKENS[1:]]
    assert next(tokens, None) is None
    shared = iter(segment(" ".join(f"слово{number}" for number in range(5000)), lang="ru").tokens)
    with ThreadPoolExecutor(4) as pool:
        parts = list(pool.map(lambda _: [token.index for token in shared], range(4)))
    assert sorted(index for part in parts for index in part) == list(range(5000))


def test_huge_integers(doc):
    with pytest.raises(IndexError):
        doc.tokens[10**30]
    with pytest.raises(IndexError):
        doc.tokens[-(10**30)]
    assert doc.tokens.index(doc.tokens[1], -(10**30), 10**30) == 1
    with pytest.raises(ValueError, match="head of word 0 is 1180591620717411303424"):
        doc.set_columns(head=[2**70] + [None] * (len(doc.words) - 1))
    for starts in ([-1], [10**30]):
        with pytest.raises(ValueError, match="sentence starts must be increasing"):
            Document.from_tokens("Ёж идёт", [0, 3], [2, 7], sentence_starts=starts)
    with pytest.raises(ValueError, match="token 1: ends after the text"):
        Document.from_tokens("Ёж идёт", [0, 3], [2, 10**30])


def test_long_texts():
    # Texts this long are segmented without the GIL, from a temporary copy in UTF-8
    piece = "Ёж идёт домой. Кот спит.\n\n"
    text = piece * 400
    segmenter = Segmenter("ru")
    size = sys.getsizeof(text)
    tokens = segmenter.tokenize(text)
    sentences = segmenter.sentenize(text)
    doc = segmenter.segment(text)
    assert sys.getsizeof(text) == size
    assert len(tokens) == 2800 and len(sentences) == 800
    assert tokens[-1] == Span(len(text) - 3, len(text) - 2, ".")
    assert [sentence.text for sentence in sentences[-2:]] == ["Ёж идёт домой.", "Кот спит."]
    assert [(token.start, token.end) for token in doc.tokens] == [(t.start, t.end) for t in tokens]
    (back,) = read_conllu(doc.to_conllu())
    assert [token.text for token in back.tokens] == [token.text for token in tokens]


def test_column_and_set_columns_from_threads():
    # A list made under the lock may start a collection whose finalizers release the GIL to a writer that
    # waits for the lock: the process hung on Python 3.11
    script = """
import threading, time
from anyseg import segment
doc = segment(" ".join(f"слово{i}" for i in range(50)), lang="ru")
stop = threading.Event()
class Cycle:
    __slots__ = ("me", "__weakref__")
    def __init__(self):
        self.me = self
    def __del__(self):
        time.sleep(0.0001)
def reader():
    kept = []
    while not stop.is_set():
        Cycle()
        kept.append(doc.column("upos"))
def writer():
    values = [["NOUN"] * 50, ["VERB"] * 50]
    while not stop.is_set():
        doc.set_columns(upos=values[0])
        values.reverse()
threads = [threading.Thread(target=work) for work in (reader, writer)]
for thread in threads:
    thread.start()
time.sleep(3)
stop.set()
for thread in threads:
    thread.join()
"""
    subprocess.run([sys.executable, "-c", script], check=True, timeout=60)

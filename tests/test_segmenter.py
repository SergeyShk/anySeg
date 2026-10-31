import copy
import gc
import pickle
import random
import re
import weakref
from concurrent.futures import ThreadPoolExecutor
from itertools import pairwise

import pytest

import anyseg
from anyseg import Segmenter, Span, sentenize, tokenize

# Letters with a stress mark, a supplementary-plane emoji, a byte order mark, a no-break space, a
# line separator, CRLF and a tab: offsets in characters differ from offsets in UTF-8 bytes and
# UTF-16 units
TEXT = "\ufeffЁж\u00a0иду\u0301т\t😀\r\nдомо\u0301й\u2028end"

# Whitespace in the alphabet of the random texts and the invisible separators
SEPARATORS = " \n\r\t\u00a0\u2028\u3000\ufeff\u200b\u2060\u00ad\u200c\u200d"


# A segmenter with lists of the user
CUSTOM = Segmenter("ru", abbreviations={"Сущ.", "глаг"}, tokens=("C++", "т.н."))


@pytest.fixture(scope="module")
def segmenter():
    return Segmenter("ru")


def test_language(segmenter):
    assert segmenter.lang == "ru"
    assert repr(segmenter) == "Segmenter('ru')"
    assert segmenter == Segmenter("ru")
    assert segmenter != "ru"
    assert len({segmenter, Segmenter("ru")}) == 1


def test_lists(segmenter):
    text = "сущ. и глаг. на C++, т.н.\n\nТаков т.н."
    assert [span.text for span in segmenter.tokenize(text)] == [
        *("сущ", ".", "и", "глаг", ".", "на", "C", "++", ",", "т.", "н", "."),
        *("Таков", "т.", "н", "."),
    ]
    assert [span.text for span in CUSTOM.tokenize(text)] == [
        *("сущ.", "и", "глаг.", "на", "C++", ",", "т.н.", "Таков", "т.н."),
    ]
    assert [token.text for token in CUSTOM.segment(text).tokens] == [
        span.text for span in CUSTOM.tokenize(text)
    ]
    assert (CUSTOM.abbreviations, CUSTOM.tokens) == (("глаг", "сущ"), ("C++", "т.н."))
    assert (segmenter.abbreviations, segmenter.tokens) == ((), ())
    assert repr(CUSTOM) == "Segmenter('ru', abbreviations=('глаг', 'сущ'), tokens=('C++', 'т.н.'))"
    assert repr(Segmenter("ru", tokens=["C++"])) == "Segmenter('ru', tokens=('C++',))"
    same = Segmenter("ru", abbreviations=iter(["глаг.", "СУЩ"]), tokens=["т.н.", "C++", "C++"])
    assert same == CUSTOM
    assert len({CUSTOM, same, segmenter}) == 2
    assert Segmenter("ru", abbreviations=None, tokens=[]) == segmenter


@pytest.mark.parametrize(
    ("lists", "error", "message"),
    [
        (
            {"abbreviations": "сущ"},
            TypeError,
            "abbreviations must be an iterable of strings, not a string",
        ),
        ({"tokens": "C++"}, TypeError, "tokens must be an iterable of strings, not a string"),
        ({"tokens": ["C++", 1]}, TypeError, None),
        ({"abbreviations": 1}, TypeError, None),
        (
            {"abbreviations": ["т.н"]},
            ValueError,
            "abbreviation 'т.н' is not a word of letters with an optional final period",
        ),
        (
            {"tokens": ["a b"]},
            ValueError,
            "token 'a b' is empty, has whitespace or control characters or starts or ends with an invisible separator",
        ),
        ({"tokens": [""]}, ValueError, "token '' is empty"),
        ({"tokens": ["\u200ba"]}, ValueError, "token '\\u200ba' is empty"),
        ({"tokens": ["a\x1fb"]}, ValueError, "token 'a\\x1fb' is empty"),
    ],
)
def test_invalid_lists(lists, error, message):
    with pytest.raises(error, match=None if message is None else f"^{re.escape(message)}"):
        Segmenter("ru", **lists)


@pytest.mark.parametrize("lang", ["xx", "RU", "", "ru "])
def test_unsupported_language(lang):
    message = f"unsupported language {lang!r}, expected one of: ru"
    with pytest.raises(ValueError, match=f"^{re.escape(message)}$"):
        Segmenter(lang)
    with pytest.raises(ValueError, match="unsupported language"):
        tokenize("text", lang=lang)


def test_tokens_in_characters(segmenter):
    spans = segmenter.tokenize(TEXT)
    assert [span.text for span in spans] == ["Ёж", "иду\u0301т", "😀", "домо\u0301й", "end"]
    assert [(span.start, span.end) for span in spans] == [
        (1, 3),
        (4, 9),
        (10, 11),
        (13, 19),
        (20, 23),
    ]
    assert all(TEXT[span.start : span.end] == span.text for span in spans)


def test_ascii_text(segmenter):
    text = "  Mom washed\tthe frame.\n"
    spans = segmenter.tokenize(text)
    assert [(span.start, span.end, span.text) for span in spans] == [
        (2, 5, "Mom"),
        (6, 12, "washed"),
        (13, 16, "the"),
        (17, 22, "frame"),
        (22, 23, "."),
    ]


def test_sentences(segmenter):
    (span,) = segmenter.sentenize("\n " + TEXT + " \u3000")
    assert (span.start, span.end, span.text) == (3, 2 + len(TEXT), TEXT[1:])
    text = "Ёж идёт.\nКот спит в 1914 г. Затем\n \u200b\n\nУтро\u2029Ночь"
    assert [span.text for span in segmenter.sentenize(text)] == [
        "Ёж идёт.",
        "Кот спит в 1914 г.",
        "Затем",
        "Утро",
        "Ночь",
    ]
    assert [span.text for span in segmenter.tokenize(text)][6:9] == ["1914", "г", "."]


@pytest.mark.parametrize("text", ["", " ", "\n\r\n\t\u00a0\u2028\u3000", "\ufeff\u200b\u2060"])
def test_blank_text(segmenter, text):
    assert segmenter.tokenize(text) == []
    assert segmenter.sentenize(text) == []


def test_lone_surrogate(segmenter):
    with pytest.raises(UnicodeEncodeError):
        segmenter.tokenize("Ёж \ud83d идёт")
    with pytest.raises(UnicodeEncodeError):
        segmenter.sentenize("\udc00")


def test_random_texts(segmenter):
    rng = random.Random(0)
    alphabet = "ab Ёж\u0301\u00ad😀👍🏽\u200d.,-—\n\r\t\u00a0\u2028\u3000\ufeff\u200b\u2060"
    for _ in range(300):
        text = "".join(rng.choices(alphabet, k=rng.randrange(40)))
        spans = segmenter.tokenize(text)
        assert all(text[span.start : span.end] == span.text for span in spans)
        assert all(a.end <= b.start for a, b in pairwise(spans))
        visible = lambda piece: "".join(char for char in piece if char not in SEPARATORS)  # noqa: E731
        assert visible("".join(span.text for span in spans)) == visible(text)
        assert all(
            span.text[0] not in SEPARATORS and span.text[-1] not in SEPARATORS for span in spans
        )


def test_threads(segmenter):
    texts = [f"{TEXT} {number}" * 50 for number in range(64)]
    with ThreadPoolExecutor(8) as pool:
        results = list(pool.map(segmenter.tokenize, texts))
    assert results == [segmenter.tokenize(text) for text in texts]


def test_pipe(segmenter):
    long = f"{TEXT} " * 2000
    texts = [
        TEXT,
        "",
        " \n",
        "Ёж идёт. Кот спит",
        long,
        "Утро",
        *(f"{TEXT} {n}" for n in range(20)),
    ]
    expected = [segmenter.segment(text) for text in texts]
    for threads in (1, 3):
        for batch_size in (1, 4, 1000):
            documents = list(segmenter.pipe(iter(texts), threads=threads, batch_size=batch_size))
            assert [(doc.text, doc.tokens.starts, doc.sentences.starts) for doc in documents] == [
                (doc.text, doc.tokens.starts, doc.sentences.starts) for doc in expected
            ]
    assert next(segmenter.pipe(texts)).text is texts[0]
    assert list(segmenter.pipe([])) == []
    assert CUSTOM.pipe(["сущ. C++"]).__next__().tokens[0].text == "сущ."


def test_pipe_is_lazy(segmenter):
    taken = []

    def texts():
        for number in range(10):
            taken.append(number)
            yield f"Текст {number}"

    pipe = segmenter.pipe(texts(), batch_size=3)
    assert iter(pipe) is pipe
    assert next(pipe).text == "Текст 0"
    assert taken == [0, 1, 2]
    assert [doc.text for doc in pipe] == [f"Текст {number}" for number in range(1, 10)]


def test_pipe_errors(segmenter):
    def texts(bad):
        yield "Ёж"
        yield "Кот"
        yield bad
        yield "Утро"

    for bad, error, message in [
        (5, TypeError, "text 2 is int, not a string"),
        ("\ud800", UnicodeEncodeError, "surrogates not allowed"),
        ("ок\ud800" * 3000, UnicodeEncodeError, "surrogates not allowed"),
    ]:
        pipe = segmenter.pipe(texts(bad), batch_size=10)
        assert [doc.text for doc in [next(pipe), next(pipe)]] == ["Ёж", "Кот"]
        with pytest.raises(error, match=message):
            next(pipe)
        assert list(pipe) == []

    def failing():
        yield "Ёж"
        raise LookupError("no more texts")

    pipe = segmenter.pipe(failing())
    assert next(pipe).text == "Ёж"
    with pytest.raises(LookupError, match="no more texts"):
        next(pipe)
    with pytest.raises(TypeError, match="not iterable"):
        segmenter.pipe(5)
    for name in ("threads", "batch_size"):
        with pytest.raises(ValueError, match=f"{name} must be at least 1, not 0"):
            segmenter.pipe([], **{name: 0})


def test_pipe_runs_once_at_a_time(segmenter):
    def texts():
        yield "Ёж"
        next(pipe)

    pipe = segmenter.pipe(texts())
    assert next(pipe).text == "Ёж"
    with pytest.raises(ValueError, match="the pipe is already running"):
        next(pipe)
    with pytest.raises(TypeError, match="cannot pickle"):
        pickle.dumps(segmenter.pipe([]))


def test_pipe_in_a_cycle(segmenter):
    # A generator that holds its own pipe is collected with it
    class Marker:
        pass

    markers = []
    for _ in range(10):
        holder, marker = [], Marker()
        markers.append(weakref.ref(marker))

        def texts(holder=holder, marker=marker):
            yield from ["Ёж", "Кот"]

        pipe = segmenter.pipe(texts(), batch_size=1)
        holder.append(pipe)
        next(pipe)
    del holder, marker, pipe, texts
    gc.collect()
    assert all(marker() is None for marker in markers)


def test_pipe_threads_share_a_segmenter(segmenter):
    texts = [f"{TEXT} {number}" * 50 for number in range(64)]
    with ThreadPoolExecutor(4) as pool:
        results = list(pool.map(lambda text: next(segmenter.pipe([text])).tokens.starts, texts))
    assert results == [segmenter.segment(text).tokens.starts for text in texts]


def test_span():
    span = Span(3, 7, "идёт")
    assert (span.start, span.end, span.text) == (3, 7, "идёт")
    assert repr(span) == "Span(start=3, end=7, text='идёт')"
    assert span == Span(3, 7, "идёт")
    assert span != Span(3, 7, "иду")
    assert span != (3, 7, "идёт")
    assert len({span, Span(3, 7, "идёт"), Span(0, 4, "идёт")}) == 2
    with pytest.raises(AttributeError):
        span.start = 0


@pytest.mark.parametrize("protocol", range(pickle.HIGHEST_PROTOCOL + 1))
def test_pickle(segmenter, protocol):
    span = segmenter.tokenize("Ёж идёт")[1]
    assert pickle.loads(pickle.dumps(span, protocol)) == span
    for value in (segmenter, CUSTOM):
        restored = pickle.loads(pickle.dumps(value, protocol))
        assert restored == value
        assert restored.tokenize(TEXT) == value.tokenize(TEXT)


@pytest.mark.parametrize("function", [copy.copy, copy.deepcopy])
def test_copy(segmenter, function):
    span = segmenter.tokenize("Ёж идёт")[1]
    assert function(span) == span
    assert function(segmenter) == segmenter
    assert function(CUSTOM) == CUSTOM


def test_functions(segmenter):
    assert tokenize(TEXT, lang="ru") == segmenter.tokenize(TEXT)
    assert sentenize(TEXT, lang="ru") == segmenter.sentenize(TEXT)
    assert anyseg._segmenter("ru") is anyseg._segmenter("ru")


def test_language_is_required():
    with pytest.raises(TypeError):
        tokenize(TEXT)
    with pytest.raises(TypeError):
        sentenize(TEXT, "ru")

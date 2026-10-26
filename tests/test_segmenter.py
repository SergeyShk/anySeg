import copy
import pickle
import random
import re
from concurrent.futures import ThreadPoolExecutor
from itertools import pairwise

import pytest

import anyseg
from anyseg import Segmenter, Span, sentenize, tokenize

# Letters with a stress mark, a supplementary-plane emoji, a no-break space, a line separator,
# CRLF and a tab: offsets in characters differ from offsets in UTF-8 bytes and UTF-16 units
TEXT = "\ufeffЁж\u00a0иду\u0301т\t😀\r\nдомо\u0301й\u2028end"


@pytest.fixture(scope="module")
def segmenter():
    return Segmenter("ru")


def test_language(segmenter):
    assert segmenter.lang == "ru"
    assert repr(segmenter) == "Segmenter('ru')"
    assert segmenter == Segmenter("ru")
    assert segmenter != "ru"
    assert len({segmenter, Segmenter("ru")}) == 1


@pytest.mark.parametrize("lang", ["xx", "RU", "", "ru "])
def test_unsupported_language(lang):
    message = f"unsupported language {lang!r}, expected one of: ru"
    with pytest.raises(ValueError, match=f"^{re.escape(message)}$"):
        Segmenter(lang)
    with pytest.raises(ValueError, match="unsupported language"):
        tokenize("text", lang=lang)


def test_tokens_in_characters(segmenter):
    spans = segmenter.tokenize(TEXT)
    assert [span.text for span in spans] == ["\ufeffЁж", "иду\u0301т", "😀", "домо\u0301й", "end"]
    assert [(span.start, span.end) for span in spans] == [
        (0, 3),
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
        (17, 23, "frame."),
    ]


def test_sentence_is_trimmed_text(segmenter):
    (span,) = segmenter.sentenize("\n " + TEXT + " \u3000")
    assert (span.start, span.end, span.text) == (2, 2 + len(TEXT), TEXT)


@pytest.mark.parametrize("text", ["", " ", "\n\r\n\t\u00a0\u2028\u3000"])
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
    alphabet = "ab Ёж\u0301\u00ad😀👍🏽\u200d.,-—\n\r\t\u00a0\u2028\u3000\ufeff\u200b"
    for _ in range(300):
        text = "".join(rng.choices(alphabet, k=rng.randrange(40)))
        spans = segmenter.tokenize(text)
        assert all(text[span.start : span.end] == span.text for span in spans)
        assert all(a.end < b.start for a, b in pairwise(spans))
        assert "".join(span.text for span in spans) == "".join(text.split())


def test_threads(segmenter):
    texts = [f"{TEXT} {number}" * 50 for number in range(64)]
    with ThreadPoolExecutor(8) as pool:
        results = list(pool.map(segmenter.tokenize, texts))
    assert results == [segmenter.tokenize(text) for text in texts]


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
    restored = pickle.loads(pickle.dumps(segmenter, protocol))
    assert restored == segmenter
    assert restored.tokenize(TEXT) == segmenter.tokenize(TEXT)


@pytest.mark.parametrize("function", [copy.copy, copy.deepcopy])
def test_copy(segmenter, function):
    span = segmenter.tokenize("Ёж идёт")[1]
    assert function(span) == span
    assert function(segmenter) == segmenter


def test_functions(segmenter):
    assert tokenize(TEXT, lang="ru") == segmenter.tokenize(TEXT)
    assert sentenize(TEXT, lang="ru") == segmenter.sentenize(TEXT)
    assert anyseg._segmenter("ru") is anyseg._segmenter("ru")


def test_language_is_required():
    with pytest.raises(TypeError):
        tokenize(TEXT)
    with pytest.raises(TypeError):
        sentenize(TEXT, "ru")

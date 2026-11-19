import codecs
import gc
import io
import pickle
import weakref

import pytest

from anyseg import Segmenter, Stream

PARAGRAPHS = "Кот спит. Ёж идёт.\n\nУтро «настало», 😀 и всё.\n\n" * 2000


def pieces(segmenter, file):
    """Tokens, sentences and words of the documents of a stream, at their positions in the text"""
    stream = segmenter.stream(file)
    tokens, sentences, words, continued = [], [], [], []
    for document in stream:
        start = stream.start
        tokens += [(token.start + start, token.end + start) for token in document.tokens]
        sentences += [
            (sentence.start + start, sentence.end + start) for sentence in document.sentences
        ]
        words += document.column("form")
        continued.append(stream.continued)
    return tokens, sentences, words, continued


def whole(segmenter, text):
    document = segmenter.segment(text)
    return (
        [(token.start, token.end) for token in document.tokens],
        [(sentence.start, sentence.end) for sentence in document.sentences],
        document.column("form"),
    )


@pytest.mark.parametrize("binary", [False, True])
def test_stream_is_the_whole_text(binary):
    segmenter = Segmenter("ru")
    file = io.BytesIO(PARAGRAPHS.encode()) if binary else io.StringIO(PARAGRAPHS)
    tokens, sentences, words, continued = pieces(segmenter, file)
    assert (tokens, sentences, words) == whole(segmenter, PARAGRAPHS)
    assert len(continued) > 1 and not any(continued)


def test_stream_of_a_file(tmp_path):
    path = tmp_path / "text.txt"
    text = "﻿Vengo del mar.\r\n\r\nQuiero decírselo.\r\n" * 3000
    path.write_bytes(text.encode())
    segmenter = Segmenter("es")
    with path.open("rb") as file:
        tokens, sentences, words, _ = pieces(segmenter, file)
    # The byte order mark is no part of the text, but positions count it as the text read from the file does
    expected = whole(segmenter, text)
    assert (tokens, sentences, words) == expected
    with path.open(encoding="utf-8", newline="") as file:
        assert pieces(segmenter, file)[:3] == expected


def test_long_paragraph():
    segmenter = Segmenter("ru")
    text = "Мама мыла раму. Папа читал.\n" * 20000
    tokens, sentences, words, continued = pieces(segmenter, io.StringIO(text))
    assert (tokens, sentences, words) == whole(segmenter, text)
    assert continued[0] is False and all(continued[1:])


def test_stream_gives_what_has_come():
    class Pipe(io.BufferedIOBase):
        """Gives with read1 what has come, as a pipe does; read would wait for the whole size"""

        def __init__(self, *chunks):
            self.chunks, self.reads = list(chunks), 0

        def read1(self, size):
            self.reads += 1
            return self.chunks.pop(0) if self.chunks else b""

        def read(self, size):
            raise AssertionError("read waits for the whole size")

    pipe = Pipe("Кот спит.\n\n".encode(), "Ёж идёт.\n".encode())
    stream = Segmenter("ru").stream(pipe)
    assert [sentence.text for sentence in next(stream).sentences] == ["Кот спит."]
    assert pipe.reads == 1
    assert [document.text for document in stream] == ["Ёж идёт.\n"]


def test_text_readers_of_codecs(tmp_path):
    # A reader of codecs passes read1 to the bytes under it, which are not UTF-8
    path = tmp_path / "text.txt"
    path.write_bytes("Ёж идёт.\n\nКот спит.\n".encode("cp1251"))
    with path.open("rb") as file:
        stream = Segmenter("ru").stream(codecs.getreader("cp1251")(file))
        assert "".join(document.text for document in stream) == "Ёж идёт.\n\nКот спит.\n"


def test_lone_surrogate():
    segmenter = Segmenter("ru")
    text = "Кот спит.\n\nЁж \udcff идёт.\n"
    stream = segmenter.stream(io.StringIO(text))
    documents = [(stream.start, document) for document in stream]
    assert [token.text for _, document in documents for token in document.tokens] == [
        "Кот",
        "спит",
        ".",
        "Ёж",
        "\ufffd",
        "идёт",
        ".",
    ]
    assert [token.start + start for start, document in documents for token in document.tokens] == [
        token.start for token in segmenter.segment(text).tokens
    ]


def test_empty_file():
    stream = Segmenter("ru").stream(io.StringIO(""))
    assert list(stream) == []
    assert (stream.start, stream.continued) == (0, False)
    assert list(Segmenter("ru").stream(io.StringIO(" \n\n "))) == []


def test_stream_errors():
    segmenter = Segmenter("ru")
    with pytest.raises(ValueError, match="line 2: not UTF-8"):
        list(segmenter.stream(io.BytesIO(b"\xd0\x81\xd0\xb6.\n\xff\n")))

    class Failing:
        def read(self, size):
            raise KeyError("boom")

    with pytest.raises(KeyError, match="boom"):
        next(segmenter.stream(Failing()))

    class Numbers:
        def read(self, size):
            return 5

    with pytest.raises(TypeError, match="read of the file gave int, not str or bytes"):
        next(segmenter.stream(Numbers()))
    with pytest.raises(TypeError, match="file must have a method read, list has none"):
        segmenter.stream(["Ёж идёт."])
    # A stream that failed ends
    stream = segmenter.stream(Failing())
    with pytest.raises(KeyError):
        next(stream)
    assert list(stream) == []


def test_stream_is_not_picklable():
    stream = Segmenter("ru").stream(io.StringIO("Ёж идёт."))
    assert isinstance(stream, Stream)
    assert iter(stream) is stream
    with pytest.raises(TypeError, match="cannot pickle"):
        pickle.dumps(stream)


@pytest.mark.parametrize("taken", [0, 1, None])
def test_stream_in_a_cycle_is_collected(taken):
    class Source:
        def __init__(self, text):
            self.file = io.StringIO(text)
            self.stream = Segmenter("ru").stream(self)

        def read(self, size):
            return self.file.read(size)

    source = Source("Кот спит.\n\n" * 20000)
    if taken is None:
        list(source.stream)
    else:
        for _ in range(taken):
            next(source.stream)
    alive = weakref.ref(source)
    del source
    gc.collect()
    assert alive() is None

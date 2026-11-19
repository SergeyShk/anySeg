"""
Figures of the documentation, drawn from what anySeg gives

Description:
    Draws from real output of the package: how a text becomes tokens, words, sentences and
    paragraphs (docs/img/pipeline.svg), how line breaks start sentences or not
    (docs/img/lines.svg), and how a stream cuts a file into documents (docs/img/stream.svg).
    Run in the environment of the project, with anyseg built.

Example:
    uv run python scripts/figures.py
"""

import io
from html import escape
from pathlib import Path

from anyseg import Document, Segmenter

ROOT = Path(__file__).parents[1]
IMG = ROOT / "docs" / "img"

# The colors of the logo and the site: Material orange
DARK, LIGHT, INK, MUTED, SOFT, PALE = (
    "#e65100",
    "#ffb74d",
    "#263238",
    "#78909c",
    "#fff3e0",
    "#eceff1",
)
BAND = "#ffe0b2"
FONT = "PT Sans, Source Sans Pro, Arial, sans-serif"

PIPELINE = "Vengo del mar. El Sr. Pérez quiere decírselo.\n\nAl fin llegó."
LINES = (
    (
        "Verse: the author breaks the lines, and a line goes on with its sentence",
        "Мороз и солнце; день чудесный!\nЕще ты дремлешь, друг прелестный -\n"
        "Пора, красавица, проснись:\nОткрой сомкнуты негой взоры",
    ),
    (
        "Text wrapped at the width of the page: a sentence goes on across the lines",
        "Он долго шел по дороге и думал о том,\nчто скажет ей при встрече. Дом стоял\n"
        "на краю села, у самой реки.",
    ),
    (
        "A list and a heading: a line break starts a sentence",
        "Взять с собой:\n- паспорт\n- билеты\nГлава 2\nУтро выдалось холодным.",
    ),
)
# Short paragraphs, one paragraph without blank lines longer than 256 KiB, short paragraphs again
SHORT = "Кот спит на теплой печке. Ёж идет домой через лес.\n\n" * 2600
LONG = "Мама мыла раму, а папа читал газету. " * 18000 + "\n\n"
STREAM = SHORT + LONG + SHORT


def width(value: str, size: float) -> float:
    """Width of a text in PT Sans, about"""
    return len(value) * size * 0.53 + 6


def text(
    x: float,
    y: float,
    value: str,
    size: float = 16,
    color: str = INK,
    bold: bool = False,
    anchor: str = "middle",
) -> str:
    weight = ' font-weight="bold"' if bold else ""
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{color}"{weight} '
        f'text-anchor="{anchor}">{escape(value)}</text>'
    )


def rect(
    x: float,
    y: float,
    w: float,
    h: float,
    fill: str = "white",
    stroke: str = LIGHT,
    radius: float = 6,
) -> str:
    return (
        f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{radius}" '
        f'fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>'
    )


def bracket(x1: float, x2: float, y: float, label: str, color: str = DARK) -> list[str]:
    """A bracket under a span with a label below it"""
    return [
        f'<path fill="none" stroke="{color}" stroke-width="2" '
        f'd="M{x1:.1f} {y - 8:.1f}L{x1:.1f} {y:.1f}L{x2:.1f} {y:.1f}L{x2:.1f} {y - 8:.1f}"/>',
        text((x1 + x2) / 2, y + 20, label, 14, color),
    ]


def save(name: str, w: float, h: float, body: list[str]) -> None:
    """A figure on a white ground: in the dark theme of the site it stays readable"""
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0f} {h:.0f}" width="{w:.0f}" '
        f'height="{h:.0f}" font-family="{FONT}" role="img">\n'
        f'<rect width="{w:.0f}" height="{h:.0f}" rx="12" fill="white"/>\n'
        + "\n".join(body)
        + "\n</svg>\n"
    )
    (IMG / f"{name}.svg").write_text(svg, encoding="utf-8")


def draw_pipeline(document: Document) -> None:
    """The text, its tokens with their positions, the words of multiword tokens, sentences and paragraphs"""
    left, gap, size = 24, 12, 18
    boxes, x = [], left
    for token in document.tokens:
        forms = [word.form for word in token.words]
        w = max(width(token.text, size), *(width(form, 15) for form in forms), 34) + 10
        boxes.append((x, w))
        x += w + gap
    total = x - gap + left
    before, after = document.text.split("\n\n")
    body = [
        text(left, 30, "1. Text", 15, DARK, True, "start"),
        f'<text x="{left}" y="60" font-size="19" fill="{INK}">{escape(before)}'
        f'<tspan fill="{LIGHT}"> ↵↵ </tspan>{escape(after)}</text>',
        text(
            left,
            104,
            "2. Tokens: spans of the text, with the position of their first character",
            15,
            DARK,
            True,
            "start",
        ),
    ]
    for token, (x, w) in zip(document.tokens, boxes, strict=True):
        body.append(rect(x, 116, w, 34))
        body.append(text(x + w / 2, 139, token.text, size))
        body.append(text(x + w / 2, 168, str(token.start), 12, MUTED))
    body.append(
        text(left, 206, "3. Words: a multiword token holds several", 15, DARK, True, "start")
    )
    for token, (x, w) in zip(document.tokens, boxes, strict=True):
        forms = [word.form for word in token.words]
        several = len(forms) > 1
        for row, form in enumerate(forms):
            y = 218 + row * 30
            body.append(
                rect(
                    x + 4, y, w - 8, 26, SOFT if several else "white", DARK if several else PALE, 5
                )
            )
            body.append(text(x + w / 2, y + 19, form, 15, DARK if several else MUTED, several))
    y = 330
    body.append(text(left, y, "4. Sentences and paragraphs", 15, DARK, True, "start"))
    for sentence in document.sentences:
        first, last = sentence.tokens[0].index, sentence.tokens[-1].index
        x1, x2 = boxes[first][0], boxes[last][0] + boxes[last][1]
        body.append(rect(x1, y + 14, x2 - x1, 30, SOFT, LIGHT))
        body.append(text((x1 + x2) / 2, y + 35, f"sentence {sentence.index}", 14, DARK))
    for paragraph in document.paragraphs:
        first, last = paragraph.tokens[0].index, paragraph.tokens[-1].index
        x1, x2 = boxes[first][0], boxes[last][0] + boxes[last][1]
        body += bracket(x1, x2, y + 66, f"paragraph {paragraph.index}")
    save("pipeline", total, y + 110, body)


def draw_lines(segmenter: Segmenter) -> None:
    """Lines of three paragraphs, how each begins, and the sentences they hold"""
    left, size, step = 24, 17, 30
    fills = (SOFT, BAND)
    body, y, total = [], 34, 0.0
    for title, paragraph in LINES:
        document = segmenter.segment(paragraph)
        body.append(text(left, y, title, 15, DARK, True, "start"))
        total = max(total, left + width(title, 15))
        y += 16
        for line in document.lines:
            kind = line.break_before or "first line"
            body.append(text(left + 86, y + 21, kind, 13, MUTED, False, "end"))
            x = left + 100
            for sentence in document.sentences:
                start, end = max(sentence.start, line.start), min(sentence.end, line.end)
                if start >= end:
                    continue
                piece = document.text[start:end]
                w = width(piece, size)
                body.append(
                    rect(x, y + 4, w, 26, fills[sentence.index % 2], fills[sentence.index % 2], 4)
                )
                body.append(text(x + 6, y + 22, piece, size, INK, False, "start"))
                x += w + 6
            total = max(total, x)
            y += step
        y += 34
    note = "a band is a sentence; the word on the left is how the line begins"
    body.append(text(left, y - 6, note, 13, MUTED, False, "start"))
    save("lines", total + left, y + 10, body)


def draw_stream(segmenter: Segmenter) -> None:
    """The pieces of a file as documents: complete paragraphs, and a long paragraph in pieces of sentences"""
    data = STREAM.encode()
    stream = segmenter.stream(io.BytesIO(data))
    pieces = []
    for document in stream:
        start = len(STREAM[: stream.start].encode())
        pieces.append((start, len(document.text.encode()), stream.continued))
    left, bar, kib = 24, 920, 1024
    scale = bar / len(data)
    long_start = len(SHORT.encode())
    long_end = long_start + len(LONG.encode())
    body = [
        text(left, 30, "The file", 15, DARK, True, "start"),
        rect(left, 44, bar, 30, PALE, PALE, 4),
        rect(left + long_start * scale, 44, (long_end - long_start) * scale, 30, BAND, LIGHT, 4),
        text(left + long_start * scale / 2, 64, "short paragraphs", 13, MUTED),
        text(
            left + (long_start + long_end) / 2 * scale,
            64,
            f"one paragraph of {(long_end - long_start) // kib} KiB without blank lines",
            13,
            DARK,
        ),
        text(left + (long_end + len(data)) / 2 * scale, 64, "short paragraphs", 13, MUTED),
        text(left, 112, "The documents of segmenter.stream(file)", 15, DARK, True, "start"),
    ]
    for start, length, continued in pieces:
        x, w = left + start * scale, max(length * scale - 2, 2)
        body.append(rect(x, 124, w, 30, DARK if continued else SOFT, DARK, 4))
        if w > 50:
            body.append(
                text(x + w / 2, 144, f"{length // kib} KiB", 12, "white" if continued else INK)
            )
    body += [
        rect(left, 178, 18, 18, SOFT, DARK, 4),
        text(
            left + 26,
            192,
            "starts with a paragraph: complete ones in pieces of about 64 KiB, or the first sentences of a long one",
            13,
            INK,
            False,
            "start",
        ),
        rect(left, 206, 18, 18, DARK, DARK, 4),
        text(
            left + 26,
            220,
            "continued: goes on with the paragraph of the document before, in whole sentences",
            13,
            INK,
            False,
            "start",
        ),
        text(
            left,
            254,
            f"{len(pieces)} documents in order; start tells where each begins in the text of the file",
            13,
            MUTED,
            False,
            "start",
        ),
    ]
    save("stream", bar + 2 * left, 272, body)


def main() -> None:
    IMG.mkdir(exist_ok=True)
    draw_pipeline(Segmenter("es").segment(PIPELINE))
    draw_lines(Segmenter("ru"))
    draw_stream(Segmenter("ru"))


if __name__ == "__main__":
    main()

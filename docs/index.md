# anySeg

**anySeg** segments text into tokens and sentences with their positions in the source text. The core is written in Rust, the package has a Python API and no dependencies. The text is never changed: every token and sentence is a span of the original string, and tokens follow the conventions of [Universal Dependencies](https://universaldependencies.org/).

The project is in development: Russian comes first, Spanish and English follow.

## Installation

Requires Python 3.11 or newer. Until the first release, the package is built from the repository: see [Installation](installation.md).

## Quick start

=== "Code"

    ``` python
    from anyseg import Segmenter

    Segmenter("ru").tokenize("Ёж идёт домой")
    ```

=== "Result"

    ```
    [Span(start=0, end=2, text='Ёж'), Span(start=3, end=7, text='идёт'), Span(start=8, end=13, text='домой')]
    ```

Positions are offsets in the characters of a Python string: `text[span.start:span.end] == span.text`.

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

A segmenter takes more abbreviations and strings that are one token each: `Segmenter("ru", abbreviations=["сущ"], tokens=["C++"])`.

A document holds the tokens, the words with the columns of [CoNLL-U](https://universaldependencies.org/format.html), the sentences, the paragraphs (the parts of the text between blank lines) and their lines:

=== "Code"

    ``` python
    doc = Segmenter("ru").segment("Ёж идёт домой\n\nКот спит")
    doc.set_columns(upos=["NOUN", "VERB", "ADV", "NOUN", "VERB"])
    [sentence.text for sentence in doc.sentences], doc.words[1]
    ```

=== "Result"

    ```
    (['Ёж идёт домой', 'Кот спит'], Word(id=2, form='идёт', upos='VERB'))
    ```

`doc.to_conllu()` writes a document in CoNLL-U and `read_conllu(text)` reads documents from it; a document read from CoNLL-U is written back unchanged.

How anySeg compares with other segmenters of Russian in quality and speed: [Comparison](comparison.md).

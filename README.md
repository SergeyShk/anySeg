# anySeg

[![Build](https://github.com/SergeyShk/anySeg/actions/workflows/ci.yml/badge.svg)](https://github.com/SergeyShk/anySeg/actions/workflows/ci.yml)
[![Ruff](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json)](https://github.com/astral-sh/ruff)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/SergeyShk/anySeg/blob/master/LICENSE.txt)

**anySeg** segments text into tokens and sentences with their positions in the source text. The core is written in Rust, the package has a Python API and no dependencies. The text is never changed: every token and sentence is a span of the original string, and tokens follow the conventions of [Universal Dependencies](https://universaldependencies.org/).

The project is in development: Russian comes first, Spanish and English follow.

## Installation

Requires Python 3.11 or newer. Until the first release, the package is built from the repository, which needs [uv](https://docs.astral.sh/uv/) and [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer:

```bash
git clone https://github.com/SergeyShk/anySeg.git
cd anySeg
uv sync
```

## Quick start

```python
from anyseg import Segmenter, tokenize

segmenter = Segmenter("ru")
segmenter.tokenize("Ёж идёт домой")
# [Span(start=0, end=2, text='Ёж'), Span(start=3, end=7, text='идёт'), Span(start=8, end=13, text='домой')]

[span.text for span in tokenize("Ёж идёт домой", lang="ru")]
# ['Ёж', 'идёт', 'домой']

doc = segmenter.segment("Ёж идёт домой\n\nКот спит")
[sentence.text for sentence in doc.sentences]
# ['Ёж идёт домой', 'Кот спит']

doc.set_columns(upos=["NOUN", "VERB", "ADV", "NOUN", "VERB"])
doc.words[1]
# Word(id=2, form='идёт', upos='VERB')
```

Positions are offsets in the characters of a Python string: `text[span.start:span.end] == span.text`. The Rust crate gives them in bytes.

A segmenter takes more abbreviations and strings that are one token each: `Segmenter("ru", abbreviations=["сущ"], tokens=["C++"])`.

A document holds the tokens, the words with the columns of [CoNLL-U](https://universaldependencies.org/format.html), the sentences, the paragraphs (the parts of the text between blank lines) and their lines. `doc.to_conllu()` writes it in CoNLL-U and `read_conllu(text)` reads documents from it; a document read from CoNLL-U is written back unchanged.

## Comparison

On the test files of the Russian treebanks of Universal Dependencies 2.18, F1 (* marks neural tools trained on SynTagRus):

| Tool | Tokens, SynTagRus | Tokens, Taiga | Sentence ends, SynTagRus | Sentence ends, Taiga | Tokens/s from Python |
| :-- | --: | --: | --: | --: | --: |
| **anySeg 0.1** | 99.60 | **99.10** | 99.45 | **95.89** | **2.42 M** |
| razdel 0.5.0 | 98.73 | 95.74 | 99.41 | 85.58 | 489 k |
| NLTK 3.10.4 | 98.93 | 96.57 | 97.61 | 82.59 | 195 k |
| spaCy 3.8.16 | 98.38 | 96.85 | 89.60 | 76.18 | 225 k |
| Stanza 1.15.0 * | **99.75** | 95.55 | 99.51 | 95.71 | 2.1 k |
| trankit 1.1.2 * | 99.74 | 95.54 | **99.55** | 94.79 | 1.4 k |

From Rust, the crate gives 13.5 M tokens per second with sentences. All tools and sets, and how it is measured: [Comparison](https://sergeyshk.github.io/anySeg/comparison/).

## License

MIT, see [LICENSE.txt](https://github.com/SergeyShk/anySeg/blob/master/LICENSE.txt).

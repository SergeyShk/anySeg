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
```

Positions are offsets in the characters of a Python string: `text[span.start:span.end] == span.text`. The Rust crate gives them in bytes.

## License

MIT, see [LICENSE.txt](https://github.com/SergeyShk/anySeg/blob/master/LICENSE.txt).

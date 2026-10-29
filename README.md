<p align="center">
  <img src="https://raw.githubusercontent.com/SergeyShk/anySeg/master/docs/img/anyseg.png" alt="anySeg" width="480">
</p>

<h1 align="center">anySeg</h1>

<p align="center">
  <b>Any Segmentation</b> - fast segmentation of text into tokens and sentences, written in Rust with a Python API
</p>

<p align="center">
  <a href="https://sergeyshk.github.io/anySeg/">Documentation</a> ·
  <a href="https://github.com/SergeyShk/anySeg">GitHub</a>
</p>

<p align="center">
  <a href="https://github.com/SergeyShk/anySeg/actions/workflows/ci.yml"><img src="https://github.com/SergeyShk/anySeg/actions/workflows/ci.yml/badge.svg" alt="Build"></a>
  <a href="https://github.com/astral-sh/ruff"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json" alt="Ruff"></a>
  <a href="https://github.com/SergeyShk/anySeg/blob/master/LICENSE.txt"><img src="https://img.shields.io/github/license/sergeyshk/anyseg.svg" alt="License"></a>
</p>

---

**anySeg** segments text into tokens, words and sentences with their positions in the source text: a Rust core and a Python package without dependencies. The text is never changed - every token and sentence is a span of the original string - and the tokens follow the conventions of [Universal Dependencies](https://universaldependencies.org/), so the results can be compared with the treebanks and with the tools trained on them. One package serves all languages: the rules are shared, and a language brings its lists. Russian comes first, Spanish and English follow.

From Python, tokens and sentences come out at 2.4 M tokens per second, five times faster than razdel; the Rust crate gives 13.5 M. On the test files of the Russian treebanks of UD, token F1 is 99.1-99.8, above every other tool on Taiga, GSD and PUD, and the sentence boundaries are above razdel on all four sets.

* **[Tokens](https://sergeyshk.github.io/anySeg/russian/#tokens)** - words with stress marks and soft hyphens, hyphenated words by the UD guidelines, numbers, dates and times, URLs, e-mail addresses, mentions, hashtags, emoticons and emoji, abbreviations and initials with their period
* **[Sentences](https://sergeyshk.github.io/anySeg/russian/#sentences)** - final marks with the quotes and brackets after them, abbreviations that end a sentence, lines of dialogue, list items, headings and verse
* **[Paragraphs and lines](https://sergeyshk.github.io/anySeg/russian/#lines)** - paragraphs between blank lines, and every line marked as a break of the author or a wrap at the width of the page, as in text from PDF
* **[Document model](https://sergeyshk.github.io/anySeg/document/)** - tokens, words with the columns of [CoNLL-U](https://universaldependencies.org/format.html), sentences, paragraphs and lines; reading and writing CoNLL-U, a document read from it written back as it was
* **[Lists of your own](https://sergeyshk.github.io/anySeg/segmenter/#lists)** - more abbreviations and strings that are one token each
* **[Comparison](https://sergeyshk.github.io/anySeg/comparison/)** - quality and speed against razdel, NLTK, spaCy, Stanza, trankit, SaT and other tools

> [!NOTE]
> Early version: before 1.0 the API may change between 0.x versions.

## Installation

Python 3.11 or newer is required. Until the first release, the package is built from the repository, which needs [uv](https://docs.astral.sh/uv/) and [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer:

```bash
git clone https://github.com/SergeyShk/anySeg.git
cd anySeg
uv sync
```

## Quick start

```python
>>> from anyseg import Segmenter, tokenize

>>> segmenter = Segmenter("ru")
>>> [span.text for span in segmenter.sentenize("Кот спал в 1914 г. Затем ушёл, т. е. исчез.")]
['Кот спал в 1914 г.', 'Затем ушёл, т. е. исчез.']
>>> segmenter.tokenize("Ёж идёт домой")
[Span(start=0, end=2, text='Ёж'), Span(start=3, end=7, text='идёт'), Span(start=8, end=13, text='домой')]
>>> [span.text for span in tokenize("Ростова-на-Дону, кто-то, 2,5-3,5 млн", lang="ru")]
['Ростова', '-', 'на', '-', 'Дону', ',', 'кто-то', ',', '2,5', '-', '3,5', 'млн']
```

Create the segmenter once and use it throughout the program: it is immutable, can be shared between threads and pickled. Positions are offsets in the characters of a Python string, `text[span.start:span.end] == span.text`; the Rust crate gives them in bytes.

A document holds the tokens, the words with the columns of CoNLL-U, the sentences, the paragraphs and their lines:

```python
>>> doc = segmenter.segment("Ёж идёт домой\n\nКот спит")
>>> [sentence.text for sentence in doc.sentences]
['Ёж идёт домой', 'Кот спит']
>>> doc.set_columns(upos=["NOUN", "VERB", "ADV", "NOUN", "VERB"])
>>> doc.words[1]
Word(id=2, form='идёт', upos='VERB')
```

`doc.to_conllu()` writes a document in CoNLL-U, and `read_conllu(text)` reads documents from it. A segmenter takes more abbreviations and strings that are one token each: `Segmenter("ru", abbreviations=["сущ"], tokens=["C++"])`.

## Comparison

On the test files of the Russian treebanks of Universal Dependencies 2.18, F1 (* marks neural tools trained on SynTagRus):

| Tool | Tokens, SynTagRus | Tokens, Taiga | Sentence ends, SynTagRus | Sentence ends, Taiga | Tokens/s from Python |
| :-- | --: | --: | --: | --: | --: |
| **anySeg 0.1** | 99.61 | **99.10** | 99.46 | **95.94** | **2.42 M** |
| razdel 0.5.0 | 98.73 | 95.74 | 99.41 | 85.58 | 489 k |
| NLTK 3.10.4 | 98.93 | 96.57 | 97.61 | 82.59 | 195 k |
| spaCy 3.8.16 | 98.38 | 96.85 | 89.60 | 76.18 | 225 k |
| Stanza 1.15.0 * | **99.75** | 95.55 | 99.51 | 95.71 | 2.1 k |
| trankit 1.1.2 * | 99.74 | 95.54 | **99.55** | 94.79 | 1.4 k |

From Rust, the crate gives 13.5 M tokens per second with sentences. All tools and sets, and how it is measured: [Comparison](https://sergeyshk.github.io/anySeg/comparison/).

## Development

The project uses [uv](https://docs.astral.sh/uv/) to manage dependencies, [maturin](https://www.maturin.rs/) to build the Rust module and [ruff](https://docs.astral.sh/ruff/) for linting and formatting. [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer is required.

```bash
git clone https://github.com/SergeyShk/anySeg.git
cd anySeg

make deps        # create the environment, install the dependencies and build the Rust module
make test        # run the Python tests, the docstring examples (doctest) and the Rust tests
make lint        # ruff, mypy, stubtest, rustfmt and clippy
```

The full list of commands is `make help`. Before submitting changes, install the hooks that run the linters on commit and the tests on push:

```bash
uv run pre-commit install
```

## Contributing

Bug reports, ideas and pull requests are welcome - the [issues](https://github.com/SergeyShk/anySeg/issues) are open. The workflow and the checks to run before submitting a pull request are in [CONTRIBUTING.md](https://github.com/SergeyShk/anySeg/blob/master/CONTRIBUTING.md); the rules of conduct are in the [code of conduct](https://github.com/SergeyShk/anySeg/blob/master/CODE_OF_CONDUCT.md).

<details>
<summary><b>Project structure</b></summary>

<br>

*   **crates/anyseg** - the core in Rust: tokens, sentences, the document model and CoNLL-U, with the lists of every language in `data/` behind a Cargo feature
*   **crates/anyseg-py** - the Python module `anyseg._anyseg` built with PyO3
*   **python/anyseg** - the Python package
*   **scripts** - checking the installed package before publishing
*   **docs** - MkDocs documentation
*   **tests** - tests of the Python package

</details>

## Authors

*   Sergey Shkarin (kouki.sergey@gmail.com)

## License

The code and the lists of the languages are distributed under the [MIT](https://github.com/SergeyShk/anySeg/blob/master/LICENSE.txt) license.

## Citation

If you use **anySeg** in research or software, please cite it with the following BibTeX entry - citations help the development and support of the project. The same metadata is in [CITATION.cff](https://github.com/SergeyShk/anySeg/blob/master/CITATION.cff), which GitHub shows under the "Cite this repository" button.

```bibtex
@software{anySeg,
  author = {Sergey Shkarin},
  title = {{anySeg, fast segmentation of text into tokens and sentences}},
  year = 2026,
  url = {https://github.com/SergeyShk/anySeg}
}
```

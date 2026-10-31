# Segmenter

``` {.python .signature}
Segmenter(lang, *, abbreviations=None, tokens=None)
```

Segmenter of texts in one language: it gives the tokens and the sentences of a text as spans of it, and the [document](document.md) of a text. The text is never changed. A segmenter is immutable: create it once and use it throughout the program, from several threads at once, which segment long texts in parallel; it can be pickled and equals the segmenters of the same language and lists.

Exceptions:

- `ValueError` - the language is not supported, an [abbreviation](#lists) is not a word of letters, or a [token](#lists) is empty, has whitespace or control characters or starts or ends with an invisible separator;
- `TypeError` - `abbreviations` or `tokens` is a string or holds something other than strings.

=== "Code"

    ``` python
    from anyseg import Segmenter

    segmenter = Segmenter("ru")
    segmenter.tokenize("Ёж идёт домой")
    ```

=== "Result"

    ```
    [Span(start=0, end=2, text='Ёж'), Span(start=3, end=7, text='идёт'), Span(start=8, end=13, text='домой')]
    ```

## Parameters

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `lang` | `str` | - | ISO 639-1 code of the language: `"ru"` |
| `abbreviations` | `Iterable[str]` | `None` | [More abbreviations](#lists): words that keep a period after them |
| `tokens` | `Iterable[str]` | `None` | [Strings that are one token each](#lists) |

The object keeps them read-only: `lang` is the code of the language, `abbreviations` and `tokens` are sorted tuples of the lists.

## Lists of your own { #lists }

The rules of a language know the common abbreviations and the usual kinds of tokens; a text of a special field may need more.

*   **Abbreviations** are words that keep a period after them: `"сущ"` or `"сущ."`, in any case. Such a word with its period is one token and ends a sentence as the [abbreviations of the language](russian.md#sentence-abbreviations) do. The segmenter keeps them lowercase, without the period and the stress marks.
*   **Tokens** are strings that are one token each, as written: `"C++"`, `"т.н."`. A string matches where a token would start and no letter or digit adjoins it; of several strings that match at one place, the longest wins.

=== "Code"

    ``` python
    custom = Segmenter("ru", abbreviations=["сущ."], tokens=["C++", "т.н."])
    [span.text for span in custom.tokenize("Язык C++ - т.н. сущ. язык")]
    custom
    ```

=== "Result"

    ```
    ['Язык', 'C++', '-', 'т.н.', 'сущ.', 'язык']
    Segmenter('ru', abbreviations=('сущ',), tokens=('C++', 'т.н.'))
    ```

## Methods

<div class="metric-card" markdown>

### Tokens of a text { #tokenize }

`tokenize` · `list[Span]`
{ .metric-meta }

``` {.python .signature}
tokenize(text)
```

The tokens of the text in order. Whitespace, control characters and the invisible separators - zero width space, word joiner, byte order mark, and a soft hyphen and zero width (non-)joiner outside a word - separate the tokens and are in none of them. How a language splits its text, see [Russian](russian.md).

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `text` | `str` | - | Text |

Exceptions:

- `UnicodeEncodeError` - the text contains a lone surrogate.

=== "Code"

    ``` python
    [span.text for span in segmenter.tokenize("Ёж идёт, т. е. бежит: https://example.ru 😀")]
    ```

=== "Result"

    ```
    ['Ёж', 'идёт', ',', 'т.', 'е.', 'бежит', ':', 'https://example.ru', '😀']
    ```

</div>

<div class="metric-card" markdown>

### Sentences of a text { #sentenize }

`sentenize` · `list[Span]`
{ .metric-meta }

``` {.python .signature}
sentenize(text)
```

The sentences of the text in order, each from the start of its first token to the end of its last one. A paragraph - the part of the text between blank lines - always starts a sentence; inside it, final marks, abbreviations, emoticons and line breaks end sentences by the rules of the [language](russian.md#sentences).

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `text` | `str` | - | Text |

Exceptions:

- `UnicodeEncodeError` - the text contains a lone surrogate.

=== "Code"

    ``` python
    text = "Кот спал в 1914 г. Затем ушёл. «Куда?» — спросил пёс."
    [span.text for span in segmenter.sentenize(text)]
    segmenter.sentenize(" Ёж идёт. Кот спит ")
    ```

=== "Result"

    ```
    ['Кот спал в 1914 г.', 'Затем ушёл.', '«Куда?» — спросил пёс.']
    [Span(start=1, end=9, text='Ёж идёт.'), Span(start=10, end=18, text='Кот спит')]
    ```

</div>

<div class="metric-card" markdown>

### Document of a text { #segment }

`segment` · `Document`
{ .metric-meta }

``` {.python .signature}
segment(text)
```

The [document](document.md) of the text: its tokens as `tokenize` gives them, one word per token, its sentences as `sentenize` gives them, its paragraphs and their lines. The document keeps the text and gives the parts as views of it.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `text` | `str` | - | Text |

Exceptions:

- `UnicodeEncodeError` - the text contains a lone surrogate.

=== "Code"

    ``` python
    doc = segmenter.segment("Ёж идёт домой.\n\nКот спит")
    [sentence.text for sentence in doc.sentences], len(doc.tokens)
    ```

=== "Result"

    ```
    (['Ёж идёт домой.', 'Кот спит'], 6)
    ```

</div>

<div class="metric-card" markdown>

### Documents of texts { #pipe }

`pipe` · `Iterator[Document]`
{ .metric-meta }

``` {.python .signature}
pipe(texts, *, threads=1, batch_size=1000)
```

The documents of many texts, as `segment` gives them, in the order of the texts. The texts are taken from the iterable as the documents are asked for, `batch_size` at a time, and every batch is segmented without the GIL, on up to `threads` threads: other Python threads run meanwhile, and several threads speed up a large collection of texts even with the GIL. A text that is not a string or holds a lone surrogate raises its error after the documents of the texts before it and ends the iteration. One iterator serves one thread at a time: a call while another runs raises `ValueError`.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `texts` | `Iterable[str]` | - | Texts |
| `threads` | `int` | `1` | Number of threads that segment a batch |
| `batch_size` | `int` | `1000` | Number of texts in a batch |

Exceptions:

- `ValueError` - `threads` or `batch_size` is less than 1;
- `TypeError` - `texts` is not iterable, or a text is not a string;
- `UnicodeEncodeError` - a text contains a lone surrogate.

=== "Code"

    ``` python
    texts = ["Ёж идёт домой. Кот спит", "Утро", "Ночь  наступила"]
    [len(doc.sentences) for doc in segmenter.pipe(texts, threads=2)]
    ```

=== "Result"

    ```
    [2, 1, 1]
    ```

</div>

## Functions

`tokenize(text, *, lang)`, `sentenize(text, *, lang)` and `segment(text, *, lang)` do the same as the methods of `Segmenter(lang)`: the module keeps one segmenter per language. The language is a keyword argument and has no default.

=== "Code"

    ``` python
    from anyseg import sentenize, tokenize

    [span.text for span in tokenize("Мама мыла раму", lang="ru")]
    [span.text for span in sentenize("Мама мыла раму. Кот спит", lang="ru")]
    ```

=== "Result"

    ```
    ['Мама', 'мыла', 'раму']
    ['Мама мыла раму.', 'Кот спит']
    ```

## Span

``` {.python .signature}
Span(start, end, text)
```

A piece of a text with its position: `start` is the position of its first character, `end` the position after the last one, and `text` the piece. Positions are offsets in the characters of the Python string, as in its slices: `text[span.start:span.end] == span.text`. Spans are immutable, compare by all three attributes and can be pickled.

=== "Code"

    ``` python
    from anyseg import Span

    text = "Ёж идёт"
    span = Span(3, 7, "идёт")
    span, text[span.start : span.end] == span.text
    ```

=== "Result"

    ```
    (Span(start=3, end=7, text='идёт'), True)
    ```

## Rust

The segmenter is the Rust crate `anyseg`, with every language behind a Cargo feature: `ru` (Russian, on by default). It has the same `Segmenter` with `tokenize`, `sentenize` and `segment`, and gives positions in bytes of the UTF-8 string.

``` rust
use anyseg::Segmenter;

let segmenter = Segmenter::new("ru").unwrap();
let text = "Мама мыла раму";
let tokens: Vec<&str> = segmenter.tokenize(text).iter().map(|span| span.slice(text)).collect();
assert_eq!(tokens, ["Мама", "мыла", "раму"]);
```

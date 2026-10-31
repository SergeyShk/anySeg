# spaCy

The tokenizer and the sentence starts of anySeg in a [spaCy](https://spacy.io/) pipeline, and the conversion between a spaCy `Doc` and a [document](document.md).

``` bash
pip install "anyseg[spacy]"
```

The tokenizer and the component register with spaCy when the package is installed and are used by name, without importing anyseg. spaCy has no prebuilt wheels for the free-threaded Python 3.14t and 3.15t.

=== "Code"

    ``` python
    import spacy

    tokenizer = {"@tokenizers": "anyseg.Tokenizer.v1"}
    nlp = spacy.blank("ru", config={"nlp": {"tokenizer": tokenizer}})
    nlp.add_pipe("anyseg_sentencizer")
    doc = nlp("В 1914 г. цена была 3 000 руб. Затем выросла")
    [token.text for token in doc]
    [sentence.text for sentence in doc.sents]
    ```

=== "Result"

    ```
    Sentencizer('ru')
    ['В', '1914', 'г.', 'цена', 'была', '3 000', 'руб', '.', 'Затем', 'выросла']
    ['В 1914 г. цена была 3 000 руб.', 'Затем выросла']
    ```

## Tokenizer { #tokenizer }

`anyseg.Tokenizer.v1` is the tokenizer of the section `[nlp.tokenizer]` of a configuration. The text of a `Doc` is the text given, and the whitespace is kept as spaCy keeps it: one space after a token is its `whitespace_`, and the rest of the text between tokens - line breaks, more spaces, invisible separators - is a token of its own.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `lang` | `str` | `None` | ISO 639-1 code of the language; by default the language of the pipeline |
| `abbreviations` | `list[str]` | `None` | [More abbreviations](segmenter.md#lists) |
| `tokens` | `list[str]` | `None` | [Strings that are one token each](segmenter.md#lists) |

=== "Code"

    ``` python
    custom = {"@tokenizers": "anyseg.Tokenizer.v1", "tokens": ["C++"]}
    nlp = spacy.blank("ru", config={"nlp": {"tokenizer": custom}})
    [(token.text, token.whitespace_) for token in nlp("Язык C++\n\nКот  спит")]
    ```

=== "Result"

    ```
    [('Язык', ' '), ('C++', ''), ('\n\n', ''), ('Кот', ' '), (' ', ''), ('спит', '')]
    ```

The settings are in the configuration, so a saved pipeline loads with the tokenizer. A trained pipeline takes it when it is loaded:

``` python
nlp = spacy.load("ru_core_news_sm", config={"nlp": {"tokenizer": tokenizer}})
```

## Sentences { #sentencizer }

The component `anyseg_sentencizer` sets the sentence starts (`is_sent_start`) of a document as anySeg splits its text; before the parser it fixes the sentences, and the parser keeps them. It works with any tokenizer: a sentence starts at the token where its first character is, and the whitespace after a sentence stays in it.

``` python
nlp = spacy.load("ru_core_news_sm", config={"nlp": {"tokenizer": tokenizer}})
nlp.add_pipe("anyseg_sentencizer", before="parser")
```

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `lang` | `str` | `None` | ISO 639-1 code of the language; by default the language of the pipeline |
| `abbreviations` | `list[str]` | `None` | [More abbreviations](segmenter.md#lists) |
| `tokens` | `list[str]` | `None` | [Strings that are one token each](segmenter.md#lists) |
| `overwrite` | `bool` | `False` | Replace the sentence starts set before; `False` - set only the tokens whose start is unknown, as the `sentencizer` of spaCy does |
| `scorer` | `Callable` | `spacy.senter_scorer.v1` | Scores of the sentences in an evaluation: `sents_p`, `sents_r` and `sents_f`, as for the `sentencizer` of spaCy |

=== "Code"

    ``` python
    nlp = spacy.blank("ru")
    nlp.add_pipe("anyseg_sentencizer")
    [sentence.text for sentence in nlp("Кот спит в 1914 г. Затем\n\nУтро").sents]
    ```

=== "Result"

    ```
    Sentencizer('ru')
    ['Кот спит в 1914 г.', 'Затем\n\n', 'Утро']
    ```

With the tokenizer and the component, `ru_core_news_sm` 3.8.0 finds sentences and parses better. F1 of sentences and LAS on the test parts of the Russian treebanks of UD 2.18, from their raw text:

| Test set | Sentences, spaCy | Sentences, anySeg | LAS, spaCy | LAS, anySeg |
| :------- | ---------------: | ----------------: | ---------: | ----------: |
| SynTagRus | 95.39 | **98.53** | 81.83 | **82.96** |
| Taiga | 80.42 | **89.92** | 71.55 | **73.39** |
| GSD | 85.85 | **96.32** | 75.80 | **79.34** |
| PUD | 96.45 | **99.25** | 86.14 | **87.26** |

The tokenizer gives 2.3 M tokens per second, eight times as many as the tokenizer of spaCy; with the component, 1.4 M against 0.27 M with the `sentencizer` of spaCy (SynTagRus test, one thread, Apple M3 Pro).

## Documents { #documents }

<div class="metric-card" markdown>

### spaCy document of a document { #to_doc }

`anyseg.spacy.to_doc` · `Doc`
{ .metric-meta }

``` {.python .signature}
to_doc(document, vocab)
```

The `Doc` of a document: its text, its tokens with the whitespace between them as the tokenizer makes it, and its sentences. The columns `lemma`, `upos`, `xpos` and `feats` become `lemma_`, `pos_`, `tag_` and `morph` where set. Heads and relations are set when every word has a head: the relation `root` becomes `ROOT`, a word without a relation gets `dep` (a root - `ROOT`), and a whitespace token depends on the token before it with `dep`. A whitespace token has its text for a lemma and `SPACE` for a part of speech.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `document` | `Document` | - | Document |
| `vocab` | `Vocab` | - | Vocabulary of the pipeline |

Exceptions:

- `ValueError` - the document has a multiword token, or a part of speech that spaCy does not know.

=== "Code"

    ``` python
    from anyseg import Segmenter
    from anyseg.spacy import from_doc, to_doc

    document = Segmenter("ru").segment("Ёж идёт.  Кот спит")
    document.set_columns(upos=["NOUN", "VERB", "PUNCT", "NOUN", "VERB"])
    doc = to_doc(document, nlp.vocab)
    [(token.text, token.pos_) for token in doc]
    ```

=== "Result"

    ```
    [('Ёж', 'NOUN'), ('идёт', 'VERB'), ('.', 'PUNCT'), (' ', 'SPACE'), ('Кот', 'NOUN'), ('спит', 'VERB')]
    ```

</div>

<div class="metric-card" markdown>

### Document of a spaCy document { #from_doc }

`anyseg.spacy.from_doc` · `Document`
{ .metric-meta }

``` {.python .signature}
from_doc(doc)
```

The [document](document.md) of a `Doc`, one word per token. Tokens lose the whitespace and the invisible separators at their edges, and tokens of nothing else are dropped. A sentence of the `Doc` starts at its first token left, a `Doc` without sentences is a sentence per paragraph, and every paragraph starts a sentence. `lemma_`, `pos_`, `tag_`, `morph`, the heads and `dep_` become the columns `lemma`, `upos`, `xpos`, `feats`, `head` and `deprel`, empty values `None`; a token without a head has neither. The relation `ROOT` becomes `root`, and a word whose head is a dropped token depends on the head of that token. A sentence has one root: the root of the parse or, if the sentence lacks it, the first word whose head is in another sentence; the other such words depend on the root with the relation `dep`.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `doc` | `Doc` | - | spaCy document |

Exceptions:

- `ValueError` - a token runs over a blank line, or a value cannot be in its column, as for [`set_columns`](document.md#set_columns).

=== "Code"

    ``` python
    document = from_doc(doc)
    document, document.words[3]
    ```

=== "Result"

    ```
    (Document(tokens=5, words=5, sentences=2, paragraphs=1), Word(id=1, form='Кот', upos='NOUN'))
    ```

</div>

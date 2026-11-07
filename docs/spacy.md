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

`anyseg.Tokenizer.v1` is the tokenizer of the section `[nlp.tokenizer]` of a configuration. The text of a `Doc` is the text given, except that a lone surrogate, which spaCy cannot keep, is U+FFFD. One space after a token is its `whitespace_`, and the rest of the text between tokens - line breaks, more spaces, invisible separators - is a token of its own with `is_space`. `nlp.tokenizer.pipe(texts, batch_size=1000)` gives the documents of many texts, segmented in batches without the GIL as [`Segmenter.pipe`](segmenter.md#pipe) does.

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

The settings are in the configuration, so a saved pipeline loads with the tokenizer, and a pickled `Doc` of such a pipeline needs anySeg to load. A trained pipeline takes it when it is loaded:

``` python
nlp = spacy.load("ru_core_news_sm", config={"nlp": {"tokenizer": tokenizer}})
```

### Multiword tokens { #words }

The tokens of a `Doc` are those of the text, and the words of a multiword token are in its extension `token._.anyseg_words`: a list of dictionaries with their forms, and `None` for a token of one word or one that retokenizing changed.

=== "Code"

    ``` python
    spanish = spacy.blank("es", config={"nlp": {"tokenizer": tokenizer}})
    doc = spanish("Vengo del mar")
    [token.text for token in doc], doc[1]._.anyseg_words
    ```

=== "Result"

    ```
    (['Vengo', 'del', 'mar'], [{'form': 'de'}, {'form': 'el'}])
    ```

The extension lives in the user data of a `Doc`:

*   a `DocBin` keeps it with `store_user_data=True`, as a tuple in place of the list, and `Span.as_doc` with `copy_user_data=True`;
*   `Doc.from_docs` keeps it by the positions in the text, so a `Doc` cut from a longer one should not hold keys of the part cut away;
*   `Doc.to_json(underscore=["anyseg_words"])` needs a multiword token in the `Doc`, and `Doc.from_json` fails on a key of a token that was retokenized or cut away.

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

With the tokenizer and the component, `ru_core_news_sm` 3.8.0 finds sentences and parses better. F1 of sentences and LAS on the test parts of the Russian treebanks of UD 2.18, from their raw text, every paragraph a document, scored with the evaluation script of CoNLL 2018. [Comparison](comparison.md) segments every test file as one text, so its F1 of sentences of the same pipeline is lower on Taiga, whose paragraphs often end without a final mark:

| Test set | Sentences, spaCy | Sentences, anySeg | LAS, spaCy | LAS, anySeg |
| :------- | ---------------: | ----------------: | ---------: | ----------: |
| SynTagRus | 95.39 | **98.92** | 81.83 | **82.95** |
| Taiga | 80.42 | **89.92** | 71.55 | **73.39** |
| GSD | 85.85 | **96.32** | 75.80 | **79.34** |
| PUD | 96.45 | **99.25** | 86.14 | **87.26** |

With both, `es_core_news_sm` 3.8.0 does so on the test parts of the Spanish treebanks too. The parser reads the tokens of the text, and [`from_doc`](#from_doc) gives the words of the multiword tokens for the score:

| Test set | Sentences, spaCy | Sentences, anySeg | LAS, spaCy | LAS, anySeg |
| :------- | ---------------: | ----------------: | ---------: | ----------: |
| AnCora | 97.70 | **98.66** | 80.43 | **82.06** |
| GSD | 88.14 | **90.97** | 69.35 | **71.09** |
| PUD | 97.34 | **98.80** | 73.35 | **74.57** |

The tokenizer gives 2.3 M tokens per second, nearly ten times as many as the tokenizer of spaCy; with the component, 1.4 M against 0.24 M with the `sentencizer` of spaCy (SynTagRus test, one thread, Apple M3 Pro). On AnCora test it gives 2.3 M against 0.42 M, and 1.6 M against 0.39 M.

## Documents { #documents }

<div class="metric-card" markdown>

### spaCy document of a document { #to_doc }

`anyseg.spacy.to_doc` · `Doc`
{ .metric-meta }

``` {.python .signature}
to_doc(document, vocab)
```

The `Doc` of a document: its text, its tokens with the whitespace between them as the tokenizer makes it, and its sentences.

*   The columns `lemma`, `upos`, `xpos` and `feats` become `lemma_`, `pos_`, `tag_` and `morph` where set. A whitespace token has its text for a lemma and `SPACE` for a part of speech.
*   Heads and relations are set when every word has a head, and the heads of every sentence must form a tree with one root. The relation `root` becomes `ROOT`, a word without a relation gets `dep` (a root - `ROOT`), and a whitespace token depends on the token before it with `dep`.
*   The words of a multiword token go to `token._.anyseg_words` with their columns: `head` is the ID of a word in its sentence, `sentence_words` the number of words in it, and `main` marks the word that gave the token its values, its word closest to the root or its first word without heads.
*   The vocabulary takes the rule of anySeg for `is_space`, so that a pickled document keeps it; a tokenizer of spaCy on that vocabulary then marks invisible separators as whitespace too.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `document` | `Document` | - | Document |
| `vocab` | `Vocab` | - | Vocabulary of the pipeline |

Exceptions:

- `ValueError` - a token takes a part of speech that spaCy does not know, features that are not `name=value` pairs, or the heads of a sentence do not form a tree with one root.

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

The [document](document.md) of a `Doc`, one word per token except those with `token._.anyseg_words`.

*   Tokens lose the whitespace and the invisible separators at their edges, and tokens of nothing else are dropped. A sentence of the `Doc` starts at its first token left, a `Doc` without sentences is a sentence per paragraph, and every paragraph starts a sentence.
*   `lemma_`, `pos_`, `tag_`, `morph`, the heads and `dep_` become the columns `lemma`, `upos`, `xpos`, `feats`, `head` and `deprel`, empty values `None`, the features in the order of UD; a token without a head has neither. In a lemma, a line break or a tab becomes a space inside, with the spaces around it, and goes away at an edge.
*   The relation `ROOT` becomes `root`, and a word whose head is a dropped token depends on the head of that token. A sentence has one root: the root of the parse or, if the sentence lacks it, the first word whose head is in another sentence; the other such words depend on the root with `dep`.
*   The words of a multiword token keep the columns they have in the extension, but the word marked `main` takes the value of the token, which a pipeline may have changed, also over an edit of that word in the extension. Without a mark, the first word takes the value for a column none of them has.
*   Their heads stay if every word has `head` and `sentence_words` and the parse and the number of words of the sentence still agree with them; otherwise the first word takes the head and the relation of the token, and the other words depend on it with `dep`. As a parse knows only tokens, a word of one token that depends on a word of a multiword token other than the one that stands for it moves to that one.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `doc` | `Doc` | - | spaCy document |

Exceptions:

- `ValueError` - a token runs over a blank line, the extension is not a list of words with their forms or holds a form that cannot be in a document, or a value cannot be in its column, as for [`set_columns`](document.md#set_columns);
- `TypeError` - a value of the extension is of a wrong type, as for `set_columns`.

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

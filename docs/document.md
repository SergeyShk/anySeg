# Document

A segmented text: its tokens, its words with the columns of [CoNLL-U](https://universaldependencies.org/format.html), its sentences, its paragraphs and their lines. A document comes from [`Segmenter.segment`](segmenter.md#segment), from [`Document.from_tokens`](#from_tokens) or from [`read_conllu`](#read_conllu).

*   The text is never changed: tokens, sentences, paragraphs and lines are its spans, with positions in the characters of the Python string.
*   A token holds one word or, for a multiword token, several (Spanish «del» is «de» + «el»). The segmenter gives one word per token.
*   Sentences hold whole tokens, paragraphs - the parts of the text between blank lines - hold whole sentences, and lines are those of the paragraphs.
*   Words have the columns of CoNLL-U, empty until set; [`set_columns`](#set_columns) is the only change of a document, so a document can be shared between threads.

=== "Code"

    ``` python
    from anyseg import Segmenter

    doc = Segmenter("ru").segment("Ёж идёт домой.\nКот спит\n\nУтро  наступило")
    doc, [sentence.text for sentence in doc.sentences]
    [(line.text, line.break_before) for line in doc.lines]
    ```

=== "Result"

    ```
    (Document(tokens=8, words=8, sentences=3, paragraphs=2), ['Ёж идёт домой.', 'Кот спит', 'Утро  наступило'])
    [('Ёж идёт домой.', None), ('Кот спит', 'author'), ('Утро  наступило', None)]
    ```

## Parts of a document

| Property | Type | Description |
| :------- | :--- | :---------- |
| `text` | `str` | The text |
| `tokens` | `Tokens` | [Tokens](#token) in order |
| `words` | `Words` | [Words](#word) in order |
| `sentences` | `Sentences` | [Sentences](#sentence) in order |
| `paragraphs` | `Paragraphs` | [Paragraphs](#paragraph) in order |
| `lines` | `Lines` | [Lines](#line) of the paragraphs in order |

The parts are views of the document, made when they are read; iterating over a collection makes them one at a time. `Tokens`, `Words` and the other collections are sequences (`collections.abc.Sequence`): they have a length, indices from the end and slices, and a slice with step 1 is a collection of the same kind. Two tokens, words, sentences, paragraphs or lines are equal when they are the same part of the same document; the collections compare by identity. A document and its parts cannot be pickled.

=== "Code"

    ``` python
    token = doc.tokens[1]
    token, token.spaces_after, token.sentence
    doc.tokens[-2:], doc.sentences[2].paragraph
    ```

=== "Result"

    ```
    (Token(start=3, end=7, text='идёт'), ' ', Sentence(start=0, end=14, text='Ёж идёт домой.'))
    (Tokens([Token(start=25, end=29, text='Утро'), Token(start=31, end=40, text='наступило')]), Paragraph(start=25, end=40, text='Утро  наступило'))
    ```

### Token { #token }

A token of the document: a span of its text.

| Property | Type | Description |
| :------- | :--- | :---------- |
| `index` | `int` | Position of the token in the document, from 0 |
| `start`, `end` | `int` | Position of the first character and the position after the last one |
| `text` | `str` | Text of the token |
| `spaces_after` | `str` | Text between the token and the next one, or the end of the text |
| `words` | `Words` | Words of the token |
| `sentence` | `Sentence` | Sentence of the token |

### Word { #word }

A word of the document with its columns of CoNLL-U. Empty columns are `None`. The form of a word is the text of its token, except for the words of a multiword token.

| Property | Type | Description |
| :------- | :--- | :---------- |
| `index` | `int` | Position of the word in the document, from 0 |
| `id` | `int` | ID of the word in its sentence, from 1 |
| `form` | `str` | Form of the word |
| `lemma`, `upos`, `xpos`, `feats`, `deprel`, `deps`, `misc` | `str | None` | Columns of CoNLL-U |
| `head` | `int | None` | ID of the head word in the sentence, 0 for the root |
| `token` | `Token` | Token of the word |
| `sentence` | `Sentence` | Sentence of the word |

### Sentence { #sentence }

A sentence of the document: from the start of its first token to the end of its last one.

| Property | Type | Description |
| :------- | :--- | :---------- |
| `index` | `int` | Position of the sentence in the document, from 0 |
| `start`, `end` | `int` | Position of the first character and the position after the last one |
| `text` | `str` | Text of the sentence |
| `tokens`, `words` | `Tokens`, `Words` | Tokens and words of the sentence |
| `paragraph` | `Paragraph` | Paragraph of the sentence |

### Paragraph { #paragraph }

A paragraph of the document: a part of its text between blank lines, from the start of its first token to the end of its last one.

| Property | Type | Description |
| :------- | :--- | :---------- |
| `index` | `int` | Position of the paragraph in the document, from 0 |
| `start`, `end` | `int` | Position of the first character and the position after the last one |
| `text` | `str` | Text of the paragraph |
| `sentences`, `tokens` | `Sentences`, `Tokens` | Sentences and tokens of the paragraph |

### Line { #line }

A line of a paragraph, without the whitespace around it.

| Property | Type | Description |
| :------- | :--- | :---------- |
| `index` | `int` | Position of the line in the document, from 0 |
| `start`, `end` | `int` | Position of the first character and the position after the last one |
| `text` | `str` | Text of the line |
| `break_before` | `str | None` | How the line begins: `None` for the first line of a paragraph, `"author"` for a break the author made (a verse line, a list item, a paragraph per line), `"typeset"` for a wrap at the width of the page, as in text extracted from PDF |

## Methods

<div class="metric-card" markdown>

### Values of a column { #column }

`column` · `list[str | None]` / `list[int | None]`
{ .metric-meta }

``` {.python .signature}
column(name)
```

The values of a column of CoNLL-U, one per word: `form`, `lemma`, `upos`, `xpos`, `feats`, `head`, `deprel`, `deps` or `misc`. Empty values are `None`, heads are numbers. On a whole document the method is faster than reading the words one by one.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `name` | `str` | - | Name of the column |

Exceptions:

- `ValueError` - there is no such column.

</div>

<div class="metric-card" markdown>

### Set columns { #set_columns }

`set_columns` · `None`
{ .metric-meta }

``` {.python .signature}
set_columns(*, lemma=..., upos=..., xpos=..., feats=..., head=..., deprel=..., deps=..., misc=...)
```

Sets columns of the words, one value per word for every column given: all at once, or none if a value is wrong. `None` empties a value; a head is 0 or the ID of a word of its sentence. The forms come from the tokens and cannot be set.

Exceptions:

- `ValueError` - there is no such column, a list has not one value per word, a value is empty, a lemma or MISC holds a tab or a line break or another value holds whitespace, or a head is out of its sentence;
- `TypeError` - a value is neither a string nor `None`, or a head is not an integer.

=== "Code"

    ``` python
    doc = Segmenter("ru").segment("Ёж идёт домой")
    doc.set_columns(
        lemma=["ёж", "идти", "домой"],
        upos=["NOUN", "VERB", "ADV"],
        head=[2, 0, 2],
        deprel=["nsubj", "root", "advmod"],
    )
    doc.column("upos"), doc.column("head"), doc.words[0]
    ```

=== "Result"

    ```
    (['NOUN', 'VERB', 'ADV'], [2, 0, 2], Word(id=1, form='Ёж', lemma='ёж', upos='NOUN', head=2, deprel='nsubj'))
    ```

</div>

<div class="metric-card" markdown>

### Document in CoNLL-U { #to_conllu }

`to_conllu` · `str`
{ .metric-meta }

``` {.python .signature}
to_conllu(*, doc_id=None)
```

The document in CoNLL-U, every sentence followed by a blank line. A document read from CoNLL-U is written as it was read, with the columns set since. Otherwise every sentence gets the comments `sent_id` and `text`, the first one `newdoc` and every paragraph `newpar`, and the space after a token inside a paragraph goes to MISC: `SpaceAfter=No` when there is none and `SpacesAfter` when it is not one space. Forms, the comment `text` and the values are in Unicode NFC, as CoNLL-U requires; the document keeps its text.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `doc_id` | `str` | `None` | ID of the document: `newdoc` gets it and `sent_id` becomes `ID-N`, so that the sentences of several documents in one file have distinct IDs; a document read from CoNLL-U keeps its own comments and ignores it |

Exceptions:

- `ValueError` - the ID is empty or holds whitespace.

=== "Code"

    ``` python
    print(doc.to_conllu(), end="")
    print(Segmenter("ru").segment("Ёж идёт.  Кот спит").to_conllu(doc_id="news-1"), end="")
    ```

=== "Result"

    ```
    # newdoc
    # newpar
    # sent_id = 1
    # text = Ёж идёт домой
    1	Ёж	ёж	NOUN	_	_	2	nsubj	_	_
    2	идёт	идти	VERB	_	_	0	root	_	_
    3	домой	домой	ADV	_	_	2	advmod	_	_

    # newdoc id = news-1
    # newpar
    # sent_id = news-1-1
    # text = Ёж идёт.
    1	Ёж	_	_	_	_	_	_	_	_
    2	идёт	_	_	_	_	_	_	_	SpaceAfter=No
    3	.	_	_	_	_	_	_	_	SpacesAfter=\s\s

    # sent_id = news-1-2
    # text = Кот спит
    1	Кот	_	_	_	_	_	_	_	_
    2	спит	_	_	_	_	_	_	_	_
    ```

</div>

<div class="metric-card" markdown>

### Document of tokens { #from_tokens }

`Document.from_tokens` · `Document`
{ .metric-meta }

``` {.python .signature}
Document.from_tokens(text, starts, ends, *, sentence_starts=None)
```

The document of the tokens of a text made by another tool, one word per token. Paragraphs are the parts of the text between blank lines, and every paragraph starts a sentence; without sentence starts a paragraph is one sentence.

| Parameter | Type | Default | Description |
| :-------- | :--- | :------ | :---------- |
| `text` | `str` | - | Text |
| `starts` | `Sequence[int]` | - | Positions of the first characters of the tokens |
| `ends` | `Sequence[int]` | - | Positions after their last characters |
| `sentence_starts` | `Sequence[int]` | `None` | Indices of the tokens that start sentences |

Exceptions:

- `ValueError` - the lists differ in length, a token is empty, out of the text, overlaps the token before it, starts or ends with whitespace or runs over a paragraph boundary, the text between the tokens holds anything but whitespace and the invisible separators, or the sentence starts are not increasing token indices;
- `TypeError` - a list holds something other than integers.

=== "Code"

    ``` python
    from anyseg import Document

    doc = Document.from_tokens("Ёж идёт домой", [0, 3, 8], [2, 7, 13], sentence_starts=[0, 2])
    [sentence.text for sentence in doc.sentences]
    ```

=== "Result"

    ```
    ['Ёж идёт', 'домой']
    ```

</div>

## Reading CoNLL-U { #read_conllu }

``` {.python .signature}
read_conllu(text)
```

The documents of a text in CoNLL-U, in order. The comment `newdoc` starts a document and `newpar` a paragraph. The text of a sentence is its comment `text` or, without it, the forms of its tokens with the spaces of `SpaceAfter` and `SpacesAfter`; the sentences of a paragraph are joined by a space (none after `SpaceAfter=No`), the paragraphs by a blank line. Comments, the lines of multiword tokens and empty nodes are kept, so that `to_conllu` writes the text back as it was, with LF line ends and heads as plain numbers. A byte order mark at the start is skipped.

Exceptions:

- `ValueError` - the text does not end with a blank line, a line is neither a comment, a blank line nor a token line of 10 columns with IDs in order and a numeric head, a comment follows token lines, a sentence has no words or a multiword token misses some, or the forms do not match the text of their sentence or the text holds characters that no token has.

=== "Code"

    ``` python
    from anyseg import read_conllu

    conllu = "# text = Ёж идёт\n1\tЁж\tёж\tNOUN\t_\t_\t2\tnsubj\t_\t_\n2\tидёт\tидти\tVERB\t_\t_\t0\troot\t_\t_\n\n"
    docs = read_conllu(conllu)
    docs[0].words[1], docs[0].to_conllu() == conllu
    ```

=== "Result"

    ```
    (Word(id=2, form='идёт', lemma='идти', upos='VERB', head=0, deprel='root'), True)
    ```

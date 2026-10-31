# anyseg

Fast segmentation of text into tokens and sentences with their positions in the source text. The text is never changed: every token and sentence is a span of byte offsets into the original string, and tokens follow the conventions of [Universal Dependencies](https://universaldependencies.org/). Every language is a Cargo feature: `ru` (Russian, on by default).

```rust
use anyseg::Segmenter;

let segmenter = Segmenter::new("ru").unwrap();
let text = "Ёж идёт домой";
let tokens: Vec<&str> = segmenter.tokenize(text).iter().map(|span| span.slice(text)).collect();
assert_eq!(tokens, ["Ёж", "идёт", "домой"]);

let doc = segmenter.segment("Ёж идёт домой\n\nКот спит");
assert_eq!(doc.sentence(1).span.slice(doc.text()), "Кот спит");
```

A `Document` holds the tokens, the words with the columns of CoNLL-U, the sentences, the paragraphs and their lines; `Document::to_conllu` and `read_conllu` write and read CoNLL-U, and a document read from CoNLL-U is written back as it was, with its comments, multiword tokens and empty nodes.

The feature `cli` builds the command `anyseg`, which writes the tokens, the sentences or CoNLL-U of files and the standard input: `cargo install anyseg --features cli`, then `anyseg --lang ru --format conllu text.txt`.

The Python package with the same segmenter is [`anyseg`](https://pypi.org/project/anyseg/) on PyPI; the documentation is at [sergeyshk.github.io/anySeg](https://sergeyshk.github.io/anySeg/).

## License

MIT

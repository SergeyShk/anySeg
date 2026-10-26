# anyseg

Fast segmentation of text into tokens and sentences with their positions in the source text. The text is never changed: every token and sentence is a span of byte offsets into the original string, and tokens follow the conventions of [Universal Dependencies](https://universaldependencies.org/). Every language is a Cargo feature: `ru` (Russian, on by default).

```rust
use anyseg::Segmenter;

let segmenter = Segmenter::new("ru").unwrap();
let text = "Ёж идёт домой";
let tokens: Vec<&str> = segmenter.tokenize(text).iter().map(|span| span.slice(text)).collect();
assert_eq!(tokens, ["Ёж", "идёт", "домой"]);
```

The Python package with the same segmenter is [`anyseg`](https://github.com/SergeyShk/anySeg) on PyPI.

## License

MIT

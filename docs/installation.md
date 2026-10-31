# Installation

Python 3.11 or newer is required.

``` bash
pip install anyseg
```

The package has no dependencies. The tokenizer and the sentence starts for [spaCy](spacy.md) come with the extra `spacy`:

``` bash
pip install "anyseg[spacy]"
```

## Prebuilt wheels

PyPI hosts wheels for Linux (x86_64 and aarch64; manylinux and musllinux for Alpine), macOS (Intel and Apple Silicon) and Windows (x64 and arm64). One wheel serves every Python version starting with 3.11, and the free-threaded Python 3.14t and 3.15t have wheels of their own; Rust is not needed to install them.

Pre-release versions `X.Y.Z.devN` are published only on TestPyPI:

``` bash
pip install --index-url https://test.pypi.org/simple/ --pre anyseg
```

## Building from source

On other platforms pip builds the package from source, which requires [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer:

``` bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
pip install anyseg
```

The development environment is described in [CONTRIBUTING.md](https://github.com/SergeyShk/anySeg/blob/master/CONTRIBUTING.md).

## Rust { #rust }

The segmenter is published on crates.io as the [`anyseg`](https://crates.io/crates/anyseg) crate, with every language behind a Cargo feature: `ru` (Russian, on by default). It gives positions in bytes of the UTF-8 string. The API documentation is on [docs.rs](https://docs.rs/anyseg).

``` bash
cargo add anyseg
```

The [command](cli.md) `anyseg` comes with the Python package; without Python, the crate installs it:

``` bash
cargo install anyseg --features cli
```

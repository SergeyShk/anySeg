# Installation

## Requirements

*   `python` 3.11 or newer, including free-threaded 3.14t and 3.15t

The package has no dependencies. Building it from the source also needs [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer.

## From the repository

``` bash
git clone https://github.com/SergeyShk/anySeg.git
cd anySeg
uv sync
```

`uv sync` builds the Rust module and installs the package into the environment of the project.

## Rust crate

The segmenter itself is the Rust crate `anyseg`, with every language behind a Cargo feature: `ru` (Russian, on by default). It gives positions in bytes of the UTF-8 string.

``` toml
[dependencies]
anyseg = { git = "https://github.com/SergeyShk/anySeg" }
```

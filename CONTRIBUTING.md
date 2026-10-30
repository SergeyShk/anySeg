# Contributing

Bug reports, ideas and pull requests are welcome.

## Reporting a bug or proposing an idea

Open an [issue](https://github.com/SergeyShk/anySeg/issues/new/choose) using a template. A bug report needs the text that is segmented wrongly, the tokens or sentences you expected and the ones you got, and the versions of anySeg, Python and the OS. A proposal needs the behaviour you want and its source: tokens follow the [Universal Dependencies](https://universaldependencies.org/) guidelines and, where they are silent, the conventions of the main treebank of the language.

## Development environment

The project uses [uv](https://docs.astral.sh/uv/) for dependencies, [maturin](https://www.maturin.rs/) to build the Rust module and [ruff](https://docs.astral.sh/ruff/) for linting and formatting. Python 3.11 or newer and [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer are required.

```bash
git clone https://github.com/SergeyShk/anySeg.git
cd anySeg

make deps                   # create the environment, install all dependencies and build the Rust module
uv run pre-commit install   # hooks: linters on commit, tests on push
```

The Rust module is built when the package is installed into the environment; `uv run` rebuilds it by itself when the files of `crates/` or `Cargo.toml` change. The full list of commands is in `make help`.

## Checks

Before submitting a pull request, these must pass:

```bash
make lint        # ruff check, ruff format, mypy, stubtest, cargo fmt, cargo clippy
make test-cov    # pytest with the examples and the coverage threshold, cargo test
make docs-build  # mkdocs build --strict, if the documentation changed
```

CI runs the same on Python 3.11-3.15 and free-threaded 3.14t and 3.15t (Linux), on Windows and macOS for one Python version, and checks the build on the minimum Rust version 1.88. When raising the minimum Rust version, change it in `rust-version` in `Cargo.toml`, in the `msrv` job of `ci.yml` and in the documentation.

## What a pull request is expected to contain

- **One topic per pull request.** Unrelated changes go separately.
- **Languages are data.** The mechanism is shared by all languages; a language adds its lists and tables behind a Cargo feature, not code paths of its own.
- **Measurements.** A change of the segmentation gives the F1 of tokens, words and sentences on all Universal Dependencies treebanks of the language before and after, measured from the raw text with the [CoNLL 2018 evaluation script](https://universaldependencies.org/conll18/evaluation.html), and says how it was measured. A rule is added only when it does not make the other treebanks of the language worse beyond noise. A pure difference of conventions between treebanks is resolved by the guidelines of Universal Dependencies, not by a rule for one treebank.
- **Tests.** New code is covered by tests: the Rust code by tests in `crates/`, the Python API by tests in `tests/`; Python coverage stays at 100%. The examples of the docstrings, the README and the documentation pages run in the tests and show the actual output.
- **Style.** Python is formatted with ruff and checked with mypy; Rust is formatted with rustfmt and checked with clippy without warnings (`make format`, `make lint`). Docstrings, comments and documentation are in English; docstrings follow the project format (`Description`, `Arguments`, `Returns`, `Raises`, `Example`). Comments do not repeat what the code already says.
- **Commits.** Messages in English, imperative mood: "Add ...", "Fix ...", "Remove ...".

The pull request description answers two questions: what was done and why. If it closes an issue, mention its number.

## Repository layout

- `crates/anyseg/` - the segmenter in Rust, the languages behind Cargo features; published on crates.io.
- `crates/anyseg-py/` - the Python module `anyseg._anyseg` (PyO3).
- `python/anyseg/` - the Python package.
- `tests/` - tests of the Python API.
- `docs/` - MkDocs (Material) documentation, `mkdocs.yml` - navigation.
- `.github/workflows/` - CI (`ci.yml`), publishing to PyPI and crates.io (`publish.yml`), documentation (`docs.yml`).

The version lives in `pyproject.toml` and, in its semver form, in `Cargo.toml`; a test checks that they match. Releases are made through GitHub Releases, changes are described in the release notes, there is no separate CHANGELOG file.

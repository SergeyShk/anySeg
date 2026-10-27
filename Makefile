.PHONY: help uv deps lock lint ruff format mypy rust-lint test test-cov clean clean-build clean-pyc clean-test clean-rust build docs-build docs-serve
.DEFAULT_GOAL := help
SRC_PATH := python
TESTS_PATH := tests
SCRIPTS_PATH := scripts
PACKAGE := anyseg

help: ## Show the list of commands
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "%-20s %s\n", $$1, $$2}'

uv: ## Check that uv and Rust are installed
	@which uv >/dev/null 2>&1 || { \
		echo "uv is not installed. Run 'curl -LsSf https://astral.sh/uv/install.sh | sh' or 'brew install uv'"; \
		exit 1; \
	}
	@which cargo >/dev/null 2>&1 || { \
		echo "Rust is not installed. Run 'curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh'"; \
		exit 1; \
	}

deps: uv ## Install the dependencies and build the extension module
ifeq ($(MODE), ci)
	uv sync --locked --all-groups
else
	uv sync --all-groups
endif

lock: uv ## Update the lock files to the latest versions of the dependencies
	uv lock --upgrade
	cargo update

lint: ruff mypy rust-lint ## Run all code checks

ruff: deps ## Check and format the Python code with ruff
ifeq ($(MODE), ci)
	uv run ruff check $(SRC_PATH) $(SCRIPTS_PATH) $(TESTS_PATH)
	uv run ruff format $(SRC_PATH) $(SCRIPTS_PATH) $(TESTS_PATH) --check
else
	uv run ruff check $(SRC_PATH) $(SCRIPTS_PATH) $(TESTS_PATH) --fix
	uv run ruff format $(SRC_PATH) $(SCRIPTS_PATH) $(TESTS_PATH)
endif

format: deps ## Format the code
	uv run ruff format $(SRC_PATH) $(SCRIPTS_PATH) $(TESTS_PATH)
	cargo fmt --all

mypy: deps ## Check the types with mypy and the type stub of the Rust module with stubtest
	uv run mypy
	uv run python -m mypy.stubtest anyseg._anyseg

rust-lint: uv ## Check the Rust code with rustfmt and clippy
ifeq ($(MODE), ci)
	cargo fmt --all --check
else
	cargo fmt --all
endif
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy -p $(PACKAGE) --no-default-features --all-targets -- -D warnings

test: deps ## Run the tests
	uv run pytest
	cargo test -p $(PACKAGE)
	cargo test -p $(PACKAGE) --no-default-features

test-cov: deps ## Run the tests with the coverage check
	uv run pytest --cov $(PACKAGE)
	cargo test -p $(PACKAGE)
	cargo test -p $(PACKAGE) --no-default-features

clean: clean-build clean-pyc clean-test ## Remove all artifacts except the Rust build
	rm -f .coverage coverage.xml

clean-build: ## Remove the package build artifacts
	rm -fr build/ dist/ .eggs/
	find . -name '*.egg-info' -not -path './target/*' -exec rm -fr {} +

clean-pyc: ## Remove the Python compilation artifacts
	find . -name '*.pyc' -not -path './target/*' -delete
	find . -name '*.pyo' -not -path './target/*' -delete
	find . -name '__pycache__' -type d -not -path './target/*' -exec rm -rf {} + 2>/dev/null || true

clean-test: ## Remove the test and lint artifacts
	rm -fr .pytest_cache .mypy_cache .ruff_cache

clean-rust: ## Remove the Rust build
	cargo clean

build: clean uv ## Build the distribution for the current platform
	uv build
	ls -l dist

docs-build: deps ## Build the documentation
	rm -fr site/
	uv run mkdocs build --strict

docs-serve: deps ## Serve the documentation locally
	uv run mkdocs serve

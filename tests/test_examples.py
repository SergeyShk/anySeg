"""The examples of the docstrings of the extension module, of the README and of the documentation pages"""

import ast
import contextlib
import doctest
import inspect
import io
import re
import textwrap
from pathlib import Path

import pytest

from anyseg import _anyseg

ROOT = Path(__file__).parents[1]
FLAGS = doctest.NORMALIZE_WHITESPACE | doctest.ELLIPSIS
# A Code tab and the Result tab after it, both indented under their `=== "..."` lines
TABS = re.compile(r'=== "Code"\n\n((?:    .*\n|\n)+?)=== "Result"\n\n((?:    .*\n|\n)+)')
PAGES = sorted(
    path
    for path in (ROOT / "docs").glob("*.md")
    if '=== "Code"' in path.read_text(encoding="utf-8")
)


def test_docstrings_of_the_extension_module():
    # pytest collects the doctests of Python files only
    finder = doctest.DocTestFinder()
    runner = doctest.DocTestRunner(optionflags=FLAGS)
    objects = [
        value for value in vars(_anyseg).values() if inspect.isclass(value) or callable(value)
    ]
    tests = [test for value in objects for test in finder.find(value, module=False, globs={})]
    for test in tests:
        runner.run(test)
    assert runner.failures == 0
    assert sum(len(test.examples) for test in tests) >= 30


def test_readme():
    # A fence right after an output would count as a line of it
    text = re.sub(r"^```.*$", "", (ROOT / "README.md").read_text(encoding="utf-8"), flags=re.M)
    test = doctest.DocTestParser().get_doctest(text, {}, "README.md", None, 0)
    runner = doctest.DocTestRunner(optionflags=FLAGS)
    runner.run(test)
    assert runner.failures == 0
    assert len(test.examples) >= 8


def run(code, namespace):
    """The output of code as the Python prompt shows it: the value of every expression but a call of print"""
    out = io.StringIO()
    for node in ast.parse(code).body:
        shown = isinstance(node, ast.Expr) and not (
            isinstance(node.value, ast.Call) and getattr(node.value.func, "id", "") == "print"
        )
        with contextlib.redirect_stdout(out):
            if shown:
                value = eval(compile(ast.Expression(node.value), "<docs>", "eval"), namespace)
                if value is not None:
                    print(repr(value))
            else:
                exec(compile(ast.Module([node], []), "<docs>", "exec"), namespace)
    return out.getvalue()


def lines(text):
    return "\n".join(line.rstrip() for line in text.splitlines()).strip("\n")


@pytest.mark.parametrize("page", PAGES, ids=[page.name for page in PAGES])
def test_documentation(page):
    text = page.read_text(encoding="utf-8")
    if "import spacy" in text:
        pytest.importorskip("spacy")
    namespace = {}
    tabs = list(TABS.finditer(text))
    assert tabs
    for match in tabs:
        code = re.sub(r"^``` ?\w*\n|^```\n?", "", textwrap.dedent(match.group(1)), flags=re.M)
        result = re.sub(r"^```\w*\n|^```\n?", "", textwrap.dedent(match.group(2)), flags=re.M)
        assert lines(run(code.strip("\n"), namespace)) == lines(result), code

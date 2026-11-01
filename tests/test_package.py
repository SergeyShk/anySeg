import importlib
import importlib.metadata
import re
import shutil
import subprocess
import sys
import sysconfig
import tomllib
import zipfile
from pathlib import Path

import pytest

import anyseg

ROOT = Path(__file__).parents[1]


def test_version():
    assert re.fullmatch(r"\d+\.\d+\.\d+(\.dev\d+)?", anyseg.__version__)
    assert anyseg.__version__ == importlib.metadata.version("anyseg")


def test_crate_version():
    with (ROOT / "Cargo.toml").open("rb") as file:
        crate = tomllib.load(file)["workspace"]["package"]["version"]
    with (ROOT / "pyproject.toml").open("rb") as file:
        package = tomllib.load(file)["project"]["version"]
    assert crate == re.sub(r"\.dev(\d+)$", r"-dev.\1", package)


def test_public_names():
    assert anyseg.__all__ == sorted(anyseg.__all__)
    assert all(hasattr(anyseg, name) for name in anyseg.__all__)
    assert anyseg.Segmenter.__module__ == anyseg.Span.__module__ == "anyseg"


def test_extension():
    extension = importlib.import_module("anyseg._anyseg")
    assert Path(extension.__file__).parent == Path(anyseg.__file__).parent


def test_spacy_bridge_without_spacy(monkeypatch):
    monkeypatch.setitem(sys.modules, "spacy", None)
    monkeypatch.delitem(sys.modules, "anyseg.spacy", raising=False)
    with pytest.raises(ImportError, match='pip install "anyseg\\[spacy\\]"'):
        importlib.import_module("anyseg.spacy")


def test_broken_spacy(monkeypatch):
    # An error inside spaCy is its own, not a missing extra
    class Broken:
        @staticmethod
        def find_spec(name, path=None, target=None):
            if name == "spacy":
                raise ModuleNotFoundError("No module named 'numpy'", name="numpy")

    monkeypatch.delitem(sys.modules, "spacy", raising=False)
    monkeypatch.delitem(sys.modules, "anyseg.spacy", raising=False)
    monkeypatch.setattr(sys, "meta_path", [Broken(), *sys.meta_path])
    with pytest.raises(ModuleNotFoundError, match="numpy"):
        importlib.import_module("anyseg.spacy")


@pytest.mark.skipif(shutil.which("cargo") is None, reason="the crates are read with cargo")
def test_third_party_notices(tmp_path):
    # The notices follow Cargo.lock: after a change of it, run python scripts/notices.py
    path = tmp_path / "notices.txt"
    subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "notices.py"), str(path)],
        capture_output=True,
        check=True,
    )
    expected = (ROOT / "THIRD-PARTY-NOTICES.txt").read_text(encoding="utf-8")
    assert path.read_text(encoding="utf-8") == expected, "run python scripts/notices.py"


def test_package_data():
    assert (Path(anyseg.__file__).parent / "py.typed").is_file()


@pytest.mark.skipif(shutil.which("uv") is None, reason="the wheel is built with uv")
def test_wheel_contents(tmp_path):
    subprocess.run(
        [
            "uv",
            "build",
            "--wheel",
            "--python",
            sys.executable,
            "--out-dir",
            str(tmp_path),
            str(ROOT),
        ],
        capture_output=True,
        check=True,
    )
    (wheel,) = tmp_path.glob("*.whl")
    assert ("-abi3-" in wheel.name) != bool(sysconfig.get_config_var("Py_GIL_DISABLED"))
    with zipfile.ZipFile(wheel) as archive:
        names = set(archive.namelist())
    assert {
        "anyseg/__init__.py",
        "anyseg/_anyseg.pyi",
        "anyseg/py.typed",
        "anyseg/spacy.py",
    } <= names
    licenses = {name.rsplit("/", 1)[-1] for name in names if "/licenses/" in name}
    assert licenses == {"LICENSE.txt", "THIRD-PARTY-NOTICES.txt"}
    modules = [
        name for name in names if re.fullmatch(r"anyseg/_anyseg(\.[\w-]+)?\.(so|pyd)", name)
    ]
    assert len(modules) == 1
    assert all(name.startswith(("anyseg/", "anyseg-")) for name in names)

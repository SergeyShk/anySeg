"""The anyseg command: segmentation of text from files and the standard input"""

import shutil
import signal
import subprocess
import sys
from pathlib import Path

import pytest

import anyseg
from anyseg import Segmenter, __main__ as command

TEXT = "Ёж идёт домой. Кот спит\n\nУтро настало\n"


def run(*args: str, text: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-m", "anyseg", *args],
        input=text,
        capture_output=True,
        encoding="utf-8",
        check=False,
    )


def test_formats(tmp_path):
    result = run("-l", "ru", text=TEXT)
    assert (result.returncode, result.stderr) == (0, "")
    assert result.stdout == "Ёж\nидёт\nдомой\n.\n\nКот\nспит\n\nУтро\nнастало\n\n"
    result = run("-l", "ru", "-f", "sentences", text=TEXT)
    assert result.stdout == "Ёж идёт домой.\nКот спит\n\nУтро настало\n"
    path = tmp_path / "text.txt"
    path.write_text(TEXT, encoding="utf-8")
    output = tmp_path / "text.conllu"
    result = run("--lang", "ru", "--format", "conllu", "-o", str(output), str(path))
    assert (result.returncode, result.stdout) == (0, "")
    assert output.read_text(encoding="utf-8") == Segmenter("ru").segment(TEXT).to_conllu()


def test_errors():
    result = run(text=TEXT)
    assert result.returncode == 2
    assert result.stderr.startswith("anyseg: the language is required")
    assert run("-l", "ru", "missing.txt").returncode == 1


def test_script():
    script = shutil.which("anyseg", path=str(Path(sys.executable).parent))
    assert script is not None
    result = subprocess.run(
        [script, "--version"], capture_output=True, encoding="utf-8", check=True
    )
    assert result.stdout == f"anyseg {anyseg.__version__}\n"


def test_main_in_process(tmp_path, monkeypatch, capfd):
    path = tmp_path / "text.txt"
    path.write_text(TEXT, encoding="utf-8")
    handlers = []
    monkeypatch.setattr(signal, "signal", lambda *args: handlers.append(args))
    monkeypatch.setattr(sys, "argv", ["anyseg", "-l", "ru", "-f", "sentences", str(path)])
    with pytest.raises(SystemExit) as exit_info:
        command.main()
    assert exit_info.value.code == 0
    assert handlers == [(signal.SIGINT, signal.SIG_DFL)]
    assert capfd.readouterr().out == "Ёж идёт домой.\nКот спит\n\nУтро настало\n"
    # An ignored SIGINT stays ignored
    handlers.clear()
    monkeypatch.setattr(signal, "getsignal", lambda _: signal.SIG_IGN)
    with pytest.raises(SystemExit):
        command.main()
    assert handlers == []


def test_paragraph_by_paragraph():
    # A paragraph is written as soon as a blank line ends it: the command can be kept open
    process = subprocess.Popen(
        [sys.executable, "-m", "anyseg", "-l", "ru", "-f", "sentences"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        encoding="utf-8",
    )
    assert process.stdin is not None and process.stdout is not None
    process.stdin.write("Ёж идёт. Кот спит\n\n")
    process.stdin.flush()
    assert [process.stdout.readline() for _ in range(2)] == ["Ёж идёт.\n", "Кот спит\n"]
    process.stdin.write("Утро\n")
    process.stdin.close()
    assert process.stdout.read() == "\nУтро\n"
    assert process.wait(timeout=10) == 0

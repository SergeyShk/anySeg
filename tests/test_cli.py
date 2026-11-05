"""The anyseg command: segmentation of text from files and the standard input"""

import io
import shutil
import signal
import subprocess
import sys
import threading
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

    def install(*args):
        handlers.append(args)
        return signal.default_int_handler

    monkeypatch.setattr(signal, "signal", install)
    monkeypatch.setattr(sys, "argv", ["anyseg", "-l", "ru", "-f", "sentences", str(path)])
    with pytest.raises(SystemExit) as exit_info:
        command.main()
    assert exit_info.value.code == 0
    # The handler of Python comes back after the command
    assert handlers == [
        (signal.SIGINT, signal.SIG_DFL),
        (signal.SIGINT, signal.default_int_handler),
    ]
    assert capfd.readouterr().out == "Ёж идёт домой.\nКот спит\n\nУтро настало\n"
    # An ignored SIGINT stays ignored, and another thread does not touch the handlers
    handlers.clear()
    monkeypatch.setattr(signal, "getsignal", lambda _: signal.SIG_IGN)
    with pytest.raises(SystemExit):
        command.main()
    monkeypatch.setattr(signal, "getsignal", lambda _: signal.default_int_handler)
    codes = []

    def run_in_thread():
        try:
            command.main()
        except SystemExit as error:
            codes.append(error.code)

    thread = threading.Thread(target=run_in_thread)
    thread.start()
    thread.join()
    assert (codes, handlers) == ([0], [])


def test_main_writes_after_python(tmp_path, monkeypatch, capfd):
    # What Python printed before the command comes before its output
    path = tmp_path / "text.txt"
    path.write_text("Кот спит", encoding="utf-8")
    monkeypatch.setattr(sys, "argv", ["anyseg", "-l", "ru", "-f", "sentences", str(path)])
    print("header", end="\n")
    with pytest.raises(SystemExit):
        command.main()
    assert capfd.readouterr().out == "header\nКот спит\n"
    # Without sys.stdout, as under pythonw, with it closed or without flush, the command writes to the
    # descriptor all the same
    closed = io.StringIO()
    closed.close()
    for stream in (None, closed, object()):
        monkeypatch.setattr(sys, "stdout", stream)
        with pytest.raises(SystemExit) as exit_info:
            command.main()
        assert exit_info.value.code == 0
        assert capfd.readouterr().out == "Кот спит\n"


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


@pytest.mark.skipif(sys.platform == "win32", reason="a closed descriptor of a shell")
def test_closed_standard_output():
    for args in ("-l ru", "--version"):
        result = subprocess.run(
            ["sh", "-c", f'"{sys.executable}" -m anyseg {args} >&-'],
            input=TEXT,
            capture_output=True,
            encoding="utf-8",
            check=False,
        )
        assert (result.returncode, result.stderr) == (1, "anyseg: the standard output is closed\n")


def test_complete_paragraphs_before_waiting():
    # A blank line and the next text in one read: the paragraph before it comes out while the command waits
    process = subprocess.Popen(
        [sys.executable, "-m", "anyseg", "-l", "ru", "-f", "sentences"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        encoding="utf-8",
    )
    assert process.stdin is not None and process.stdout is not None
    process.stdin.write("Ёж идёт.\n\nКот")
    process.stdin.flush()
    assert process.stdout.readline() == "Ёж идёт.\n"
    process.stdin.close()
    assert process.stdout.read() == "\nКот\n"
    assert process.wait(timeout=10) == 0


@pytest.mark.skipif(sys.platform == "win32", reason="/dev/stdin")
def test_output_is_the_standard_input(tmp_path):
    path = tmp_path / "text.txt"
    path.write_text(TEXT, encoding="utf-8")
    with path.open(encoding="utf-8") as stdin:
        result = subprocess.run(
            [sys.executable, "-m", "anyseg", "-l", "ru", "-o", str(path), "/dev/stdin"],
            stdin=stdin,
            capture_output=True,
            encoding="utf-8",
            check=False,
        )
    assert result.returncode == 1
    assert "the output file is an input file" in result.stderr
    assert path.read_text(encoding="utf-8") == TEXT


def test_conllu_of_multiword_tokens():
    result = run("-l", "es", "-f", "conllu", text="Vengo del mar\n")
    assert result.returncode == 0
    assert "2-3\tdel\t_\t_\t_\t_\t_\t_\t_\t_\n2\tde\t" in result.stdout
    tokens = run("-l", "es", text="Vengo del mar\n")
    assert tokens.stdout.split() == ["Vengo", "del", "mar"]

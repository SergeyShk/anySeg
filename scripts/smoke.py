"""Check of the installed package before publishing: segmentation, positions, threads, pickle and the command

Run: `python scripts/smoke.py [version]`; from the `scripts` directory Python imports the installed
package, not the sources of the repository.
"""

import pickle
import subprocess
import sys
import sysconfig

import anyseg
from anyseg import Segmenter, sentenize, tokenize

text = "Ёж идёт домой 😀"
spans = tokenize(text, lang="ru")
assert [span.text for span in spans] == ["Ёж", "идёт", "домой", "😀"], spans
assert all(text[span.start : span.end] == span.text for span in spans), spans
assert [span.text for span in sentenize(f" {text}\n", lang="ru")] == [text]
assert pickle.loads(pickle.dumps(Segmenter("ru"))).tokenize(text) == spans
docs = list(Segmenter("ru").pipe([text] * 4, threads=2, batch_size=3))
assert [doc.tokens.starts for doc in docs] == [[span.start for span in spans]] * 4, docs
command = subprocess.run(
    [sys.executable, "-m", "anyseg", "-l", "ru", "-f", "sentences"],
    input=f" {text}\n",
    capture_output=True,
    encoding="utf-8",
    check=True,
)
assert command.stdout == f"{text}\n", command
assert "site-packages" in anyseg.__file__, anyseg.__file__
# Importing the module must not turn the GIL back on in free-threaded Python
if sysconfig.get_config_var("Py_GIL_DISABLED"):
    assert not sys._is_gil_enabled()
if len(sys.argv) > 1:
    assert anyseg.__version__ == sys.argv[1], (anyseg.__version__, sys.argv[1])
print(f"anyseg {anyseg.__version__}: {anyseg.__file__}")

"""Check of the installed package before publishing: segmentation, positions, threads, pickle, a stream, a document
with other words and sentences, a part of a document, and the command

Run: `python scripts/smoke.py [version]`; from the `scripts` directory Python imports the installed
package, not the sources of the repository.
"""

import io
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
spanish = "¿Vienes del mar? Dámelo."
assert [span.text for span in sentenize(spanish, lang="es")] == ["¿Vienes del mar?", "Dámelo."]
words = [word.form for word in Segmenter("es").segment(spanish).words]
assert words == ["¿", "Vienes", "de", "el", "mar", "?", "Da", "me", "lo", "."], words
command = subprocess.run(
    [sys.executable, "-m", "anyseg", "-l", "es", "-f", "conllu"],
    input="Vengo del mar\n",
    capture_output=True,
    encoding="utf-8",
    check=True,
)
assert "2-3\tdel\t" in command.stdout, command
# A paragraph without blank lines longer than 256 KiB comes in pieces of whole sentences
long = "Кот спит. Ёж идёт.\n" * 20000
stream = Segmenter("ru").stream(io.BytesIO(long.encode()))
starts = [stream.start + start for doc in stream for start in doc.sentences.starts]
assert starts == Segmenter("ru").segment(long).sentences.starts, len(starts)
assert stream.continued
order = Segmenter("es").segment("Cómpraselo ya.")
words = [word.form for word in order.with_words({0: ["Compra", "se", "lo"]}).words]
assert words == ["Compra", "se", "lo", "ya", "."], words
assert [s.text for s in order.with_sentences([0, 1]).sentences] == ["Cómpraselo", "ya."]
# Positions of a part count in characters from its start, after a character outside the BMP
story = Segmenter("es").segment(f"{text}. Vengo del mar.")
sentence = story.sentences[1]
part = story.slice(sentence.start, sentence.end)
assert (part.text, part.tokens.starts) == ("Vengo del mar.", [0, 6, 10, 13]), part
assert [word.form for word in part.words] == ["Vengo", "de", "el", "mar", "."], part
assert "site-packages" in anyseg.__file__, anyseg.__file__
# Importing the module must not turn the GIL back on in free-threaded Python
if sysconfig.get_config_var("Py_GIL_DISABLED"):
    assert not sys._is_gil_enabled()
if len(sys.argv) > 1:
    assert anyseg.__version__ == sys.argv[1], (anyseg.__version__, sys.argv[1])
print(f"anyseg {anyseg.__version__}: {anyseg.__file__}")

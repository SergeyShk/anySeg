# The anyseg command

``` bash
anyseg --lang LANG [options] [file ...]
```

The command segments text from files or the standard input and writes its tokens, its sentences or [CoNLL-U](https://universaldependencies.org/format.html), for pipelines of [Universal Dependencies](https://universaldependencies.org/) tools. It comes with the package (`pip install anyseg`) and also runs as `python -m anyseg`; without Python, `cargo install anyseg --features cli` installs it.

``` bash
echo "Кот спит в 1914 г. Затем ушёл" | anyseg -l ru -f sentences
```

```
Кот спит в 1914 г.
Затем ушёл
```

## Options { #options }

| Option | Default | Description |
| :----- | :------ | :---------- |
| `-l`, `--lang` | - | ISO 639-1 code of the language of the text: `ru` |
| `-f`, `--format` | `tokens` | [Output format](#formats): `tokens`, `sentences`, `conllu` |
| `-o`, `--output` | the standard output | The result file; `-` is the standard output |
| `--abbreviations` | | A file of [more abbreviations](segmenter.md#lists), one per line; the option can be repeated |
| `--tokens` | | A file of [strings that are one token each](segmenter.md#lists), one per line; the option can be repeated |
| `-h`, `--help` | | Show the help |
| `-V`, `--version` | | Show the version |

Files are read one after another; without files and in place of `-` the standard input is read. The input is in UTF-8: a file in another encoding (cp1251, UTF-16) is not segmented, and a single line with wrong bytes is segmented with U+FFFD in their place. In the files of lists, spaces around an entry are ignored, and blank lines and lines starting with `#` are skipped. An option value is written separately or attached: `-f conllu`, `-fconllu`, `--format=conllu`.

The text is segmented paragraph by paragraph: a paragraph is written as soon as a blank line or the end of the input closes it, so the command can be kept open and fed text. The result is the same as for the whole text.

In Windows PowerShell 5.1 the `>` redirection and a pipeline (`Get-Content text.txt | anyseg -l ru`) corrupt Cyrillic: pass the file as an argument and write the result with the `-o` option (in PowerShell 7 they work too). Text passed with `echo` in cmd comes in the console encoding: run `chcp 65001` first.

## Output formats { #formats }

### tokens

A token per line and a blank line after every sentence.

``` bash
echo "Ёж идёт домой. Кот спит" | anyseg -l ru
```

```
Ёж
идёт
домой
.

Кот
спит

```

### sentences

A sentence per line, its line breaks turned into spaces, and a blank line between paragraphs.

### conllu

CoNLL-U as [`to_conllu`](document.md#to_conllu) writes it: every file starts with `# newdoc`, every paragraph with `# newpar`, and the sentences are numbered through the whole output; the space after a token goes to MISC.

``` bash
echo "Ёж идёт.  Кот" | anyseg -l ru -f conllu
```

```
# newdoc
# newpar
# sent_id = 1
# text = Ёж идёт.
1	Ёж	_	_	_	_	_	_	_	_
2	идёт	_	_	_	_	_	_	_	SpaceAfter=No
3	.	_	_	_	_	_	_	_	SpacesAfter=\s\s

# sent_id = 2
# text = Кот
1	Кот	_	_	_	_	_	_	_	_

```

## Exit codes { #exit-codes }

| Code | When |
| :--- | :--- |
| 0 | Success, or the reader of the output closed it (`anyseg ... \| head`) |
| 1 | A file does not open, is not in UTF-8 or has a line with wrong bytes, a list has a wrong entry, or the output cannot be written |
| 2 | Wrong options: no language, an unknown option or value |

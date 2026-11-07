# Comparison

anySeg against the segmenters of Russian and Spanish, rule-based and neural, on the test files of the treebanks of [Universal Dependencies](https://universaldependencies.org/) 2.18: SynTagRus, Taiga, GSD and PUD for [Russian](#russian), AnCora, GSD and PUD for [Spanish](#spanish).

## How it is measured

- The raw text of a treebank is built from the text of its sentences: documents and paragraphs are separated by blank lines, the sentences of a paragraph are joined by spaces, and a paragraph stays on one line (the `conllu_to_text.pl` script of UD also wraps lines at 80 characters).
- Tokens and sentences are scored with the evaluation script of the CoNLL 2018 shared task (F1). Sentence boundaries are scored as the positions of the sentence ends in the text (F1).
- The scores follow the conventions of the treebanks, which anySeg follows too: a tool with other conventions loses points for them.
- The rules of anySeg were chosen on the train and dev files; the test files are used only for this comparison.
- Speed is measured on one thread of an Apple M3 Pro, on the raw text of SynTagRus test (903,085 characters, 157,706 tokens of the treebank) and of AnCora test (278,779 characters, 52,454 tokens). Tokens per second count the tokens of the treebank, the same for every tool.

Neural tools are marked with *. For Russian, Stanza, UDPipe and trankit use models trained on SynTagRus. For Spanish, Stanza uses its model `combined` and UDPipe a model trained on AnCora of UD 2.5, where «del» and «al» were not yet split. trankit is not in the Spanish tables, as its model could not be downloaded.

## Russian

### Tokens

F1 of the CoNLL 2018 script.

| Tool | SynTagRus | Taiga | GSD | PUD |
| :-- | --: | --: | --: | --: |
| **anySeg 0.3** | 99.61 | **99.10** | **99.14** | **99.78** |
| razdel 0.5.0 | 98.73 | 95.74 | 92.64 | 99.26 |
| NLTK 3.10.4, `word_tokenize` | 98.93 | 96.57 | 94.45 | 99.60 |
| spaCy 3.8.16, `ru` | 98.38 | 96.85 | 94.66 | 98.46 |
| blingfire 0.1.8 | 98.04 | 92.87 | 90.65 | 97.99 |
| ICU 77.1, UAX #29 | 98.20 | 92.98 | 89.86 | 98.10 |
| Stanza 1.15.0 * | **99.75** | 95.55 | 91.59 | 99.51 |
| UDPipe 1.4 * | 99.67 | 93.99 | 92.50 | 97.33 |
| trankit 1.1.2 * | 99.74 | 95.54 | 93.10 | 99.62 |

The Rust crates unicode-segmentation 1.13.3 and icu_segmenter 2.4.0 give the same tokens as ICU.

### Sentence boundaries

F1 of the positions of the sentence ends.

| Tool | SynTagRus | Taiga | GSD | PUD |
| :-- | --: | --: | --: | --: |
| **anySeg 0.3** | 99.61 | **95.94** | 98.66 | 99.75 |
| razdel 0.5.0 | 99.41 | 85.58 | 96.94 | 99.65 |
| rusenttokenize 0.0.5 | 99.36 | 84.11 | 98.07 | 99.60 |
| NLTK 3.10.4, Punkt | 97.61 | 82.59 | 96.92 | 99.04 |
| spaCy 3.8.16, `ru` with `sentencizer` | 89.60 | 76.18 | 96.66 | 95.70 |
| spaCy, `ru_core_news_sm` with `senter` * | 97.69 | 79.54 | 96.83 | 97.92 |
| spaCy, `ru_core_news_sm` with the parser * | 98.32 | 82.07 | 94.25 | 98.27 |
| sentencex 1.0.32 | 66.56 | 77.89 | 16.94 | 83.57 |
| pysbd 0.3.4 | 86.50 | 89.40 | 86.93 | 99.15 |
| yasbd-lib 1.0.3 | 59.85 | 92.06 | 65.17 | 98.94 |
| sentsplit 1.0.8 | 96.98 | 93.74 | 90.58 | 98.32 |
| blingfire 0.1.8 | 98.69 | 81.53 | 96.92 | 99.04 |
| ICU 77.1, UAX #29 | 93.57 | 75.36 | 97.72 | 83.04 |
| Stanza 1.15.0 * | 99.51 | 95.71 | 98.49 | 99.75 |
| UDPipe 1.4 * | 99.47 | 94.97 | 98.15 | 99.60 |
| trankit 1.1.2 * | 99.55 | 94.79 | 98.67 | **99.90** |
| SaT 3-layer, wtpsplit 2.2.2 * | 99.28 | 94.93 | 99.00 | 99.80 |
| SaT 12-layer, wtpsplit 2.2.2 * | **99.68** | 95.86 | **99.67** | 99.85 |

### Sentences

F1 of the CoNLL 2018 script: a sentence counts when both of its ends match, so the tokens matter too. Only the tools that give tokens.

| Tool | SynTagRus | Taiga | GSD | PUD |
| :-- | --: | --: | --: | --: |
| **anySeg 0.3** | **98.92** | **89.92** | **96.32** | 99.25 |
| razdel 0.5.0 | 98.37 | 70.73 | 91.33 | 98.95 |
| NLTK 3.10.4 | 93.26 | 63.96 | 91.30 | 97.23 |
| spaCy 3.8.16, `ru` | 82.19 | 56.65 | 91.99 | 91.06 |
| blingfire 0.1.8 | 96.46 | 64.89 | 91.64 | 97.23 |
| ICU 77.1 | 86.10 | 78.55 | 94.01 | 98.60 |
| spaCy, `ru_core_news_sm` with `senter` * | 93.69 | 59.99 | 92.17 | 95.05 |
| spaCy, `ru_core_news_sm` with the parser * | 95.39 | 63.33 | 85.85 | 96.10 |
| Stanza 1.15.0 * | 98.61 | 88.69 | 95.48 | 99.25 |
| UDPipe 1.4 * | 98.45 | 86.97 | 94.97 | 98.80 |
| trankit 1.1.2 * | 98.73 | 86.42 | 96.17 | **99.70** |

### Speed and memory

From Python: the time to load the tool, tokens per second, and the memory the loaded tool takes (to a tenth of a megabyte below 10 MB). anySeg gives tokens and sentences with `tokenize` and `sentenize`; "sentences" marks the tools that give only sentences.

| Tool | Gives | Load, s | Tokens/s | Memory, MB |
| :-- | :-- | --: | --: | --: |
| **anySeg 0.3** | tokens, sentences | 0.00 | **3.18 M** | 0.7 |
| razdel 0.5.0 | tokens, sentences | 0.00 | 489 k | 0.1 |
| rusenttokenize 0.0.5 | sentences | 0.00 | 317 k | 0.2 |
| NLTK 3.10.4 | tokens, sentences | 0.85 | 195 k | 121 |
| spaCy 3.8.16, `ru` with `sentencizer` | tokens, sentences | 0.95 | 225 k | 198 |
| spaCy, `ru_core_news_sm` with `senter` * | tokens, sentences | 1.24 | 44 k | 442 |
| spaCy, `ru_core_news_sm` with the parser * | tokens, sentences | 1.15 | 30 k | 441 |
| sentencex 1.0.32 | sentences | 0.00 | 930 k | 0.5 |
| pysbd 0.3.4 | sentences | 0.00 | about 90, quadratic | - |
| yasbd-lib 1.0.3 | sentences | 0.03 | 185 k | 1.7 |
| sentsplit 1.0.8 | sentences | 0.07 | 47 k | 6.1 |
| blingfire 0.1.8 | tokens, sentences | 0.03 | 442 k | 4.7 |
| ICU 77.1 | tokens, sentences | 0.02 | 1.56 M | 4.9 |
| Stanza 1.15.0 * | tokens, sentences | 2.04 | 2.1 k | 267 |
| UDPipe 1.4 * | tokens, sentences | 1.89 | 17 k | 350 |
| trankit 1.1.2 * | tokens, sentences | 4.02 | 1.4 k | 1465 |
| SaT 3-layer * | sentences | 5.14 | 1.2 k | 835 |
| SaT 12-layer * | sentences | 4.77 | 0.3 k | 844 |

From Rust: tokens per second and the peak memory of a process that reads the text, segments it and writes the positions out.

| Crate | Gives | Tokens/s | Peak memory, MB |
| :-- | :-- | --: | --: |
| **anyseg 0.3** | tokens, sentences | 13.7 M | 35 |
| unicode-segmentation 1.13.3 | tokens, sentences | 7.7 M | 28 |
| icu_segmenter 2.4.0 | tokens, sentences | **17.6 M** | 114 |
| sentencex 0.1.32 | sentences | 965 k | 21 |

unicode-segmentation and icu_segmenter find the boundaries of Unicode (UAX #29) without the rules of a language: abbreviations, numbers, URLs and hyphenated words are not theirs.

## Spanish

### Tokens

F1 of the CoNLL 2018 script.

| Tool | AnCora | GSD | PUD |
| :-- | --: | --: | --: |
| **anySeg 0.3** | 99.93 | **99.95** | **99.78** |
| NLTK 3.10.4, `word_tokenize` | 99.80 | 99.78 | 99.62 |
| spaCy 3.8.16, `es` | 99.90 | 99.85 | 99.63 |
| syntok 1.4.4 | 99.50 | 99.62 | 99.63 |
| segtok 1.5.11 | 99.89 | 99.78 | 99.61 |
| sacremoses 0.2.0 | 99.87 | 99.81 | 99.62 |
| blingfire 0.1.8 | 99.05 | 99.07 | 99.58 |
| ICU 77.1, UAX #29 | 99.27 | 99.45 | 99.68 |
| Stanza 1.15.0 * | **99.99** | 99.86 | 99.75 |
| UDPipe 1.4 * | 99.92 | 99.75 | 99.58 |

The Rust crates unicode-segmentation 1.13.3 and icu_segmenter 2.4.0 give the same tokens as ICU.

### Words

F1 of the CoNLL 2018 script on the words of the treebank, where «del» is «de» and «el». A tool that does not build [multiword tokens](spanish.md#multiword) has one word per token and stays near 96.6.

| Tool | AnCora | GSD | PUD |
| :-- | --: | --: | --: |
| **anySeg 0.3** | 99.92 | **99.86** | **99.77** |
| NLTK 3.10.4, `word_tokenize` | 96.50 | 96.41 | 96.62 |
| spaCy 3.8.16, `es` | 96.60 | 96.47 | 96.60 |
| syntok 1.4.4 | 96.21 | 96.25 | 96.62 |
| segtok 1.5.11 | 96.59 | 96.41 | 96.58 |
| sacremoses 0.2.0 | 96.57 | 96.45 | 96.59 |
| blingfire 0.1.8 | 95.77 | 95.72 | 96.57 |
| ICU 77.1, UAX #29 | 95.99 | 96.10 | 96.65 |
| Stanza 1.15.0 * | **99.95** | 99.75 | 99.66 |
| UDPipe 1.4 * | 97.13 | 96.90 | 96.95 |

### Sentence boundaries

F1 of the positions of the sentence ends.

| Tool | AnCora | GSD | PUD |
| :-- | --: | --: | --: |
| **anySeg 0.3** | 99.54 | 96.74 | 99.60 |
| NLTK 3.10.4, Punkt | 98.58 | 96.51 | 99.10 |
| spaCy 3.8.16, `es` with `sentencizer` | 89.01 | 92.31 | 98.29 |
| spaCy, `es_core_news_sm` with `senter` * | 94.36 | 95.77 | 75.31 |
| spaCy, `es_core_news_sm` with the parser * | 99.04 | 95.80 | 98.39 |
| sentencex 1.0.32 | 94.07 | 84.74 | 82.85 |
| pysbd 0.3.4 | 97.23 | 85.07 | 98.90 |
| yasbd-lib 1.0.3 | 95.55 | 84.63 | 98.75 |
| sentence-splitter 1.4 | 97.27 | 96.12 | 98.40 |
| syntok 1.4.4 | 98.69 | 96.89 | 98.30 |
| segtok 1.5.11 | 98.81 | 96.29 | 98.41 |
| blingfire 0.1.8 | 98.87 | 96.24 | 98.74 |
| ICU 77.1, UAX #29 | 92.30 | 96.25 | 82.73 |
| Stanza 1.15.0 * | **99.59** | 97.37 | 99.40 |
| UDPipe 1.4 * | 99.42 | 96.02 | 99.15 |
| SaT 3-layer, wtpsplit 2.2.2 * | 99.42 | 99.29 | 99.80 |
| SaT 12-layer, wtpsplit 2.2.2 * | 99.36 | **99.65** | **99.85** |

GSD ends a sentence at a semicolon before a capital letter in all 272 cases of its train and dev files, and AnCora in none of 47. That is a difference of conventions, so anySeg has no rule for it, and GSD counts these against it.

### Sentences

F1 of the CoNLL 2018 script: a sentence counts when both of its ends match, so the tokens matter too. Only the tools that give tokens.

| Tool | AnCora | GSD | PUD |
| :-- | --: | --: | --: |
| **anySeg 0.3** | 98.66 | 90.97 | **98.80** |
| NLTK 3.10.4 | 96.15 | 90.04 | 97.59 |
| spaCy 3.8.16, `es` | 80.41 | 81.77 | 95.98 |
| syntok 1.4.4 | 96.65 | 91.41 | 95.60 |
| segtok 1.5.11 | 96.62 | 89.61 | 96.02 |
| blingfire 0.1.8 | 96.83 | 89.48 | 96.42 |
| ICU 77.1 | 92.78 | 89.26 | 97.45 |
| spaCy, `es_core_news_sm` with `senter` * | 83.52 | 88.30 | 38.66 |
| spaCy, `es_core_news_sm` with the parser * | 97.17 | 88.14 | 95.57 |
| Stanza 1.15.0 * | **98.78** | **93.08** | 98.20 |
| UDPipe 1.4 * | 98.32 | 88.81 | 97.45 |

### Speed and memory

From Python, as for Russian, on the raw text of AnCora test. `segment`, which gives the words too, comes to 5.6 M tokens per second with the same spans read and to 3.9 M with the forms of the words.

| Tool | Gives | Load, s | Tokens/s | Memory, MB |
| :-- | :-- | --: | --: | --: |
| **anySeg 0.3** | tokens, sentences | 0.00 | 4.36 M | 0.9 |
| NLTK 3.10.4 | tokens, sentences | 1.34 | 266 k | 121 |
| spaCy 3.8.16, `es` with `sentencizer` | tokens, sentences | 1.41 | 371 k | 195 |
| spaCy, `es_core_news_sm` with `senter` * | tokens, sentences | 1.22 | 42 k | 263 |
| spaCy, `es_core_news_sm` with the parser * | tokens, sentences | 0.89 | 34 k | 263 |
| sentencex 1.0.32 | sentences | 0.00 | **27.09 M** | 0.5 |
| pysbd 0.3.4 | sentences | 0.01 | 31 k | 0.7 |
| yasbd-lib 1.0.3 | sentences | 0.03 | 236 k | 2.5 |
| sentence-splitter 1.4 | sentences | 0.01 | 157 k | 1.0 |
| syntok 1.4.4 | tokens, sentences | 0.01 | 161 k | 1.0 |
| segtok 1.5.11 | tokens, sentences | 0.01 | 189 k | 1.0 |
| sacremoses 0.2.0 | tokens | 0.14 | 316 k | 17 |
| blingfire 0.1.8 | tokens, sentences | 0.04 | 616 k | 5.5 |
| ICU 77.1 | tokens, sentences | 0.02 | 1.92 M | 5.9 |
| Stanza 1.15.0 * | tokens, sentences | 2.13 | 13 k | 262 |
| UDPipe 1.4 * | tokens, sentences | 0.83 | 17 k | 176 |
| SaT 3-layer * | sentences | 6.22 | 1.4 k | 832 |
| SaT 12-layer * | sentences | 5.90 | 0.3 k | 834 |

From Rust, as for Russian; anyseg builds the words of the multiword tokens as it segments.

| Crate | Gives | Tokens/s | Peak memory, MB |
| :-- | :-- | --: | --: |
| **anyseg 0.3** | tokens, words, sentences | 13.7 M | 13 |
| unicode-segmentation 1.13.3 | tokens, sentences | 6.9 M | 8 |
| icu_segmenter 2.4.0 | tokens, sentences | 20.8 M | 14 |
| sentencex 0.1.32 | sentences | **41.3 M** | 7 |

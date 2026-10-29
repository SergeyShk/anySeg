# Comparison

anySeg against the segmenters of Russian, rule-based and neural, on the test files of the Russian treebanks of [Universal Dependencies](https://universaldependencies.org/) 2.18: SynTagRus, Taiga, GSD and PUD.

## How it is measured

- The raw text of a treebank is built as the tools of UD build it: documents and paragraphs are separated by blank lines, and the sentences of a paragraph are joined by spaces.
- Tokens and sentences are scored with the evaluation script of the CoNLL 2018 shared task (F1). Sentence boundaries are scored as the positions of the sentence ends in the text (F1).
- The scores follow the conventions of the treebanks, which anySeg follows too: a tool with other conventions loses points for them.
- The rules of anySeg were chosen on the train and dev files; the test files are used only for this comparison.
- Speed is measured on one thread of an Apple M3 Pro, on the raw text of SynTagRus test: 903,085 characters, 157,706 tokens of the treebank. Tokens per second count the tokens of the treebank, the same for every tool.

Neural tools are marked with *. Stanza, UDPipe and trankit use models trained on SynTagRus.

## Tokens

F1 of the CoNLL 2018 script.

| Tool | SynTagRus | Taiga | GSD | PUD |
| :-- | --: | --: | --: | --: |
| **anySeg 0.1** | 99.60 | **99.10** | **99.11** | **99.78** |
| razdel 0.5.0 | 98.73 | 95.74 | 92.64 | 99.26 |
| NLTK 3.10.4, `word_tokenize` | 98.93 | 96.57 | 94.45 | 99.60 |
| spaCy 3.8.16, `ru` | 98.38 | 96.85 | 94.66 | 98.46 |
| blingfire 0.1.8 | 98.04 | 92.87 | 90.65 | 97.99 |
| ICU 77.1, UAX #29 | 98.20 | 92.98 | 89.86 | 98.10 |
| Stanza 1.15.0 * | **99.75** | 95.55 | 91.59 | 99.51 |
| UDPipe 1.4 * | 99.67 | 93.99 | 92.50 | 97.33 |
| trankit 1.1.2 * | 99.74 | 95.54 | 93.10 | 99.62 |

The Rust crates unicode-segmentation 1.13.3 and icu_segmenter 2.4.0 give the same tokens as ICU.

## Sentence boundaries

F1 of the positions of the sentence ends.

| Tool | SynTagRus | Taiga | GSD | PUD |
| :-- | --: | --: | --: | --: |
| **anySeg 0.1** | 99.45 | **95.89** | 98.49 | 99.75 |
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

In SynTagRus, a number with a period at the start of a sentence (`5. Семя Аршака…`) is a sentence of its own more often than not in the train and dev files, and anySeg follows them; in the test file it never is, which costs anySeg 26 of its 41 false boundaries there.

## Sentences

F1 of the CoNLL 2018 script: a sentence counts when both of its ends match, so the tokens matter too. Only the tools that give tokens.

| Tool | SynTagRus | Taiga | GSD | PUD |
| :-- | --: | --: | --: | --: |
| **anySeg 0.1** | 98.51 | **89.78** | 95.81 | 99.25 |
| razdel 0.5.0 | 98.37 | 70.73 | 91.33 | 98.95 |
| NLTK 3.10.4 | 93.26 | 63.96 | 91.30 | 97.23 |
| spaCy 3.8.16, `ru` | 82.19 | 56.65 | 91.99 | 91.06 |
| blingfire 0.1.8 | 96.46 | 64.89 | 91.64 | 97.23 |
| ICU 77.1 | 86.10 | 78.55 | 94.01 | 98.60 |
| Stanza 1.15.0 * | 98.61 | 88.69 | 95.48 | 99.25 |
| UDPipe 1.4 * | 98.45 | 86.97 | 94.97 | 98.80 |
| trankit 1.1.2 * | **98.73** | 86.42 | **96.17** | **99.70** |

## Speed and memory

From Python: the time to load the tool, tokens per second, and the memory the loaded tool takes. anySeg gives tokens and sentences with `tokenize` and `sentenize`; "sentences" marks the tools that give only sentences.

| Tool | Gives | Load, s | Tokens/s | Memory, MB |
| :-- | :-- | --: | --: | --: |
| **anySeg 0.1** | tokens, sentences | 0.00 | **2.42 M** | 0.5 |
| razdel 0.5.0 | tokens, sentences | 0.00 | 489 k | 0 |
| rusenttokenize 0.0.5 | sentences | 0.00 | 317 k | 0 |
| NLTK 3.10.4 | tokens, sentences | 0.85 | 195 k | 121 |
| spaCy 3.8.16, `ru` with `sentencizer` | tokens, sentences | 0.95 | 225 k | 198 |
| spaCy, `ru_core_news_sm` with `senter` * | tokens, sentences | 1.24 | 44 k | 442 |
| spaCy, `ru_core_news_sm` with the parser * | tokens, sentences | 1.15 | 30 k | 441 |
| sentencex 1.0.32 | sentences | 0.00 | 930 k | 0 |
| pysbd 0.3.4 | sentences | 0.00 | about 90, quadratic | 0 |
| yasbd-lib 1.0.3 | sentences | 0.03 | 185 k | 2 |
| sentsplit 1.0.8 | sentences | 0.07 | 47 k | 6 |
| blingfire 0.1.8 | tokens, sentences | 0.03 | 442 k | 5 |
| ICU 77.1 | tokens, sentences | 0.02 | 1.56 M | 5 |
| Stanza 1.15.0 * | tokens, sentences | 2.04 | 2.1 k | 267 |
| UDPipe 1.4 * | tokens, sentences | 1.89 | 17 k | 350 |
| trankit 1.1.2 * | tokens, sentences | 4.02 | 1.4 k | 1465 |
| SaT 3-layer * | sentences | 5.14 | 1.2 k | 835 |
| SaT 12-layer * | sentences | 4.77 | 0.3 k | 844 |

From Rust: tokens per second and the peak memory of a process that reads the text, segments it and writes the positions out.

| Crate | Gives | Tokens/s | Peak memory, MB |
| :-- | :-- | --: | --: |
| **anyseg 0.1** | tokens, sentences | 13.5 M | 34 |
| unicode-segmentation 1.13.3 | tokens, sentences | 7.7 M | 28 |
| icu_segmenter 2.4.0 | tokens, sentences | **17.6 M** | 114 |
| sentencex 0.1.32 | sentences | 965 k | 21 |

unicode-segmentation and icu_segmenter find the boundaries of Unicode (UAX #29) without the rules of a language: abbreviations, numbers, URLs and hyphenated words are not theirs.

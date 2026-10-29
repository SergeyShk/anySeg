# Russian

`Segmenter("ru")` splits Russian text by the [Universal Dependencies](https://universaldependencies.org/) guidelines and, where they are silent, by the conventions of the largest Russian treebank, [SynTagRus](https://github.com/UniversalDependencies/UD_Russian-SynTagRus). Where the treebanks disagree, the rule follows what most of them do.

## Tokens { #tokens }

### Words

A word keeps its stress marks and soft hyphens, and a word with an apostrophe is one token.

=== "Code"

    ``` python
    from anyseg import Segmenter

    segmenter = Segmenter("ru")
    [span.text for span in segmenter.tokenize("Ку́рица, До­ма, О'Нил, д’Артаньян")]
    ```

=== "Result"

    ```
    ['Ку́рица', ',', 'До\xadма', ',', "О'Нил", ',', 'д’Артаньян']
    ```

### Abbreviations and initials { #abbreviations }

A common abbreviation keeps its period: «г.», «т.», «е.», «тыс.», «руб.», «им.»; so does a single letter - an initial «А.» or a lowercase letter of an abbreviation, except the word «я». At the end of a sentence, the period of an abbreviation is a token of its own: «т. д.» at the end gives «т.», «д», «.». More abbreviations can be [added to the segmenter](segmenter.md#lists).

=== "Code"

    ``` python
    [span.text for span in segmenter.tokenize("В 1914 г. в Москве, т. е. в 2 тыс. км от А.С. Пушкина")]
    [span.text for span in segmenter.tokenize("Купили груши и т. д.")]
    [span.text for span in segmenter.tokenize("Это я.")]
    ```

=== "Result"

    ```
    ['В', '1914', 'г.', 'в', 'Москве', ',', 'т.', 'е.', 'в', '2', 'тыс.', 'км', 'от', 'А.', 'С.', 'Пушкина']
    ['Купили', 'груши', 'и', 'т.', 'д', '.']
    ['Это', 'я', '.']
    ```

### Hyphenated words { #hyphens }

A hyphenated word is one token: «кто-то», «кое-как», «мини-сауна», «вице-губернатор», «еле-еле», «Ту-154». It splits at the hyphen in these cases:

*   a particle after a hyphen - «-то», «-ка», «-таки», «-с», «-де»: «Вася-то», «скажи-ка»; indefinite pronouns and adverbs keep their «-то» («кто-то», «где-то»), and set phrases stay whole («всё-таки»);
*   a range or an approximate number: «1941-1945», «XIX-XX», «2,5-3,5», «два-три»;
*   a compound whose first part inflects, as the UD guidelines ask, where the segmenter can see it without a dictionary: two parts that end alike, as inflected forms do («учеными-энтузиастами», «человека-паука»), a word and a number («Формулы-1»), a name with «-на-» («Ростова-на-Дону»). Other compounds stay whole: «женщины-врача».

=== "Code"

    ``` python
    [span.text for span in segmenter.tokenize("кто-то, кое-как, мини-сауна, еле-еле, Ту-154")]
    [span.text for span in segmenter.tokenize("Вася-то, скажи-ка, всё-таки")]
    [span.text for span in segmenter.tokenize("1941-1945, XIX-XX, два-три, 2,5-3,5")]
    [span.text for span in segmenter.tokenize("Ростова-на-Дону, учеными-энтузиастами, Формулы-1")]
    ```

=== "Result"

    ```
    ['кто-то', ',', 'кое-как', ',', 'мини-сауна', ',', 'еле-еле', ',', 'Ту-154']
    ['Вася', '-', 'то', ',', 'скажи', '-', 'ка', ',', 'всё-таки']
    ['1941', '-', '1945', ',', 'XIX', '-', 'XX', ',', 'два', '-', 'три', ',', '2,5', '-', '3,5']
    ['Ростова', '-', 'на', '-', 'Дону', ',', 'учеными', '-', 'энтузиастами', ',', 'Формулы', '-', '1']
    ```

### Numbers

Decimal numbers, numbers with groups of digits separated by spaces, dates with dots, times, and three or more groups of digits joined by hyphens, as in phone numbers, are one token each. A phone number written with spaces or brackets and a date with slashes split.

=== "Code"

    ``` python
    text = "4,5 и 1.5, 3 000 руб., 12.10.2026, 12:30, +7-495-123-45-67, 15%"
    [span.text for span in segmenter.tokenize(text)]
    ```

=== "Result"

    ```
    ['4,5', 'и', '1.5', ',', '3 000', 'руб.', ',', '12.10.2026', ',', '12:30', ',', '+7-495-123-45-67', ',', '15', '%']
    ```

### The web

URLs, e-mail addresses, domains and file names, mentions and hashtags are one token each. The punctuation that ends a sentence and a closing bracket that a URL did not open stay out of it.

=== "Code"

    ``` python
    text = "Пишите на info@example.ru, сайт https://example.ru/news?id=5, файл IMG_1234.jpg, @anna и #шопинг"
    [span.text for span in segmenter.tokenize(text)]
    ```

=== "Result"

    ```
    ['Пишите', 'на', 'info@example.ru', ',', 'сайт', 'https://example.ru/news?id=5', ',', 'файл', 'IMG_1234.jpg', ',', '@anna', 'и', '#шопинг']
    ```

### Marks and symbols

A run of final marks is one token: «?!», «...», «!!!». So are an emoticon, a run of emoji with their modifiers, «°C» and a quote written as an HTML entity.

=== "Code"

    ``` python
    [span.text for span in segmenter.tokenize("Ура!!! Что?! Ну... 20°C :) ))) 👍🏻 ❤️ &quot;кот&quot;")]
    ```

=== "Result"

    ```
    ['Ура', '!!!', 'Что', '?!', 'Ну', '...', '20', '°C', ':)', ')))', '👍🏻', '❤️', '&quot;', 'кот', '&quot;']
    ```

## Sentences { #sentences }

A paragraph always starts a sentence. Inside it, a sentence ends at final marks - «.», «!», «?», «…» and their runs, with the closing quotes and brackets right after them - when a capital letter, a digit, an opening quote or bracket before a capital letter, or a dash before a capital letter follows - but not a source in brackets after a quotation: «…!» (Пушкин). A lowercase letter goes on with the sentence: the words of the author after a line of dialogue («— спросил он») and a thought after an ellipsis. A period without a space before a capital letter ends a sentence too.

=== "Code"

    ``` python
    text = "Кот спит. Пёс лает! «Куда?» — спросил он. — Домой! — Он ушёл."
    [span.text for span in segmenter.sentenize(text)]
    [span.text for span in segmenter.sentenize("Ну и ну… и что? Тишина… Потом шум.")]
    [span.text for span in segmenter.sentenize("Он пришел.Она ушла. В 10.30.Начали.")]
    ```

=== "Result"

    ```
    ['Кот спит.', 'Пёс лает!', '«Куда?» — спросил он.', '— Домой!', '— Он ушёл.']
    ['Ну и ну… и что?', 'Тишина…', 'Потом шум.']
    ['Он пришел.', 'Она ушла.', 'В 10.30.', 'Начали.']
    ```

### Abbreviations { #sentence-abbreviations }

An abbreviation ends a sentence before a capital letter when the language lists it as one that usually stands at the end - «т. д.», «т. п.», «др.», «и пр.», «проч.», «н. э.», «тел.» - or when it follows a number: «в 1914 г.», «15 000 р.», «60 км/ч.». Other abbreviations and initials do not end a sentence: «г. Москва», «А. С. Пушкин».

=== "Code"

    ``` python
    text = "Купили груши и т. д. Потом ушли. Это было в г. Москва. В 1914 г. Затем всё. Пушкин А. С. Лермонтов"
    [span.text for span in segmenter.sentenize(text)]
    [span.text for span in segmenter.sentenize("Цена 15 000 р. Пишите. Едем 60 км/ч. Всё.")]
    ```

=== "Result"

    ```
    ['Купили груши и т. д.', 'Потом ушли.', 'Это было в г. Москва.', 'В 1914 г.', 'Затем всё.', 'Пушкин А. С. Лермонтов']
    ['Цена 15 000 р.', 'Пишите.', 'Едем 60 км/ч.', 'Всё.']
    ```

### Posts and messages

An emoticon or an emoji after final marks stays in the sentence; without them, it ends a sentence before a capital letter, as a period would. A mention after final marks or an emoticon starts a sentence. A separator of stars - «* * *», «***» - is a sentence of its own on a line of its own, or after final marks before a capital letter.

=== "Code"

    ``` python
    text = "Привет 😀 Как дела? Ура! :) Пойдём. Спасибо! @anna да."
    [span.text for span in segmenter.sentenize(text)]
    [span.text for span in segmenter.sentenize("Текст. * * * Новая глава.")]
    ```

=== "Result"

    ```
    ['Привет 😀', 'Как дела?', 'Ура! :)', 'Пойдём.', 'Спасибо!', '@anna да.']
    ['Текст.', '* * *', 'Новая глава.']
    ```

### Numbered items

A number with a period at the start of a paragraph or a line marks a list item and stays in its sentence. Inside a paragraph, after the end of a sentence, such a number before a capital letter is a sentence of its own, as the treebanks most often mark it.

=== "Code"

    ``` python
    [span.text for span in segmenter.sentenize("1. Введение. Текст. 2. Обзор")]
    ```

=== "Result"

    ```
    ['1. Введение.', 'Текст.', '2.', 'Обзор']
    ```

## Paragraphs and lines { #lines }

A paragraph is the part of the text between blank lines (or the paragraph separator U+2029). Inside a paragraph, a line break is a space, except where it starts a sentence:

*   before a list item - a bullet, an emoji, «1)», «1.», «а)» - or a line of dialogue;
*   at a line without words: «***», «-»;
*   after a line without final punctuation before a line with a capital letter: a heading, a line of a post. In a paragraph where some line ends with a comma or a semicolon before a line with a capital letter, as in verse, or some line goes on with a lowercase letter, as in text wrapped at the width of the page, this rule is silent for all its lines.

=== "Code"

    ``` python
    [span.text for span in segmenter.sentenize("Глава 1\nВведение\nЭто текст главы. Он короткий.")]
    [span.text for span in segmenter.sentenize("Нужно взять:\n- паспорт\n- СНИЛС\n1) полис\n✅ фото")]
    verse = "Скажи-ка, дядя, ведь не даром\nМосква, спалённая пожаром,\nФранцузу отдана?\nВедь были ж схватки боевые"
    [span.text for span in segmenter.sentenize(verse)]
    ```

=== "Result"

    ```
    ['Глава 1', 'Введение', 'Это текст главы.', 'Он короткий.']
    ['Нужно взять:', '- паспорт', '- СНИЛС', '1) полис', '✅ фото']
    ['Скажи-ка, дядя, ведь не даром\nМосква, спалённая пожаром,\nФранцузу отдана?', 'Ведь были ж схватки боевые']
    ```

Every line of a [document](document.md#line) tells how it begins: `"author"` for a break the author made - a verse line, a list item, a paragraph per line - and `"typeset"` for a wrap at the width of the page, as in text extracted from PDF: a line that goes on with a lowercase letter outside verse and does not start a sentence.

=== "Code"

    ``` python
    pdf = "Компания увеличила при-\nбыль на 15 %. Это\nрекорд."
    [span.text for span in segmenter.sentenize(pdf)]
    [line.break_before for line in segmenter.segment(pdf).lines]
    ```

=== "Result"

    ```
    ['Компания увеличила при-\nбыль на 15 %.', 'Это\nрекорд.']
    [None, 'typeset', 'typeset']
    ```

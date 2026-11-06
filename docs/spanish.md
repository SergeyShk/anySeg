# Spanish

`Segmenter("es")` splits Spanish text by the [Universal Dependencies](https://universaldependencies.org/) guidelines and, where they are silent, by the conventions of the largest Spanish treebank, [AnCora](https://github.com/UniversalDependencies/UD_Spanish-AnCora). Where the treebanks disagree, the rule follows AnCora.

## Tokens { #tokens }

### Words

A word keeps its accents and soft hyphens, and a word with an apostrophe or a similar mark inside is one token.

=== "Code"

    ``` python
    from anyseg import Segmenter

    segmenter = Segmenter("es")
    [span.text for span in segmenter.tokenize("Cañón, pa­labra, O'Neal, O`Neal, d’Ebre, d´Ebre")]
    ```

=== "Result"

    ```
    ['Cañón', ',', 'pa\xadlabra', ',', "O'Neal", ',', 'O`Neal', ',', 'd’Ebre', ',', 'd´Ebre']
    ```

### Abbreviations and initials { #abbreviations }

A common abbreviation keeps its period: «Sr.», «Dra.», «Ud.», «pág.», «p. ej.», «aprox.», «ene.»; so does a single letter - an initial «J.» or a lowercase letter of an abbreviation, except the words «y», «e», «o», «u». An abbreviation written without spaces is one token: «EE.UU.», «S.A.». At the end of a sentence, the period of an abbreviation is a token of its own.

Some forms keep the period only where the text after them shows they are abbreviations:

*   a word that is also a common word or a name, before a digit: «No. 45», «Nos. 5 y 6», «arts. 5 y 6», but «No. Se fue», «Electronic Arts. Luego»;
*   a word in capitals, before a digit or another word in capitals: «ART. 5», «EL SR. GARCÍA», but «Es del PP. Luego». «EE. UU.», the commonest titles - «DR. Juan Pérez» - and the forms «Col.» and «Vid.» keep it anywhere.

=== "Code"

    ``` python
    text = "El Sr. Pérez, p. ej., pág. 7, aprox. 5, la Dra. J. Ruiz en EE.UU. y S.A. hoy"
    [span.text for span in segmenter.tokenize(text)]
    [span.text for span in segmenter.tokenize("Compró peras, etc.")]
    [span.text for span in segmenter.tokenize("La Ley No. 45. No. Se fue.")]
    [span.text for span in segmenter.tokenize("ART. 5 y DR. Juan Pérez. Es del PP. Luego")]
    ```

=== "Result"

    ```
    ['El', 'Sr.', 'Pérez', ',', 'p.', 'ej.', ',', 'pág.', '7', ',', 'aprox.', '5', ',', 'la', 'Dra.', 'J.', 'Ruiz', 'en', 'EE.UU.', 'y', 'S.A.', 'hoy']
    ['Compró', 'peras', ',', 'etc', '.']
    ['La', 'Ley', 'No.', '45', '.', 'No', '.', 'Se', 'fue', '.']
    ['ART.', '5', 'y', 'DR.', 'Juan', 'Pérez', '.', 'Es', 'del', 'PP', '.', 'Luego']
    ```

### Multiword tokens { #multiword }

A token can hold several words, as the UD guidelines ask for the words that Spanish writes together. The tokens stay those of the text, and a [document](document.md#word) gives their words:

*   the contractions «del» and «al» are «de» + «el» and «a» + «el»;
*   an infinitive or a gerund with pronouns is the verb and the pronouns, without the accent that the pronouns brought: «darlo», «dárselo», «viéndolo». Words that only look so stay whole: «parte», «muerte», «gobiernos». So does a verb with «os», as «-eros» and «-aros» end nouns far more often: «deciros»;
*   frequent imperatives with pronouns are in a table: «dame», «dímelo», «hazlo», «véase». Other imperatives need a dictionary and stay one word: «cómpralo», «cálmate».

Inside a sentence, a form with a first capital letter is a name and stays whole: «El Al». A string of the [list of tokens](segmenter.md#lists) stays one word.

=== "Code"

    ``` python
    text = "Vengo del mar y voy al sol. Quiero dárselo, viéndolo bien. Dímelo, cómpralo, es parte de El Al."
    doc = segmenter.segment(text)
    [(token.text, [word.form for word in token.words]) for token in doc.multiword_tokens]
    print(segmenter.segment("Vengo del mar").to_conllu(), end="")
    ```

=== "Result"

    ```
    [('del', ['de', 'el']), ('al', ['a', 'el']), ('dárselo', ['dar', 'se', 'lo']), ('viéndolo', ['viendo', 'lo']), ('Dímelo', ['Di', 'me', 'lo'])]
    # newdoc
    # newpar
    # sent_id = 1
    # text = Vengo del mar
    1	Vengo	_	_	_	_	_	_	_	_
    2-3	del	_	_	_	_	_	_	_	_
    2	de	_	_	_	_	_	_	_	_
    3	el	_	_	_	_	_	_	_	_
    4	mar	_	_	_	_	_	_	_	_
    ```

### Hyphens and marks inside a token { #hyphens }

A hyphenated word is one token, and so is a range: «franco-alemán», «sub-23», «1990-1995», «XIX-XX», «2,5-3,5»; between digits, an en dash joins them too: «1990–1995». A slash and a plus between letters or digits stay inside a token too: «km/h», «y/o», «I+D», «15/01/2011», «30/1.992». With a space beside them, these marks are tokens of their own. A superscript digit is always a token of its own: «km²».

=== "Code"

    ``` python
    text = "franco-alemán, sub-23, 1990-1995, 1990–1995, XIX-XX, 2,5-3,5"
    [span.text for span in segmenter.tokenize(text)]
    [span.text for span in segmenter.tokenize("km/h, y/o, I+D, 15/01/2011, 30/1.992, a / b")]
    [span.text for span in segmenter.tokenize("10 km², 5 m/s²")]
    ```

=== "Result"

    ```
    ['franco-alemán', ',', 'sub-23', ',', '1990-1995', ',', '1990–1995', ',', 'XIX-XX', ',', '2,5-3,5']
    ['km/h', ',', 'y/o', ',', 'I+D', ',', '15/01/2011', ',', '30/1.992', ',', 'a', '/', 'b']
    ['10', 'km', '²', ',', '5', 'm/s', '²']
    ```

### Numbers

Decimal numbers with a comma, groups of three digits with points, times, three or more groups of digits joined by hyphens, as in phone numbers, and ordinal numbers with their ending are one token each. A minus before a digit is in the number after a space or an opening mark. Digits separated by spaces and a percent sign are tokens of their own.

=== "Code"

    ``` python
    text = "20.000, 1.500,50, 0,4, 12:30, +34-91-123-45-67, 3.º, 2.ª, 1.er, de -5 grados, 3 000, 15%"
    [span.text for span in segmenter.tokenize(text)]
    ```

=== "Result"

    ```
    ['20.000', ',', '1.500,50', ',', '0,4', ',', '12:30', ',', '+34-91-123-45-67', ',', '3.º', ',', '2.ª', ',', '1.er', ',', 'de', '-5', 'grados', ',', '3', '000', ',', '15', '%']
    ```

### The web

URLs, e-mail addresses, domains and file names, mentions and hashtags are one token each. The punctuation that ends a sentence stays out of them. A top-level domain that is a Spanish word starts a sentence after a missing space when it has a first capital letter: «casa.De pronto», but «Amazon.Com».

=== "Code"

    ``` python
    text = "Escribe a info@ejemplo.es, en https://ejemplo.es/noticias?id=5, el archivo datos.csv, @ana y #fútbol."
    [span.text for span in segmenter.tokenize(text)]
    [span.text for span in segmenter.tokenize("En casa.De pronto, Amazon.Com")]
    ```

=== "Result"

    ```
    ['Escribe', 'a', 'info@ejemplo.es', ',', 'en', 'https://ejemplo.es/noticias?id=5', ',', 'el', 'archivo', 'datos.csv', ',', '@ana', 'y', '#fútbol', '.']
    ['En', 'casa', '.', 'De', 'pronto', ',', 'Amazon.Com']
    ```

### Marks and symbols

A run of one final mark is one token: «...», «!!»; different final marks are apart: «?», «!». «¿» and «¡» are tokens one by one. An emoticon, a run of emoji with their modifiers and «°C» are one token each.

=== "Code"

    ``` python
    [span.text for span in segmenter.tokenize("¡¡Hola!! ¿Qué?! Bueno... 20°C :) 👍🏻 ❤️")]
    ```

=== "Result"

    ```
    ['¡', '¡', 'Hola', '!!', '¿', 'Qué', '?', '!', 'Bueno', '...', '20', '°C', ':)', '👍🏻', '❤️']
    ```

## Sentences { #sentences }

A paragraph always starts a sentence. Inside it, a sentence ends at final marks - «.», «!», «?», «…» and their runs, with the closing quotes and brackets right after them - when a capital letter, a digit or an inverted mark follows, maybe after an opening quote, a bracket or a dash. «¿» and «¡» start a sentence before a lowercase letter too, unless an ellipsis comes before them or they are glued to the mark. Any other lowercase letter goes on with the sentence. A period without a space before a capital letter ends a sentence too.

=== "Code"

    ``` python
    text = '¿Qué hora es? ¡Claro! ¡qué bien! Se fue. ¿por qué? Se fue. "¿por qué?"'
    [span.text for span in segmenter.sentenize(text)]
    [span.text for span in segmenter.sentenize("Bueno... ¿qué dices? Tal vez… Luego.")]
    [span.text for span in segmenter.sentenize("Vino a casa.Es tarde.")]
    ```

=== "Result"

    ```
    ['¿Qué hora es?', '¡Claro!', '¡qué bien!', 'Se fue.', '¿por qué?', 'Se fue.', '"¿por qué?"']
    ['Bueno... ¿qué dices?', 'Tal vez…', 'Luego.']
    ['Vino a casa.', 'Es tarde.']
    ```

### Dialogue and articles { #dialogue }

A dash starts a line of dialogue before a capital letter, a digit or an inverted mark; before a lowercase letter it brings the words of the author, which go on with the line. Glued to a final mark with a space after it, a dash longer than a hyphen closes its sentence, and a hyphen after a period numbers an article, which goes on after it.

=== "Code"

    ``` python
    [span.text for span in segmenter.sentenize("—¿Vienes? —preguntó ella. —Sí. —Y se fue.")]
    [span.text for span in segmenter.sentenize("¿Cuánto? —5 euros. La viuda.-- El niño lloró.")]
    text = "Artículo 1.- Objeto de la ley. Artículo 2.- Ámbito."
    [span.text for span in segmenter.sentenize(text)]
    ```

=== "Result"

    ```
    ['—¿Vienes? —preguntó ella.', '—Sí.', '—Y se fue.']
    ['¿Cuánto?', '—5 euros.', 'La viuda.--', 'El niño lloró.']
    ['Artículo 1.- Objeto de la ley.', 'Artículo 2.- Ámbito.']
    ```

### Abbreviations { #sentence-abbreviations }

An abbreviation ends a sentence before a capital letter when the language lists it as one that usually stands at the end - «etc.», «a. C.», «d. C.», «a. m.», «p. m.», «EE. UU.», «et al.», «y ss.» - or when it is in lowercase and follows a number, as a unit does: «Mide 5 cm.», but not one of an address, a title or an edition: «Calle 15 esq. Centro», «3.ª ed. Madrid». Other abbreviations and initials do not end a sentence: «el Sr. Pérez», «J. L. Borges», «A.M. Turing». A single capital letter inside a sentence does before a word that seldom follows an initial: «Felipe V. El rey», «vitamina C. Es buena»; such words as «El» and «Su» cost a name now and then: «M. El Baradei».

=== "Code"

    ``` python
    text = "Compró peras, etc. Luego se fue. Murió en 44 a. C. Vive en EE. UU. Trabaja. Mide 5 cm. Pesa 3 kg."
    [span.text for span in segmenter.sentenize(text)]
    text = "Vino el Sr. Pérez. Calle 15 esq. Centro. Vino J. L. Borges."
    [span.text for span in segmenter.sentenize(text)]
    text = "Fue Felipe V. El rey vino. Tomo vitamina C. Es buena."
    [span.text for span in segmenter.sentenize(text)]
    ```

=== "Result"

    ```
    ['Compró peras, etc.', 'Luego se fue.', 'Murió en 44 a. C.', 'Vive en EE. UU.', 'Trabaja.', 'Mide 5 cm.', 'Pesa 3 kg.']
    ['Vino el Sr. Pérez.', 'Calle 15 esq. Centro.', 'Vino J. L. Borges.']
    ['Fue Felipe V.', 'El rey vino.', 'Tomo vitamina C.', 'Es buena.']
    ```

### Posts and messages

An emoticon or an emoji after final marks stays in the sentence; without them, it ends a sentence before a capital letter or an inverted mark. A mention after final marks or an emoticon starts a sentence, and so does emphasis in Markdown before a capital letter. A separator of stars is a sentence of its own on a line of its own, or after final marks before a capital letter.

=== "Code"

    ``` python
    text = "Hola 😀 ¿Qué tal? ¡Bien! :) Vamos. Gracias! @ana sí."
    [span.text for span in segmenter.sentenize(text)]
    [span.text for span in segmenter.sentenize("Texto. * * * Nuevo capítulo.")]
    ```

=== "Result"

    ```
    ['Hola 😀', '¿Qué tal?', '¡Bien! :)', 'Vamos.', 'Gracias!', '@ana sí.']
    ['Texto.', '* * *', 'Nuevo capítulo.']
    ```

### Numbered items

A number with a period at the start of a paragraph or a line marks a list item and stays in its sentence, as the UD guidelines ask. So does a number of up to three digits inside a line before a capital letter: at the start of a sentence, when the sentence before or after it has the number next to it, and after a colon, when the sentence after it starts with the next number. Any other number with a period after the end of a sentence, before a capital letter, is a sentence of its own: a heading, a year, an answer.

=== "Code"

    ``` python
    [span.text for span in segmenter.sentenize("Plan. 1. Llegar. 2. Salir.")]
    [span.text for span in segmenter.sentenize("1. Introducción. Texto. 2. Método")]
    ```

=== "Result"

    ```
    ['Plan.', '1. Llegar.', '2. Salir.']
    ['1. Introducción.', 'Texto.', '2.', 'Método']
    ```

## Paragraphs and lines { #lines }

A paragraph is the part of the text between blank lines (or the paragraph separator U+2029). Inside a paragraph, a line break is a space, except where it starts a sentence:

*   before a list item - a bullet, a number with a minus, an emoji, «1)», «1.», «a)» - or a line of dialogue;
*   at a line without words: «***», «-»;
*   after a line without final punctuation before a line with a capital letter: a heading, a line of a post. This rule is silent in text wrapped at the width of the page, where some line goes on with a lowercase letter, and in verse, except after a line without lowercase letters in a paragraph that has them, which is a heading. A paragraph reads as verse when some line ends with a comma or a semicolon before a line with a capital letter, or when its lines rhyme. Verse without rhymes and such marks splits by lines, as a post does.

=== "Code"

    ``` python
    text = "Capítulo 1\nIntroducción\nEste es el texto. Es corto."
    [span.text for span in segmenter.sentenize(text)]
    [span.text for span in segmenter.sentenize("Compra:\n- pan\n-2 kilos\n1) leche\n✅ fruta")]
    verse = "Volverán las oscuras golondrinas\nen tu balcón sus nidos a colgar,\nY otra vez con el ala a sus cristales\njugando llamarán."
    [span.text for span in segmenter.sentenize(verse)]
    ```

=== "Result"

    ```
    ['Capítulo 1', 'Introducción', 'Este es el texto.', 'Es corto.']
    ['Compra:', '- pan', '-2 kilos', '1) leche', '✅ fruta']
    ['Volverán las oscuras golondrinas\nen tu balcón sus nidos a colgar,\nY otra vez con el ala a sus cristales\njugando llamarán.']
    ```

Every line of a [document](document.md#line) tells how it begins: `"author"` for a break the author made - a verse line, a list item, a paragraph per line - and `"typeset"` for a wrap at the width of the page, as in text extracted from PDF:

=== "Code"

    ``` python
    pdf = "La empresa aumentó sus benefi-\ncios un 15 %. Es\nun récord."
    [span.text for span in segmenter.sentenize(pdf)]
    [line.break_before for line in segmenter.segment(pdf).lines]
    ```

=== "Result"

    ```
    ['La empresa aumentó sus benefi-\ncios un 15 %.', 'Es\nun récord.']
    [None, 'typeset', 'typeset']
    ```


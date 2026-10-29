//! Sentences: the boundaries inside a paragraph, from its tokens, its lines and the lists of a language

use crate::Span;
use crate::document::{Line, LineBreak};
use crate::lang::Tables;
use crate::tokens::{EMOTICONS, Exceptions, is_emoji, is_mark};

/// Marks that start a list item at the beginning of a line
const BULLETS: &str = "-‐‒–—―−•·*▪◦‣●■►▸→✓✔";

/// A token of final marks: «.», «?!», «...», «…»
fn is_final(piece: &str) -> bool {
    piece
        .chars()
        .all(|character| matches!(character, '.' | '!' | '?' | '…'))
}

/// A quote written as an HTML entity: «&quot;», «&#39;&#39;»
fn is_entity(piece: &str) -> bool {
    piece.len() > 2 && piece.starts_with('&') && piece.ends_with(';')
}

/// A closing quote or bracket, which stays with the final mark before it
fn is_closing(piece: &str) -> bool {
    matches!(
        piece,
        "»" | "”" | "“" | "\"" | "'" | "''" | ")" | "]" | "}" | "’" | "›"
    ) || is_entity(piece)
}

fn is_opening(piece: &str) -> bool {
    matches!(
        piece,
        "«" | "„" | "“" | "\"" | "'" | "``" | "(" | "[" | "{" | "‘" | "‹"
    ) || is_entity(piece)
}

fn is_dash(piece: &str) -> bool {
    piece
        .chars()
        .all(|character| matches!(character, '-' | '‐' | '‒' | '–' | '—' | '―' | '−'))
}

/// An emoji or an emoticon: «😀», «:)», «)))»; decided by the first byte, as it runs for every token
fn is_emoticon(piece: &str) -> bool {
    let bytes = piece.as_bytes();
    match bytes.first() {
        Some(b':' | b';' | b'=') => bytes.len() > 1,
        Some(&bracket @ (b')' | b'(')) => bytes.len() > 1 && bytes.iter().all(|&b| b == bracket),
        // The first bytes of «^_^», «-_-», «<3», «o_O» and «о_О»
        Some(b'^' | b'-' | b'<' | b'o' | b'O' | 0xD0) => EMOTICONS.contains(&piece),
        Some(0xE2 | 0xF0) => piece.chars().next().is_some_and(is_emoji),
        _ => false,
    }
}

fn is_stars(piece: &str) -> bool {
    piece.len() >= 3 && piece.bytes().all(|byte| byte == b'*')
}

/// A mention, which starts a sentence after final marks or an emoticon: «Спасибо! @anna да»
fn is_mention(piece: &str) -> bool {
    piece.len() > 1 && piece.starts_with('@')
}

fn has_word(piece: &str) -> bool {
    let mut chars = piece.chars();
    chars.next().is_some_and(char::is_alphanumeric) || chars.any(char::is_alphanumeric)
}

fn starts_upper(piece: &str) -> bool {
    piece.chars().next().is_some_and(char::is_uppercase)
}

fn is_number(piece: &str) -> bool {
    piece.starts_with(|c: char| c.is_ascii_digit()) || piece.chars().all(|c| "IVXLCDM".contains(c))
}

/// A word with the period of an abbreviation in the token: «г.», «т.», «А.»
fn is_abbreviation(piece: &str) -> bool {
    piece.len() > 1
        && piece.ends_with('.')
        && piece[..piece.len() - 1]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphabetic() || is_mark(c))
}

/// The first letter of a line, past the quotes, dashes and bullets before it, is a capital one
fn begins_upper(line: &str) -> bool {
    line.chars()
        .find(|c| c.is_alphanumeric())
        .is_some_and(char::is_uppercase)
}

/// A line that does not end with punctuation: a heading, a line of a post
fn ends_open(line: &str) -> bool {
    line.chars().next_back().is_some_and(|c| {
        c.is_alphanumeric() || !(c.is_ascii_punctuation() || "«»„“”‘’…—–‒―−".contains(c))
    })
}

struct Splitter<'a> {
    text: &'a str,
    lines: &'a [Span],
    tokens: &'a [Span],
    tables: &'a Tables,
    /// The first token of every line
    line_tokens: Vec<usize>,
    /// Lines end with a comma before a line with a capital letter, as in verse
    verse: bool,
    /// Some line goes on with a lowercase letter, as in text wrapped at the width of a page
    typeset: bool,
}

/// A text split into tokens, sentences and paragraphs
#[derive(Default)]
pub(crate) struct Split {
    pub(crate) tokens: Vec<Span>,
    /// The first token of every sentence
    pub(crate) starts: Vec<usize>,
    /// The first sentence of every paragraph
    pub(crate) paragraphs: Vec<usize>,
    /// The lines with how each begins, if kept
    pub(crate) lines: Option<Vec<Line>>,
}

/// Splits the tokens of a paragraph into sentences and appends them to `out`, with the period of an abbreviation
/// that ends a sentence as a token of its own
pub(crate) fn split(
    text: &str,
    lines: &[Span],
    tokens: &[Span],
    tables: &Tables,
    exceptions: &Exceptions,
    out: &mut Split,
) {
    if tokens.is_empty() {
        return;
    }
    let mut splitter = Splitter {
        text,
        lines,
        tokens,
        tables,
        line_tokens: Vec::new(),
        verse: false,
        typeset: false,
    };
    if lines.len() > 1 {
        splitter.read_lines();
    }
    let first_line = out.lines.as_ref().map_or(0, Vec::len);
    if let Some(breaks) = out.lines.as_mut() {
        breaks.extend(lines.iter().enumerate().map(|(index, &span)| Line {
            span,
            break_before: (index > 0).then_some(LineBreak::Author),
        }));
    }
    out.paragraphs.push(out.starts.len());
    out.starts.push(out.tokens.len());
    let mut words = false;
    let mut next_line = 1;
    let mut index = 0;
    while index < tokens.len() {
        let (last, mut boundary) = splitter.end(index, words);
        // At a line break after the part, the rules of lines may start a sentence where the marks do not
        if !boundary && next_line < lines.len() && splitter.line_tokens[next_line] == last + 1 {
            boundary = splitter.line_boundary(next_line - 1);
        }
        for position in index..=last {
            words = words || has_word(splitter.piece(position));
            let span = tokens[position];
            let piece = splitter.piece(position);
            let ends = position == last && (boundary || last + 1 == tokens.len());
            if ends && is_abbreviation(piece) && !exceptions.tokens.contains(piece) {
                out.tokens.push(Span {
                    start: span.start,
                    end: span.end - 1,
                });
                out.tokens.push(Span {
                    start: span.end - 1,
                    end: span.end,
                });
            } else {
                out.tokens.push(span);
            }
            if next_line < lines.len() && splitter.line_tokens[next_line] == position + 1 {
                let kind = if (position == last && boundary) || !splitter.wraps(next_line - 1) {
                    LineBreak::Author
                } else {
                    LineBreak::Typeset
                };
                if let Some(breaks) = out.lines.as_mut() {
                    breaks[first_line + next_line].break_before = Some(kind);
                }
                next_line += 1;
            }
        }
        if boundary && last + 1 < tokens.len() {
            out.starts.push(out.tokens.len());
            words = false;
        }
        index = last + 1;
    }
}

impl Splitter<'_> {
    fn piece(&self, index: usize) -> &str {
        self.tokens[index].slice(self.text)
    }

    fn line(&self, index: usize) -> &str {
        self.lines[index].slice(self.text)
    }

    /// The token right after the one before it, without a space between
    fn attached(&self, index: usize) -> bool {
        self.tokens[index - 1].end == self.tokens[index].start
    }

    fn read_lines(&mut self) {
        let mut token = 0;
        for line in self.lines {
            while token + 1 < self.tokens.len() && self.tokens[token].start < line.start {
                token += 1;
            }
            self.line_tokens.push(token);
        }
        for pair in self.lines.windows(2) {
            let (before, after) = (pair[0].slice(self.text), pair[1].slice(self.text));
            self.verse |= before.ends_with([',', ';']) && begins_upper(after);
            self.typeset |= after.starts_with(char::is_lowercase);
        }
    }

    /// The last token of the sentence part that starts at `index`, and whether a sentence ends after it. A part is
    /// one token, or a final mark or an abbreviation with the final marks and closing quotes and brackets right
    /// after it
    fn end(&self, index: usize, words: bool) -> (usize, bool) {
        let piece = self.piece(index);
        let count = self.tokens.len();
        if index + 1 == count {
            return (index, false);
        }
        let candidate =
            matches!(piece.as_bytes().last(), Some(b'.' | b'!' | b'?')) || piece.ends_with('…');
        let stars = is_stars(piece) || (piece == "*" && index > 0 && self.piece(index - 1) == "*");
        if !candidate && !stars && !is_emoticon(piece) {
            return (index, false);
        }
        let words = words || has_word(piece);
        if stars {
            // A separator of stars is a sentence of its own: «* * *», «***»
            return (index, starts_upper(self.piece(index + 1)));
        }
        if is_emoticon(piece) {
            // An emoticon after the words of a sentence ends it before a capital letter or a mention, unless it
            // starts a line as a bullet
            if self.line_start(index) {
                return (index, false);
            }
            let next = self.piece(index + 1);
            return (index, words && (starts_upper(next) || is_mention(next)));
        }
        if !candidate || self.omission(index) {
            return (index, false);
        }
        let mut last = index;
        while last + 1 < count
            && self.attached(last + 1)
            && (is_closing(self.piece(last + 1)) || is_final(self.piece(last + 1)))
        {
            last += 1;
        }
        let marks = (index..=last).any(|position| is_final(self.piece(position)));
        // Emoticons after final marks stay in the sentence: «Ура! :)»
        while marks
            && last + 1 < count
            && is_emoticon(self.piece(last + 1))
            && !self.line_start(last + 1)
        {
            last += 1;
        }
        if last + 1 == count {
            return (last, false);
        }
        // A source in brackets after a quotation stays in its sentence: «…» (Пушкин)
        let source =
            matches!(self.piece(last), "»" | "”" | "\"" | "''") && self.piece(last + 1) == "(";
        let boundary = if marks {
            words && !source && !self.list_number(index) && self.starts(last + 1, true)
        } else {
            is_abbreviation(piece) && self.starts(last + 1, false) && self.ends(index)
        };
        (last, boundary)
    }

    /// Whether the tokens from `index` start a sentence: a capital letter, maybe after an opening quote or bracket,
    /// and after final marks also a digit, a mention or a dash before a capital letter
    fn starts(&self, index: usize, marks: bool) -> bool {
        let opens = |piece: &str| {
            piece.starts_with(|c: char| c.is_uppercase() || (marks && c.is_ascii_digit()))
        };
        let piece = self.piece(index);
        if opens(piece) {
            return true;
        }
        if piece.starts_with(char::is_alphabetic) {
            return false;
        }
        let next = |offset: usize| {
            self.tokens
                .get(index + offset)
                .map(|span| span.slice(self.text))
        };
        if is_opening(piece) {
            return next(1).is_some_and(opens);
        }
        if !marks {
            return false;
        }
        if is_mention(piece) {
            return true;
        }
        if piece.starts_with(['…', '.']) && piece != "." && is_final(piece) {
            // An ellipsis before a capital letter opens a sentence: «Он вышел. … Тополиный пух»
            return next(1).is_some_and(starts_upper);
        }
        if piece == "*" {
            return next(1) == Some("*");
        }
        if is_stars(piece) {
            return true;
        }
        if is_dash(piece) {
            return match next(1) {
                Some(after) if is_opening(after) => next(2).is_some_and(starts_upper),
                Some(after) => starts_upper(after),
                None => false,
            };
        }
        false
    }

    /// Whether an abbreviation before a capital letter ends a sentence: one of the language that does («и т. д.»),
    /// or a lowercase one after a number, maybe through other lowercase ones («в 1914 г.», «15 тыс. руб.»)
    fn ends(&self, index: usize) -> bool {
        let lowercase = |piece: &str| !piece.chars().any(char::is_uppercase);
        // The abbreviations before this one and the word before them, as the list keys them: «т. д.», «и пр.»
        let mut key = String::new();
        for position in (index.saturating_sub(3)..=index).rev() {
            let piece = self.piece(position);
            let abbreviation = is_abbreviation(piece) && lowercase(piece);
            let word = self.tables.key(if abbreviation {
                &piece[..piece.len() - 1]
            } else {
                piece
            });
            key = if key.is_empty() {
                word
            } else {
                format!("{word} {key}")
            };
            if self.tables.final_abbreviations.contains(&key) {
                return true;
            }
            if !abbreviation {
                break;
            }
        }
        // Back to a number over units: «5 млрд руб.», «60 км/ч.»
        if !lowercase(self.piece(index)) {
            return false;
        }
        for position in (index.saturating_sub(4)..index).rev() {
            let piece = self.piece(position);
            if is_number(piece) {
                return true;
            }
            let word = piece.trim_end_matches('.');
            let unit = piece == "/"
                || self.piece(position + 1) == "/"
                || (lowercase(piece)
                    && word.chars().count() <= 4
                    && word.chars().all(char::is_alphabetic)
                    && (word.len() < piece.len()
                        || self.tables.abbreviations.contains(&self.tables.key(word))));
            if !unit {
                return false;
            }
        }
        false
    }

    /// An ellipsis right after an opening quote or bracket marks an omission, not an end: «"…», «[…]»
    fn omission(&self, index: usize) -> bool {
        index > 0
            && self.piece(index).starts_with(['…', '.'])
            && self.piece(index) != "."
            && self.attached(index)
            && is_opening(self.piece(index - 1))
            && (index == 1 || !self.attached(index - 1))
    }

    /// A number with a period at the start of a line: the marker of a list item, not the end of a sentence
    fn list_number(&self, index: usize) -> bool {
        index > 0
            && self.piece(index) == "."
            && self.attached(index)
            && is_number(self.piece(index - 1))
            && (index == 1 || self.line_start(index - 1))
    }

    /// Whether a token starts a line of a paragraph of several lines
    fn line_start(&self, index: usize) -> bool {
        self.line_tokens.binary_search(&index).is_ok()
    }

    /// Whether a line break starts a sentence: before a list item, at a line without words, or after a heading
    fn line_boundary(&self, line: usize) -> bool {
        let first = self.line_tokens[line + 1];
        let after = self.piece(first);
        let marker = after.chars().all(|c| BULLETS.contains(c))
            || after.starts_with(is_emoji)
            || after.contains('\u{20e3}')
            || (first + 1 < self.tokens.len()
                && self.attached(first + 1)
                && matches!(self.piece(first + 1), "." | ")")
                && (is_number(after) || after.chars().count() == 1));
        if marker {
            return true;
        }
        let (before, next) = (self.line(line), self.line(line + 1));
        if !has_word(before) || !has_word(next) {
            return true;
        }
        !self.verse && !self.typeset && ends_open(before) && begins_upper(next)
    }

    /// Whether a line break without a sentence boundary is a wrap at the width of the page: the next line goes on
    /// with a lowercase letter, outside verse
    fn wraps(&self, line: usize) -> bool {
        !self.verse && self.line(line + 1).starts_with(char::is_lowercase)
    }
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use crate::{LineBreak, Segmenter};

    fn sentences(text: &str) -> Vec<&str> {
        Segmenter::new("ru")
            .unwrap()
            .sentenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    fn tokens(text: &str) -> Vec<&str> {
        Segmenter::new("ru")
            .unwrap()
            .tokenize(text)
            .iter()
            .map(|span| span.slice(text))
            .collect()
    }

    #[test]
    fn final_marks() {
        assert_eq!(
            sentences("Ура!!! Ура?! Ура... Слово – слово… и №1"),
            ["Ура!!!", "Ура?!", "Ура...", "Слово – слово… и №1"]
        );
        assert_eq!(
            sentences("Он пришел.Она ушла.Все спят. В 10.30.Начали"),
            ["Он пришел.", "Она ушла.", "Все спят.", "В 10.30.", "Начали"]
        );
        assert_eq!(
            sentences("Это важно (очень важно!). Он вышел. (Дверь хлопнула.) Все ушли."),
            [
                "Это важно (очень важно!).",
                "Он вышел.",
                "(Дверь хлопнула.)",
                "Все ушли."
            ]
        );
        assert_eq!(
            sentences("Кот спит. . . . Пёс лает! и бегает. 5 котов спят."),
            ["Кот спит. . . .", "Пёс лает! и бегает.", "5 котов спят."]
        );
        assert_eq!(
            sentences("Писал: \"… Мир велик\". Он вышел. … Пух летит […] Да."),
            [
                "Писал: \"… Мир велик\".",
                "Он вышел.",
                "… Пух летит […] Да."
            ]
        );
    }

    #[test]
    fn abbreviations_and_initials() {
        assert_eq!(
            sentences("Купили груши и т. д. Потом ушли. Это г. Москва. В 1914 г. Затем"),
            [
                "Купили груши и т. д.",
                "Потом ушли.",
                "Это г. Москва.",
                "В 1914 г.",
                "Затем"
            ]
        );
        assert_eq!(
            sentences("Цена 15 000 р. Пишите. Едем 60 км/ч. Стоим 5 млрд руб. Всё."),
            [
                "Цена 15 000 р.",
                "Пишите.",
                "Едем 60 км/ч.",
                "Стоим 5 млрд руб.",
                "Всё."
            ]
        );
        assert_eq!(
            sentences("Кот, т. е. зверь, спит. А.С.Пушкин жил в Москве.Учился в Лицее."),
            [
                "Кот, т. е. зверь, спит.",
                "А.С.Пушкин жил в Москве.",
                "Учился в Лицее."
            ]
        );
        assert_eq!(
            sentences("Вуз им. Н. Э. Баумана. Умер в 1224 г. до н. э. Затем"),
            ["Вуз им. Н. Э. Баумана.", "Умер в 1224 г. до н. э.", "Затем"]
        );
        assert_eq!(
            tokens("в 1914 г. Затем, т.д.Потом и т. д.!"),
            [
                "в",
                "1914",
                "г",
                ".",
                "Затем",
                ",",
                "т.",
                "д",
                ".",
                "Потом",
                "и",
                "т.",
                "д.",
                "!"
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            sentences("1. Введение. Текст. 2. Обзор"),
            ["1. Введение.", "Текст.", "2.", "Обзор"]
        );
        assert_eq!(
            sentences("Счёт 2:3. 4 гола забил он."),
            ["Счёт 2:3.", "4 гола забил он."]
        );
    }

    #[test]
    fn dialogue_and_quotes() {
        assert_eq!(
            sentences("— Ты придёшь? — спросила она. — Приду. — Придумала! — Она встала."),
            [
                "— Ты придёшь? — спросила она.",
                "— Приду.",
                "— Придумала!",
                "— Она встала."
            ]
        );
        assert_eq!(
            sentences("Она спросила: «Ты придёшь?» Я кивнул. «Стой! — крикнул он. — Не двигайся!»"),
            [
                "Она спросила: «Ты придёшь?»",
                "Я кивнул.",
                "«Стой! — крикнул он.",
                "— Не двигайся!»"
            ]
        );
        assert_eq!(
            sentences("«Не спится!» (Пушкин). Ушёл… Больше не видели. Тишина… и шум."),
            [
                "«Не спится!» (Пушкин).",
                "Ушёл…",
                "Больше не видели.",
                "Тишина… и шум."
            ]
        );
    }

    #[test]
    fn social() {
        assert_eq!(
            sentences(
                "Привет 😀 Как дела? Ура! :) Пойдём. Спасибо! @anna да. Платье)) @anna_k оцени!"
            ),
            [
                "Привет 😀",
                "Как дела?",
                "Ура! :)",
                "Пойдём.",
                "Спасибо!",
                "@anna да.",
                "Платье))",
                "@anna_k оцени!"
            ]
        );
        assert_eq!(
            sentences("Текст. * * * Новая глава. *** Конец."),
            ["Текст.", "* * *", "Новая глава.", "***", "Конец."]
        );
    }

    #[test]
    fn line_breaks() {
        assert_eq!(
            sentences("Глава 1\nВведение\nЭто текст. Он короткий."),
            ["Глава 1", "Введение", "Это текст.", "Он короткий."]
        );
        assert_eq!(
            sentences("Нужно:\n- паспорт\n• СНИЛС\n1) полис\n2. фото\nа) справка"),
            [
                "Нужно:",
                "- паспорт",
                "• СНИЛС",
                "1) полис",
                "2. фото",
                "а) справка"
            ]
        );
        assert_eq!(
            sentences("Кот спал.\r\nПёс ел.\n***\nКот ушел.\n-"),
            ["Кот спал.", "Пёс ел.", "***", "Кот ушел.", "-"]
        );
        assert_eq!(
            sentences(
                "Скажи-ка, дядя, ведь не даром\nМосква, спаленная пожаром,\nФранцузу отдана?\nВедь были"
            ),
            [
                "Скажи-ка, дядя, ведь не даром\nМосква, спаленная пожаром,\nФранцузу отдана?",
                "Ведь были"
            ]
        );
        assert_eq!(
            sentences("Абзац заканчивается\nздесь. Второй\nНачинается тут."),
            ["Абзац заканчивается\nздесь.", "Второй\nНачинается тут."]
        );
    }

    #[test]
    fn line_break_kinds() {
        let kinds = |text: &str| -> Vec<LineBreak> {
            Segmenter::new("ru")
                .unwrap()
                .segment(text)
                .lines()
                .iter()
                .filter_map(|line| line.break_before)
                .collect()
        };
        assert_eq!(
            kinds("Тени становились длин-\nнее. Мы шли\nпо дороге."),
            [LineBreak::Typeset, LineBreak::Typeset]
        );
        assert_eq!(
            kinds(
                "Прошёл фестиваль.\nОрганизаторы довольны.\n- Да\nМосква, спаленная пожаром,\nФранцузу"
            ),
            [
                LineBreak::Author,
                LineBreak::Author,
                LineBreak::Author,
                LineBreak::Author
            ]
        );
    }

    #[test]
    fn found_by_the_review() {
        assert_eq!(
            sentences(
                "Родился 5 мая 1950 года в г. Ташкенте. Через 5 лет у акад. Сахарова отняли награды."
            ),
            [
                "Родился 5 мая 1950 года в г. Ташкенте.",
                "Через 5 лет у акад. Сахарова отняли награды."
            ]
        );
        assert_eq!(
            sentences(
                "Объём 300 куб. м. Затем всё. Офис на пр. Мира. Купили соль и пр. Потом ушли."
            ),
            [
                "Объём 300 куб. м.",
                "Затем всё.",
                "Офис на пр. Мира.",
                "Купили соль и пр.",
                "Потом ушли."
            ]
        );
        assert_eq!(
            sentences("Справки по тел. 8 (495) 123-45-67. Нет у них тел. Есть души."),
            [
                "Справки по тел. 8 (495) 123-45-67.",
                "Нет у них тел.",
                "Есть души."
            ]
        );
        assert_eq!(
            tokens("Купили и т. д\u{301}. Потом и т. д\u{301}."),
            [
                "Купили",
                "и",
                "т.",
                "д\u{301}",
                ".",
                "Потом",
                "и",
                "т.",
                "д\u{301}",
                "."
            ]
        );
        assert_eq!(
            sentences("Наши услуги:\n✅ Ремонт квартир\n✅ Покраска стен\n\nСкидки!\n🔥 Успейте"),
            [
                "Наши услуги:",
                "✅ Ремонт квартир",
                "✅ Покраска стен",
                "Скидки!",
                "🔥 Успейте"
            ]
        );
        assert_eq!(
            sentences("Программа\n1\u{fe0f}\u{20e3} Регистрация\n2\u{fe0f}\u{20e3} Доклады"),
            [
                "Программа",
                "1\u{fe0f}\u{20e3} Регистрация",
                "2\u{fe0f}\u{20e3} Доклады"
            ]
        );
    }

    #[test]
    fn long_runs() {
        // Linear scans: a quadratic one takes seconds on each of these
        for text in [
            "Кот спит. ".repeat(20_000),
            "1. ".repeat(20_000),
            "a\n".repeat(20_000),
            "- пункт\n".repeat(20_000),
            "* ".repeat(40_000),
            "т. ".repeat(40_000),
            "😀 ".repeat(40_000),
            "@a ".repeat(40_000),
            "в 1914 г. ".repeat(20_000),
        ] {
            let spans = Segmenter::new("ru").unwrap().sentenize(&text);
            assert!(!spans.is_empty());
        }
    }
}

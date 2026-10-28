//! Paragraphs and lines of a text

use crate::Span;

/// Whitespace and the invisible characters that separate tokens without being tokens: zero width space,
/// word joiner, byte order mark, and a soft hyphen and zero width (non-)joiner, which inside a word or an
/// emoji belong to it
pub(crate) fn is_separator(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '\u{200b}' | '\u{2060}' | '\u{feff}' | '\u{ad}' | '\u{200c}' | '\u{200d}'
        )
}

/// A span without the separators at its edges, if anything is left
pub(crate) fn trim(text: &str, span: Span) -> Option<Span> {
    let piece = span.slice(text);
    let rest = piece.trim_start_matches(is_separator);
    let start = span.start + piece.len() - rest.len();
    let end = start + rest.trim_end_matches(is_separator).len();
    (start < end).then_some(Span { start, end })
}

/// The runs of characters between separators inside a span
#[cfg(test)]
pub(crate) fn runs(text: &str, span: Span, out: &mut Vec<Span>) {
    let mut start = None;
    for (index, character) in span.slice(text).char_indices() {
        let index = span.start + index;
        if is_separator(character) {
            if let Some(start) = start.take() {
                out.push(Span { start, end: index });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(start) = start {
        out.push(Span {
            start,
            end: span.end,
        });
    }
}

/// The paragraphs of a text, each as its lines without the separators around them. A line ends at LF, CR, CRLF,
/// VT, FF, NEL or U+2028; a blank line (of separators only) or U+2029 ends a paragraph
pub(crate) fn paragraphs(text: &str) -> Vec<Vec<Span>> {
    let mut paragraphs = Vec::new();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        let ends_paragraph = match character {
            '\u{2029}' => true,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' => false,
            _ => continue,
        };
        let mut next = index + character.len_utf8();
        if character == '\r' && matches!(chars.peek(), Some((_, '\n'))) {
            chars.next();
            next += 1;
        }
        add_line(
            text,
            Span { start, end: index },
            &mut lines,
            &mut paragraphs,
        );
        if ends_paragraph {
            close(&mut lines, &mut paragraphs);
        }
        start = next;
    }
    add_line(
        text,
        Span {
            start,
            end: text.len(),
        },
        &mut lines,
        &mut paragraphs,
    );
    close(&mut lines, &mut paragraphs);
    paragraphs
}

fn add_line(text: &str, line: Span, lines: &mut Vec<Span>, paragraphs: &mut Vec<Vec<Span>>) {
    match trim(text, line) {
        Some(line) => lines.push(line),
        None => close(lines, paragraphs),
    }
}

fn close(lines: &mut Vec<Span>, paragraphs: &mut Vec<Vec<Span>>) {
    if !lines.is_empty() {
        paragraphs.push(std::mem::take(lines));
    }
}

/// A text with its line breaks replaced by single spaces, as `# text` of CoNLL-U keeps it
pub(crate) fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(' ');
            }
            '\n' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => out.push(' '),
            _ => out.push(character),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(text: &str) -> Vec<Vec<&str>> {
        paragraphs(text)
            .iter()
            .map(|lines| lines.iter().map(|line| line.slice(text)).collect())
            .collect()
    }

    #[test]
    fn blank_lines_end_paragraphs() {
        assert_eq!(
            pieces(
                " Один.\r\nДва \n \t\n\u{feff}\n\u{200b}\nТри\u{2029}Четыре\u{2028}пять\r\rШесть"
            ),
            [
                vec!["Один.", "Два"],
                vec!["Три"],
                vec!["Четыре", "пять"],
                vec!["Шесть"],
            ]
        );
    }

    #[test]
    fn crlf_is_one_line_break() {
        assert_eq!(pieces("а\r\nб"), [vec!["а", "б"]]);
        assert_eq!(pieces("а\r\n\r\nб"), [vec!["а"], vec!["б"]]);
        assert_eq!(pieces("а\u{85}б\u{b}в\u{c}г"), [vec!["а", "б", "в", "г"]]);
    }

    #[test]
    fn blank_text_has_no_paragraphs() {
        for text in ["", " ", "\n\n", "\u{feff}\u{200b} \u{2029}"] {
            assert!(paragraphs(text).is_empty(), "{text:?}");
        }
    }

    #[test]
    fn runs_skip_separators() {
        let text = "\u{feff}Ёж\u{200b}идёт \u{2060}домой";
        let mut out = Vec::new();
        runs(
            text,
            Span {
                start: 0,
                end: text.len(),
            },
            &mut out,
        );
        let tokens: Vec<&str> = out.iter().map(|span| span.slice(text)).collect();
        assert_eq!(tokens, ["Ёж", "идёт", "домой"]);
    }

    #[test]
    fn one_line_text() {
        assert_eq!(one_line("а\r\nб\nв\u{2028}г"), "а б в г");
    }
}

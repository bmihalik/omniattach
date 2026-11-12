//! Line-end de-hyphenation, adapted from the supplied Thinkerizer reference.
use crate::{BlockKind, Document};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub joined: usize,
    pub skipped_long: usize,
}

/// Join line-end word fragments, preserving the rest of each source line.
/// Zero disables processing. Fenced code and pipe-table rows are protected.
/// The budget estimates one token per four ASCII or three non-ASCII characters.
pub fn dehyphenate_text(text: &str, max_tokens: usize) -> (String, Stats) {
    process(text, max_tokens, true)
}

fn process(text: &str, max_tokens: usize, markdown: bool) -> (String, Stats) {
    let mut stats = Stats::default();
    if max_tokens == 0 {
        return (text.to_owned(), stats);
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let mut fence: Option<String> = None;
    let mut i = 0;
    while i + 1 < lines.len() {
        let trimmed = lines[i].trim_start();
        if markdown {
            if let Some(marker) = fence_marker(trimmed) {
                fence = match &fence {
                    Some(open) if marker.starts_with(open.as_str()) => None,
                    Some(open) => Some(open.clone()),
                    None => Some(marker),
                };
                i += 1;
                continue;
            }
            if fence.is_some() || trimmed.starts_with('|') {
                i += 1;
                continue;
            }
        }
        let Some((head, prefix)) = hyphenated_tail(&lines[i]) else {
            i += 1;
            continue;
        };
        let next = lines[i + 1].trim_start();
        if !next.chars().next().is_some_and(char::is_lowercase) {
            i += 1;
            continue;
        }
        let split = next
            .find(|c: char| !(c.is_alphanumeric() || c == '\''))
            .unwrap_or(next.len());
        let (suffix, rest) = next.split_at(split);
        let word = format!("{prefix}{suffix}");
        let per = if word.is_ascii() { 4 } else { 3 };
        if word.chars().count().div_ceil(per).max(1) > max_tokens {
            stats.skipped_long += 1;
            i += 1;
            continue;
        }
        let punctuation_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (punctuation, rest) = rest.split_at(punctuation_end);
        let joined = format!("{head}{word}{punctuation}");
        let rest = rest.trim_start().to_owned();
        lines[i] = joined;
        if rest.is_empty() {
            lines.remove(i + 1);
        } else {
            lines[i + 1] = rest;
        }
        stats.joined += 1;
    }
    (lines.join("\n"), stats)
}
fn fence_marker(line: &str) -> Option<String> {
    let first = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run: String = line.chars().take_while(|c| *c == first).collect();
    (run.len() >= 3).then_some(run)
}
fn hyphenated_tail(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_end_matches([' ', '\t']);
    let hyphen = line
        .chars()
        .last()
        .filter(|c| matches!(c, '-' | '\u{00ad}' | '\u{2010}'))?;
    let body = &line[..line.len() - hyphen.len_utf8()];
    if !body.chars().last().is_some_and(char::is_alphabetic) {
        return None;
    }
    let start = body
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric())
        .last()
        .map(|(i, _)| i)?;
    Some((&body[..start], &body[start..]))
}

pub(crate) fn apply_document(doc: &mut Document, max_tokens: usize) {
    for diagnostic in &mut doc.diagnostics {
        if diagnostic.code == "PDF_TEXT_ONLY" {
            diagnostic.message = diagnostic.message.replace("No de-hyphenation, ", "No ");
            diagnostic.message.push_str(" Line-end de-hyphenation is a separate structured-document pass controlled by dehyphenate_max_tokens (default 6; 0 disables).");
        }
    }
    if max_tokens == 0 {
        return;
    }
    let mut total = Stats::default();
    for block in &mut doc.blocks {
        if matches!(
            block.kind,
            BlockKind::Paragraph | BlockKind::ListItem { .. } | BlockKind::Quote
        ) {
            let (text, stats) = process(&block.text, max_tokens, false);
            block.text = text;
            total.joined = total.joined.saturating_add(stats.joined);
            total.skipped_long = total.skipped_long.saturating_add(stats.skipped_long);
        }
    }
    if total.joined != 0 || total.skipped_long != 0 {
        doc.warn("DEHYPHENATED", format!("Joined {} line-end word fragments; skipped {} over-budget candidates (dehyphenate_max_tokens={max_tokens})", total.joined, total.skipped_long));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_joins_punctuation_unicode_and_chains() {
        for (input, expected, count) in [
            (
                "A számító-\ngép működik. Más\nsor marad.",
                "A számítógép\nműködik. Más\nsor marad.",
                1,
            ),
            ("The infor-\nmation, here", "The information,\nhere", 1),
            ("a con\u{00ad}\ntent\nnext", "a content\nnext", 1),
            ("a con\u{2010}\n tent b", "a content\nb", 1),
            ("ab-\ncd ef-\ngh", "abcd\nefgh", 2),
            ("x ab-\ncd-\nef y", "x abcdef\ny", 2),
            ("rögzí-\nti, így", "rögzíti,\nígy", 1),
        ] {
            let (text, stats) = dehyphenate_text(input, 6);
            assert_eq!(text, expected);
            assert_eq!(stats.joined, count);
            assert_eq!(stats.skipped_long, 0);
        }
    }
    #[test]
    fn reference_preserves_dashes_fences_tables_and_boundaries() {
        for text in [
            "Jean-\nPaul",
            "range 3-\n5",
            "a pause --\nthen",
            "a pause -\nthen",
            "end of text-",
            "```\nwrap-\nped\n```",
            "~~~rust\nwrap-\nped\n~~~",
            "| a- |\n| b |",
            "| a-\nb",
            "x-\n\nnext paragraph",
        ] {
            assert_eq!(
                dehyphenate_text(text, 6),
                (text.into(), Stats::default()),
                "{text:?}"
            );
        }
        assert_eq!(
            dehyphenate_text("```\nx\n```\nab-\ncd", 6).0,
            "```\nx\n```\nabcd"
        );
    }
    #[test]
    fn reference_budgets_and_disabling() {
        for (input, expected) in [
            (
                "a kozmikus ciklus kiter-\njedését, amelynek",
                "a kozmikus ciklus kiterjedését,\namelynek",
            ),
            ("összeil-\nlesztésére, s", "összeillesztésére,\ns"),
        ] {
            assert_eq!(dehyphenate_text(input, 6).0, expected);
        }
        for text in [
            "munkavédelmiszabály-\nzat-módosításokról",
            "Donaudampfschiff-\nfahrtsgesellschaftskapitaen ok",
        ] {
            assert_eq!(
                dehyphenate_text(text, 6),
                (
                    text.into(),
                    Stats {
                        joined: 0,
                        skipped_long: 1
                    }
                )
            );
        }
        assert_eq!(
            dehyphenate_text("Donaudampfschiff-\nfahrtsgesellschaftskapitaen ok", 12).0,
            "Donaudampfschifffahrtsgesellschaftskapitaen\nok"
        );
        assert_eq!(
            dehyphenate_text("ab-\ncd", 0),
            ("ab-\ncd".into(), Stats::default())
        );
    }
    #[test]
    fn structured_blocks_protect_nonprose_and_keep_sources() {
        use crate::{Cell, Options, SourceSpan, Table};
        let mut doc = crate::extract_bytes("empty.md", b"", &Options::default()).unwrap();
        let span = SourceSpan {
            page: Some(1),
            path: Some("ocr".into()),
            ..Default::default()
        };
        for kind in [
            BlockKind::Paragraph,
            BlockKind::ListItem { ordered: false },
            BlockKind::Quote,
            BlockKind::Code { language: None },
            BlockKind::Heading { level: 1 },
            BlockKind::ThematicBreak,
        ] {
            doc.paragraph(kind, "ab-\ncd".into(), span.clone());
        }
        doc.add_table(Table {
            id: String::new(),
            name: None,
            columns: vec!["col".into()],
            has_header: true,
            rows: vec![vec![Cell::text("ta-\nble")]],
            source: span.clone(),
        });
        let before = doc.clone();
        apply_document(&mut doc, 6);
        for i in 0..3 {
            assert_eq!(doc.blocks[i].text, "abcd");
            assert_eq!(doc.blocks[i].source, span);
            assert_eq!(doc.blocks[i].id, before.blocks[i].id);
        }
        assert_eq!(&doc.blocks[3..], &before.blocks[3..]);
        assert_eq!(doc.tables, before.tables);
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.code == "DEHYPHENATED" && d.message.contains("Joined 3 "))
        );
    }
    #[test]
    fn extraction_applies_once_before_all_rendering() {
        let doc = crate::extract_bytes("a.md", b"ab-\ncd", &crate::Options::default()).unwrap();
        assert_eq!(doc.to_text(), "abcd");
        assert_eq!(doc.to_markdown(), "abcd\n");
        let json: serde_json::Value = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
        assert_eq!(json["blocks"][0]["text"], "abcd");
        assert_eq!(
            doc.diagnostics
                .iter()
                .filter(|d| d.code == "DEHYPHENATED")
                .count(),
            1
        );
        let options = crate::Options {
            dehyphenate_max_tokens: 0,
            ..Default::default()
        };
        let raw = crate::extract_bytes("a.md", b"ab-\ncd", &options).unwrap();
        assert_eq!(raw.to_text(), "ab-\ncd");
        assert!(!raw.diagnostics.iter().any(|d| d.code == "DEHYPHENATED"));
    }
}

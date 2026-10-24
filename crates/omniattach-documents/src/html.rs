use crate::{BlockKind, Cell, Document, Error, Options, Result, SourceSpan, Table};
use scraper::{ElementRef, Html, Node, Selector};

fn selector(s: &str) -> Selector {
    Selector::parse(s).unwrap()
}
fn hidden(element: ElementRef<'_>) -> bool {
    std::iter::once(element)
        .chain(element.ancestors().filter_map(ElementRef::wrap))
        .any(|e| {
            matches!(
                e.value().name(),
                "script" | "style" | "noscript" | "template" | "head"
            ) || e.value().attr("hidden").is_some()
        })
}
fn content(element: ElementRef<'_>, pre: bool) -> String {
    let mut out = String::new();
    for node in element.descendants() {
        match node.value() {
            Node::Text(text) => {
                if !node.ancestors().filter_map(ElementRef::wrap).any(hidden) {
                    out.push_str(text);
                }
            }
            Node::Element(e) if e.name() == "br" => out.push('\n'),
            _ => {}
        }
    }
    if pre {
        out
    } else {
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
fn selected_ancestor(element: ElementRef<'_>) -> bool {
    element.ancestors().filter_map(ElementRef::wrap).any(|e| {
        matches!(
            e.value().name(),
            "p" | "pre"
                | "li"
                | "td"
                | "th"
                | "table"
                | "blockquote"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        )
    })
}
pub(crate) fn extract(
    doc: &mut Document,
    text: &str,
    member: Option<&str>,
    options: &Options,
) -> Result<()> {
    let html = Html::parse_document(text);
    if doc.title.is_none() {
        doc.title = html
            .select(&selector("title"))
            .next()
            .map(|e| e.text().collect::<String>());
    }
    let start = doc.blocks.len();
    for element in html.select(&selector("h1,h2,h3,h4,h5,h6,p,pre,li,blockquote,table")) {
        if hidden(element) || selected_ancestor(element) {
            continue;
        }
        if doc.blocks.len() >= options.limits.blocks {
            return Err(Error::LimitExceeded("HTML blocks".into()));
        }
        let span = SourceSpan {
            member: member.map(str::to_owned),
            ..Default::default()
        };
        let name = element.value().name();
        if name == "table" {
            let mut rows = Vec::new();
            let mut header = false;
            let mut merged = false;
            let mut count = 0;
            for (ri, tr) in element.select(&selector("tr")).enumerate() {
                // Ignore rows belonging to nested tables.
                if tr
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|e| e.value().name() == "table")
                    .is_some_and(|e| e.id() != element.id())
                {
                    continue;
                }
                let mut row = Vec::new();
                for td in tr
                    .children()
                    .filter_map(ElementRef::wrap)
                    .filter(|e| matches!(e.value().name(), "td" | "th"))
                {
                    if ri == 0 && td.value().name() == "th" {
                        header = true;
                    }
                    merged |= td.value().attr("colspan").is_some()
                        || td.value().attr("rowspan").is_some();
                    count += 1;
                    if count > options.limits.cells {
                        return Err(Error::LimitExceeded("HTML table cells".into()));
                    }
                    let mut cell = Cell::text(content(td, false));
                    cell.source = span.clone();
                    row.push(cell);
                }
                rows.push(row);
            }
            let width = rows.iter().map(Vec::len).max().unwrap_or(0);
            let columns = if header && !rows.is_empty() {
                rows.remove(0)
                    .iter()
                    .map(|c| c.display().to_owned())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mut table = Table {
                id: String::new(),
                name: None,
                columns,
                has_header: header,
                rows,
                source: span,
            };
            while table.columns.len() < width {
                table
                    .columns
                    .push(format!("column_{}", table.columns.len() + 1));
            }
            if table.rows.len().saturating_mul(width) > options.limits.cells {
                return Err(Error::LimitExceeded("padded HTML cells".into()));
            }
            for row in &mut table.rows {
                row.resize(width, Cell::default());
            }
            if merged {
                doc.warn(
                    "TABLE_MERGES_FLATTENED",
                    "HTML row/column spans are not reconstructed",
                );
            }
            doc.add_table(table);
            continue;
        }
        let value = content(element, name == "pre");
        if value.trim().is_empty() {
            continue;
        }
        if let Some(level) = name.strip_prefix('h').and_then(|s| s.parse::<u8>().ok()) {
            doc.heading(level, value, span);
        } else {
            let kind = match name {
                "pre" => BlockKind::Code { language: None },
                "li" => BlockKind::ListItem {
                    ordered: element
                        .parent()
                        .and_then(ElementRef::wrap)
                        .is_some_and(|e| e.value().name() == "ol"),
                },
                "blockquote" => BlockKind::Quote,
                _ => BlockKind::Paragraph,
            };
            doc.paragraph(kind, value, span);
        }
    }
    if start == doc.blocks.len()
        && let Some(body) = html.select(&selector("body")).next()
    {
        let value = content(body, false);
        if !value.is_empty() {
            doc.paragraph(
                BlockKind::Paragraph,
                value,
                SourceSpan {
                    member: member.map(str::to_owned),
                    ..Default::default()
                },
            );
        }
    }
    doc.warn("HTML_TEXT_PROJECTION", "Inline markup/link destinations/images and CSS visibility are not preserved; scripts/styles are omitted, no resources are fetched");
    Ok(())
}

use crate::{BlockKind, Cell, Document, Error, Options, Result, SourceSpan, Table};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

pub(crate) fn decode(bytes: &[u8], encoding: Option<&str>) -> Result<(String, String)> {
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        return Err(Error::InvalidInput(
            "UTF-32 is not supported; transcode to UTF-8 first".into(),
        ));
    }
    if let Some((enc, skip)) = encoding_rs::Encoding::for_bom(bytes) {
        let (s, errors) = enc.decode_without_bom_handling(&bytes[skip..]);
        if errors {
            return Err(Error::InvalidInput("invalid BOM-encoded text".into()));
        }
        return Ok((s.into_owned(), enc.name().into()));
    }
    if let Some(label) = encoding {
        let enc = encoding_rs::Encoding::for_label(label.as_bytes())
            .ok_or_else(|| Error::InvalidInput(format!("unknown encoding: {label}")))?;
        let (s, errors) = enc.decode_without_bom_handling(bytes);
        if errors {
            return Err(Error::InvalidInput("invalid encoded text".into()));
        }
        return Ok((s.into_owned(), enc.name().into()));
    }
    Ok((
        std::str::from_utf8(bytes)
            .map_err(|e| {
                Error::InvalidInput(format!(
                    "text must be UTF-8/BOM or use Options::encoding: {e}"
                ))
            })?
            .to_owned(),
        "UTF-8".into(),
    ))
}
pub(crate) fn source_span(text: &str, start: usize, end: usize) -> SourceSpan {
    SourceSpan {
        decoded_byte_start: Some(start),
        decoded_byte_end: Some(end),
        line_start: Some(text[..start].bytes().filter(|b| *b == b'\n').count() as u32 + 1),
        line_end: Some(
            text.as_bytes()[..end.saturating_sub(1).max(start)]
                .iter()
                .copied()
                .filter(|b| *b == b'\n')
                .count() as u32
                + 1,
        ),
        ..Default::default()
    }
}
pub(crate) fn plain(
    doc: &mut Document,
    text: &str,
    code: bool,
    language: Option<String>,
    options: &Options,
) -> Result<()> {
    if code {
        if !text.is_empty() {
            doc.paragraph(
                BlockKind::Code { language },
                text.to_owned(),
                source_span(text, 0, text.len()),
            );
        }
    } else {
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let content = line.trim_end_matches(['\n', '\r']);
            if !content.trim().is_empty() {
                if doc.blocks.len() >= options.limits.blocks {
                    return Err(Error::LimitExceeded("text blocks".into()));
                }
                doc.paragraph(
                    BlockKind::Paragraph,
                    content.into(),
                    source_span(text, offset, offset + content.len()),
                );
            }
            offset += line.len();
        }
    }
    Ok(())
}
pub(crate) fn delimited(
    doc: &mut Document,
    text: &str,
    delimiter: u8,
    options: &Options,
) -> Result<()> {
    if matches!(delimiter, 0 | b'"' | b'\n' | b'\r') {
        return Err(Error::InvalidInput("invalid delimiter".into()));
    }
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut table = Table {
        id: String::new(),
        name: Some(doc.source.name.clone()),
        columns: Vec::new(),
        has_header: options.csv_has_headers,
        rows: Vec::new(),
        source: SourceSpan::default(),
    };
    let mut count = 0usize;
    for (index, result) in reader.records().enumerate() {
        let record = result?;
        count += record.len();
        if count > options.limits.cells {
            return Err(Error::LimitExceeded("CSV cells".into()));
        }
        if index == 0 && options.csv_has_headers {
            table.columns = record.iter().map(str::to_owned).collect();
            continue;
        }
        while table.columns.len() < record.len() {
            table
                .columns
                .push(format!("column_{}", table.columns.len() + 1));
        }
        let source = SourceSpan {
            line_start: record.position().map(|p| p.line() as u32),
            row: Some((index + 1) as u32),
            ..Default::default()
        };
        table.rows.push(
            record
                .iter()
                .enumerate()
                .map(|(col, s)| {
                    let mut cell = Cell::text(s);
                    cell.source = source.clone();
                    cell.source.column = Some(col as u32 + 1);
                    cell
                })
                .collect(),
        );
    }
    if table.rows.len().saturating_mul(table.columns.len()) > options.limits.cells {
        return Err(Error::LimitExceeded("padded CSV cells".into()));
    }
    for row in &mut table.rows {
        row.resize(table.columns.len(), Cell::default());
    }
    doc.add_table(table);
    Ok(())
}
fn level(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
struct Pending {
    kind: BlockKind,
    text: String,
    start: usize,
    end: usize,
}
fn flush(doc: &mut Document, text: &str, pending: &mut Option<Pending>) {
    if let Some(p) = pending.take() {
        let span = source_span(text, p.start, p.end);
        match p.kind {
            BlockKind::Heading { level } => doc.heading(level, p.text, span),
            _ => doc.paragraph(p.kind, p.text, span),
        }
    }
}
pub(crate) fn markdown(doc: &mut Document, text: &str, options: &Options) -> Result<()> {
    let flags = pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_TASKLISTS;
    let mut pending: Option<Pending> = None;
    let mut lists = Vec::new();
    let mut quotes = 0usize;
    let mut table: Option<Table> = None;
    let mut row = Vec::new();
    let mut cell: Option<String> = None;
    let mut table_head = false;
    let mut cell_count = 0usize;
    let mut stripped_html = false;
    for (event, range) in Parser::new_ext(text, flags).into_offset_iter() {
        if doc.blocks.len() >= options.limits.blocks {
            return Err(Error::LimitExceeded("Markdown blocks".into()));
        }
        if let Some(p) = &mut pending {
            p.end = range.end;
        }
        match event {
            Event::Start(Tag::Table(_)) => {
                flush(doc, text, &mut pending);
                table = Some(Table {
                    id: String::new(),
                    name: None,
                    columns: Vec::new(),
                    has_header: true,
                    rows: Vec::new(),
                    source: source_span(text, range.start, range.end),
                });
            }
            Event::Start(Tag::TableHead) => {
                table_head = true;
                row.clear();
            }
            Event::Start(Tag::TableRow) => row.clear(),
            Event::Start(Tag::TableCell) => {
                cell = Some(String::new());
                cell_count += 1;
                if cell_count > options.limits.cells {
                    return Err(Error::LimitExceeded("Markdown cells".into()));
                }
            }
            Event::End(TagEnd::TableCell) => row.push(Cell::text(cell.take().unwrap_or_default())),
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = &mut table {
                    t.columns = row.iter().map(|c| c.display().to_owned()).collect();
                }
                table_head = false;
            }
            Event::End(TagEnd::TableRow) => {
                if !table_head && let Some(t) = &mut table {
                    t.rows.push(std::mem::take(&mut row));
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take() {
                    doc.add_table(t);
                }
            }
            Event::Start(Tag::List(start)) => {
                flush(doc, text, &mut pending);
                lists.push(start.is_some());
            }
            Event::End(TagEnd::List(_)) => {
                flush(doc, text, &mut pending);
                lists.pop();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(doc, text, &mut pending);
                quotes += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(doc, text, &mut pending);
                quotes -= 1;
            }
            Event::Start(Tag::Heading { level: l, .. }) => {
                flush(doc, text, &mut pending);
                pending = Some(Pending {
                    kind: BlockKind::Heading { level: level(l) },
                    text: String::new(),
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(doc, text, &mut pending);
                let language = match kind {
                    CodeBlockKind::Fenced(s) if !s.is_empty() => {
                        Some(s.split_whitespace().next().unwrap_or("").into())
                    }
                    _ => None,
                };
                pending = Some(Pending {
                    kind: BlockKind::Code { language },
                    text: String::new(),
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::Item) => {
                flush(doc, text, &mut pending);
                pending = Some(Pending {
                    kind: BlockKind::ListItem {
                        ordered: *lists.last().unwrap_or(&false),
                    },
                    text: String::new(),
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::Paragraph) if pending.is_none() && table.is_none() => {
                pending = Some(Pending {
                    kind: if quotes > 0 {
                        BlockKind::Quote
                    } else {
                        BlockKind::Paragraph
                    },
                    text: String::new(),
                    start: range.start,
                    end: range.end,
                });
            }
            Event::End(TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::Item) => {
                flush(doc, text, &mut pending)
            }
            Event::End(TagEnd::Paragraph) if lists.is_empty() => flush(doc, text, &mut pending),
            Event::Text(s) | Event::Code(s) => {
                if let Some(c) = &mut cell {
                    c.push_str(&s);
                } else if let Some(p) = &mut pending {
                    p.text.push_str(&s);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(c) = &mut cell {
                    c.push('\n');
                } else if let Some(p) = &mut pending {
                    p.text.push('\n');
                }
            }
            Event::Rule => {
                flush(doc, text, &mut pending);
                doc.paragraph(
                    BlockKind::ThematicBreak,
                    String::new(),
                    source_span(text, range.start, range.end),
                );
            }
            Event::TaskListMarker(checked) => {
                if let Some(p) = &mut pending {
                    p.text.push_str(if checked { "[x] " } else { "[ ] " });
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => stripped_html = true,
            _ => {}
        }
    }
    flush(doc, text, &mut pending);
    doc.warn("SEMANTIC_MARKDOWN", "Inline formatting, link/image destinations, list numbering and nesting are not retained; text and block structure are extracted");
    if stripped_html {
        doc.warn(
            "RAW_HTML_OMITTED",
            "Raw HTML in Markdown was omitted; use the HTML parser for HTML documents",
        );
    }
    Ok(())
}

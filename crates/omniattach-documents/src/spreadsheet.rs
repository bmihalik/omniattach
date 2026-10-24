use crate::{Cell, CellValue, Document, Error, Options, Result, SourceSpan, Table};
use calamine::{Data, Reader, open_workbook_auto_from_rs};
use std::io::Cursor;

pub(crate) fn extract(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    if bytes.starts_with(b"PK") {
        let pkg = crate::package::Package::open(bytes, options)?;
        if pkg.contains("content.xml") && !pkg.contains("xl/workbook.xml") {
            return ods(doc, bytes, options);
        }
    }
    let mut book = open_workbook_auto_from_rs(Cursor::new(bytes))
        .map_err(|e| Error::Parse(format!("workbook: {e}")))?;
    let mut total = 0usize;
    for name in book.sheet_names() {
        let range = book
            .worksheet_range(&name)
            .map_err(|e| Error::Parse(format!("sheet {name}: {e}")))?;
        let formula = match book.worksheet_formula(&name) {
            Ok(range) => Some(range),
            Err(e) => {
                doc.warn("FORMULAS_UNAVAILABLE", format!("{name}: {e}"));
                None
            }
        };
        let bounds = match (
            range.start(),
            range.end(),
            formula.as_ref().and_then(|r| r.start()),
            formula.as_ref().and_then(|r| r.end()),
        ) {
            (None, _, None, _) => None,
            (Some(s), Some(e), None, _) | (None, _, Some(s), Some(e)) => Some((s, e)),
            (Some(s), Some(e), Some(fs), Some(fe)) => Some((
                (s.0.min(fs.0), s.1.min(fs.1)),
                (e.0.max(fe.0), e.1.max(fe.1)),
            )),
            _ => None,
        };
        let mut table = Table {
            id: String::new(),
            name: Some(name.clone()),
            columns: Vec::new(),
            has_header: false,
            rows: Vec::new(),
            source: SourceSpan {
                sheet: Some(name.clone()),
                ..Default::default()
            },
        };
        if let Some((start, end)) = bounds {
            let height = (end.0 - start.0) as usize + 1;
            let width = (end.1 - start.1) as usize + 1;
            total = total.saturating_add(height.saturating_mul(width));
            if total > options.limits.cells {
                return Err(Error::LimitExceeded("workbook cells".into()));
            }
            table.columns = (start.1..=end.1).map(column_name).collect();
            table.source.row = Some(start.0 + 1);
            table.source.column = Some(start.1 + 1);
            for row in start.0..=end.0 {
                let mut cells = Vec::new();
                for col in start.1..=end.1 {
                    let value = range.get_value((row, col)).unwrap_or(&Data::Empty);
                    let mut cell = Cell {
                        source: SourceSpan {
                            sheet: Some(name.clone()),
                            row: Some(row + 1),
                            column: Some(col + 1),
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    if !matches!(value, Data::Empty) {
                        cell.raw = Some(value.to_string());
                    }
                    cell.value = match value {
                        Data::Int(v) => Some(CellValue::Integer(*v)),
                        Data::Float(v) => Some(CellValue::Number(*v)),
                        Data::Bool(v) => Some(CellValue::Boolean(*v)),
                        Data::String(v) => Some(CellValue::Text(v.clone())),
                        Data::DateTimeIso(v) => Some(CellValue::DateTime(v.clone())),
                        Data::DurationIso(v) => Some(CellValue::Text(v.clone())),
                        Data::DateTime(v) => Some(CellValue::SpreadsheetDate {
                            serial: v.as_f64(),
                            calendar: if v.is_datetime() {
                                let (y, m, d, h, min, s, ms) = v.to_ymd_hms_milli();
                                Some(format!(
                                    "{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{ms:03}"
                                ))
                            } else {
                                None
                            },
                        }),
                        Data::Error(v) => Some(CellValue::Error(v.to_string())),
                        Data::Empty => None,
                    };
                    cell.formula = formula
                        .as_ref()
                        .and_then(|r| r.get_value((row, col)))
                        .filter(|s| !s.is_empty())
                        .cloned();
                    cells.push(cell);
                }
                table.rows.push(cells);
            }
        }
        doc.add_table(table);
    }
    doc.warn("WORKBOOK_VALUES", "Values are parser-normalized, not lexical XML or formatted display; formulas use cached results, never recalculated. Styles, merges, visibility, comments and assets are omitted");
    Ok(())
}
fn column_name(mut col: u32) -> String {
    let mut chars = Vec::new();
    loop {
        chars.push((b'A' + (col % 26) as u8) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    chars.into_iter().rev().collect()
}

/// Native ODS route bounds repeated ranges before allocating cells.
pub(crate) fn ods(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
    const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
    const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
    let mut pkg = crate::package::Package::open(bytes, options)?;
    let input = pkg.text("content.xml")?;
    let tree = crate::xml::parse(&input, options)?;
    let mut total = 0usize;
    for sheet in tree
        .descendants()
        .filter(|n| n.has_tag_name((TABLE, "table")))
    {
        let name = sheet
            .attribute((TABLE, "name"))
            .unwrap_or("Sheet")
            .to_owned();
        let mut table = Table {
            id: String::new(),
            name: Some(name.clone()),
            columns: Vec::new(),
            has_header: false,
            rows: Vec::new(),
            source: SourceSpan {
                sheet: Some(name.clone()),
                member: Some("content.xml".into()),
                ..Default::default()
            },
        };
        let mut rownum = 0usize;
        for row in sheet
            .descendants()
            .filter(|n| n.has_tag_name((TABLE, "table-row")))
        {
            let repeats = repeat(row, TABLE, "number-rows-repeated")?;
            let mut cells = Vec::new();
            let mut colnum = 0usize;
            for cell in row.children().filter(|n| {
                n.has_tag_name((TABLE, "table-cell"))
                    || n.has_tag_name((TABLE, "covered-table-cell"))
            }) {
                let repeat_cols = repeat(cell, TABLE, "number-columns-repeated")?;
                let display = cell
                    .descendants()
                    .filter(|n| n.has_tag_name((TEXT, "p")))
                    .map(crate::xml::text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let formula = cell.attribute((TABLE, "formula")).map(str::to_owned);
                let value_type = cell.attribute((OFFICE, "value-type"));
                let lexical = match value_type {
                    Some("float" | "percentage" | "currency") => cell.attribute((OFFICE, "value")),
                    Some("boolean") => cell.attribute((OFFICE, "boolean-value")),
                    Some("date") => cell.attribute((OFFICE, "date-value")),
                    Some("time") => cell.attribute((OFFICE, "time-value")),
                    _ => cell.attribute((OFFICE, "string-value")),
                };
                let value = match value_type {
                    Some("float" | "percentage" | "currency") => lexical
                        .and_then(|s| s.parse::<f64>().ok())
                        .map(CellValue::Number),
                    Some("boolean") => lexical.map(|s| CellValue::Boolean(s == "true" || s == "1")),
                    Some("date") => lexical.map(|s| CellValue::DateTime(s.into())),
                    Some("time") => lexical.map(|s| CellValue::Text(s.into())),
                    Some("string") => Some(CellValue::Text(lexical.unwrap_or(&display).into())),
                    _ if !display.is_empty() => Some(CellValue::Text(display.clone())),
                    _ => None,
                };
                let raw = if let Some(v) = lexical {
                    Some(v.to_owned())
                } else if value.is_some() || !display.is_empty() {
                    Some(display)
                } else {
                    None
                };
                let next_col = colnum
                    .checked_add(repeat_cols)
                    .ok_or_else(|| Error::LimitExceeded("ODS column overflow".into()))?;
                if raw.is_some() || formula.is_some() {
                    if next_col > options.limits.cells {
                        return Err(Error::LimitExceeded("ODS repeated columns".into()));
                    }
                    cells.resize(colnum, Cell::default());
                    for offset in 0..repeat_cols {
                        let mut source = crate::xml::span(cell, Some("content.xml"));
                        source.sheet = Some(name.clone());
                        source.row = Some((rownum + 1) as u32);
                        source.column = Some((colnum + offset + 1) as u32);
                        cells.push(Cell {
                            raw: raw.clone(),
                            value: value.clone(),
                            formula: formula.clone(),
                            source,
                        });
                    }
                }
                colnum = next_col;
            }
            let next_row = rownum
                .checked_add(repeats)
                .ok_or_else(|| Error::LimitExceeded("ODS row overflow".into()))?;
            if !cells.is_empty() {
                let width = table.columns.len().max(cells.len());
                total = total.saturating_add(
                    width.saturating_mul(next_row.saturating_sub(table.rows.len())),
                );
                if next_row > options.limits.cells || total > options.limits.cells {
                    return Err(Error::LimitExceeded("ODS repeated rows/cells".into()));
                }
                table.rows.resize(rownum, Vec::new());
                for offset in 0..repeats {
                    let mut copy = cells.clone();
                    for c in &mut copy {
                        c.source.row = Some((rownum + offset + 1) as u32);
                    }
                    table.rows.push(copy);
                }
                table.columns = (0..width).map(|i| column_name(i as u32)).collect();
            }
            rownum = next_row;
        }
        if table.rows.len().saturating_mul(table.columns.len()) > options.limits.cells {
            return Err(Error::LimitExceeded("ODS padded cells".into()));
        }
        for row in &mut table.rows {
            row.resize(table.columns.len(), Cell::default());
        }
        doc.add_table(table);
        if doc.blocks.len() > options.limits.blocks {
            return Err(Error::LimitExceeded("ODS sheets".into()));
        }
    }
    doc.warn("ODS_VALUES", "Lexical typed values and cached formula results retained; no formula evaluation. Styles, currency units, formatted display, merges/visibility and assets omitted. Trailing blank ranges trimmed; repetitions expanded within cell budget");
    Ok(())
}
fn repeat(node: roxmltree::Node<'_, '_>, ns: &str, name: &str) -> Result<usize> {
    let count = node
        .attribute((ns, name))
        .unwrap_or("1")
        .parse::<usize>()
        .map_err(|e| Error::Parse(e.to_string()))?;
    if count == 0 {
        return Err(Error::InvalidInput(
            "ODF repetition must be positive".into(),
        ));
    }
    Ok(count)
}

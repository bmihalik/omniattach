use crate::{BlockKind, Document, Result, Table};

/// CSV null policy is explicit; an empty null marker collapses missing and empty.
#[derive(Debug, Clone)]
pub struct CsvOptions {
    pub delimiter: u8,
    pub include_headers: bool,
    pub null: String,
    /// Optional spreadsheet-safe presentation; canonical values are unchanged.
    pub escape_formulas: bool,
}
impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            include_headers: true,
            null: String::new(),
            escape_formulas: false,
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '!' | '|' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}
fn table_cell(text: &str) -> String {
    escape(text).replace('\n', "<br>").replace('\r', "")
}

impl Table {
    pub fn to_csv(&self) -> Result<String> {
        self.to_csv_with(&CsvOptions::default())
    }
    pub fn to_csv_with(&self, options: &CsvOptions) -> Result<String> {
        if matches!(options.delimiter, 0 | b'"' | b'\n' | b'\r') {
            return Err(crate::Error::InvalidInput("invalid CSV delimiter".into()));
        }
        let present = |s: &str| -> String {
            if options.escape_formulas && s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                format!("'{s}")
            } else {
                s.to_owned()
            }
        };
        let mut writer = csv::WriterBuilder::new()
            .delimiter(options.delimiter)
            .from_writer(Vec::new());
        if options.include_headers {
            writer.write_record(self.columns.iter().map(|s| present(s)))?;
        }
        for row in &self.rows {
            writer.write_record((0..self.columns.len()).map(|i| {
                present(
                    row.get(i)
                        .and_then(|c| c.raw.as_deref())
                        .unwrap_or(&options.null),
                )
            }))?;
        }
        let bytes = writer
            .into_inner()
            .map_err(|e| crate::Error::Io(e.into_error()))?;
        String::from_utf8(bytes).map_err(|e| crate::Error::Parse(e.to_string()))
    }
    pub fn to_markdown(&self) -> String {
        if self.columns.is_empty() {
            return String::new();
        }
        let row = |items: Vec<String>| format!("| {} |\n", items.join(" | "));
        let mut out = row(self.columns.iter().map(|s| table_cell(s)).collect());
        out.push_str(&row(self.columns.iter().map(|_| "---".into()).collect()));
        for cells in &self.rows {
            out.push_str(&row((0..self.columns.len())
                .map(|i| table_cell(cells.get(i).map(|c| c.display()).unwrap_or("")))
                .collect()));
        }
        out
    }
}
impl Document {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
    pub fn to_text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| match &b.kind {
                BlockKind::Table { table_id } => {
                    self.tables
                        .iter()
                        .find(|t| t.id == *table_id)
                        .map(|t| {
                            let mut rows = Vec::new();
                            if t.has_header {
                                rows.push(t.columns.join("\t"));
                            }
                            rows.extend(t.rows.iter().map(|r| {
                                r.iter().map(|c| c.display()).collect::<Vec<_>>().join("\t")
                            }));
                            rows.join("\n")
                        })
                        .unwrap_or_default()
                }
                BlockKind::ThematicBreak => "---".into(),
                _ => b.text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
    /// Semantic projection, not a byte-preserving Markdown roundtrip.
    pub fn to_markdown(&self) -> String {
        let mut out = Vec::new();
        for b in &self.blocks {
            out.push(match &b.kind {
                BlockKind::Heading { level } => format!(
                    "{} {}",
                    "#".repeat((*level).clamp(1, 6) as usize),
                    escape(&b.text)
                ),
                BlockKind::Code { language } => {
                    let run = b.text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
                    let fence = "`".repeat(3.max(run + 1));
                    let language = language
                        .as_deref()
                        .unwrap_or("")
                        .replace(['`', '\n', '\r'], "");
                    format!(
                        "{fence}{language}\n{}\n{fence}",
                        b.text.trim_end_matches('\n')
                    )
                }
                BlockKind::ListItem { ordered } => format!(
                    "{} {}",
                    if *ordered { "1." } else { "-" },
                    escape(&b.text).replace('\n', "\n  ")
                ),
                BlockKind::Quote => b
                    .text
                    .lines()
                    .map(|s| format!("> {}", escape(s)))
                    .collect::<Vec<_>>()
                    .join("\n"),
                BlockKind::ThematicBreak => "---".into(),
                BlockKind::Table { table_id } => self
                    .tables
                    .iter()
                    .find(|t| t.id == *table_id)
                    .map(Table::to_markdown)
                    .unwrap_or_default(),
                _ => escape(&b.text),
            });
        }
        if out.is_empty() {
            String::new()
        } else {
            out.join("\n\n") + "\n"
        }
    }
}

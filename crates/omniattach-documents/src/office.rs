use crate::{BlockKind, Cell, Document, Error, Options, Result, SourceSpan, Table};
use crate::{
    package::{Package, resolve},
    xml,
};
use roxmltree::Node;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
fn is(node: Node<'_, '_>, uri: &str, name: &str) -> bool {
    node.has_tag_name((uri, name))
}
fn ancestor(node: Node<'_, '_>, uri: &str, name: &str) -> bool {
    node.ancestors().skip(1).any(|n| is(n, uri, name))
}
fn run_text(node: Node<'_, '_>, uri: &str) -> String {
    let mut out = String::new();
    for n in node.descendants().filter(|n| n.is_element()) {
        if is(n, uri, "t") {
            out.push_str(&xml::text(n));
        } else if is(n, uri, "tab") {
            out.push('\t');
        } else if is(n, uri, "br") || is(n, uri, "cr") {
            out.push('\n');
        }
    }
    out
}
fn odf_text(node: Node<'_, '_>) -> String {
    let mut out = String::new();
    for n in node.descendants() {
        if n.is_text() {
            out.push_str(n.text().unwrap_or(""));
        } else if is(n, TEXT, "tab") {
            out.push('\t');
        } else if is(n, TEXT, "line-break") {
            out.push('\n');
        } else if is(n, TEXT, "s") {
            out.push(' ');
        }
    }
    out
}
fn new_table(
    name: Option<String>,
    source: SourceSpan,
    rows: Vec<Vec<Cell>>,
    options: &Options,
) -> Result<Table> {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if rows.len().saturating_mul(width) > options.limits.cells {
        return Err(Error::LimitExceeded("padded Office cells".into()));
    }
    let mut table = Table {
        id: String::new(),
        name,
        columns: (1..=width).map(|i| format!("column_{i}")).collect(),
        has_header: false,
        rows,
        source,
    };
    for row in &mut table.rows {
        row.resize(width, Cell::default());
    }
    Ok(table)
}
pub(crate) fn docx(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    let mut pkg = Package::open(bytes, options)?;
    let input = pkg.text("word/document.xml")?;
    let tree = xml::parse(&input, options)?;
    let mut cells = 0usize;
    for node in tree.descendants().filter(|n| n.is_element()) {
        if is(node, W, "tbl") && !ancestor(node, W, "tbl") {
            let mut rows = Vec::new();
            for (ri, row) in node.children().filter(|n| is(*n, W, "tr")).enumerate() {
                let mut values = Vec::new();
                for (ci, cell) in row.children().filter(|n| is(*n, W, "tc")).enumerate() {
                    cells += 1;
                    if cells > options.limits.cells {
                        return Err(Error::LimitExceeded("DOCX cells".into()));
                    }
                    let mut value = Cell::text(
                        cell.descendants()
                            .filter(|n| is(*n, W, "p"))
                            .map(|n| run_text(n, W))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                    value.source = xml::span(cell, Some("word/document.xml"));
                    value.source.row = Some(ri as u32 + 1);
                    value.source.column = Some(ci as u32 + 1);
                    values.push(value);
                }
                rows.push(values);
            }
            doc.add_table(new_table(
                None,
                xml::span(node, Some("word/document.xml")),
                rows,
                options,
            )?);
        }
        if !is(node, W, "p") || ancestor(node, W, "tbl") {
            continue;
        }
        let value = run_text(node, W);
        if value.trim().is_empty() {
            continue;
        }
        let style = node
            .descendants()
            .find(|n| is(*n, W, "pStyle"))
            .and_then(|n| n.attribute((W, "val")))
            .unwrap_or("");
        let outline = node
            .descendants()
            .find(|n| is(*n, W, "outlineLvl"))
            .and_then(|n| n.attribute((W, "val")))
            .and_then(|v| v.parse::<u8>().ok())
            .map(|v| v.saturating_add(1));
        let level = outline.or_else(|| {
            style
                .to_ascii_lowercase()
                .strip_prefix("heading")
                .and_then(|s| s.parse::<u8>().ok())
        });
        let span = xml::span(node, Some("word/document.xml"));
        if let Some(level) = level {
            doc.heading(level.clamp(1, 6), value, span);
        } else {
            doc.paragraph(BlockKind::Paragraph, value, span);
        }
        if doc.blocks.len() > options.limits.blocks {
            return Err(Error::LimitExceeded("DOCX blocks".into()));
        }
    }
    doc.warn("DOCX_BASIC", "Main body text/tables only: inherited styles, numbering, revisions, headers/footers, merges, images and layout are not reconstructed");
    Ok(())
}
pub(crate) fn pptx(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    let mut pkg = Package::open(bytes, options)?;
    let presentation = pkg.text("ppt/presentation.xml")?;
    let relationships = pkg.text("ppt/_rels/presentation.xml.rels")?;
    let tree = xml::parse(&presentation, options)?;
    let rels = xml::parse(&relationships, options)?;
    let mut slide = 0u32;
    for item in tree.descendants().filter(|n| is(*n, P, "sldId")) {
        let id = item
            .attribute((R, "id"))
            .ok_or_else(|| Error::Parse("slide relationship id missing".into()))?;
        let rel = rels
            .descendants()
            .find(|n| is(*n, REL, "Relationship") && n.attribute("Id") == Some(id))
            .ok_or_else(|| Error::Parse("slide relationship missing".into()))?;
        if rel.attribute("TargetMode") == Some("External") {
            return Err(Error::InvalidInput("external slide relationship".into()));
        }
        let member = resolve(
            "ppt/presentation.xml",
            rel.attribute("Target")
                .ok_or_else(|| Error::Parse("slide target missing".into()))?,
        )?;
        let input = pkg.text(&member)?;
        let xml = xml::parse(&input, options)?;
        slide += 1;
        for node in xml.descendants().filter(|n| is(*n, A, "p")) {
            let value = run_text(node, A);
            if value.trim().is_empty() {
                continue;
            }
            let mut span = xml::span(node, Some(&member));
            span.slide = Some(slide);
            doc.paragraph(BlockKind::Paragraph, value, span);
            if doc.blocks.len() > options.limits.blocks {
                return Err(Error::LimitExceeded("PPTX blocks".into()));
            }
        }
    }
    doc.warn("PPTX_TEXT_ONLY", "Slide order follows presentation relationships; shape text only, not spatial reading order, notes, charts, images or previews");
    Ok(())
}
pub(crate) fn odf(doc: &mut Document, bytes: &[u8], slides: bool, options: &Options) -> Result<()> {
    let mut pkg = Package::open(bytes, options)?;
    let input = pkg.text("content.xml")?;
    let tree = xml::parse(&input, options)?;
    let pages: Vec<_> = tree
        .descendants()
        .filter(|n| is(*n, DRAW, "page"))
        .collect();
    let mut cells = 0usize;
    for node in tree.descendants().filter(|n| n.is_element()) {
        let mut span = xml::span(node, Some("content.xml"));
        if slides {
            span.slide = node
                .ancestors()
                .find(|n| is(*n, DRAW, "page"))
                .and_then(|p| pages.iter().position(|x| *x == p))
                .map(|n| n as u32 + 1);
        }
        if is(node, TABLE, "table") && !ancestor(node, TABLE, "table") {
            let mut rows = Vec::new();
            for row in node.descendants().filter(|n| is(*n, TABLE, "table-row")) {
                let repeat_rows = row
                    .attribute((TABLE, "number-rows-repeated"))
                    .unwrap_or("1")
                    .parse::<usize>()
                    .map_err(|e| Error::Parse(e.to_string()))?;
                if repeat_rows == 0 {
                    return Err(Error::InvalidInput(
                        "ODF row repetition must be positive".into(),
                    ));
                }
                let mut values = Vec::new();
                for cell in row
                    .children()
                    .filter(|n| is(*n, TABLE, "table-cell") || is(*n, TABLE, "covered-table-cell"))
                {
                    let repeat = cell
                        .attribute((TABLE, "number-columns-repeated"))
                        .unwrap_or("1")
                        .parse::<usize>()
                        .map_err(|e| Error::Parse(e.to_string()))?;
                    if repeat == 0 {
                        return Err(Error::InvalidInput(
                            "ODF column repetition must be positive".into(),
                        ));
                    }
                    if values.len().saturating_add(repeat) > options.limits.cells {
                        return Err(Error::LimitExceeded("ODF repeated cells".into()));
                    }
                    let mut value = Cell::text(
                        cell.descendants()
                            .filter(|n| is(*n, TEXT, "p"))
                            .map(odf_text)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                    value.source = xml::span(cell, Some("content.xml"));
                    value.source.slide = span.slide;
                    values.extend(std::iter::repeat_n(value, repeat));
                }
                cells = cells.saturating_add(values.len().saturating_mul(repeat_rows));
                if cells > options.limits.cells
                    || rows.len().saturating_add(repeat_rows) > options.limits.cells
                {
                    return Err(Error::LimitExceeded("ODF repeated rows/cells".into()));
                }
                rows.extend(std::iter::repeat_n(values, repeat_rows));
            }
            doc.add_table(new_table(
                node.attribute((TABLE, "name")).map(str::to_owned),
                span,
                rows,
                options,
            )?);
        } else if (is(node, TEXT, "p") || is(node, TEXT, "h")) && !ancestor(node, TABLE, "table") {
            let value = odf_text(node);
            if value.trim().is_empty() {
                continue;
            }
            if is(node, TEXT, "h") {
                let level = node
                    .attribute((TEXT, "outline-level"))
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(1);
                doc.heading(level.clamp(1, 6), value, span);
            } else {
                doc.paragraph(BlockKind::Paragraph, value, span);
            }
        }
        if doc.blocks.len() > options.limits.blocks {
            return Err(Error::LimitExceeded("ODF blocks".into()));
        }
    }
    doc.warn("ODF_BASIC", "Text/tables only: lists become paragraphs, repeated text:s counts collapse to one space; merges, styles, images, notes and page layout are not reconstructed");
    Ok(())
}
pub(crate) fn epub(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    const CONTAINER: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
    const OPF: &str = "http://www.idpf.org/2007/opf";
    let mut pkg = Package::open(bytes, options)?;
    let container = pkg.text("META-INF/container.xml")?;
    let tree = xml::parse(&container, options)?;
    let path = tree
        .descendants()
        .find(|n| is(*n, CONTAINER, "rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| Error::Parse("EPUB rootfile missing".into()))?;
    let path = resolve("container.xml", path)?;
    let input = pkg.text(&path)?;
    let tree = xml::parse(&input, options)?;
    doc.title = tree
        .descendants()
        .find(|n| n.has_tag_name(("http://purl.org/dc/elements/1.1/", "title")))
        .map(xml::text)
        .or(doc.title.clone());
    for item in tree.descendants().filter(|n| is(*n, OPF, "itemref")) {
        let id = item
            .attribute("idref")
            .ok_or_else(|| Error::Parse("EPUB idref missing".into()))?;
        let entry = tree
            .descendants()
            .find(|n| is(*n, OPF, "item") && n.attribute("id") == Some(id))
            .ok_or_else(|| Error::Parse("EPUB manifest entry missing".into()))?;
        let member = resolve(
            &path,
            entry
                .attribute("href")
                .ok_or_else(|| Error::Parse("EPUB href missing".into()))?,
        )?;
        let html = pkg.text(&member)?;
        crate::html::extract(doc, &html, Some(&member), options)?;
    }
    doc.warn(
        "EPUB_TEXT_ONLY",
        "EPUB spine order preserved; images, navigation and DRM are not supported",
    );
    Ok(())
}

use crate::{BlockKind, Cell, Document, Error, Options, Result, SourceSpan, Table};
use crate::{
    package::{Package, resolve},
    xml,
};
use roxmltree::Node;
use std::collections::BTreeMap;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const XLINK: &str = "http://www.w3.org/1999/xlink";
/// Upper bound for one <text:s text:c="N"/> run, so a hostile count cannot blow up memory.
const MAX_SPACE_RUN: usize = 1000;
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
            let count = n
                .attribute((TEXT, "c"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .min(MAX_SPACE_RUN);
            out.extend(std::iter::repeat_n(' ', count));
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

/// Raster image types only. SVG, EMF and WMF are deliberately not extracted.
fn raster_type(member: &str) -> Option<&'static str> {
    let ext = member.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    })
}

fn media_members(pkg: &Package<'_>, prefix: &str) -> Vec<String> {
    pkg.member_names()
        .into_iter()
        .filter(|n| n.starts_with(prefix) && !n.ends_with('/'))
        .collect()
}

/// Copies one embedded image into the document, enforcing the archive byte and entry budgets.
fn attach(
    doc: &mut Document,
    pkg: &mut Package<'_>,
    member: &str,
    slide: Option<u32>,
    options: &Options,
) -> Result<()> {
    let Some(media_type) = raster_type(member) else {
        doc.warn(
            "ASSET_SKIPPED",
            format!("{member}: not a supported raster image type"),
        );
        return Ok(());
    };
    let bytes = pkg.bytes(member)?;
    let total = doc
        .assets
        .iter()
        .map(|a| a.byte_length as u64)
        .sum::<u64>()
        .saturating_add(bytes.len() as u64);
    if total > options.limits.archive_bytes || doc.assets.len() >= options.limits.archive_entries {
        return Err(Error::LimitExceeded("embedded asset bytes or count".into()));
    }
    let filename = member.rsplit('/').next().unwrap_or(member).to_owned();
    let source = SourceSpan {
        member: Some(member.into()),
        slide,
        ..Default::default()
    };
    doc.asset(filename, media_type, bytes, source);
    Ok(())
}

/// Internal targets of relationships whose Type ends with `suffix`, resolved against `member`.
fn rel_targets(
    pkg: &mut Package<'_>,
    member: &str,
    suffix: &str,
    options: &Options,
) -> Result<Vec<String>> {
    let (dir, file) = member.rsplit_once('/').unwrap_or(("", member));
    let rels = format!("{dir}/_rels/{file}.rels");
    if !pkg.contains(&rels) {
        return Ok(Vec::new());
    }
    let text = pkg.text(&rels)?;
    let tree = xml::parse(&text, options)?;
    let mut out = Vec::new();
    for rel in tree.descendants().filter(|n| is(*n, REL, "Relationship")) {
        let matches = rel.attribute("Type").is_some_and(|t| t.ends_with(suffix));
        if matches
            && rel.attribute("TargetMode") != Some("External")
            && let Some(target) = rel.attribute("Target")
        {
            out.push(resolve(member, target)?);
        }
    }
    Ok(out)
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
    for name in media_members(&pkg, "word/media/") {
        attach(doc, &mut pkg, &name, None, options)?;
    }
    doc.warn("DOCX_BASIC", "Main body text/tables and embedded raster images only: inherited styles, numbering, revisions, headers/footers, merges and layout are not reconstructed");
    Ok(())
}

/// Shape position as (y, x) from the first a:off, so shapes read top to bottom, then left to right.
fn shape_position(shape: Node<'_, '_>) -> (i64, i64) {
    shape
        .descendants()
        .find(|n| is(*n, A, "off"))
        .map(|off| {
            let x = off.attribute("x").and_then(|v| v.parse().ok()).unwrap_or(0);
            let y = off.attribute("y").and_then(|v| v.parse().ok()).unwrap_or(0);
            (y, x)
        })
        .unwrap_or((0, 0))
}

pub(crate) fn pptx(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    let mut pkg = Package::open(bytes, options)?;
    let presentation = pkg.text("ppt/presentation.xml")?;
    let relationships = pkg.text("ppt/_rels/presentation.xml.rels")?;
    let tree = xml::parse(&presentation, options)?;
    let rels = xml::parse(&relationships, options)?;
    let mut slide = 0u32;
    let mut cells = 0usize;
    let mut owners: BTreeMap<String, u32> = BTreeMap::new();
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
        for target in rel_targets(&mut pkg, &member, "/image", options)? {
            owners.entry(target).or_insert(slide);
        }
        let mut shapes: Vec<Node<'_, '_>> = match xml.descendants().find(|n| is(*n, P, "spTree")) {
            Some(tree) => tree
                .children()
                .filter(|n| is(*n, P, "sp") || is(*n, P, "graphicFrame") || is(*n, P, "grpSp"))
                .collect(),
            None => vec![xml.root_element()],
        };
        shapes.sort_by_key(|n| shape_position(*n));
        for shape in shapes {
            for tbl in shape.descendants().filter(|n| is(*n, A, "tbl")) {
                let mut rows = Vec::new();
                for (ri, row) in tbl.children().filter(|n| is(*n, A, "tr")).enumerate() {
                    let mut values = Vec::new();
                    for (ci, cell) in row.children().filter(|n| is(*n, A, "tc")).enumerate() {
                        cells += 1;
                        if cells > options.limits.cells {
                            return Err(Error::LimitExceeded("PPTX cells".into()));
                        }
                        let mut value = Cell::text(
                            cell.descendants()
                                .filter(|n| is(*n, A, "p"))
                                .map(|n| run_text(n, A))
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                        value.source = xml::span(cell, Some(&member));
                        value.source.slide = Some(slide);
                        value.source.row = Some(ri as u32 + 1);
                        value.source.column = Some(ci as u32 + 1);
                        values.push(value);
                    }
                    rows.push(values);
                }
                let mut source = xml::span(tbl, Some(&member));
                source.slide = Some(slide);
                doc.add_table(new_table(None, source, rows, options)?);
            }
            for node in shape
                .descendants()
                .filter(|n| is(*n, A, "p") && !ancestor(*n, A, "tbl"))
            {
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
        for notes in rel_targets(&mut pkg, &member, "/notesSlide", options)? {
            let input = pkg.text(&notes)?;
            let tree = xml::parse(&input, options)?;
            for body in tree.descendants().filter(|n| {
                is(*n, P, "sp")
                    && n.descendants()
                        .any(|d| is(d, P, "ph") && d.attribute("type") == Some("body"))
            }) {
                for node in body.descendants().filter(|n| is(*n, A, "p")) {
                    let value = run_text(node, A);
                    if value.trim().is_empty() {
                        continue;
                    }
                    let mut span = xml::span(node, Some(&notes));
                    span.slide = Some(slide);
                    span.path = Some("notes".into());
                    doc.paragraph(BlockKind::Paragraph, value, span);
                    if doc.blocks.len() > options.limits.blocks {
                        return Err(Error::LimitExceeded("PPTX blocks".into()));
                    }
                }
            }
        }
    }
    for name in media_members(&pkg, "ppt/media/") {
        let owner = owners.get(&name).copied();
        attach(doc, &mut pkg, &name, owner, options)?;
    }
    doc.warn("PPTX_TEXT_ONLY", "Slides read in shape-position order (top to bottom, then left to right), not true reading order: shape text, tables, speaker notes and embedded raster images only; charts, SmartArt and previews are not extracted");
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
    let mut owners: BTreeMap<String, Option<u32>> = BTreeMap::new();
    for node in tree.descendants().filter(|n| n.is_element()) {
        let mut span = xml::span(node, Some("content.xml"));
        if slides {
            span.slide = node
                .ancestors()
                .find(|n| is(*n, DRAW, "page"))
                .and_then(|p| pages.iter().position(|x| *x == p))
                .map(|n| n as u32 + 1);
        }
        if let Some(href) = node.attribute((XLINK, "href")).filter(|_| is(node, DRAW, "image")) {
            owners
                .entry(resolve("content.xml", href)?)
                .or_insert(span.slide);
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
            } else if ancestor(node, TEXT, "list-item") {
                let depth = node.ancestors().filter(|n| is(*n, TEXT, "list")).count();
                span.path = Some(format!("list-level:{depth}"));
                doc.paragraph(BlockKind::ListItem { ordered: false }, value, span);
            } else {
                doc.paragraph(BlockKind::Paragraph, value, span);
            }
        }
        if doc.blocks.len() > options.limits.blocks {
            return Err(Error::LimitExceeded("ODF blocks".into()));
        }
    }
    for name in media_members(&pkg, "Pictures/") {
        let owner = owners.get(&name).copied().flatten();
        attach(doc, &mut pkg, &name, owner, options)?;
    }
    doc.warn("ODF_BASIC", "Text/tables and embedded raster images only: list items are marked with their nesting level in the source path but not as a nested tree; merges, styles, notes and page layout are not reconstructed");
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
    for item in tree.descendants().filter(|n| is(*n, OPF, "item")) {
        if !item.attribute("media-type").unwrap_or("").starts_with("image/") {
            continue;
        }
        let href = item
            .attribute("href")
            .ok_or_else(|| Error::Parse("EPUB href missing".into()))?;
        let member = resolve(&path, href)?;
        attach(doc, &mut pkg, &member, None, options)?;
    }
    doc.warn(
        "EPUB_TEXT_ONLY",
        "EPUB spine order preserved; embedded raster images are extracted as assets; navigation and DRM are not supported",
    );
    Ok(())
}

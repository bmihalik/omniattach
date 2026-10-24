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
/// Maximum basedOn hops when resolving inherited heading styles, so cycles cannot loop forever.
const MAX_STYLE_DEPTH: usize = 16;
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

/// Paragraph style facts needed to resolve inherited headings.
struct StyleInfo {
    name: String,
    based_on: Option<String>,
    outline: Option<u8>,
}

fn docx_styles(pkg: &mut Package<'_>, options: &Options) -> Result<BTreeMap<String, StyleInfo>> {
    let mut out = BTreeMap::new();
    if !pkg.contains("word/styles.xml") {
        return Ok(out);
    }
    let text = pkg.text("word/styles.xml")?;
    let tree = xml::parse(&text, options)?;
    for style in tree.descendants().filter(|n| is(*n, W, "style")) {
        let Some(id) = style.attribute((W, "styleId")) else {
            continue;
        };
        let name = style
            .children()
            .find(|n| is(*n, W, "name"))
            .and_then(|n| n.attribute((W, "val")))
            .unwrap_or("")
            .to_ascii_lowercase();
        let based_on = style
            .children()
            .find(|n| is(*n, W, "basedOn"))
            .and_then(|n| n.attribute((W, "val")))
            .map(str::to_owned);
        let outline = style
            .descendants()
            .find(|n| is(*n, W, "outlineLvl"))
            .and_then(|n| n.attribute((W, "val")))
            .and_then(|v| v.parse::<u8>().ok())
            .map(|v| v.saturating_add(1));
        out.insert(
            id.to_owned(),
            StyleInfo {
                name,
                based_on,
                outline,
            },
        );
    }
    Ok(out)
}

/// Heading level from a style, or from the nearest ancestor style that defines one.
fn style_heading_level(styles: &BTreeMap<String, StyleInfo>, id: &str) -> Option<u8> {
    let mut current = Some(id.to_owned());
    for _ in 0..MAX_STYLE_DEPTH {
        let Some(style) = current.as_deref().and_then(|c| styles.get(c)) else {
            break;
        };
        if let Some(rest) = style.name.strip_prefix("heading ")
            && let Ok(level) = rest.trim().parse::<u8>()
        {
            return Some(level);
        }
        if let Some(level) = style.outline {
            return Some(level);
        }
        current = style.based_on.clone();
    }
    id.to_ascii_lowercase()
        .strip_prefix("heading")
        .and_then(|s| s.parse::<u8>().ok())
}

/// One list level from word/numbering.xml.
#[derive(Clone)]
struct NumLevel {
    format: String,
    start: u32,
    text: String,
}

/// numId -> (ilvl -> level definition).
fn docx_numbering(
    pkg: &mut Package<'_>,
    options: &Options,
) -> Result<BTreeMap<String, BTreeMap<u8, NumLevel>>> {
    let mut out = BTreeMap::new();
    if !pkg.contains("word/numbering.xml") {
        return Ok(out);
    }
    let text = pkg.text("word/numbering.xml")?;
    let tree = xml::parse(&text, options)?;
    let mut abstracts: BTreeMap<String, BTreeMap<u8, NumLevel>> = BTreeMap::new();
    for an in tree.descendants().filter(|n| is(*n, W, "abstractNum")) {
        let Some(id) = an.attribute((W, "abstractNumId")) else {
            continue;
        };
        let mut levels = BTreeMap::new();
        for lvl in an.children().filter(|n| is(*n, W, "lvl")) {
            let Some(ilvl) = lvl
                .attribute((W, "ilvl"))
                .and_then(|v| v.parse::<u8>().ok())
            else {
                continue;
            };
            let child_val = |name: &str| {
                lvl.children()
                    .find(|n| is(*n, W, name))
                    .and_then(|n| n.attribute((W, "val")))
            };
            let format = child_val("numFmt").unwrap_or("decimal").to_owned();
            let start = child_val("start")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1);
            let text = child_val("lvlText").unwrap_or("").to_owned();
            levels.insert(
                ilvl,
                NumLevel {
                    format,
                    start,
                    text,
                },
            );
        }
        abstracts.insert(id.to_owned(), levels);
    }
    for num in tree.descendants().filter(|n| is(*n, W, "num")) {
        let Some(id) = num.attribute((W, "numId")) else {
            continue;
        };
        let Some(abs) = num
            .children()
            .find(|n| is(*n, W, "abstractNumId"))
            .and_then(|n| n.attribute((W, "val")))
        else {
            continue;
        };
        if let Some(levels) = abstracts.get(abs) {
            out.insert(id.to_owned(), levels.clone());
        }
    }
    Ok(out)
}

/// Formats a counter in the given numFmt. Unknown formats fall back to decimal.
fn format_number(n: u32, fmt: &str) -> String {
    match fmt {
        "lowerLetter" | "upperLetter" => {
            let mut s = String::new();
            let mut x = n;
            while x > 0 {
                x -= 1;
                s.insert(0, char::from(b'a' + (x % 26) as u8));
                x /= 26;
            }
            if fmt == "upperLetter" {
                s.to_ascii_uppercase()
            } else {
                s
            }
        }
        "lowerRoman" | "upperRoman" => {
            const SYMBOLS: [(u32, &str); 13] = [
                (1000, "m"),
                (900, "cm"),
                (500, "d"),
                (400, "cd"),
                (100, "c"),
                (90, "xc"),
                (50, "l"),
                (40, "xl"),
                (10, "x"),
                (9, "ix"),
                (5, "v"),
                (4, "iv"),
                (1, "i"),
            ];
            let mut s = String::new();
            let mut rest = n;
            for (value, symbol) in SYMBOLS {
                while rest >= value {
                    s.push_str(symbol);
                    rest -= value;
                }
            }
            if fmt == "upperRoman" {
                s.to_ascii_uppercase()
            } else {
                s
            }
        }
        _ => n.to_string(),
    }
}

/// Renders a level's lvlText. %1..%9 are replaced by the current counter of that level.
fn render_marker(
    counters: &BTreeMap<(String, u8), u32>,
    num_id: &str,
    levels: &BTreeMap<u8, NumLevel>,
    ilvl: u8,
) -> String {
    let Some(def) = levels.get(&ilvl) else {
        return String::new();
    };
    let mut out = String::new();
    let mut rest = def.text.as_str();
    while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        match after.chars().next().and_then(|c| c.to_digit(10)) {
            Some(d) if (1..=9).contains(&d) => {
                let level = (d - 1) as u8;
                let fmt = levels.get(&level).map_or("decimal", |l| l.format.as_str());
                let value = counters
                    .get(&(num_id.to_owned(), level))
                    .copied()
                    .unwrap_or(1);
                out.push_str(&format_number(value, fmt));
                rest = &after[1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Footnotes, headers and footers, appended after the body in source-part order.
fn docx_note_members(pkg: &Package<'_>) -> Vec<String> {
    pkg.member_names()
        .into_iter()
        .map(|n| n.to_string())
        .filter(|n| {
            n == "word/footnotes.xml"
                || ((n.starts_with("word/header") || n.starts_with("word/footer"))
                    && n.ends_with(".xml"))
        })
        .collect()
}

fn docx_note_part(
    doc: &mut Document,
    pkg: &mut Package<'_>,
    member: &str,
    options: &Options,
) -> Result<()> {
    let text = pkg.text(member)?;
    let tree = xml::parse(&text, options)?;
    let footnote = member.ends_with("footnotes.xml");
    let kind = if footnote {
        "footnote"
    } else if member.starts_with("word/header") {
        "header"
    } else {
        "footer"
    };
    let scopes: Vec<Node<'_, '_>> = if footnote {
        tree.descendants()
            .filter(|n| is(*n, W, "footnote"))
            .collect()
    } else {
        vec![tree.root_element()]
    };
    for scope in scopes {
        let path = if footnote {
            let id = scope
                .attribute((W, "id"))
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            if id < 1 {
                continue;
            }
            format!("footnote:{id}")
        } else {
            kind.to_owned()
        };
        for p in scope.descendants().filter(|n| is(*n, W, "p")) {
            let value = run_text(p, W);
            if value.trim().is_empty() {
                continue;
            }
            let mut span = xml::span(p, Some(member));
            span.path = Some(path.clone());
            doc.paragraph(BlockKind::Paragraph, value, span);
            if doc.blocks.len() > options.limits.blocks {
                return Err(Error::LimitExceeded("DOCX blocks".into()));
            }
        }
    }
    Ok(())
}

pub(crate) fn docx(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    let mut pkg = Package::open(bytes, options)?;
    let styles = docx_styles(&mut pkg, options)?;
    let numbering = docx_numbering(&mut pkg, options)?;
    let input = pkg.text("word/document.xml")?;
    let tree = xml::parse(&input, options)?;
    let mut cells = 0usize;
    // Current counter per (numId, level), advanced in document order.
    let mut counters: BTreeMap<(String, u8), u32> = BTreeMap::new();
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
        let direct_outline = node
            .descendants()
            .find(|n| is(*n, W, "outlineLvl"))
            .and_then(|n| n.attribute((W, "val")))
            .and_then(|v| v.parse::<u8>().ok())
            .map(|v| v.saturating_add(1));
        let level = direct_outline.or_else(|| style_heading_level(&styles, style));
        let span = xml::span(node, Some("word/document.xml"));
        if let Some(level) = level {
            doc.heading(level.clamp(1, 6), value, span);
        } else if let Some(num_pr) = node.descendants().find(|n| is(*n, W, "numPr")) {
            let ilvl_text = num_pr
                .children()
                .find(|n| is(*n, W, "ilvl"))
                .and_then(|n| n.attribute((W, "val")))
                .unwrap_or("0");
            let num_id = num_pr
                .children()
                .find(|n| is(*n, W, "numId"))
                .and_then(|n| n.attribute((W, "val")))
                .unwrap_or("");
            let ilvl = ilvl_text.parse::<u8>().unwrap_or(0);
            let empty: BTreeMap<u8, NumLevel> = BTreeMap::new();
            let levels = numbering.get(num_id).unwrap_or(&empty);
            let ordered = levels
                .get(&ilvl)
                .is_some_and(|d| !matches!(d.format.as_str(), "bullet" | "none"));
            let mut marker = String::new();
            if let Some(def) = levels.get(&ilvl) {
                let c = counters
                    .entry((num_id.to_owned(), ilvl))
                    .or_insert(def.start.saturating_sub(1));
                *c += 1;
                counters.retain(|(n, l), _| !(n.as_str() == num_id && *l > ilvl));
                if ordered {
                    marker = render_marker(&counters, num_id, levels, ilvl);
                }
            }
            let mut span = span;
            span.path = Some(if marker.is_empty() {
                format!("list-level:{ilvl}")
            } else {
                format!("list-level:{ilvl};marker:{marker}")
            });
            doc.paragraph(BlockKind::ListItem { ordered }, value, span);
        } else {
            doc.paragraph(BlockKind::Paragraph, value, span);
        }
        if doc.blocks.len() > options.limits.blocks {
            return Err(Error::LimitExceeded("DOCX blocks".into()));
        }
    }
    for member in docx_note_members(&pkg) {
        docx_note_part(doc, &mut pkg, &member, options)?;
    }
    for name in media_members(&pkg, "word/media/") {
        attach(doc, &mut pkg, &name, None, options)?;
    }
    doc.warn("DOCX_BASIC", "Body text, tables, headings from styles and outline levels, list items (bullet or numbered; decimal, letter and roman formats, with the rendered marker in source.path), footnotes, headers, footers and embedded raster images only; revisions, text boxes and page layout are not reconstructed");
    Ok(())
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
        // Document order of spTree children is the slide's reading order (selection pane order).
        let shapes: Vec<Node<'_, '_>> = match xml.descendants().find(|n| is(*n, P, "spTree")) {
            Some(tree) => tree
                .children()
                .filter(|n| is(*n, P, "sp") || is(*n, P, "graphicFrame") || is(*n, P, "grpSp"))
                .collect(),
            None => vec![xml.root_element()],
        };
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
    doc.warn("PPTX_TEXT_ONLY", "Slides read in shape order (p:spTree document order, the order PowerPoint uses for its selection pane and accessibility reading order); shape text, tables, speaker notes and embedded raster images only; charts, SmartArt and previews are not extracted");
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
    // Open list items as (list depth, item node, block id), outermost first.
    let mut list_stack = Vec::new();
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
            list_stack.clear();
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
                list_stack.clear();
                let level = node
                    .attribute((TEXT, "outline-level"))
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(1);
                doc.heading(level.clamp(1, 6), value, span);
            } else if ancestor(node, TEXT, "list-item") {
                let depth = node.ancestors().filter(|n| is(*n, TEXT, "list")).count();
                let item = node
                    .ancestors()
                    .find(|n| is(*n, TEXT, "list-item"))
                    .map(|n| n.id());
                span.path = Some(format!("list-level:{depth}"));
                let continuation = list_stack
                    .last()
                    .is_some_and(|(_, top_item, _): &(usize, _, String)| *top_item == item);
                if !continuation {
                    while list_stack
                        .last()
                        .is_some_and(|(d, _, _): &(usize, _, String)| *d >= depth)
                    {
                        list_stack.pop();
                    }
                }
                let parent: Option<String> = list_stack.last().map(|(_, _, id)| id.clone());
                doc.paragraph(BlockKind::ListItem { ordered: false }, value, span);
                if !continuation {
                    let heading = doc
                        .blocks
                        .iter()
                        .rev()
                        .skip(1)
                        .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
                        .map(|b| b.id.clone());
                    let id = doc.blocks.last().map(|b| b.id.clone()).unwrap_or_default();
                    if let Some(block) = doc.blocks.last_mut() {
                        block.parent_id = parent.or(heading);
                    }
                    list_stack.push((depth, item, id));
                }
            } else {
                list_stack.clear();
                doc.paragraph(BlockKind::Paragraph, value, span);
                let heading = doc
                    .blocks
                    .iter()
                    .rev()
                    .skip(1)
                    .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
                    .map(|b| b.id.clone());
                if let Some(block) = doc.blocks.last_mut() {
                    block.parent_id = heading;
                }
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
    doc.warn("ODF_BASIC", "Text/tables and embedded raster images only: nested list items link to their enclosing item through parent_id and carry their list depth in the source path; merges, styles, notes and page layout are not reconstructed");
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

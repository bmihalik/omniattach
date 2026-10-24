//! Restricted selectors: absolute record paths, relative element paths, `@attr`, `.`.
//! No XPath predicates, wildcards, descendant axis, or automatic row explosion.
use crate::{Cell, Document, Error, Options, Result, SourceSpan, Table};
use roxmltree::{Document as Xml, Node};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlTableSpec {
    pub record_path: String,
    pub namespaces: BTreeMap<String, String>,
    pub columns: Vec<XmlColumn>,
}
impl XmlTableSpec {
    pub fn new(record_path: impl Into<String>, columns: Vec<XmlColumn>) -> Self {
        Self {
            record_path: record_path.into(),
            namespaces: BTreeMap::new(),
            columns,
        }
    }
    pub fn namespace(mut self, prefix: impl Into<String>, uri: impl Into<String>) -> Self {
        self.namespaces.insert(prefix.into(), uri.into());
        self
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlColumn {
    pub name: String,
    pub selector: String,
    pub repeated: Repeated,
}
impl XmlColumn {
    pub fn new(name: impl Into<String>, selector: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            selector: selector.into(),
            repeated: Repeated::Reject,
        }
    }
    pub fn repeated(mut self, repeated: Repeated) -> Self {
        self.repeated = repeated;
        self
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", content = "separator", rename_all = "snake_case")]
pub enum Repeated {
    Reject,
    Join(String),
    Json,
}

pub(crate) fn parse<'a>(text: &'a str, options: &Options) -> Result<Xml<'a>> {
    Xml::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: options.limits.xml_nodes,
            ..Default::default()
        },
    )
    .map_err(|e| Error::Parse(format!("XML: {e}")))
}
pub(crate) fn text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter_map(|n| if n.is_text() { n.text() } else { None })
        .collect()
}
pub(crate) fn span(node: Node<'_, '_>, member: Option<&str>) -> SourceSpan {
    let range = node.range();
    let pos = node.document().text_pos_at(range.start);
    let end = node.document().text_pos_at(range.end);
    SourceSpan {
        member: member.map(str::to_owned),
        decoded_byte_start: Some(range.start),
        decoded_byte_end: Some(range.end),
        line_start: Some(pos.row),
        line_end: Some(end.row),
        ..Default::default()
    }
}
fn expanded<'a>(
    name: &'a str,
    ns: &'a BTreeMap<String, String>,
) -> Result<(Option<&'a str>, &'a str)> {
    if name.is_empty() || name.contains(['[', ']', '*', '(', ')', ' ']) || name == ".." {
        return Err(Error::InvalidInput(format!(
            "unsupported XML name/selector: {name}"
        )));
    }
    if let Some((prefix, local)) = name.split_once(':') {
        if local.is_empty() || local.contains(':') {
            return Err(Error::InvalidInput(format!("invalid XML QName: {name}")));
        }
        Ok((
            Some(
                ns.get(prefix)
                    .ok_or_else(|| {
                        Error::InvalidInput(format!("unbound namespace prefix: {prefix}"))
                    })?
                    .as_str(),
            ),
            local,
        ))
    } else {
        Ok((None, name))
    }
}
fn matches(node: Node<'_, '_>, name: &str, ns: &BTreeMap<String, String>) -> Result<bool> {
    let (uri, local) = expanded(name, ns)?;
    Ok(node.is_element() && node.tag_name().name() == local && node.tag_name().namespace() == uri)
}
fn children<'a, 'input>(
    nodes: Vec<Node<'a, 'input>>,
    part: &str,
    ns: &BTreeMap<String, String>,
) -> Result<Vec<Node<'a, 'input>>> {
    expanded(part, ns)?;
    let mut out = Vec::new();
    for node in nodes {
        for child in node.children().filter(|n| n.is_element()) {
            if matches(child, part, ns)? {
                out.push(child);
            }
        }
    }
    Ok(out)
}
fn select_values(
    record: Node<'_, '_>,
    selector: &str,
    ns: &BTreeMap<String, String>,
) -> Result<Vec<String>> {
    if selector == "." {
        return Ok(vec![text(record)]);
    }
    if selector.is_empty() || selector.starts_with('/') {
        return Err(Error::InvalidInput(
            "column selectors must be relative".into(),
        ));
    }
    let mut nodes = vec![record];
    let parts: Vec<_> = selector.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        if let Some(attr) = part.strip_prefix('@') {
            if index + 1 != parts.len() {
                return Err(Error::InvalidInput(
                    "attribute selector must be terminal".into(),
                ));
            }
            expanded(attr, ns)?;
        } else {
            expanded(part, ns)?;
        }
    }
    for (i, part) in parts.iter().enumerate() {
        if let Some(attr) = part.strip_prefix('@') {
            if i + 1 != parts.len() {
                return Err(Error::InvalidInput(
                    "attribute selector must be terminal".into(),
                ));
            }
            let (uri, local) = expanded(attr, ns)?;
            return Ok(nodes
                .iter()
                .filter_map(|n| {
                    n.attributes()
                        .find(|a| a.name() == local && a.namespace() == uri)
                        .map(|a| a.value())
                        .map(str::to_owned)
                })
                .collect());
        }
        nodes = children(nodes, part, ns)?;
    }
    Ok(nodes.into_iter().map(text).collect())
}
pub(crate) fn extract(
    doc: &mut Document,
    input: &str,
    spec: &XmlTableSpec,
    options: &Options,
) -> Result<()> {
    if !spec.record_path.starts_with('/')
        || spec.record_path.contains("//")
        || spec.columns.is_empty()
    {
        return Err(Error::InvalidInput(
            "XML mapping needs an absolute record path and columns".into(),
        ));
    }
    let xml = parse(input, options)?;
    // Validate even when the input has no matching records.
    for column in &spec.columns {
        select_values(xml.root_element(), &column.selector, &spec.namespaces)?;
    }
    let mut nodes = vec![xml.root()];
    for part in spec.record_path[1..].split('/') {
        nodes = children(nodes, part, &spec.namespaces)?;
    }
    if nodes.len().saturating_mul(spec.columns.len()) > options.limits.cells {
        return Err(Error::LimitExceeded("XML cells".into()));
    }
    let mut table = Table {
        id: String::new(),
        name: Some("XML records".into()),
        columns: spec.columns.iter().map(|c| c.name.clone()).collect(),
        has_header: true,
        rows: Vec::new(),
        source: SourceSpan::default(),
    };
    for (row, node) in nodes.into_iter().enumerate() {
        let mut cells = Vec::new();
        for column in &spec.columns {
            let values = select_values(node, &column.selector, &spec.namespaces)?;
            let value = match values.len() {
                0 => None,
                1 if !matches!(column.repeated, Repeated::Json) => Some(values[0].clone()),
                _ => match &column.repeated {
                    Repeated::Reject => {
                        return Err(Error::InvalidInput(format!(
                            "repeated values in column {}; choose Join or Json",
                            column.name
                        )));
                    }
                    Repeated::Join(separator) => Some(values.join(separator)),
                    Repeated::Json => Some(serde_json::to_string(&values)?),
                },
            };
            let mut cell = value.map(Cell::text).unwrap_or_default();
            cell.source = span(node, None);
            cell.source.path = Some(format!(
                "{}[{}]/{}",
                spec.record_path,
                row + 1,
                column.selector
            ));
            cells.push(cell);
        }
        table.rows.push(cells);
    }
    if table.rows.is_empty() {
        doc.warn("NO_RECORDS", "XML mapping matched no records");
    }
    doc.add_table(table);
    Ok(())
}

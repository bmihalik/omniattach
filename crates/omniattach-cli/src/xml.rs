use crate::{CliError, Result, Runtime, args::XmlArgs};
use quick_xml::{
    NsReader,
    events::Event,
    name::{QName, ResolveResult},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExpandedName {
    pub namespace: Option<String>,
    pub local: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct AttributeCount {
    pub name: ExpandedName,
    pub occurrences: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct XmlPath {
    /// Each element name is namespace URI + local name, not a prefix-dependent XPath string.
    pub elements: Vec<ExpandedName>,
    pub occurrences: u64,
    pub max_occurrences_per_parent: u64,
    pub text_events: u64,
    pub attributes: Vec<AttributeCount>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct XmlReport {
    pub schema: String,
    pub source: std::path::PathBuf,
    pub bytes: u64,
    pub elements: u64,
    pub max_depth: usize,
    pub paths: Vec<XmlPath>,
    pub warnings: Vec<String>,
}
#[derive(Default)]
struct Stats {
    occurrences: u64,
    max_per_parent: u64,
    text: u64,
    attributes: BTreeMap<ExpandedName, u64>,
}
struct Frame {
    index: Vec<ExpandedName>,
    children: BTreeMap<Vec<ExpandedName>, u64>,
}
fn invalid(error: impl std::fmt::Display) -> CliError {
    CliError::input(format!("XML: {error}"))
}
fn string(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes).map_err(invalid)?.to_owned())
}
fn resolve(reader: &NsReader<&[u8]>, name: QName<'_>, attribute: bool) -> Result<ExpandedName> {
    let (namespace, local) = if attribute {
        reader.resolver().resolve_attribute(name)
    } else {
        reader.resolver().resolve_element(name)
    };
    let namespace = match namespace {
        ResolveResult::Bound(uri) => Some(string(uri.as_ref())?),
        ResolveResult::Unbound => None,
        ResolveResult::Unknown(prefix) => {
            return Err(invalid(format!(
                "undeclared namespace prefix {}",
                String::from_utf8_lossy(&prefix)
            )));
        }
    };
    let local = string(local.as_ref())?;
    if local.is_empty() {
        return Err(invalid("empty local name"));
    }
    Ok(ExpandedName { namespace, local })
}
fn check(runtime: &Runtime, deadline: Instant) -> Result<()> {
    runtime.check()?;
    if Instant::now() >= deadline {
        Err(CliError::new("TIMEOUT", "XML probe deadline exceeded", 124))
    } else {
        Ok(())
    }
}
fn valid_char(c: char) -> bool {
    matches!(c,'\u{9}'|'\u{a}'|'\u{d}'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')
}
fn entities(text: &str) -> Result<()> {
    let expanded = quick_xml::escape::unescape(text).map_err(invalid)?;
    if expanded.chars().any(|c| !valid_char(c)) {
        return Err(invalid("invalid XML 1.0 character reference"));
    }
    Ok(())
}

fn read(args: &XmlArgs, runtime: &Runtime, deadline: Instant) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(&args.input)?;
    if !meta.is_file() {
        return Err(CliError::input("XML input is not a regular file"));
    }
    if meta.len() > args.max_input_bytes {
        return Err(CliError::limit("XML input bytes"));
    }
    let mut file = std::fs::File::open(&args.input)?;
    let mut buffer = [0; 65536];
    let mut bytes = Vec::new();
    loop {
        check(runtime, deadline)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if (bytes.len() as u64).saturating_add(count as u64) > args.max_input_bytes {
            return Err(CliError::limit("XML input bytes"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}
pub fn xml_probe(args: &XmlArgs, runtime: &Runtime) -> Result<XmlReport> {
    if args.max_depth == 0 || args.max_paths == 0 {
        return Err(CliError::usage(
            "XML max-depth and max-paths must be nonzero",
        ));
    }
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(args.timeout_ms))
        .ok_or_else(|| CliError::usage("XML timeout overflow"))?;
    let bytes = read(args, runtime, deadline)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        invalid("only UTF-8 XML is supported; transcode UTF-16/legacy XML explicitly")
    })?;
    // Reject disallowed XML 1.0 scalar characters before event parsing, without expanding entities.
    if text.chars().any(|c|!matches!(c,'\u{9}'|'\u{a}'|'\u{d}'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')){return Err(invalid("invalid XML 1.0 character"));}
    let mut reader = NsReader::from_str(text.trim_start_matches('\u{feff}'));
    reader.config_mut().check_comments = true;
    let mut stats: BTreeMap<Vec<ExpandedName>, Stats> = BTreeMap::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut roots = 0u64;
    let mut elements = 0u64;
    let mut max_depth = 0usize;
    let mut attributes_seen = 0usize;
    let mut declaration = false;
    loop {
        check(runtime, deadline)?;
        let event = reader.read_event().map_err(invalid)?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                let empty = matches!(event, Event::Empty(_));
                if stack.is_empty() {
                    roots += 1;
                    if roots > 1 {
                        return Err(invalid("multiple root elements"));
                    }
                }
                let depth = stack.len() + 1;
                if depth > args.max_depth {
                    return Err(CliError::limit("XML depth"));
                }
                max_depth = max_depth.max(depth);
                let name = resolve(&reader, element.name(), false)?;
                let mut path = stack.last().map(|f| f.index.clone()).unwrap_or_default();
                path.push(name);
                if !stats.contains_key(&path) && stats.len() >= args.max_paths {
                    return Err(CliError::limit("XML unique element paths"));
                }
                let siblings = if let Some(parent) = stack.last_mut() {
                    let n = parent.children.entry(path.clone()).or_default();
                    *n += 1;
                    *n
                } else {
                    1
                };
                let stat = stats.entry(path.clone()).or_default();
                stat.occurrences += 1;
                stat.max_per_parent = stat.max_per_parent.max(siblings);
                elements += 1;
                let mut names = std::collections::BTreeSet::new();
                for attribute in element.attributes() {
                    check(runtime, deadline)?;
                    let attribute = attribute.map_err(invalid)?;
                    attributes_seen = attributes_seen.saturating_add(1);
                    if attributes_seen > args.max_attributes {
                        return Err(CliError::limit("XML attributes"));
                    }
                    if attribute.value.contains(&b'<') {
                        return Err(invalid("literal < in attribute value"));
                    }
                    entities(std::str::from_utf8(attribute.value.as_ref()).map_err(invalid)?)?;
                    let raw = attribute.key.as_ref();
                    if raw == b"xmlns" || raw.starts_with(b"xmlns:") {
                        continue;
                    }
                    let name = resolve(&reader, attribute.key, true)?;
                    if !names.insert(name.clone()) {
                        return Err(invalid("duplicate expanded attribute name"));
                    }
                    *stat.attributes.entry(name).or_default() += 1;
                }
                if !empty {
                    stack.push(Frame {
                        index: path,
                        children: BTreeMap::new(),
                    });
                }
            }
            Event::End(_) => {
                stack.pop().ok_or_else(|| invalid("unexpected end tag"))?;
            }
            Event::Text(value) => {
                let decoded = value.xml10_content().map_err(invalid)?;
                entities(&decoded)?;
                if let Some(frame) = stack.last() {
                    stats.get_mut(&frame.index).unwrap().text += 1;
                } else if !decoded.trim().is_empty() {
                    return Err(invalid("text outside root"));
                }
            }
            Event::CData(value) => {
                if stack.is_empty() {
                    return Err(invalid("CDATA outside root"));
                }
                value.decode().map_err(invalid)?;
                stats.get_mut(&stack.last().unwrap().index).unwrap().text += 1;
            }
            Event::GeneralRef(reference) => {
                if stack.is_empty() {
                    return Err(invalid("entity outside root"));
                }
                let name = reference.decode().map_err(invalid)?;
                entities(&format!("&{name};"))?;
                stats.get_mut(&stack.last().unwrap().index).unwrap().text += 1;
            }
            Event::DocType(_) => {
                return Err(invalid(
                    "DOCTYPE/DTD is disabled; no external or custom entity expansion",
                ));
            }
            Event::Decl(value) => {
                if declaration || roots != 0 {
                    return Err(invalid("misplaced or repeated XML declaration"));
                }
                declaration = true;
                if let Some(encoding) = value.encoding() {
                    let encoding = encoding.map_err(invalid)?;
                    if !encoding.eq_ignore_ascii_case(b"UTF-8") {
                        return Err(invalid("only UTF-8 XML declarations are supported"));
                    }
                }
                if value.version().map_err(invalid)?.as_ref() != b"1.0" {
                    return Err(invalid("only XML 1.0 is supported"));
                }
            }
            Event::Eof => break,
            Event::Comment(_) | Event::PI(_) => {}
        }
    }
    if roots != 1 || !stack.is_empty() {
        return Err(invalid("exactly one complete root element is required"));
    }
    let paths = stats
        .into_iter()
        .map(|(elements, s)| XmlPath {
            elements,
            occurrences: s.occurrences,
            max_occurrences_per_parent: s.max_per_parent,
            text_events: s.text,
            attributes: s
                .attributes
                .into_iter()
                .map(|(name, occurrences)| AttributeCount { name, occurrences })
                .collect(),
        })
        .collect();
    Ok(XmlReport{schema:"omniattach-cli/xml-probe/0.1".into(),source:args.input.clone(),bytes:bytes.len()as u64,elements,max_depth,paths,warnings:vec!["Structural probe only: no XPath inference, record selection, CSV conversion or source text/attribute values".into(),"Entire input is inspected under byte/path/depth/attribute/time budgets; this is not full XML schema validation".into()]})
}

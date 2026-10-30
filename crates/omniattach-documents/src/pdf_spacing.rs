//! Extraction-only compatibility shim for lopdf's integer-only TJ spacing rule.
use lopdf::{Document, Object, ObjectId, Stream, content::Content, dictionary};

pub(super) fn normalize_page(pdf: &mut Document, page_id: ObjectId, limit: usize) -> lopdf::Result<bool> {
    let bytes = pdf.get_page_content_with_limit(page_id, limit)?;
    let mut content = Content::decode(&bytes)?;
    let mut changed = false;
    for operation in &mut content.operations {
        if operation.operator != "TJ" {
            continue;
        }
        for operand in &mut operation.operands {
            if let Object::Array(items) = operand {
                for item in items {
                    // lopdf inserts a space for Integer < -100, but ignores Real.
                    // Use a canonical integer marker, never a literal encoded space:
                    // a font's code for U+0020 need not be byte 0x20.
                    if let Object::Real(value) = item
                        && value.is_finite()
                        && *value < -100.0
                    {
                        *item = Object::Integer(-101);
                        changed = true;
                    }
                }
            }
        }
    }
    if changed {
        let encoded = content.encode()?;
        if encoded.len() > limit {
            return Err(lopdf::Error::Syntax("normalized PDF page exceeds content byte limit".into()));
        }
        // Use a new stream and replace only this page's Contents. Never mutate a
        // shared original stream or write the extraction copy to the input file.
        let id = pdf.add_object(Stream::new(dictionary! {}, encoded));
        pdf.get_object_mut(page_id)?.as_dict_mut()?.set("Contents", id);
    }
    Ok(changed)
}

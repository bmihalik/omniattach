use crate::*;
/// Exact text preservation for UTF-8/UTF-16 files; not an HTML/Office/PDF parser.
fn read(input: &StagedInput, context: &JobContext) -> Result<String> {
    use std::io::Read;
    context.check()?;
    let mut bytes = Vec::new();
    std::fs::File::open(&input.path)?
        .take(context.policy().max_input_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > context.policy().max_input_bytes {
        return Err(Error::Limit("text input bytes".into()));
    }
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        if bytes.len() % 2 != 0 {
            return Err(Error::InvalidInput("odd-length UTF-16".into()));
        }
        let units: Vec<_> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if little {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|e| Error::InvalidInput(e.to_string()))?
    } else {
        String::from_utf8(
            bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(&bytes)
                .to_vec(),
        )
        .map_err(|e| Error::InvalidInput(e.to_string()))?
    };
    context.check()?;
    Ok(text)
}
struct TextConverter;
impl Converter for TextConverter {
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        context: &JobContext,
    ) -> Result<ConversionData> {
        if request.options != serde_json::json!({}) && request.options != serde_json::Value::Null {
            return Err(Error::Configuration(
                "portable text route accepts no options".into(),
            ));
        }
        let text = read(input, context)?;
        let mut result = ConversionData::default();
        result.text.push(TextFragment {
            text: text.clone(),
            location: serde_json::json!({"kind":"whole_source"}),
        });
        if request.goal == Goal::Markdown {
            let markdown = if input.format == Format::Markdown {
                text
            } else {
                let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
                let fence = "`".repeat(longest.max(2) + 1);
                format!(
                    "{fence}text\n{text}{}{fence}\n",
                    if text.ends_with('\n') { "" } else { "\n" }
                )
            };
            if markdown.len() as u64 > context.policy().max_export_bytes {
                return Err(Error::Limit("Markdown output bytes".into()));
            }
            std::fs::write(input.output_dir.join("document.md"), markdown)?;
            result
                .artifacts
                .push(PendingArtifact::new("document.md", "text/markdown"));
        }
        result.warn("PORTABLE_TEXT_ONLY","Whole-source text retained; headings, tables and original page regions are not inferred");
        Ok(result)
    }
}
pub(crate) fn register(builder: OmniAttachBuilder) -> Result<OmniAttachBuilder> {
    builder.register(
        Route::new(
            "portable-text",
            vec![Format::Text, Format::Markdown],
            vec![Goal::Extract, Goal::Markdown],
            ExecutionKind::InProcess,
        )
        .described("Strict UTF-8/UTF-16 text; optional Markdown artifact"),
        TextConverter,
    )
}

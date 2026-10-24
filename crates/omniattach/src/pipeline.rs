use crate::*;

/// Runs first -> explicitly selected artifact -> second. Both converters are caller-configured.
/// Register the enclosing Route as External if EITHER component may execute external tools.
pub struct ArtifactPipeline<A, B> {
    pub first: A,
    pub second: B,
    pub first_goal: Goal,
    pub artifact_index: usize,
    pub intermediate_format: Format,
    pub first_options: serde_json::Value,
}
impl<A, B> ArtifactPipeline<A, B> {
    pub fn new(first: A, second: B, first_goal: Goal, intermediate_format: Format) -> Self {
        Self {
            first,
            second,
            first_goal,
            intermediate_format,
            artifact_index: 0,
            first_options: serde_json::json!({}),
        }
    }
}
impl<A: Converter, B: Converter> Converter for ArtifactPipeline<A, B> {
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        context: &JobContext,
    ) -> Result<ConversionData> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        context.check()?;
        let first_dir = input.output_dir.join("stage-1");
        std::fs::create_dir(&first_dir)?;
        let first_input = StagedInput {
            path: input.path.clone(),
            output_dir: first_dir.clone(),
            source: input.source.clone(),
            format: input.format,
        };
        let first_request = ConversionRequest::new(&input.path, self.first_goal.clone())
            .with_format(input.format)
            .with_options(self.first_options.clone());
        let mut first = self.first.convert(&first_input, &first_request, context)?;
        context.check()?;
        let validated = crate::bundle::validate_artifacts(&first_dir, &first.artifacts, context)?;
        let artifact = validated
            .get(self.artifact_index)
            .ok_or_else(|| Error::InvalidOutput("pipeline intermediate artifact index".into()))?;
        let path = crate::bundle::relative_file(&first_dir, &artifact.path)?;
        if artifact.bytes > context.policy().max_input_bytes {
            return Err(Error::Limit("intermediate input bytes".into()));
        }
        let mut reader = std::fs::File::open(&path)?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0; 65536];
        loop {
            context.check()?;
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes.saturating_add(count as u64);
            if bytes > context.policy().max_input_bytes {
                return Err(Error::Limit("intermediate input bytes".into()));
            }
            hash.update(&buffer[..count]);
        }
        let intermediate = SourceIdentity {
            name: path.to_string_lossy().into_owned(),
            bytes,
            sha256: format!("{:x}", hash.finalize()),
        };
        let second_dir = input.output_dir.join("stage-2");
        std::fs::create_dir(&second_dir)?;
        let second_input = StagedInput {
            path: path.clone(),
            output_dir: second_dir,
            source: intermediate.clone(),
            format: self.intermediate_format,
        };
        let second_request = ConversionRequest::new(path, request.goal.clone())
            .with_format(self.intermediate_format)
            .with_options(request.options.clone());
        let mut second = self
            .second
            .convert(&second_input, &second_request, context)?;
        context.check()?;
        let edge = serde_json::json!({"original":input.source,"intermediate":intermediate,"first_goal":self.first_goal,"final_goal":request.goal,"intermediate_artifact":format!("stage-1/{}",artifact.path)});
        first.payloads.push(Payload {
            schema: "omniattach/pipeline/0.1".into(),
            value: edge,
        });
        if !first.text.is_empty() {
            first.payloads.push(Payload {
                schema: "omniattach/pipeline-first-text/0.1".into(),
                value: serde_json::to_value(&first.text)?,
            });
        }
        for item in &mut first.artifacts {
            item.path = format!("stage-1/{}", item.path);
        }
        // Validate before prefixing so absolute paths and .. cannot be hidden by the stage prefix.
        crate::bundle::validate_artifacts(
            &input.output_dir.join("stage-2"),
            &second.artifacts,
            context,
        )?;
        for item in &mut second.artifacts {
            item.path = format!("stage-2/{}", item.path);
        }
        first.artifacts.extend(second.artifacts);
        first.payloads.extend(second.payloads);
        first.warnings.extend(second.warnings);
        first.text = second.text;
        first.warn("EXPLICIT_PIPELINE","Intermediate assets are retained; source relationship is recorded in omniattach/pipeline/0.1");
        Ok(first)
    }
}

use crate::*;
/// Signature-neutral adapter: caller supplies the ACTUAL document crate extraction API.
/// The returned serializable document is retained unchanged as a namespaced payload.
pub struct DocumentAdapter<F> {
    schema: String,
    extract: F,
}
impl<F> DocumentAdapter<F> {
    pub fn new(schema: impl Into<String>, extract: F) -> Self {
        Self {
            schema: schema.into(),
            extract,
        }
    }
}
impl<F, T, E> Converter for DocumentAdapter<F>
where
    F: Fn(&std::path::Path, &serde_json::Value) -> std::result::Result<T, E> + Send + Sync,
    T: serde::Serialize,
    E: std::fmt::Display,
{
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        context: &JobContext,
    ) -> Result<ConversionData> {
        context.check()?;
        let document =
            (self.extract)(&input.path, &request.options).map_err(|error| Error::Converter {
                route: self.schema.clone(),
                message: error.to_string(),
            })?;
        context.check()?;
        let mut result = ConversionData::default().payload(&self.schema, document)?;
        result.warn("DOCUMENT_PAYLOAD","Document schema retained unchanged; facade does not infer text or locations from unknown fields");
        Ok(result)
    }
}

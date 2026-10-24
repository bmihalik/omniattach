use crate::{BoundingBox, Error, Location, Result, TextKind, TextUnit};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptFormat {
    WhisperCpp,
    WhisperSegments,
}

fn seconds_ms(value: &serde_json::Value) -> Result<u64> {
    let n = value
        .as_f64()
        .ok_or_else(|| Error::InvalidOutput("numeric transcript timestamp required".into()))?;
    if !n.is_finite() || n < 0.0 || n * 1000.0 >= u64::MAX as f64 {
        return Err(Error::InvalidOutput(
            "timestamp outside millisecond range".into(),
        ));
    }
    Ok((n * 1000.0).round() as u64)
}
pub fn parse_transcript(json: &[u8], format: TranscriptFormat) -> Result<Vec<TextUnit>> {
    let value: serde_json::Value = serde_json::from_slice(json)?;
    let records = value
        .get(match format {
            TranscriptFormat::WhisperCpp => "transcription",
            TranscriptFormat::WhisperSegments => "segments",
        })
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::InvalidOutput("transcript segment array required".into()))?;
    let mut result = Vec::new();
    for record in records {
        let text = record
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidOutput("transcript text must be a string".into()))?;
        let (start, end) = match format {
            TranscriptFormat::WhisperCpp => {
                let offsets = record
                    .get("offsets")
                    .ok_or_else(|| Error::InvalidOutput("whisper.cpp offsets required".into()))?;
                (
                    offsets
                        .get("from")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| {
                            Error::InvalidOutput("integer offset from required".into())
                        })?,
                    offsets
                        .get("to")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| Error::InvalidOutput("integer offset to required".into()))?,
                )
            }
            TranscriptFormat::WhisperSegments => {
                (seconds_ms(&record["start"])?, seconds_ms(&record["end"])?)
            }
        };
        if end < start {
            return Err(Error::InvalidOutput("transcript end precedes start".into()));
        }
        if !text.trim().is_empty() {
            result.push(TextUnit {
                kind: TextKind::Transcript,
                text: text.trim().into(),
                location: Location {
                    start_ms: Some(start),
                    end_ms: Some(end),
                    ..Default::default()
                },
                confidence: None,
            });
        }
    }
    Ok(result)
}
pub fn parse_pdf_pages(text: &str) -> Vec<TextUnit> {
    if text.is_empty() {
        return vec![];
    }
    let text = text.strip_suffix('\u{c}').unwrap_or(text);
    text.split('\u{c}')
        .enumerate()
        .map(|(index, text)| TextUnit {
            kind: TextKind::Page,
            text: text.trim().into(),
            location: Location {
                page: Some(index as u32 + 1),
                ..Default::default()
            },
            confidence: None,
        })
        .collect()
}
pub fn parse_tesseract_tsv(text: &str) -> Result<Vec<TextUnit>> {
    let mut lines = text.lines();
    let expected = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";
    if lines.next() != Some(expected) {
        return Err(Error::InvalidOutput(
            "unexpected Tesseract TSV header".into(),
        ));
    }
    let mut words = Vec::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let fields: Vec<_> = line.splitn(12, '\t').collect();
        if fields.len() != 12 {
            return Err(Error::InvalidOutput("malformed OCR TSV record".into()));
        }
        if fields[0] != "5" {
            continue;
        }
        let number = |i: usize| {
            fields[i]
                .parse::<u32>()
                .map_err(|_| Error::InvalidOutput("invalid OCR coordinates".into()))
        };
        let confidence = fields[10]
            .parse::<f64>()
            .map_err(|_| Error::InvalidOutput("invalid OCR confidence".into()))?;
        if !confidence.is_finite() || !(-1.0..=100.0).contains(&confidence) {
            return Err(Error::InvalidOutput(
                "OCR confidence outside native range".into(),
            ));
        }
        let page = number(1)?;
        if page == 0 {
            return Err(Error::InvalidOutput("OCR page must be one-based".into()));
        }
        let bbox = BoundingBox {
            left: number(6)?,
            top: number(7)?,
            width: number(8)?,
            height: number(9)?,
        };
        if !fields[11].trim().is_empty() {
            words.push(TextUnit {
                kind: TextKind::OcrWord,
                text: fields[11].into(),
                location: Location {
                    page: Some(page),
                    bbox: Some(bbox),
                    ..Default::default()
                },
                confidence: (confidence >= 0.0).then_some(confidence),
            });
        }
    }
    Ok(words)
}
/// Parses FFmpeg showinfo records only; timestamps are normalized by the adapter's setpts filter.
pub fn parse_frame_times(stderr: &str) -> Result<Vec<u64>> {
    let mut result = Vec::new();
    for line in stderr
        .lines()
        .filter(|s| s.contains("showinfo") && s.contains("pts_time:"))
    {
        let token = line
            .split_once("pts_time:")
            .expect("matched token")
            .1
            .split_whitespace()
            .next()
            .ok_or_else(|| Error::InvalidOutput("missing frame timestamp".into()))?;
        let seconds: f64 = token
            .parse()
            .map_err(|_| Error::InvalidOutput("invalid frame timestamp".into()))?;
        if !seconds.is_finite() || seconds < 0.0 || seconds * 1000.0 >= u64::MAX as f64 {
            return Err(Error::InvalidOutput("invalid frame timestamp range".into()));
        }
        let ms = (seconds * 1000.0).round() as u64;
        if result.last().is_some_and(|last| ms < *last) {
            return Err(Error::InvalidOutput(
                "nonmonotonic selected frame timestamps".into(),
            ));
        }
        result.push(ms);
    }
    Ok(result)
}

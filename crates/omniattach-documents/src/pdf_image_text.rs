//! Conservative text assembly over inspection bounds.
//! Thresholds/normalization mirror pdf_spacing::build_lines; this is not shared
//! code yet because inspection retains normalized bounds, not signed Run metrics.
use crate::{BlockKind, Document, Error, Options, PdfElement, Result, SourceSpan};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Assembler<'a> {
    pending: Vec<&'a PdfElement>,
    output_bytes: u64,
}
impl<'a> Assembler<'a> {
    pub fn append(
        &mut self,
        doc: &mut Document,
        element: &'a PdfElement,
        options: &Options,
    ) -> Result<()> {
        if self.pending.first().is_some_and(|first| {
            first.anchor.page != element.anchor.page
                || first.anchor.form_path != element.anchor.form_path
        }) {
            self.flush(doc, options)?;
        }
        let Some(text) = element.text.as_deref() else {
            return self.flush(doc, options);
        };
        if text.is_empty() {
            return Ok(());
        }
        let geometry_ok = element.bounds.is_some_and(|b| {
            [b.left, b.right, b.bottom, b.top]
                .iter()
                .all(|n| n.is_finite())
                && b.right >= b.left
                && b.top > b.bottom
        });
        if !geometry_ok {
            self.flush(doc, options)?;
            doc.warn("LFE_TEXT_GEOMETRY_FALLBACK", "Text run without usable estimated bounds emitted separately without geometric joining");
            return self.emit(doc, text.to_owned(), span(element, element), options);
        }
        self.pending.push(element);
        Ok(())
    }
    pub fn flush(&mut self, doc: &mut Document, options: &Options) -> Result<()> {
        let Some(first) = self.pending.first().copied() else {
            return Ok(());
        };
        let last = self.pending.last().copied().unwrap_or(first);
        let source = span(first, last);
        let runs: Vec<_> = self
            .pending
            .iter()
            .filter_map(|e| {
                let bounds = e.bounds?;
                Some(Run {
                    x: bounds.left,
                    end: bounds.right,
                    y: bounds.bottom,
                    height: (bounds.top - bounds.bottom).max(1.),
                    text: e.text.as_deref()?,
                    space: e.word_gap_before,
                })
            })
            .collect();
        let joined = build_lines(&runs).join("\n");
        self.pending.clear();
        for paragraph in joined.split("\n\n").filter(|s| !s.trim().is_empty()) {
            self.emit(doc, paragraph.to_owned(), source.clone(), options)?;
        }
        Ok(())
    }
    fn emit(
        &mut self,
        doc: &mut Document,
        text: String,
        source: SourceSpan,
        options: &Options,
    ) -> Result<()> {
        self.output_bytes = self
            .output_bytes
            .checked_add(text.len() as u64)
            .filter(|n| *n <= options.limits.archive_bytes)
            .ok_or_else(|| Error::LimitExceeded("PDF assembled text bytes".into()))?;
        if doc.blocks.len() >= options.limits.blocks {
            return Err(Error::LimitExceeded("PDF image projection blocks".into()));
        }
        doc.paragraph(BlockKind::Paragraph, text, source);
        Ok(())
    }
}
fn span(first: &PdfElement, last: &PdfElement) -> SourceSpan {
    SourceSpan {
        page: Some(first.anchor.page),
        path: Some(format!(
            "content-operations {}..={} forms {:?}; segment provenance",
            first.anchor.operation_visit, last.anchor.operation_visit, first.anchor.form_path
        )),
        ..Default::default()
    }
}
struct Run<'a> {
    x: f64,
    end: f64,
    y: f64,
    height: f64,
    text: &'a str,
    space: bool,
}
fn build_lines(runs: &[Run<'_>]) -> Vec<String> {
    let mut gaps = BTreeMap::<i64, usize>::new();
    for pair in runs.windows(2) {
        let dy = (pair[1].y - pair[0].y).abs();
        let height = pair[0].height.max(pair[1].height);
        if dy > 0.5 * height && dy <= 2.6 * height {
            *gaps.entry((dy * 2.).round() as i64).or_default() += 1;
        }
    }
    let common = gaps
        .iter()
        .max_by(|(a, n), (b, m)| n.cmp(m).then_with(|| b.cmp(a)))
        .map(|(g, _)| *g as f64 / 2.);
    let mut out = String::new();
    let mut last: Option<&Run<'_>> = None;
    for run in runs {
        let mut new_line = false;
        if let Some(prev) = last {
            let dy = (run.y - prev.y).abs();
            let size = prev.height.max(run.height);
            if dy > 0.5 * size {
                let len = out.trim_end_matches([' ', '\t']).len();
                out.truncate(len);
                if !out.is_empty() {
                    out.push('\n');
                    if dy > 2.6 * size || common.is_some_and(|g| dy > 1.45 * g) {
                        out.push('\n');
                    }
                }
                new_line = true;
            } else if run.x - prev.end > 0.18 * size {
                push_space(&mut out);
            }
        }
        if run.space && !new_line {
            push_space(&mut out);
        }
        for c in run.text.chars() {
            if c.is_whitespace() {
                push_space(&mut out);
            } else if !c.is_control() {
                out.push(c);
            }
        }
        last = Some(run);
    }
    out.lines().map(|l| l.trim_end().to_owned()).collect()
}
fn push_space(out: &mut String) {
    if !out.is_empty() && !out.ends_with([' ', '\n']) {
        out.push(' ');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PdfBounds, PdfElementAnchor, PdfElementKind};
    fn element(text: Option<&str>, x: f64, y: f64, operation: usize) -> PdfElement {
        PdfElement {
            sequence: operation,
            kind: PdfElementKind::Text,
            anchor: PdfElementAnchor {
                page: 1,
                operation_visit: operation,
                form_path: Vec::new(),
                object_id: None,
                resource_name: None,
            },
            bounds: Some(PdfBounds {
                left: x,
                right: x + 12.,
                bottom: y,
                top: y + 12.,
                estimated: true,
            }),
            text: text.map(str::to_owned),
            word_gap_before: false,
            pixel_size: None,
            derived_from: Vec::new(),
            evidence: Vec::new(),
        }
    }
    fn document() -> Document {
        crate::extract_bytes("empty.txt", b"", &Options::default()).unwrap()
    }
    #[test]
    fn adjacent_fragments_join_and_geometry_or_tj_hint_adds_word_gap() {
        for (x, gap, expected) in [
            (12., false, "before"),
            (20., false, "be fore"),
            (12., true, "be fore"),
        ] {
            let one = element(Some("be"), 0., 100., 1);
            let mut two = element(Some("fore"), x, 100., 2);
            two.word_gap_before = gap;
            let mut doc = document();
            let mut assembler = Assembler::default();
            assembler
                .append(&mut doc, &one, &Options::default())
                .unwrap();
            assembler
                .append(&mut doc, &two, &Options::default())
                .unwrap();
            assembler.flush(&mut doc, &Options::default()).unwrap();
            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].text, expected);
        }
    }
    #[test]
    fn ordinary_lines_and_large_paragraph_gap_keep_text_order_and_segment_provenance() {
        let elements = [
            element(Some("one"), 0., 100., 1),
            element(Some("two"), 0., 86., 2),
            element(Some("three"), 0., 46., 3),
        ];
        let mut doc = document();
        let mut assembler = Assembler::default();
        for e in &elements {
            assembler.append(&mut doc, e, &Options::default()).unwrap();
        }
        assembler.flush(&mut doc, &Options::default()).unwrap();
        assert_eq!(
            doc.blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>(),
            vec!["one\ntwo", "three"]
        );
        assert!(
            doc.blocks[0]
                .source
                .path
                .as_deref()
                .unwrap()
                .contains("1..=3")
        );
    }
    #[test]
    fn pages_forms_and_undecodable_runs_are_hard_boundaries() {
        for mode in 0..3 {
            let one = element(Some("left"), 0., 100., 1);
            let mut boundary = element(None, 12., 100., 2);
            let mut two = element(Some("right"), 12., 100., 3);
            if mode == 0 {
                two.anchor.page = 2;
            }
            if mode == 1 {
                two.anchor.form_path.push((42, 0));
            }
            if mode != 2 {
                boundary.text = Some(String::new());
            }
            let mut doc = document();
            let mut assembler = Assembler::default();
            assembler
                .append(&mut doc, &one, &Options::default())
                .unwrap();
            assembler
                .append(&mut doc, &boundary, &Options::default())
                .unwrap();
            assembler
                .append(&mut doc, &two, &Options::default())
                .unwrap();
            assembler.flush(&mut doc, &Options::default()).unwrap();
            assert_eq!(
                doc.blocks
                    .iter()
                    .map(|b| b.text.as_str())
                    .collect::<Vec<_>>(),
                vec!["left", "right"]
            );
        }
    }
    #[test]
    fn missing_geometry_falls_back_without_losing_text() {
        let mut e = element(Some("raw"), 0., 100., 1);
        e.bounds = None;
        let mut doc = document();
        Assembler::default()
            .append(&mut doc, &e, &Options::default())
            .unwrap();
        assert_eq!(doc.blocks[0].text, "raw");
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.code == "LFE_TEXT_GEOMETRY_FALLBACK")
        );
    }
    #[test]
    fn generated_text_and_block_limits_are_enforced() {
        let e = element(Some("long text"), 0., 100., 1);
        for bytes_limit in [true, false] {
            let mut options = Options::default();
            if bytes_limit {
                options.limits.archive_bytes = 2;
            } else {
                options.limits.blocks = 0;
            }
            let mut doc = document();
            let mut assembler = Assembler::default();
            assembler.append(&mut doc, &e, &options).unwrap();
            assert!(matches!(
                assembler.flush(&mut doc, &options),
                Err(Error::LimitExceeded(_))
            ));
        }
    }
}

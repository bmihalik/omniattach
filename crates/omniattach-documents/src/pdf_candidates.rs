//! Lightweight inspection candidates, not recognized semantic content.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PdfBounds {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
    pub estimated: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfElementKind {
    Text,
    Image,
    EquationCandidate,
    TableCandidate,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PdfElementAnchor {
    pub page: u32,
    /// One-based operation visit within the page's expanded content traversal.
    pub operation_visit: usize,
    pub form_path: Vec<(u32, u16)>,
    pub object_id: Option<(u32, u16)>,
    pub resource_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PdfElement {
    pub sequence: usize,
    pub kind: PdfElementKind,
    pub anchor: PdfElementAnchor,
    pub bounds: Option<PdfBounds>,
    pub text: Option<String>,
    pub word_gap_before: bool,
    pub pixel_size: Option<[u32; 2]>,
    pub derived_from: Vec<usize>,
    pub evidence: Vec<String>,
}
pub(super) fn bounds<I: IntoIterator<Item = [f64; 2]>>(
    points: I,
    estimated: bool,
) -> Result<Option<PdfBounds>> {
    let mut result: Option<PdfBounds> = None;
    for [x, y] in points {
        if !x.is_finite() || !y.is_finite() {
            return Err(Error::Parse("non-finite PDF inspection geometry".into()));
        }
        if let Some(b) = &mut result {
            b.left = b.left.min(x);
            b.right = b.right.max(x);
            b.bottom = b.bottom.min(y);
            b.top = b.top.max(y);
        } else {
            result = Some(PdfBounds {
                left: x,
                right: x,
                bottom: y,
                top: y,
                estimated,
            });
        }
    }
    Ok(result)
}
pub(super) struct Collector {
    page: u32,
    max: usize,
    elements: Vec<PdfElement>,
}
impl Collector {
    pub(super) fn new(page: u32, max: usize) -> Self {
        Self {
            page,
            max,
            elements: Vec::new(),
        }
    }
    fn element(&self, kind: PdfElementKind, operation: usize, forms: &[(u32, u16)]) -> PdfElement {
        PdfElement {
            sequence: 0,
            kind,
            anchor: PdfElementAnchor {
                page: self.page,
                operation_visit: operation,
                form_path: forms.to_vec(),
                object_id: None,
                resource_name: None,
            },
            bounds: None,
            text: None,
            word_gap_before: false,
            pixel_size: None,
            derived_from: Vec::new(),
            evidence: Vec::new(),
        }
    }
    fn push(&mut self, mut element: PdfElement) -> Result<usize> {
        if self.elements.len() >= self.max {
            return Err(Error::LimitExceeded("PDF inspection elements".into()));
        }
        element.sequence = self.elements.len() + 1;
        let sequence = element.sequence;
        self.elements.push(element);
        Ok(sequence)
    }
    pub(super) fn text(
        &mut self,
        operation: usize,
        forms: &[(u32, u16)],
        text: Option<&str>,
        area: Option<PdfBounds>,
        gap: bool,
    ) -> Result<()> {
        let mut element = self.element(PdfElementKind::Text, operation, forms);
        element.bounds = area;
        element.text = text.map(str::to_owned);
        element.word_gap_before = gap;
        element.evidence.push(
            if text.is_some() {
                "decoded text-show run; text bounds are estimates"
            } else {
                "undecodable text-show run"
            }
            .into(),
        );
        let source = self.push(element)?;
        if text.is_some_and(math_hint) {
            let mut hint = self.element(PdfElementKind::EquationCandidate, operation, forms);
            hint.bounds = area;
            hint.derived_from.push(source);
            hint.evidence.push(
                "short mathematical-symbol/equality text hint; not formula recognition".into(),
            );
            self.push(hint)?;
        }
        Ok(())
    }
    pub(super) fn image(
        &mut self,
        operation: usize,
        forms: &[(u32, u16)],
        object: Option<(u32, u16)>,
        name: &[u8],
        area: Option<PdfBounds>,
        pixels: Option<[u32; 2]>,
    ) -> Result<()> {
        let mut element = self.element(PdfElementKind::Image, operation, forms);
        element.anchor.object_id = object;
        element.anchor.resource_name = Some(String::from_utf8_lossy(name).into_owned());
        element.bounds = area;
        element.pixel_size = pixels;
        element
            .evidence
            .push("unclassified image XObject occurrence; transformed bounds are unclipped".into());
        self.push(element)?;
        Ok(())
    }
    pub(super) fn tag(&mut self, operation: usize, forms: &[(u32, u16)], tag: &[u8]) -> Result<()> {
        let kind = match tag {
            b"Formula" => PdfElementKind::EquationCandidate,
            b"Table" => PdfElementKind::TableCandidate,
            _ => return Ok(()),
        };
        let mut element = self.element(kind, operation, forms);
        element
            .evidence
            .push("direct marked-content tag hint; structure tree not resolved".into());
        self.push(element)?;
        Ok(())
    }
    pub(super) fn grid(
        &mut self,
        operation: usize,
        forms: &[(u32, u16)],
        area: PdfBounds,
    ) -> Result<()> {
        let mut element = self.element(PdfElementKind::TableCandidate, operation, forms);
        element.bounds = Some(area);
        element.evidence.push("single stroked path with at least three spanning rulings on each axis; cells not reconstructed".into());
        self.push(element)?;
        Ok(())
    }
    pub(super) fn finish(self) -> Vec<PdfElement> {
        self.elements
    }
}
fn math_hint(text: &str) -> bool {
    let short = text.chars().count() <= 160;
    short
        && (text
            .chars()
            .any(|c| matches!(c, '∑' | '∫' | '√' | '∞' | '≈' | '≠' | '≤' | '≥'))
            || (text.contains('=')
                && text.chars().any(char::is_alphabetic)
                && text.split_whitespace().count() <= 12))
}
pub(super) struct PathTrace {
    segments: Vec<([f64; 2], [f64; 2])>,
    current: Option<[f64; 2]>,
    start: Option<[f64; 2]>,
    max: usize,
}
impl PathTrace {
    pub(super) fn new(max: usize) -> Self {
        Self {
            segments: Vec::new(),
            current: None,
            start: None,
            max,
        }
    }
    fn valid(point: [f64; 2]) -> Result<()> {
        if point.iter().all(|n| n.is_finite()) {
            Ok(())
        } else {
            Err(Error::Parse("non-finite PDF path geometry".into()))
        }
    }
    pub(super) fn move_to(&mut self, point: [f64; 2]) -> Result<()> {
        Self::valid(point)?;
        self.current = Some(point);
        self.start = Some(point);
        Ok(())
    }
    pub(super) fn line_to(&mut self, point: [f64; 2]) -> Result<()> {
        Self::valid(point)?;
        if let Some(previous) = self.current {
            if self.segments.len() >= self.max {
                return Err(Error::LimitExceeded("PDF inspection path segments".into()));
            }
            self.segments.push((previous, point));
        }
        self.current = Some(point);
        Ok(())
    }
    pub(super) fn curve_end(&mut self, point: [f64; 2]) -> Result<()> {
        Self::valid(point)?;
        self.current = Some(point);
        Ok(())
    }
    pub(super) fn close(&mut self) -> Result<()> {
        if self.current != self.start
            && let Some(start) = self.start
        {
            self.line_to(start)?;
        }
        self.current = self.start;
        Ok(())
    }
    pub(super) fn rectangle(&mut self, points: [[f64; 2]; 4]) -> Result<()> {
        self.move_to(points[0])?;
        for p in &points[1..] {
            self.line_to(*p)?;
        }
        self.close()
    }
    pub(super) fn clear(&mut self) {
        self.segments.clear();
        self.current = None;
        self.start = None;
    }
    pub(super) fn grid(&self) -> Result<Option<PdfBounds>> {
        let Some(area) = bounds(self.segments.iter().flat_map(|(a, b)| [*a, *b]), true)? else {
            return Ok(None);
        };
        let width = area.right - area.left;
        let height = area.top - area.bottom;
        if width <= 0.5 || height <= 0.5 {
            return Ok(None);
        }
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for (a, b) in &self.segments {
            if (a[1] - b[1]).abs() <= 0.5 && ((a[0] - b[0]).abs() - width).abs() <= 0.5 {
                ys.push(a[1]);
            }
            if (a[0] - b[0]).abs() <= 0.5 && ((a[1] - b[1]).abs() - height).abs() <= 0.5 {
                xs.push(a[0]);
            }
        }
        xs.sort_unstable_by(f64::total_cmp);
        ys.sort_unstable_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() <= 0.5);
        ys.dedup_by(|a, b| (*a - *b).abs() <= 0.5);
        Ok((xs.len() >= 3 && ys.len() >= 3).then_some(area))
    }
}

//! Tables on scanned pages: OCR words plus rulings found in the page raster,
//! run through the same layout detector as native pages.
//!
//! This crate launches no tools. A caller renders the page (for example with
//! `pdftoppm -gray`), runs OCR (for example Tesseract TSV) on
//! [`GrayRaster::for_ocr`] of it and passes the original raster and the
//! words in a [`PdfTableOcrPage`]. The omniattach CLI does this with `--ocr`.
//! Coordinates are raster pixels scaled to PDF units (72 per inch) with the
//! origin at the bottom-left of the image. Skewed scans are not deskewed.
use super::convert::{TextRun, convert_runs};
use super::discovery::{PdfTableCandidate, PdfTableDetector, PdfTableDiscovery};
use super::rules::{Rule, merge};
use super::{
    DISCOVERY_SCHEMA, MAX_CANDIDATES, MAX_CELLS, PdfTableConversion, invalid, layout, limit,
};
use crate::{Options, PdfBounds, Result};
use sha2::{Digest, Sha256};

/// 8-bit grayscale page image, row-major, 0 = black.
#[derive(Clone, Debug, PartialEq)]
pub struct GrayRaster {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// One recognized word; box in raster pixels from the top-left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    pub text: String,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    /// Native OCR score (Tesseract: 0..100), if known.
    pub confidence: Option<f64>,
}

/// A rendered, recognized page.
#[derive(Clone, Debug)]
pub struct PdfTableOcrPage {
    /// One-based page number in the source PDF.
    pub page: u32,
    /// Raster resolution in pixels per inch.
    pub dpi: f64,
    pub raster: GrayRaster,
    pub words: Vec<OcrWord>,
}

/// Pixels darker than this are ink (glyphs, black rules).
const INK: u8 = 170;
/// Rules may be printed lighter grey than text...
const RULE_INK: u8 = 200;
/// ...but must be this much darker than both sides of the line.
const CONTRAST: u8 = 25;
/// Words below this native confidence are dropped (Tesseract reports -1 for
/// non-word records; low but real words near rulings are kept).
const MIN_CONFIDENCE: f64 = 0.0;
/// Ruling fragments OCR glues to words.
const NOISE: &[char] = &['|', '_', '[', ']'];

/// Shading lighter than this is flattened to white before OCR.
const SHADE: u8 = 160;

impl GrayRaster {
    /// A copy prepared for OCR of a table: light cell shading becomes white
    /// and long ruling lines are erased, which keeps OCR layout analysis from
    /// splitting or dropping shaded rows.
    pub fn for_ocr(&self, dpi: f64) -> Self {
        let mut out = self.clone();
        for p in &mut out.pixels {
            if *p >= SHADE {
                *p = 255;
            }
        }
        let (w, h) = (self.width, self.height);
        for (horizontal, min) in [(true, dpi / 3.0), (false, dpi / 3.0)] {
            let min = min.max(8.0) as usize;
            let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
            let at = |o: usize, i: usize| if horizontal { o * w + i } else { i * w + o };
            for o in 0..outer {
                let mut start = None;
                for i in 0..=inner {
                    let on = i < inner && self.pixels[at(o, i)] < INK;
                    match (start, on) {
                        (None, true) => start = Some(i),
                        (Some(s), false) => {
                            if i - s >= min {
                                for k in s..i {
                                    out.pixels[at(o, k)] = 255;
                                }
                            }
                            start = None;
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }

    /// Binary PGM (`P5`) encoding.
    pub fn to_pgm(&self) -> Vec<u8> {
        let mut out = format!("P5\n{} {}\n255\n", self.width, self.height).into_bytes();
        out.extend_from_slice(&self.pixels);
        out
    }

    /// Parse a binary PGM (`P5`) or PPM (`P6`, converted to gray) image.
    pub fn from_pnm(bytes: &[u8]) -> Result<Self> {
        let mut fields = Vec::new();
        let mut i = 0;
        while fields.len() < 4 {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'#' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if start == i {
                return Err(invalid("truncated PNM header"));
            }
            fields.push(std::str::from_utf8(&bytes[start..i]).unwrap_or(""));
        }
        let channels = match fields[0] {
            "P5" => 1,
            "P6" => 3,
            _ => return Err(invalid("only binary PGM/PPM rasters are supported")),
        };
        let parse = |s: &str| {
            s.parse::<usize>()
                .map_err(|_| invalid("invalid PNM header"))
        };
        let (width, height, max) = (parse(fields[1])?, parse(fields[2])?, parse(fields[3])?);
        if width == 0 || height == 0 || max == 0 || max > 255 {
            return Err(invalid("unsupported PNM dimensions or depth"));
        }
        let size = width
            .checked_mul(height)
            .filter(|n| *n <= 200_000_000)
            .ok_or_else(|| limit("raster pixels"))?;
        let data = &bytes[(i + 1).min(bytes.len())..];
        if data.len() < size * channels {
            return Err(invalid("truncated PNM data"));
        }
        let pixels = if channels == 1 {
            data[..size].to_vec()
        } else {
            data[..size * 3]
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8)
                .collect()
        };
        let pixels = if max == 255 {
            pixels
        } else {
            pixels
                .into_iter()
                .map(|p| ((p as usize * 255) / max).min(255) as u8)
                .collect()
        };
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

impl PdfTableOcrPage {
    fn scale(&self) -> Result<f64> {
        if !(self.dpi.is_finite() && (36.0..=2400.0).contains(&self.dpi)) {
            return Err(invalid("OCR raster DPI must be in 36..=2400"));
        }
        if self.raster.pixels.len() != self.raster.width * self.raster.height {
            return Err(invalid("raster size does not match its pixels"));
        }
        Ok(72.0 / self.dpi)
    }

    /// Usable words with their PDF-unit boxes.
    fn boxes(&self) -> Result<Vec<(usize, String, PdfBounds)>> {
        let s = self.scale()?;
        let height = self.raster.height as f64;
        Ok(self
            .words
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                w.width > 0
                    && w.height > 0
                    && w.confidence.is_none_or(|c| c >= MIN_CONFIDENCE)
                    && w.text.chars().any(|c| c.is_alphanumeric())
            })
            .map(|(i, w)| (i, w, w.text.trim_matches(NOISE)))
            .map(|(i, w, text)| {
                let (l, t) = (w.left as f64, w.top as f64);
                let bounds = PdfBounds {
                    left: l * s,
                    bottom: (height - t - w.height as f64) * s,
                    right: (l + w.width as f64) * s,
                    top: (height - t) * s,
                    estimated: true,
                };
                (i + 1, text.to_string(), bounds)
            })
            .collect())
    }
}

/// Long thin ink runs as rules, in PDF units.
fn raster_rules(page: &PdfTableOcrPage) -> Result<Vec<Rule>> {
    let s = page.scale()?;
    let r = &page.raster;
    let thick = (page.dpi / 30.0).max(2.0) as usize;
    let pixel = |x: usize, y: usize| r.pixels[y * r.width + x];
    // Rule ink is darker than the paper or shading on both sides across the
    // line, so shaded areas themselves are not rules.
    let ink = |x: usize, y: usize, horizontal: bool| {
        let p = pixel(x, y);
        let side = |dx: isize, dy: isize| {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if nx < 0 || ny < 0 || nx >= r.width as isize || ny >= r.height as isize {
                255
            } else {
                pixel(nx as usize, ny as usize)
            }
        };
        let d = thick as isize + 1;
        let (a, b) = if horizontal {
            (side(0, -d), side(0, d))
        } else {
            (side(-d, 0), side(d, 0))
        };
        p < RULE_INK && p.saturating_add(CONTRAST) < a && p.saturating_add(CONTRAST) < b
    };
    let mut rules = Vec::new();
    for horizontal in [true, false] {
        let (outer, inner) = if horizontal {
            (r.height, r.width)
        } else {
            (r.width, r.height)
        };
        // A third of an inch: longer than glyph strokes, shorter than rules.
        let min_len = (page.dpi / 3.0) as usize;
        let max_gap = (page.dpi / 100.0).max(1.0) as usize;
        // Runs per scan line: (line, start, end).
        let mut runs: Vec<(usize, usize, usize)> = Vec::new();
        for o in 0..outer {
            let mut start: Option<usize> = None;
            let mut last = 0usize;
            for i in 0..inner {
                if !(if horizontal {
                    ink(i, o, true)
                } else {
                    ink(o, i, false)
                }) {
                    continue;
                }
                match start {
                    Some(_) if i - last <= max_gap + 1 => {}
                    Some(st) => {
                        if last + 1 - st >= min_len {
                            runs.push((o, st, last + 1));
                        }
                        start = Some(i);
                    }
                    None => start = Some(i),
                }
                last = i;
            }
            if let Some(st) = start
                && last + 1 - st >= min_len
            {
                runs.push((o, st, last + 1));
            }
            if runs.len() > 1_000_000 {
                return Err(limit("raster rule runs"));
            }
        }
        // Stack runs on adjacent scan lines into strokes.
        let mut strokes: Vec<(usize, usize, usize, usize)> = Vec::new(); // first, last, start, end
        for (o, a, b) in runs {
            match strokes
                .iter_mut()
                .rev()
                .take(64)
                .find(|st| st.1 + 1 >= o && a < st.3 && b > st.2)
            {
                Some(st) => {
                    st.1 = o;
                    st.2 = st.2.min(a);
                    st.3 = st.3.max(b);
                }
                None => strokes.push((o, o, a, b)),
            }
        }
        for (first, last, a, b) in strokes {
            let lines = if last + 1 - first <= thick {
                vec![(first + last) as f64 / 2.0 + 0.5]
            } else {
                vec![first as f64, last as f64 + 1.0]
            };
            for line in lines {
                let rule = if horizontal {
                    Rule {
                        horizontal: true,
                        at: (r.height as f64 - line) * s,
                        from: a as f64 * s,
                        to: b as f64 * s,
                    }
                } else {
                    Rule {
                        horizontal: false,
                        at: line * s,
                        from: (r.height as f64 - b as f64) * s,
                        to: (r.height as f64 - a as f64) * s,
                    }
                };
                rules.push(rule);
            }
        }
    }
    Ok(merge(rules))
}

/// Discover table candidates on a scanned page. `source` is the PDF the
/// page was rendered from; only its SHA-256 is recorded.
pub fn discover_ocr_tables(
    name: &str,
    source: &[u8],
    page: &PdfTableOcrPage,
    options: &Options,
) -> Result<PdfTableDiscovery> {
    if source.len() > options.limits.input_bytes {
        return Err(limit("input bytes"));
    }
    let source_sha256 = format!("{:x}", Sha256::digest(source));
    if page.page == 0 {
        return Err(invalid("page numbers are one-based"));
    }
    let boxes: Vec<layout::Box4> = page
        .boxes()?
        .iter()
        .map(|(_, _, b)| [b.left, b.bottom, b.right, b.top])
        .collect();
    let tables = layout::tables(&raster_rules(page)?, &boxes, layout::OCR);
    if tables.len() > MAX_CANDIDATES {
        return Err(limit("table candidates"));
    }
    let candidates: Vec<PdfTableCandidate> = tables
        .into_iter()
        .filter(|t| (t.xs.len() - 1) * (t.ys.len() - 1) <= options.limits.cells.min(MAX_CELLS))
        .enumerate()
        .map(|(i, t)| PdfTableCandidate {
            candidate: i + 1,
            detector: PdfTableDetector::OcrLayout,
            page: page.page,
            x_boundaries: t.xs,
            y_boundaries: t.ys,
            cell_spans: Vec::new(),
            rotation: 0,
        })
        .collect();
    let mut warnings = vec![
        "OCR page: coordinates are raster pixels scaled to PDF units from the image's \
         bottom-left corner; no deskewing"
            .to_string(),
    ];
    if !candidates.is_empty() {
        warnings.push(PdfTableDetector::OcrLayout.note().into());
    }
    Ok(PdfTableDiscovery {
        schema_version: DISCOVERY_SCHEMA.into(),
        source: name.into(),
        source_sha256,
        page: page.page,
        candidates,
        warnings,
    })
}

/// Convert candidate `candidate` (one-based) of a scanned page.
pub fn convert_ocr_table(
    name: &str,
    source: &[u8],
    page: &PdfTableOcrPage,
    candidate: usize,
    options: &Options,
) -> Result<PdfTableConversion> {
    let index = candidate
        .checked_sub(1)
        .ok_or_else(|| invalid("candidate numbers are one-based"))?;
    let discovery = discover_ocr_tables(name, source, page, options)?;
    let selected = discovery
        .candidates
        .get(index)
        .ok_or_else(|| invalid("candidate not found"))?;
    let texts: Vec<TextRun> = page
        .boxes()?
        .into_iter()
        .map(|(sequence, text, bounds)| TextRun {
            bounds,
            text: Some(text),
            gap: true,
            sequence,
        })
        .collect();
    let grid = selected.grid();
    let mut result = convert_runs(name, &discovery.source_sha256, &texts, &grid, options)?;
    result.warnings.push(selected.detector.note().into());
    result
        .warnings
        .push("source_sequences are one-based indexes into the OCR word list".into());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// White page with a ruled 2x3 table and words in its cells.
    fn page() -> PdfTableOcrPage {
        let (w, h) = (600usize, 400usize);
        let mut pixels = vec![255u8; w * h];
        for y in [50usize, 150, 250, 350] {
            for x in 50..550 {
                pixels[y * w + x] = 0;
                pixels[(y + 1) * w + x] = 0;
            }
        }
        for x in [50usize, 300, 549] {
            for y in 50..352 {
                pixels[y * w + x] = 0;
            }
        }
        let word = |text: &str, left, top| OcrWord {
            text: text.into(),
            left,
            top,
            width: 80,
            height: 30,
            confidence: Some(90.0),
        };
        PdfTableOcrPage {
            page: 1,
            dpi: 150.0,
            raster: GrayRaster {
                width: w,
                height: h,
                pixels,
            },
            words: vec![
                word("Name", 100, 80),
                word("Value", 350, 80),
                word("alpha", 100, 180),
                word("1", 350, 180),
                word("beta", 100, 280),
                word("2", 350, 280),
                word("|", 300, 280),
            ],
        }
    }

    #[test]
    fn ruled_scan_becomes_markdown() {
        let p = page();
        let d = discover_ocr_tables("scan.pdf", b"%PDF", &p, &Options::default()).unwrap();
        assert_eq!(d.candidates.len(), 1);
        assert_eq!(d.candidates[0].detector, PdfTableDetector::OcrLayout);
        let t = convert_ocr_table("scan.pdf", b"%PDF", &p, 1, &Options::default()).unwrap();
        assert_eq!(
            t.markdown,
            "| Name | Value |\n| --- | --- |\n| alpha | 1 |\n| beta | 2 |\n"
        );
    }

    #[test]
    fn ocr_copy_drops_shading_and_rules_but_keeps_glyphs() {
        let mut r = page().raster;
        r.pixels[10 * r.width + 10] = 200; // light shading
        r.pixels[100 * r.width + 100] = 0; // a glyph dot
        let clean = r.for_ocr(150.0);
        assert_eq!(clean.pixels[10 * r.width + 10], 255);
        assert_eq!(clean.pixels[50 * r.width + 200], 255);
        assert_eq!(clean.pixels[100 * r.width + 100], 0);
        assert_eq!(GrayRaster::from_pnm(&clean.to_pgm()).unwrap(), clean);
    }

    #[test]
    fn parses_pgm_and_ppm() {
        let g = GrayRaster::from_pnm(b"P5\n# c\n2 1\n255\n\x00\xff").unwrap();
        assert_eq!((g.width, g.height, g.pixels.clone()), (2, 1, vec![0, 255]));
        let c = GrayRaster::from_pnm(b"P6 1 1 255 \xff\xff\xff").unwrap();
        assert_eq!(c.pixels, vec![255]);
        assert!(GrayRaster::from_pnm(b"P5 2 2 255 \x00").is_err());
        assert!(GrayRaster::from_pnm(b"P2 1 1 255 0").is_err());
    }
}

//! End-to-end table discovery and conversion on generated PDFs.
//!
//! Real-document regressions are `#[ignore]`d and read their PDF from an
//! environment variable, so private samples never enter the repository:
//!
//! ```sh
//! OMNIATTACH_FILLED_TABLE_SAMPLE=/path/01-evibaseprac.pdf \
//! OMNIATTACH_OPEN_ROWS_SAMPLE=/path/02-healthbig.pdf \
//! OMNIATTACH_TABLE_SAMPLES=/path/to/sample-dir \
//!   cargo test -p omniattach-documents --features pdf --test pdf_tables -- --ignored
//! ```
//!
//! `OMNIATTACH_TABLE_SAMPLES` is a directory holding the layout samples
//! (`03-ecgacqu.pdf` ... `12-tarot.pdf`) under their original names.
#![cfg(feature = "pdf")]
use lopdf::{Object, Stream, dictionary};
use omniattach_documents::pdf_tables::{
    PdfTableDetector, PdfTableGrid, PdfTableSpan, convert_inspection, convert_pdf_table_bytes,
    discover_pdf_tables_bytes,
};
use omniattach_documents::{Error, Options, inspect_pdf_bytes};

const GRID: &str = "50 100 m 250 100 l 50 130 m 250 130 l 50 160 m 250 160 l \
                    50 100 m 50 160 l 150 100 m 150 160 l 250 100 m 250 160 l S\n";
const TEXT: &str = "BT /F1 10 Tf 1 0 0 1 60 140 Tm (Name) Tj 1 0 0 1 160 140 Tm (Value) Tj \
                    1 0 0 1 60 110 Tm (Alpha) Tj 1 0 0 1 160 110 Tm (42) Tj ET\n";
const EXPECTED: &str = "| Name | Value |\n| --- | --- |\n| Alpha | 42 |\n";

/// One-page PDF with Helvetica as /F1, a 1x1 image as /Im1 and optional
/// extra page entries.
fn pdf(content: &str, page_extra: &[(&str, Object)]) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.4");
    let pages = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        },
        vec![128],
    ));
    let mut page = dictionary! {
        "Type" => "Page", "Parent" => pages, "Contents" => stream,
        "Resources" => dictionary! {
            "Font" => dictionary! {"F1" => font},
            "XObject" => dictionary! {"Im1" => image},
        },
        "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
    };
    for (key, value) in page_extra {
        page.set(*key, value.clone());
    }
    let page = doc.add_object(page);
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1},
        ),
    );
    let root = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    doc.trailer.set("Root", root);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn discover(bytes: &[u8]) -> omniattach_documents::Result<Vec<PdfTableGrid>> {
    let found = discover_pdf_tables_bytes("fixture.pdf", bytes, 1, &Options::default())?;
    Ok(found.candidates.iter().map(|c| c.grid()).collect())
}

fn markdown(bytes: &[u8], candidate: usize) -> omniattach_documents::Result<String> {
    convert_pdf_table_bytes("fixture.pdf", bytes, 1, candidate, &Options::default())
        .map(|r| r.markdown)
}

fn rejected(result: omniattach_documents::Result<impl std::fmt::Debug>, needle: &str) {
    match result {
        Err(e) => assert!(
            e.to_string().contains(needle),
            "{e} does not mention {needle:?}"
        ),
        Ok(v) => panic!("expected an error mentioning {needle:?}, got {v:?}"),
    }
}

/// The strict detectors declined with `needle`; only layout candidates remain.
fn strict_declined(bytes: &[u8], needle: &str) {
    let found = discover_pdf_tables_bytes("x", bytes, 1, &Options::default()).unwrap();
    assert!(
        found
            .warnings
            .iter()
            .any(|w| w.contains("declined") && w.contains(needle)),
        "{:?}",
        found.warnings
    );
    assert!(
        found
            .candidates
            .iter()
            .all(|c| c.detector == PdfTableDetector::Layout)
    );
}

/// Parse generated Markdown rows back into plain cell text.
fn rows(markdown: &str) -> Vec<Vec<String>> {
    markdown
        .lines()
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, line)| {
            let inner = line
                .strip_prefix("| ")
                .and_then(|l| l.strip_suffix(" |"))
                .unwrap_or("");
            inner
                .split(" | ")
                .map(|cell| {
                    let cell = cell
                        .replace("<br>", " ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    [
                        ("&#124;", "|"),
                        ("&#92;", "\\"),
                        ("&#42;", "*"),
                        ("&#95;", "_"),
                        ("&#96;", "`"),
                        ("&#91;", "["),
                        ("&#93;", "]"),
                        ("&lt;", "<"),
                        ("&gt;", ">"),
                        ("&amp;", "&"),
                    ]
                    .iter()
                    .fold(cell, |s, (from, to)| s.replace(from, to))
                })
                .collect()
        })
        .collect()
}

mod stroked {
    use super::*;

    #[test]
    fn lists_and_converts_exact_markdown_with_provenance() {
        let bytes = pdf(&format!("{GRID}{TEXT}"), &[]);
        let found =
            discover_pdf_tables_bytes("fixture.pdf", &bytes, 1, &Options::default()).unwrap();
        assert_eq!(found.candidates.len(), 1);
        let c = &found.candidates[0];
        assert_eq!(
            (c.candidate, c.detector),
            (1, PdfTableDetector::StrokedGrid)
        );
        assert_eq!(c.x_boundaries, vec![50., 150., 250.]);
        assert_eq!(c.y_boundaries, vec![100., 130., 160.]);
        let result =
            convert_pdf_table_bytes("fixture.pdf", &bytes, 1, 1, &Options::default()).unwrap();
        assert_eq!(result.markdown, EXPECTED);
        assert_eq!(result.source_sha256, found.source_sha256);
        assert_eq!(result.source_sequences.len(), 4);
    }

    #[test]
    fn manual_and_discovered_grids_agree() {
        let bytes = pdf(&format!("{GRID}{TEXT}"), &[]);
        let options = Options::default();
        let report = inspect_pdf_bytes("fixture.pdf", &bytes, &options).unwrap();
        let manual = PdfTableGrid {
            page: 1,
            x_boundaries: vec![50., 150., 250.],
            y_boundaries: vec![100., 130., 160.],
            cell_spans: Vec::new(),
            rotation: 0,
            text_assignment: Default::default(),
        };
        assert_eq!(
            convert_inspection(&report, &manual, &options)
                .unwrap()
                .markdown,
            EXPECTED
        );
        assert_eq!(markdown(&bytes, 1).unwrap(), EXPECTED);
    }

    #[test]
    fn transformed_text_and_grid_stay_aligned() {
        let bytes = pdf(&format!("q 2 0 0 2 5 7 cm\n{GRID}{TEXT}Q\n"), &[]);
        let grids = discover(&bytes).unwrap();
        assert_eq!(grids[0].x_boundaries, vec![105., 305., 505.]);
        assert_eq!(grids[0].y_boundaries, vec![207., 267., 327.]);
        assert_eq!(markdown(&bytes, 1).unwrap(), EXPECTED);
    }

    #[test]
    fn multiple_candidates_and_restored_graphics_state() {
        let beta = TEXT.replace("Alpha", "Beta");
        let bytes = pdf(
            &format!("{GRID}{TEXT}q 1 0 0 1 300 0 cm\n{GRID}{beta}Q\n"),
            &[],
        );
        assert_eq!(discover(&bytes).unwrap().len(), 2);
        assert_eq!(
            markdown(&bytes, 2).unwrap(),
            EXPECTED.replace("Alpha", "Beta")
        );
        let bytes = pdf(&format!("q 1 0 0 1 300 0 cm\n{GRID}Q\n{GRID}{TEXT}"), &[]);
        assert_eq!(markdown(&bytes, 2).unwrap(), EXPECTED);
    }

    #[test]
    fn empty_grid_prose_only_and_crossing_text_are_rejected() {
        rejected(markdown(&pdf(GRID, &[]), 1), "no decoded text");
        let prose = pdf(TEXT, &[]);
        assert!(discover(&prose).unwrap().is_empty());
        rejected(markdown(&prose, 1), "candidate not found");
        let crossing = pdf(&format!("{GRID}{}", TEXT.replace("60 110", "145 110")), &[]);
        rejected(markdown(&crossing, 1), "crosses grid boundary");
    }

    #[test]
    fn invalid_selections_are_rejected() {
        let bytes = pdf(&format!("{GRID}{TEXT}"), &[]);
        rejected(
            discover_pdf_tables_bytes("x", &bytes, 0, &Options::default()),
            "page not found",
        );
        rejected(markdown(&bytes, 0), "one-based");
        rejected(markdown(&bytes, 99), "candidate not found");
    }

    #[test]
    fn clipping_rotation_and_userunit_fall_back_to_layout() {
        for bytes in [
            pdf(&format!("0 0 600 600 re W n\n{GRID}{TEXT}"), &[]),
            pdf(&format!("{GRID}{TEXT}"), &[("Rotate", 90.into())]),
            pdf(&format!("{GRID}{TEXT}"), &[("UserUnit", 2.into())]),
        ] {
            let found = discover_pdf_tables_bytes("x", &bytes, 1, &Options::default()).unwrap();
            assert!(found.warnings.iter().any(|w| w.contains("declined")));
            assert_eq!(found.candidates.len(), 1);
            assert_eq!(found.candidates[0].detector, PdfTableDetector::Layout);
            assert_eq!(markdown(&bytes, 1).unwrap(), EXPECTED);
        }
    }

    #[test]
    fn limits_are_enforced() {
        let bytes = pdf(&format!("{GRID}{TEXT}"), &[]);
        let mut options = Options::default();
        options.limits.input_bytes = 10;
        assert!(matches!(
            discover_pdf_tables_bytes("x", &bytes, 1, &options),
            Err(Error::LimitExceeded(_))
        ));
    }
}

mod filled {
    use super::*;

    fn content() -> String {
        let mut s = String::from("0 0 600 600 re W n\n");
        for y in [100, 130, 160, 190] {
            s += &format!("50 {y} 200 0.5 re f\n");
        }
        for x in [50, 250] {
            s += &format!("{x} 100 0.5 90.5 re f\n");
        }
        s += "150 100 0.5 60 re f\n";
        s + "BT /F1 10 Tf 1 0 0 1 60 170 Tm (Section) Tj 1 0 0 1 60 140 Tm (Name) Tj \
             1 0 0 1 160 140 Tm (Value) Tj 1 0 0 1 60 110 Tm (Alpha) Tj 1 0 0 1 160 110 Tm (42) Tj ET\n"
    }

    #[test]
    fn section_heading_and_data_are_separate_candidates() {
        let bytes = pdf(&content(), &[]);
        let found = discover_pdf_tables_bytes("x", &bytes, 1, &Options::default()).unwrap();
        let columns: Vec<usize> = found
            .candidates
            .iter()
            .map(|c| c.x_boundaries.len() - 1)
            .collect();
        assert_eq!(columns, vec![1, 2]);
        assert!(
            found
                .candidates
                .iter()
                .all(|c| c.detector == PdfTableDetector::FilledGrid)
        );
        assert_eq!(markdown(&bytes, 1).unwrap(), "| Section |\n| --- |\n");
        assert_eq!(markdown(&bytes, 2).unwrap(), EXPECTED);
    }

    #[test]
    fn clip_cutting_the_table_declines_the_strict_detector() {
        let bytes = pdf(&content().replace("0 0 600 600", "100 0 500 600"), &[]);
        strict_declined(&bytes, "clipped");
    }
}

mod open_rows {
    use super::*;

    fn content() -> String {
        let mut s = String::from("0 0 600 600 re W n\n");
        for y in [100, 130, 190, 220] {
            s += &format!("50 {y} 400 0.5 re f\n");
        }
        s += "130 160 320 0.5 re f\n";
        for x in [130, 230, 300] {
            s += &format!("{x} 100 0.5 120.5 re f\n");
        }
        s += "BT /F1 10 Tf ";
        for (x, y, text) in [
            (55, 205, "Kind"),
            (135, 205, "App"),
            (235, 205, "Country"),
            (305, 205, "Function"),
            (55, 175, "Group"),
            (55, 145, "continued"),
            (135, 175, "A"),
            (235, 175, "UK"),
            (305, 175, "First"),
            (135, 145, "B"),
            (235, 145, "USA"),
            (305, 145, "Second"),
            (55, 115, "Other"),
            (135, 115, "C"),
            (235, 115, "France"),
            (305, 115, "Third"),
        ] {
            s += &format!("1 0 0 1 {x} {y} Tm ({text}) Tj ");
        }
        s + "ET\n"
    }

    #[test]
    fn spanning_label_is_extracted_once() {
        let bytes = pdf(&content(), &[]);
        let found = discover_pdf_tables_bytes("x", &bytes, 1, &Options::default()).unwrap();
        assert_eq!(found.candidates.len(), 1);
        let c = &found.candidates[0];
        assert_eq!(c.detector, PdfTableDetector::OpenRows);
        assert!(c.cell_spans.contains(&PdfTableSpan {
            row: 1,
            column: 0,
            row_span: 2,
            column_span: 1
        }));
        let result = convert_pdf_table_bytes("x", &bytes, 1, 1, &Options::default()).unwrap();
        assert_eq!(
            rows(&result.markdown),
            [
                ["Kind", "App", "Country", "Function"],
                ["Group continued", "A", "UK", "First"],
                ["", "B", "USA", "Second"],
                ["Other", "C", "France", "Third"],
            ]
        );
        assert_eq!(result.grid.cell_spans, c.cell_spans);
    }

    #[test]
    fn partial_divider_does_not_invent_strict_columns() {
        let bytes = pdf(
            &content().replace("230 100 0.5 120.5", "230 100 0.5 60"),
            &[],
        );
        let found = discover_pdf_tables_bytes("x", &bytes, 1, &Options::default()).unwrap();
        assert!(
            found
                .candidates
                .iter()
                .all(|c| c.detector == PdfTableDetector::Layout)
        );
    }

    #[test]
    fn clip_cutting_the_table_declines_the_strict_detector() {
        let bytes = pdf(&content().replace("0 0 600 600", "100 0 500 600"), &[]);
        strict_declined(&bytes, "clipped");
    }
}

mod layout {
    use super::*;

    /// Three rules, no column rules, an image on the page.
    const BOOKTABS: &str = "50 300 m 350 300 l S 50 280 m 350 280 l S 50 220 m 350 220 l S\n\
        q 10 0 0 10 400 400 cm /Im1 Do Q\n\
        BT /F1 10 Tf 1 0 0 1 55 285 Tm (Lead) Tj 1 0 0 1 150 285 Tm (Se) Tj 1 0 0 1 250 285 Tm (P+) Tj \
        1 0 0 1 55 265 Tm (P236) Tj 1 0 0 1 150 265 Tm (96.29) Tj 1 0 0 1 250 265 Tm (100) Tj \
        1 0 0 1 55 250 Tm (P237) Tj 1 0 0 1 150 250 Tm (100) Tj 1 0 0 1 250 250 Tm (85.71) Tj \
        1 0 0 1 55 235 Tm (P238) Tj 1 0 0 1 150 235 Tm (98.5) Tj 1 0 0 1 250 235 Tm (99) Tj ET\n";
    const BOOKTABS_MD: &str = "| Lead | Se | P+ |\n| --- | --- | --- |\n| P236 | 96.29 | 100 |\n\
        | P237 | 100 | 85.71 |\n| P238 | 98.5 | 99 |\n";

    fn only_layout(bytes: &[u8]) -> omniattach_documents::pdf_tables::PdfTableCandidate {
        let found = discover_pdf_tables_bytes("x", bytes, 1, &Options::default()).unwrap();
        assert_eq!(found.candidates.len(), 1, "{found:?}");
        assert_eq!(found.candidates[0].detector, PdfTableDetector::Layout);
        found.candidates[0].clone()
    }

    #[test]
    fn rule_only_table_on_a_page_with_images() {
        let bytes = pdf(BOOKTABS, &[]);
        only_layout(&bytes);
        let result = convert_pdf_table_bytes("x", &bytes, 1, 1, &Options::default()).unwrap();
        assert_eq!(result.markdown, BOOKTABS_MD);
        assert_eq!(
            result.grid.text_assignment,
            omniattach_documents::pdf_tables::PdfTextAssignment::Centre
        );
    }

    #[test]
    fn table_rotated_on_the_page_is_read_upright() {
        for (matrix, rotation) in [("0 1 -1 0 600 0", 90), ("0 -1 1 0 0 600", 270)] {
            let bytes = pdf(&format!("q {matrix} cm\n{BOOKTABS}Q\n"), &[]);
            assert_eq!(only_layout(&bytes).rotation, rotation);
            assert_eq!(markdown(&bytes, 1).unwrap(), BOOKTABS_MD);
        }
    }

    #[test]
    fn mixed_stroked_and_filled_rulings() {
        let content = format!(
            "{}50 160 200 0.5 re f\n{TEXT}",
            GRID.replace("50 160 m 250 160 l ", "")
        );
        let bytes = pdf(&content, &[]);
        strict_declined(&bytes, "mixed");
        assert_eq!(markdown(&bytes, 1).unwrap(), EXPECTED);
    }

    #[test]
    fn table_cut_by_the_page_edge_keeps_its_open_rows() {
        // Header rules only; the column rules run on to the page bottom.
        let content = "50 160 m 250 160 l S 50 130 m 250 130 l S 50 160 m 50 0 l S \
            150 160 m 150 0 l S 250 160 m 250 0 l S\n\
            BT /F1 10 Tf 1 0 0 1 60 140 Tm (Name) Tj 1 0 0 1 160 140 Tm (Value) Tj \
            1 0 0 1 60 110 Tm (Alpha) Tj 1 0 0 1 160 110 Tm (42) Tj \
            1 0 0 1 60 10 Tm (Omega) Tj 1 0 0 1 160 10 Tm (7) Tj ET\n";
        let rows = rows(&markdown(&pdf(content, &[]), 1).unwrap());
        assert_eq!(rows.first().unwrap(), &["Name", "Value"]);
        assert_eq!(rows.last().unwrap(), &["Omega", "7"]);
    }

    #[test]
    fn scanned_page_hints_at_ocr() {
        let bytes = pdf("q 600 0 0 800 0 0 cm /Im1 Do Q\n", &[]);
        let found = discover_pdf_tables_bytes("x", &bytes, 1, &Options::default()).unwrap();
        assert!(found.candidates.is_empty());
        assert!(found.warnings.iter().any(|w| w.contains("OCR")));
    }
}

/// Regressions on locally supplied real PDFs (not distributed).
mod samples {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn sample(var: &str) -> Vec<u8> {
        let path =
            std::env::var(var).unwrap_or_else(|_| panic!("set {var} to the sample PDF path"));
        std::fs::read(path).unwrap()
    }

    fn convert_all(
        bytes: &[u8],
        page: u32,
    ) -> Vec<omniattach_documents::pdf_tables::PdfTableConversion> {
        let options = Options::default();
        let found = discover_pdf_tables_bytes("sample.pdf", bytes, page, &options).unwrap();
        found
            .candidates
            .iter()
            .map(|c| {
                convert_pdf_table_bytes("sample.pdf", bytes, page, c.candidate, &options).unwrap()
            })
            .collect()
    }

    const CODES: &str =
        "96150 96151 96152 96153 96154 96155 90901 90911 90875 90876 94010 94400 96002 95957 90806";
    const DESCRIPTIONS: [(&str, &str); 15] = [
        (
            "96150",
            "Health & behavior assessment (e.g., health- focused clinical interview, behavioral observations, psychophysiological monitoring, health-oriented questionnaires), each 15 minutes face-to-face with the patient; initial assessment",
        ),
        ("96151", "Re-assessment"),
        (
            "96152",
            "Health & behavioral intervention, each 15 minutes, face-to- face; individual",
        ),
        (
            "96153",
            "Health & behavioral intervention, each 15 minutes, face-to- face; group (2 or more patients)",
        ),
        (
            "96154",
            "Health & behavioral intervention, each 15 minutes, face-to- face; family (with the patient present)",
        ),
        (
            "96155",
            "Health & behavioral intervention, each 15 minutes, face-to- face; family (without the patient present)",
        ),
        ("90901", "Biofeedback training by any modality"),
        (
            "90911",
            "Biofeedback training, perineal muscles, anorectal or urethral sphincter, including EMG &/or manometry",
        ),
        (
            "90875",
            "Individual psychophysiology therapy incorporating biofeedback training by any modality (face-to- face with the patient), with psychotherapy (e.g., insight oriented, behavior modifying or supportive psychotherapy); approximately 20-30 minutes",
        ),
        (
            "90876",
            "Individual psychophysiology therapy incorporating biofeedback training by any modality (face-to- face with the patient), with psychotherapy (e.g., insight oriented, behavior modifying or supportive psychotherapy); approximately 40-45 minutes",
        ),
        (
            "94010",
            "Spirometry, including graphic record, total & timed vital capacity, expiratory flow rate measurement(s), with or without maximal voluntary ventilation",
        ),
        ("94400", "Breathing response to CO2 (CO2 response curve)"),
        (
            "96002",
            "Dynamic surface electromyography, during walking or other functional activities, 1- 12 muscles",
        ),
        (
            "95957",
            "Digital analysis of electroencephalogram (EEG) (e.g., for epileptic spike analysis)",
        ),
        (
            "90806",
            "Individual psychotherapy. Insight oriented, behavior modifying &/or supportive, in an office or outpatient facility, approximately 45-50 minutes, face-to- face with the patients",
        ),
    ];

    #[test]
    #[ignore = "needs OMNIATTACH_FILLED_TABLE_SAMPLE"]
    fn filled_sample_codes_and_descriptions() {
        let results = convert_all(&sample("OMNIATTACH_FILLED_TABLE_SAMPLE"), 1);
        assert_eq!(results.len(), 8);
        let (mut codes, mut descriptions, mut sequences) =
            (Vec::new(), BTreeMap::new(), Vec::<usize>::new());
        for result in &results {
            sequences.extend(result.source_sequences.iter().copied());
            for row in rows(&result.markdown) {
                if row.len() == 2
                    && row[0].chars().all(|c| c.is_ascii_digit())
                    && !row[0].is_empty()
                {
                    codes.push(row[0].clone());
                    descriptions.insert(row[0].clone(), row[1].clone());
                }
            }
        }
        assert_eq!(codes, CODES.split(' ').collect::<Vec<_>>());
        assert_eq!(
            sequences.len(),
            sequences.iter().collect::<BTreeSet<_>>().len()
        );
        for (code, expected) in DESCRIPTIONS {
            assert_eq!(descriptions[code], expected, "code {code}");
        }
    }

    #[test]
    #[ignore = "needs OMNIATTACH_OPEN_ROWS_SAMPLE (02-healthbig.pdf)"]
    fn open_rows_sample_pages_two_and_three() {
        let bytes = sample("OMNIATTACH_OPEN_ROWS_SAMPLE");
        let mut names = Vec::new();
        for (page, expected_rows, expected_spans) in
            [(2, 21, vec![1, 5, 5, 10]), (3, 14, vec![1, 7, 6])]
        {
            let found =
                discover_pdf_tables_bytes("sample.pdf", &bytes, page, &Options::default()).unwrap();
            assert_eq!(found.candidates.len(), 1, "page {page}");
            let c = &found.candidates[0];
            assert_eq!(c.x_boundaries.len(), 5);
            assert_eq!(c.y_boundaries.len() - 1, expected_rows);
            let spans: Vec<usize> = c
                .cell_spans
                .iter()
                .filter(|s| s.column == 0)
                .map(|s| s.row_span)
                .collect();
            assert_eq!(spans, expected_spans);
            let result = &convert_all(&bytes, page)[0];
            let table = rows(&result.markdown);
            assert_eq!(table.len(), expected_rows);
            assert_eq!(
                table[0],
                [
                    "Apps type",
                    "Apps name",
                    "Country",
                    "Introduction and function"
                ]
            );
            names.extend(
                table[1..]
                    .iter()
                    .filter(|r| !r[1].is_empty())
                    .map(|r| r[1].clone()),
            );
            if page == 2 {
                assert_eq!(table[1][1], "iTinnitus [40]");
                assert_eq!(table[table.len() - 1][1..], ["HealthPROMISE [58]", "", ""]);
            } else {
                assert_eq!(table[1][..3], ["", "", "United States"]);
                assert!(table[1][3].starts_with("Patient-engaged care"));
                assert_eq!(table[table.len() - 1][1], "eCAALYX [37]");
                assert!(table[table.len() - 1][3].contains("multiple chronic conditions"));
            }
        }
        assert_eq!(names.len(), 32);
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), 32);
    }

    /// One expected layout candidate on a sample page.
    struct Expected {
        file: &'static str,
        page: u32,
        candidates: usize,
        /// One-based candidate checked below.
        candidate: usize,
        header: &'static [&'static str],
        columns: usize,
        rows: usize,
        rotation: u16,
    }

    const LAYOUT: [Expected; 7] = [
        Expected {
            file: "03-ecgacqu.pdf",
            page: 2,
            candidates: 1,
            candidate: 1,
            header: &[
                "Patient ﬁle ID and record no. in Physionet",
                "Lead I",
                "Lead III",
            ],
            columns: 6,
            rows: 12,
            rotation: 0,
        },
        Expected {
            file: "04-ecgacqu.pdf",
            page: 2,
            candidates: 2,
            candidate: 2,
            header: &["Patient ID and lead number from Physionet", "Se", "P+"],
            columns: 3,
            rows: 9,
            rotation: 0,
        },
        Expected {
            file: "06-cyberbullying.pdf",
            page: 7,
            candidates: 1,
            candidate: 1,
            header: &["Study", "Sample", "Time frame/Type of assessment"],
            columns: 5,
            rows: 3,
            rotation: 90,
        },
        Expected {
            file: "07-redcyberbull.pdf",
            page: 2,
            candidates: 1,
            candidate: 1,
            header: &["Semester", "Month", "Target group", "Activity"],
            columns: 4,
            rows: 11,
            rotation: 0,
        },
        Expected {
            file: "08-redcyberbullschools.pdf",
            page: 2,
            candidates: 1,
            candidate: 1,
            header: &["Name", "Aim", "Content", "Strategy"],
            columns: 5,
            rows: 5,
            rotation: 0,
        },
        Expected {
            file: "09-funbiochemi.pdf",
            page: 1,
            candidates: 1,
            candidate: 1,
            header: &["Type", "Enzyme Deﬁciency", "Tissue", "Common Name"],
            columns: 5,
            rows: 11,
            rotation: 0,
        },
        Expected {
            file: "10-neurofeedback.pdf",
            page: 1,
            candidates: 1,
            candidate: 1,
            header: &["Symptome", "Schweregrad (1 10)"],
            columns: 2,
            rows: 39,
            rotation: 0,
        },
    ];

    #[test]
    #[ignore = "needs OMNIATTACH_TABLE_SAMPLES (directory of layout samples)"]
    fn layout_samples_booktabs_rotated_mixed_and_shaded() {
        let dir = std::path::PathBuf::from(
            std::env::var("OMNIATTACH_TABLE_SAMPLES").expect("set OMNIATTACH_TABLE_SAMPLES"),
        );
        for Expected {
            file,
            page,
            candidates: count,
            candidate,
            header,
            columns,
            rows: rows_expected,
            rotation,
        } in LAYOUT
        {
            let bytes = std::fs::read(dir.join(file)).unwrap();
            let found = discover_pdf_tables_bytes(file, &bytes, page, &Options::default()).unwrap();
            assert_eq!(found.candidates.len(), count, "{file} p{page}");
            let c = &found.candidates[candidate - 1];
            assert_eq!(c.detector, PdfTableDetector::Layout, "{file}");
            assert_eq!(c.rotation, rotation, "{file}");
            assert_eq!(c.x_boundaries.len() - 1, columns, "{file}");
            assert_eq!(c.y_boundaries.len() - 1, rows_expected, "{file}");
            let result =
                convert_pdf_table_bytes(file, &bytes, page, candidate, &Options::default())
                    .unwrap();
            let table = rows(&result.markdown);
            assert_eq!(&table[0][..header.len()], header, "{file}");
        }
        // Every rotated page of the chapter's Table 5.1 is found.
        let bytes = std::fs::read(dir.join("06-cyberbullying.pdf")).unwrap();
        for page in 6..=18 {
            let found = discover_pdf_tables_bytes("06", &bytes, page, &Options::default()).unwrap();
            assert_eq!(found.candidates.len(), 1, "page {page}");
            assert_eq!(found.candidates[0].rotation, 90, "page {page}");
        }
        // Scanned pages yield nothing natively and point at the OCR path.
        for file in ["05-bluetoothlow.pdf", "11-starcraft.pdf", "12-tarot.pdf"] {
            let bytes = std::fs::read(dir.join(file)).unwrap();
            let found = discover_pdf_tables_bytes(file, &bytes, 1, &Options::default()).unwrap();
            assert!(found.candidates.is_empty(), "{file}");
            assert!(found.warnings.iter().any(|w| w.contains("OCR")), "{file}");
        }
    }
}

#![cfg(feature = "pdf")]
use lopdf::{Document as Pdf, Object, Stream, dictionary};
use omniattach_documents::*;
use std::sync::Arc;

fn fixture(contents: &[&str], form: Option<&str>, cycle: bool) -> Vec<u8> {
    let mut pdf = Pdf::with_version("1.5");
    let font=pdf.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica","Encoding"=>"WinAnsiEncoding"});
    let form_id = pdf.new_object_id();
    let mut r = dictionary! {"Font"=>dictionary! {"F1"=>font}};
    if form.is_some() {
        r.set("XObject", dictionary! {"Fm"=>form_id});
    }
    if let Some(text) = form {
        pdf.objects.insert(form_id,Object::Stream(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),200.into(),200.into()],"Matrix"=>vec![1.into(),0.into(),0.into(),1.into(),10.into(),0.into()],"Resources"=>if cycle {r.clone()} else {dictionary! {"Font"=>dictionary! {"F1"=>font}}}},text.as_bytes().to_vec())));
    }
    let pages = pdf.new_object_id();
    let mut kids = Vec::new();
    for text in contents {
        let stream = pdf.add_object(Stream::new(dictionary! {}, text.as_bytes().to_vec()));
        let page = pdf.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>stream});
        kids.push(page.into());
    }
    let count = kids.len() as i64;
    pdf.objects.insert(pages,Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count,"Resources"=>r,"MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()]}));
    let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}
fn raw_options() -> Options {
    Options {
        dehyphenate_max_tokens: 0,
        ..Default::default()
    }
}
fn extract_pdf(contents: &[&str]) -> Document {
    extract_bytes("lines.pdf", &fixture(contents, None, false), &raw_options()).unwrap()
}
#[test]
fn pdf_text_page_provenance_and_scan_diagnostic() {
    let doc = extract_pdf(&["BT /F1 12 Tf 30 50 Td (Hello PDF) Tj ET", ""]);
    assert!(doc.to_text().contains("Hello PDF"));
    assert_eq!(doc.blocks[0].source.page, Some(1));
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PAGE_WITHOUT_TEXT" && d.source.as_ref().unwrap().page == Some(2))
    );
}
#[test]
fn invalid_pdf_is_error_not_silent_empty_success() {
    assert!(extract_bytes("a.pdf", b"not PDF", &raw_options()).is_err());
}
#[test]
fn td_lines_paragraphs_and_line_end_hyphen_survive_markdown() {
    let doc = extract_pdf(&[
        "BT /F1 12 Tf 72 700 Td (A szamito-) Tj 0 -14 Td (gep mukodik.) Tj 0 -14 Td [(Har)-20(om) -300 (szo)] TJ 0 -30 Td (Uj bekezdes.) Tj ET",
    ]);
    let expected = "A szamito-\ngep mukodik.\nHarom szo\n\nUj bekezdes.";
    assert_eq!(doc.to_text(), expected);
    assert_eq!(doc.to_markdown(), format!("{expected}\n"));
    assert_eq!(doc.blocks.len(), 2);
}
#[test]
fn tm_tstar_and_separate_chunks_on_one_line() {
    let doc = extract_pdf(&[
        "BT /F1 10 Tf 14 TL 1 0 0 1 72 700 Tm (first) Tj T* (second) Tj ET BT /F1 10 Tf 1 0 0 1 72 672 Tm (third) Tj 1 0 0 1 120 672 Tm (word) Tj ET BT /F1 10 Tf 1 0 0 1 72 658 Tm (kern) Tj 1 0 0 1 93.5 658 Tm (ed) Tj ET",
    ]);
    assert_eq!(doc.to_text(), "first\nsecond\nthird word\nkerned");
}
#[test]
fn page_numbers_headers_and_page_break_hyphens() {
    let body = [
        "vagy a hala-",
        "los vilagossag",
        "es a sotet ejszaka",
        "a hegyek folott",
    ];
    let text:Vec<_>=body.iter().enumerate().map(|(i,b)| format!("BT /F1 9 Tf 72 800 Td (H. P. Lovecraft) Tj ET BT /F1 12 Tf 72 700 Td ({b}) Tj ET BT /F1 9 Tf 290 60 Td ({}) Tj ET",i+5)).collect();
    let refs: Vec<_> = text.iter().map(String::as_str).collect();
    let doc = extract_pdf(&refs);
    assert_eq!(doc.to_text(), body.join("\n"));
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].source.page, Some(1));
    assert!(doc.to_markdown().contains("hala-\nlos"));
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_PAGE_FURNITURE_REMOVED" && d.message.contains("Removed 8 "))
    );
}
#[test]
fn quote_operators_and_text_state() {
    let doc = extract_pdf(&[
        "BT /F1 10 Tf 14 TL 72 700 Td (one) Tj (two) ' 0 0 (three) \" 0 -14 TD (four) Tj T* (five) Tj ET",
    ]);
    assert_eq!(doc.to_text(), "one\ntwo\nthree\nfour\nfive");
    let doc = extract_pdf(&[
        "BT /F1 10 Tf 2 Tc 3 Tw 80 Tz 72 700 Td (a b) Tj 1 0 0 1 92.8 700 Tm (c) Tj ET",
    ]);
    assert_eq!(doc.to_text(), "a bc");
}
#[test]
fn forms_resources_matrix_and_saved_graphics_state() {
    let bytes = fixture(
        &["q 1 0 0 1 62 700 cm /Fm Do Q BT /F1 10 Tf 1 0 0 1 72 686 Tm (after) Tj ET"],
        Some("BT /F1 10 Tf 0 0 Td (inside) Tj ET"),
        false,
    );
    assert_eq!(
        extract_bytes("form.pdf", &bytes, &raw_options())
            .unwrap()
            .to_text(),
        "inside\nafter"
    );
}
#[test]
fn form_cycles_and_invocation_budget_fail_explicitly() {
    let bytes = fixture(&["/Fm Do"], Some("/Fm Do"), true);
    assert!(extract_bytes("cycle.pdf", &bytes, &raw_options()).is_err());
    let bytes = fixture(
        &["/Fm Do /Fm Do"],
        Some("BT /F1 10 Tf (inside) Tj ET"),
        false,
    );
    let mut options = raw_options();
    options.limits.archive_entries = 1;
    assert!(matches!(
        extract_bytes("forms.pdf", &bytes, &options),
        Err(Error::LimitExceeded(_))
    ));
}
#[test]
fn ocr_success_empty_and_failure_keep_page_diagnostics() {
    let bytes = fixture(&["", "", ""], None, false);
    let mut options = raw_options();
    options.pdf_ocr = Some(PdfOcr(Arc::new(|p| match p {
        1 => Ok("recognized-\ntext".into()),
        2 => Ok(String::new()),
        _ => Err(Error::Parse("fixture".into())),
    })));
    let doc = extract_bytes("ocr.pdf", &bytes, &options).unwrap();
    assert_eq!(doc.to_text(), "recognized-\ntext");
    assert_eq!(doc.blocks[0].source.path.as_deref(), Some("ocr"));
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PAGE_WITHOUT_TEXT")
    );
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_PAGE_OCR_FAILED")
    );
}
#[test]
fn missing_font_is_counted_and_limits_are_not_silent() {
    let doc = extract_pdf(&["BT /Missing 10 Tf (unknown) Tj ET"]);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_UNDECODED_TEXT" && d.message.starts_with("1 "))
    );
    let bytes = fixture(&["BT /F1 10 Tf (body) Tj ET"], None, false);
    let mut options = raw_options();
    options.limits.xml_nodes = 1;
    assert!(matches!(
        extract_bytes("limited.pdf", &bytes, &options),
        Err(Error::LimitExceeded(_))
    ));
}
#[test]
#[ignore = "set OMNIATTACH_PDF_LINES_DIR; requires pdftotext and four authorized local fixtures"]
fn handoff_pdfs_match_poppler_words_and_source_lines() {
    let dir = std::path::PathBuf::from(
        std::env::var("OMNIATTACH_PDF_LINES_DIR").expect("set OMNIATTACH_PDF_LINES_DIR"),
    );
    for name in ["hpl.pdf", "hpl_tm.pdf", "hpl_td.pdf", "lo.pdf"] {
        let path = dir.join(name);
        let output = std::process::Command::new("pdftotext")
            .arg("-layout")
            .arg("-enc")
            .arg("UTF-8")
            .arg(&path)
            .arg("-")
            .output()
            .unwrap();
        assert!(output.status.success(), "pdftotext failed for {name}");
        let reference = clean_reference(&String::from_utf8(output.stdout).unwrap());
        let doc = extract(&path, &raw_options()).unwrap();
        let body = doc
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Paragraph))
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            body.split_whitespace().collect::<Vec<_>>(),
            reference.split_whitespace().collect::<Vec<_>>(),
            "word mismatch: {name}"
        );
        let lines = |s: &str| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
        };
        assert_eq!(lines(&body), lines(&reference), "line mismatch: {name}");
        assert!(!body.contains("H. P. Lovecraft legjobb művei"));
    }
}
fn clean_reference(text: &str) -> String {
    use std::collections::BTreeMap;
    let mut pages: Vec<Vec<String>> = text
        .split('\u{c}')
        .map(|p| {
            p.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_owned())
                .collect()
        })
        .collect();
    if pages.last().is_some_and(Vec::is_empty) {
        pages.pop();
    }
    let key = |s: &str| {
        s.chars()
            .filter(|c| !c.is_ascii_digit())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    for _ in 0..2 {
        for page in &mut pages {
            let is_num = |s: &str| {
                let s = s.trim().trim_matches(['-', '–', '—', ' ']);
                !s.is_empty()
                    && s.len() <= 8
                    && (s.chars().all(|c| c.is_ascii_digit())
                        || s.chars().all(|c| "ivxlcdm".contains(c)))
            };
            if page.last().is_some_and(|s| is_num(s)) {
                page.pop();
            }
            if page.first().is_some_and(|s| is_num(s)) {
                page.remove(0);
            }
        }
    }
    let threshold = (pages.len() / 4).max(3);
    for _ in 0..2 {
        let mut counts: [BTreeMap<String, usize>; 2] = Default::default();
        for page in &pages {
            for (slot, line) in [page.first(), page.last()].into_iter().enumerate() {
                if let Some(line) = line {
                    let k = key(line);
                    if !k.is_empty() && k.chars().count() <= 80 {
                        *counts[slot].entry(k).or_default() += 1;
                    }
                }
            }
        }
        for page in &mut pages {
            let first = page
                .first()
                .is_some_and(|s| counts[0].get(&key(s)).is_some_and(|n| *n >= threshold));
            let last = page
                .last()
                .is_some_and(|s| counts[1].get(&key(s)).is_some_and(|n| *n >= threshold));
            if page.len() == 2 && first && last {
                continue;
            }
            if last {
                page.pop();
            }
            if first && !page.is_empty() {
                page.remove(0);
            }
        }
    }
    pages.into_iter().flatten().collect::<Vec<_>>().join("\n")
}
#[test]
fn explicit_zero_width_and_cid_ranges_control_chunk_gaps() {
    let bytes = fixture(
        &["BT /F1 10 Tf 72 700 Td (A) Tj 1 0 0 1 75 700 Tm (B) Tj ET"],
        None,
        false,
    );
    let mut pdf = Pdf::load_mem(&bytes).unwrap();
    let font = pdf
        .objects
        .iter()
        .find_map(|(id, o)| {
            o.as_dict()
                .ok()
                .filter(|d| {
                    d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Font".as_slice())
                })
                .map(|_| *id)
        })
        .unwrap();
    let d = pdf.get_object_mut(font).unwrap().as_dict_mut().unwrap();
    d.set("FirstChar", 65);
    d.set("Widths", vec![Object::Integer(0), Object::Integer(500)]);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    assert_eq!(
        extract_bytes("width.pdf", &bytes, &raw_options())
            .unwrap()
            .to_text(),
        "A B"
    );
    let bytes = fixture(
        &[
            "BT /F1 10 Tf 72 700 Td <0001> Tj 1 0 0 1 78 700 Tm <0002> Tj 1 0 0 1 85 700 Tm <0003> Tj ET",
        ],
        None,
        false,
    );
    let mut pdf = Pdf::load_mem(&bytes).unwrap();
    let font = pdf
        .objects
        .iter()
        .find_map(|(id, o)| {
            o.as_dict()
                .ok()
                .filter(|d| {
                    d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Font".as_slice())
                })
                .map(|_| *id)
        })
        .unwrap();
    let descendant=pdf.add_object(dictionary! {"Type"=>"Font","Subtype"=>"CIDFontType2","BaseFont"=>"Fixture","DW"=>1000,"W"=>vec![Object::Integer(1),Object::Array(vec![Object::Integer(200)]),Object::Integer(2),Object::Integer(2),Object::Integer(700)]});
    let cmap=pdf.add_object(Stream::new(dictionary! {},b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Fixture def /CMapType 2 def 1 begincodespacerange <0000> <ffff> endcodespacerange 3 beginbfchar <0001> <0041> <0002> <0042> <0003> <0043> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end".to_vec()));
    pdf.objects.insert(font,Object::Dictionary(dictionary! {"Type"=>"Font","Subtype"=>"Type0","BaseFont"=>"Fixture","Encoding"=>"Identity-H","DescendantFonts"=>vec![Object::Reference(descendant)],"ToUnicode"=>cmap}));
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    assert_eq!(
        extract_bytes("cid.pdf", &bytes, &raw_options())
            .unwrap()
            .to_text(),
        "A BC"
    );
}
#[test]
fn default_dehyphenation_joins_across_pages_and_zero_preserves_lines() {
    let bytes = fixture(
        &[
            "BT /F1 12 Tf 72 700 Td (A szamito-) Tj ET",
            "BT /F1 12 Tf 72 700 Td (gep mukodik.) Tj ET",
        ],
        None,
        false,
    );
    let doc = extract_bytes("joined.pdf", &bytes, &Options::default()).unwrap();
    assert_eq!(doc.to_text(), "A szamitogep\nmukodik.");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].source.page, Some(1));
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "DEHYPHENATED" && d.message.contains("Joined 1 "))
    );
    assert!(
        !doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_TEXT_ONLY" && d.message.contains("No de-hyphenation"))
    );
    let raw = extract_bytes("joined.pdf", &bytes, &raw_options()).unwrap();
    assert_eq!(raw.to_text(), "A szamito-\ngep mukodik.");
    assert!(!raw.diagnostics.iter().any(|d| d.code == "DEHYPHENATED"));
}
#[test]
fn default_dehyphenation_includes_ocr_blocks() {
    let options = Options {
        pdf_ocr: Some(PdfOcr(Arc::new(|_| Ok("recog-\nnized text".into())))),
        ..Default::default()
    };
    let doc = extract_bytes("ocr.pdf", &fixture(&[""], None, false), &options).unwrap();
    assert_eq!(doc.to_text(), "recognized\ntext");
    assert_eq!(doc.blocks[0].source.path.as_deref(), Some("ocr"));
}

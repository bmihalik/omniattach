#![cfg(feature="pdf")]
use omniattach_documents::*;
fn inspection(name:&str,kinds:&[PdfElementKind])->PdfInspection {
    PdfInspection {schema_version:"omniattach/pdf-inspection/0.1".into(),source:Source {name:name.into(),sha256:"a".repeat(64),byte_length:0,encoding:None},page_count:1,coordinate_space:"fixture".into(),diagnostics:vec![],elements:kinds.iter().enumerate().map(|(i,kind)| PdfElement {sequence:i+1,kind:*kind,anchor:PdfElementAnchor {page:1,operation_visit:i+1,form_path:vec![],object_id:None,resource_name:None},bounds:None,text:None,word_gap_before:false,pixel_size:None,derived_from:vec![],evidence:vec![]}).collect()}
}
#[test]
fn transliterates_accents_nonlatin_scripts_and_cross_platform_basenames() {
    assert_eq!(sanitize_document_stem("Műszaki összefoglaló.PDF"),"muszaki-osszefoglalo");
    assert_eq!(sanitize_document_stem("Ű.pdf"),"u");
    assert_eq!(sanitize_document_stem("北京.pdf"),"bei-jing");
    assert_eq!(sanitize_document_stem("Москва.pdf"),"moskva");
    assert_eq!(sanitize_document_stem(r"C:\books\Műszaki.pdf"),"muszaki");
    assert_eq!(sanitize_document_stem("/books/Műszaki.pdf"),"muszaki");
}
#[test]
fn sanitizes_controls_punctuation_unknowns_reserved_names_and_length() {
    assert_eq!(sanitize_document_stem(" ../../A # B?.pdf"),"a-b");
    assert_eq!(sanitize_document_stem("a\0b.pdf"),"a-b");
    assert_eq!(sanitize_document_stem("---.pdf"),"document");
    assert_eq!(sanitize_document_stem("\u{10ffff}.pdf"),"document");
    assert_eq!(sanitize_document_stem("CON.pdf"),"document-con");
    assert_eq!(sanitize_document_stem("LPT1.pdf"),"document-lpt1");
    let long=sanitize_document_stem(&format!("{}.pdf","á".repeat(300))); assert_eq!(long.len(),64);
    assert!(long.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b==b'-'));
}
#[test]
fn one_nontext_counter_generates_images_paths_without_mutating_source() {
    use PdfElementKind::*;
    let report=inspection("Műszaki.pdf",&[Text,Image,Text,EquationCandidate,TableCandidate,Image]); let before=report.clone();
    let plan=report.naming_plan().unwrap(); assert_eq!(report,before); assert_eq!(plan.source(),&report.source); assert_eq!(plan.elements().len(),4);
    assert_eq!(plan.elements().iter().map(|e| e.index()).collect::<Vec<_>>(),vec![1,2,3,4]);
    assert_eq!(plan.elements().iter().map(|e| e.element_sequence()).collect::<Vec<_>>(),vec![2,4,5,6]);
    assert_eq!(plan.elements()[0].image_path("PNG").unwrap(),"images/muszaki-aaaaaaaa-fe-000001.png");
    assert_eq!(plan.elements()[1].file_name("tex").unwrap(),"muszaki-aaaaaaaa-fe-000002.tex");
    assert_eq!(plan.elements()[2].kind(),TableCandidate); assert!(plan.elements()[3].stem().ends_with("-fe-000004"));
    assert_eq!(plan,report.naming_plan().unwrap());
    let json:serde_json::Value=serde_json::from_str(&plan.to_json().unwrap()).unwrap(); assert_eq!(json["source"]["name"],"Műszaki.pdf"); assert_eq!(json["source"]["sha256"],"a".repeat(64));
}
#[test]
fn malformed_hashes_sequences_pages_and_extensions_fail_explicitly() {
    let mut report=inspection("book.pdf",&[PdfElementKind::Image]);
    for hash in ["a".repeat(63),"z".repeat(64)] {report.source.sha256=hash; assert!(report.naming_plan().is_err());}
    report.source.sha256="A".repeat(64); assert_eq!(lfe_document_prefix(&report.source).unwrap(),"book-aaaaaaaa");
    assert!(LfeNamingPlan::from_inspection_with_hash_len(&report,7).is_err()); assert!(LfeNamingPlan::from_inspection_with_hash_len(&report,65).is_err());
    report.elements[0].sequence=2; assert!(report.naming_plan().is_err()); report.elements[0].sequence=1;
    report.elements[0].anchor.page=0; assert!(report.naming_plan().is_err()); report.elements[0].anchor.page=1;
    let plan=report.naming_plan().unwrap(); let name=&plan.elements()[0];
    for extension in ["","../png",".png","png/x","png\\x","pñg"] {assert!(name.file_name(extension).is_err());}
    assert!(name.image_path("csv").is_err()); assert!(name.image_path("svg").is_err());
}
#[test]
fn detects_short_hash_or_plan_collisions_and_allows_explicit_longer_prefix() {
    let mut a=inspection("book.pdf",&[PdfElementKind::Image]); let mut b=a.clone();
    a.source.sha256=format!("aaaaaaaa{}","0".repeat(56)); b.source.sha256=format!("aaaaaaaa{}","1".repeat(56));
    let pa=a.naming_plan().unwrap(); let pb=b.naming_plan().unwrap(); assert_eq!(pa.document_prefix(),pb.document_prefix()); assert!(pa.check_collisions(&pb).is_err());
    assert!(pa.check_collisions(&pa.clone()).is_ok());
    let longer=LfeNamingPlan::from_inspection_with_hash_len(&b,12).unwrap(); assert!(pa.check_collisions(&longer).is_ok());
    let mut changed=a.clone(); changed.elements[0].kind=PdfElementKind::TableCandidate; assert!(pa.check_collisions(&changed.naming_plan().unwrap()).is_err());
    let empty=inspection("book.pdf",&[PdfElementKind::Text]); assert!(empty.naming_plan().unwrap().elements().is_empty());
}

use clap::Parser;
use omniattach_cli::{args::{Cli,Command,TableOutputFormat},dispatch,Runtime,service::InspectionOnlyEngine};
#[test]
fn parses_explicit_selection_and_default_json() {
    let cli=Cli::try_parse_from(["omniattach","pdf-table","input.pdf","--page","2","--candidate","3"]).unwrap();
    let Command::PdfTable(args)=cli.command else { panic!("wrong command"); };
    assert_eq!(args.selection.page,2); assert_eq!(args.candidate,3); assert_eq!(args.output_format,TableOutputFormat::Json);
}
#[test]
fn requires_page_and_rejects_zero_page() {
    assert!(Cli::try_parse_from(["omniattach","pdf-tables","input.pdf"]).is_err());
    assert!(Cli::try_parse_from(["omniattach","pdf-tables","input.pdf","--page","0"]).is_err());
}
#[test]
fn zero_candidate_is_usage_error_without_stdout() {
    let cli=Cli::try_parse_from(["omniattach","pdf-table","missing.pdf","--page","1","--candidate","0"]).unwrap();
    let mut out=Vec::new();
    let error=dispatch(&cli,&InspectionOnlyEngine,&Runtime::default(),&mut out).unwrap_err();
    assert_eq!(error.code,"INVALID_ARGUMENT"); assert!(out.is_empty());
}
#[cfg(not(feature="documents"))]
#[test]
fn missing_documents_is_explicitly_unsupported() {
    let cli=Cli::try_parse_from(["omniattach","pdf-tables","missing.pdf","--page","1"]).unwrap();
    let mut out=Vec::new();
    let error=dispatch(&cli,&InspectionOnlyEngine,&Runtime::default(),&mut out).unwrap_err();
    assert_eq!(error.code,"UNSUPPORTED"); assert!(out.is_empty());
}

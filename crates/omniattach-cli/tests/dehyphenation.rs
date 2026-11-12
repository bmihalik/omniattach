use clap::Parser;
use omniattach_cli::{args::{Cli,Command},service::make_request};
fn request(args:&[&str]) -> omniattach_cli::Result<omniattach_cli::service::EngineRequest> {
    let cli=Cli::try_parse_from(std::iter::once("omniattach").chain(args.iter().copied())).unwrap();
    let Command::Plan(plan)=cli.command else {panic!("expected plan")};
    make_request(&plan.request)
}
#[test]
fn flag_is_optional_and_forwards_zero_and_valid_budgets() {
    assert!(request(&["plan","a.pdf"]).unwrap().options.get("dehyphenate_max_tokens").is_none());
    for n in ["0","6","64"] {
        let r=request(&["plan","a.pdf","--dehyphenate-max-tokens",n]).unwrap();
        assert_eq!(r.options["dehyphenate_max_tokens"],n.parse::<u64>().unwrap());
    }
    let r=request(&["plan","a.pdf","--options-json","{\"ocr\":true}","--dehyphenate-max-tokens","0"]).unwrap();
    assert_eq!(r.options,serde_json::json!({"ocr":true,"dehyphenate_max_tokens":0}));
}
#[test]
fn out_of_range_and_duplicate_sources_are_rejected() {
    for n in ["-1","65","1.5","true"] { assert!(Cli::try_parse_from(["omniattach","plan","a.pdf","--dehyphenate-max-tokens",n]).is_err()); }
    assert!(request(&["plan","a.pdf","--options-json","{\"dehyphenate_max_tokens\":6}","--dehyphenate-max-tokens","6"]).is_err());
    let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("options.json");
    std::fs::write(&path,"{\"dehyphenate_max_tokens\":0}").unwrap();
    assert!(request(&["plan","a.pdf","--options-file",path.to_str().unwrap(),"--dehyphenate-max-tokens","0"]).is_err());
}

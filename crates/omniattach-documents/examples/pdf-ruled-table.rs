//! Thin wrapper around the reusable STEP-5 library API.
#[cfg(feature = "pdf")]
fn main()->Result<(),Box<dyn std::error::Error>> {
    use omniattach_documents::{Options,pdf_tables::discovery::{discover_pdf_tables,convert_pdf_table}};
    let args:Vec<String>=std::env::args().skip(1).collect();
    if args.len()!=3 { return Err("usage: pdf-ruled-table INPUT.pdf PAGE list|CANDIDATE_NUMBER".into()); }
    let page:u32=args[1].parse()?;
    let options=Options::default();
    if args[2]=="list" {
        println!("{}",serde_json::to_string_pretty(&discover_pdf_tables(&args[0],page,&options)?)?);
    } else {
        let result=convert_pdf_table(&args[0],page,args[2].parse()?,&options)?;
        eprintln!("page {page}, candidate {}; first row is header; estimated text bounds; source SHA-256 {}",args[2],result.source_sha256);
        print!("{}",result.markdown);
    }
    Ok(())
}
#[cfg(not(feature = "pdf"))]
fn main() { eprintln!("requires --features pdf"); std::process::exit(1); }

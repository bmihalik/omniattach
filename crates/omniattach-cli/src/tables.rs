//! Opt-in synchronous table commands through the facade's documents re-export.
use crate::{CliError,Result,Runtime,args::{PdfTablesArgs,PdfTableArgs}};
use std::io::Write;
#[cfg(feature="documents")]
use crate::args::TableOutputFormat;

#[cfg(feature="documents")]
fn options(args:&PdfTablesArgs)->Result<omniattach::documents::Options> {
    if args.page==0 { return Err(CliError::usage("page is one-based")); }
    let mut options=omniattach::documents::Options::default();
    let cap=usize::try_from(args.max_input_bytes).map_err(|_|CliError::usage("max-input-bytes is too large on this platform"))?;
    options.limits.input_bytes=options.limits.input_bytes.min(cap);
    Ok(options)
}
#[cfg(feature="documents")]
fn map_error(error:omniattach::documents::Error)->CliError {
    match error {
        omniattach::documents::Error::LimitExceeded(message)=>CliError::limit(message),
        other=>CliError::input(other.to_string()),
    }
}
#[cfg(feature="documents")]
fn emit<T:serde::Serialize>(value:&T,pretty:bool,limit:u64,runtime:&Runtime,out:&mut dyn Write)->Result<()> {
    let mut buffer=if pretty { serde_json::to_vec_pretty(value)? } else { serde_json::to_vec(value)? };
    buffer.push(b'\n');
    if buffer.len() as u64>limit { return Err(CliError::limit("table command output bytes")); }
    runtime.check()?; out.write_all(&buffer)?; Ok(())
}
pub(super) fn list(args:&PdfTablesArgs,pretty:bool,runtime:&Runtime,out:&mut dyn Write)->Result<()> {
    #[cfg(feature="documents")]
    {
        runtime.check()?;
        let options=options(args)?;
        let result=omniattach::documents::pdf_tables::discovery::discover_pdf_tables(&args.input,args.page,&options).map_err(map_error)?;
        emit(&result,pretty,args.max_output_bytes,runtime,out)
    }
    #[cfg(not(feature="documents"))]
    { let _=(args,pretty,runtime,out); Err(CliError::unsupported("pdf-tables requires the documents Cargo feature")) }
}
pub(super) fn convert(args:&PdfTableArgs,pretty:bool,runtime:&Runtime,out:&mut dyn Write)->Result<()> {
    if args.candidate==0 { return Err(CliError::usage("candidate numbers are one-based")); }
    #[cfg(feature="documents")]
    {
        runtime.check()?;
        let options=options(&args.selection)?;
        let result=omniattach::documents::pdf_tables::discovery::convert_pdf_table(&args.selection.input,args.selection.page,args.candidate,&options).map_err(map_error)?;
        match args.output_format {
            TableOutputFormat::Json=>emit(&result,pretty,args.selection.max_output_bytes,runtime,out),
            TableOutputFormat::Markdown=>{
                if result.markdown.len() as u64>args.selection.max_output_bytes { return Err(CliError::limit("table command output bytes")); }
                runtime.check()?; out.write_all(result.markdown.as_bytes())?; Ok(())
            }
        }
    }
    #[cfg(not(feature="documents"))]
    { let _=(args,pretty,runtime,out); Err(CliError::unsupported("pdf-table requires the documents Cargo feature")) }
}

//! Page-local stroked grids and bounded filled-border band discovery.
use super::{PdfTableGrid,PdfTableConversion,convert_inspection};
use crate::{Error,Options,Result};
use lopdf::{Object,content::Content};
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use std::{io::Read,path::Path};
#[path="filled.rs"] mod filled;
type Point=[f64;2];
type Matrix=[f64;6];
const ID: Matrix=[1.,0.,0.,1.,0.,0.];
/// Zero-based, top-down cell anchor and extent. Current fallback uses row spans only.
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct PdfTableSpan { pub row:usize,pub column:usize,pub row_span:usize,pub column_span:usize }
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct PdfTableCandidate {
    pub candidate: usize,
    pub page: u32,
    pub x_boundaries: Vec<f64>,
    pub y_boundaries: Vec<f64>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub cell_spans: Vec<PdfTableSpan>,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct PdfTableDiscovery {
    pub schema_version: String,
    pub source: String,
    pub source_sha256: String,
    pub page: u32,
    pub candidates: Vec<PdfTableCandidate>,
    pub warnings: Vec<String>,
}
fn invalid(message: impl Into<String>)->Error { Error::InvalidInput(message.into()) }
fn number(o:&Object)->Result<f64> {
    let n=match o { Object::Integer(v)=>*v as f64,Object::Real(v)=>*v as f64,_=>return Err(invalid("non-numeric geometry")) };
    if !n.is_finite() { return Err(invalid("non-finite geometry")); } Ok(n)
}
fn point(m:Matrix,p:Point)->Point { [m[0]*p[0]+m[2]*p[1]+m[4],m[1]*p[0]+m[3]*p[1]+m[5]] }
fn compose(a:Matrix,b:Matrix)->Matrix {
    [a[0]*b[0]+a[1]*b[2],a[0]*b[1]+a[1]*b[3],a[2]*b[0]+a[3]*b[2],a[2]*b[1]+a[3]*b[3],a[4]*b[0]+a[5]*b[2]+b[4],a[4]*b[1]+a[5]*b[3]+b[5]]
}
fn grid(segments:&[(Point,Point)])->Option<(Vec<f64>,Vec<f64>)> {
    if segments.is_empty() { return None; }
    let left=segments.iter().flat_map(|(a,b)|[a[0],b[0]]).min_by(f64::total_cmp)?;
    let right=segments.iter().flat_map(|(a,b)|[a[0],b[0]]).max_by(f64::total_cmp)?;
    let bottom=segments.iter().flat_map(|(a,b)|[a[1],b[1]]).min_by(f64::total_cmp)?;
    let top=segments.iter().flat_map(|(a,b)|[a[1],b[1]]).max_by(f64::total_cmp)?;
    if right-left<=0.5 || top-bottom<=0.5 { return None; }
    let mut xs=Vec::new(); let mut ys=Vec::new();
    for (a,b) in segments {
        if (a[1]-b[1]).abs()<=0.01 && (a[0].min(b[0])-left).abs()<=0.01 && (a[0].max(b[0])-right).abs()<=0.01 { ys.push(a[1]); }
        else if (a[0]-b[0]).abs()<=0.01 && (a[1].min(b[1])-bottom).abs()<=0.01 && (a[1].max(b[1])-top).abs()<=0.01 { xs.push(a[0]); }
        else { return None; }
    }
    for axis in [&mut xs,&mut ys] {
        axis.sort_by(f64::total_cmp); axis.dedup_by(|a,b|(*a-*b).abs()<=0.01);
        if axis.len()<3 || axis.len()>257 || axis.windows(2).any(|w|w[1]-w[0]<=0.5) { return None; }
    }
    if (xs[0]-left).abs()>0.01 || (xs[xs.len()-1]-right).abs()>0.01 || (ys[0]-bottom).abs()>0.01 || (ys[ys.len()-1]-top).abs()>0.01 { return None; }
    Some((xs,ys))
}
fn filled_content(content:&Content)->bool {
    content.operations.iter().any(|o|o.operator=="re") && content.operations.iter().any(|o|matches!(o.operator.as_str(),"f"|"F"|"f*"))
}
fn discover(content:Content,page:u32,options:&Options)->Result<Vec<PdfTableCandidate>> {
    if filled_content(&content) { return filled::discover(content,page,options); }
    if content.operations.len()>options.limits.xml_nodes as usize { return Err(Error::LimitExceeded("content operations".into())); }
    let mut m=ID; let mut stack=Vec::new(); let mut segments=Vec::new();
    let mut current=None; let mut start=None; let mut bad=false; let mut candidates=Vec::new();
    for op in content.operations {
        let a=&op.operands;
        let pair=|i:usize|->Result<Point> { Ok([number(a.get(i).ok_or_else(||invalid("missing coordinate"))?)?,number(a.get(i+1).ok_or_else(||invalid("missing coordinate"))?)?]) };
        match op.operator.as_str() {
            "q"=>{ if stack.len()>=128 { return Err(Error::LimitExceeded("graphics stack".into())); } stack.push(m); }
            "Q"=>m=stack.pop().ok_or_else(||invalid("unbalanced Q"))?,
            "cm"=>{ if a.len()!=6 { return Err(invalid("invalid cm")); } let v:Vec<f64>=a.iter().map(number).collect::<Result<_>>()?; m=compose([v[0],v[1],v[2],v[3],v[4],v[5]],m); if m.iter().any(|v|!v.is_finite()) { return Err(invalid("matrix overflow")); } }
            "Do"=>return Err(invalid("selected page invokes XObjects; Forms and images unsupported")),
            "W"|"W*"=>return Err(invalid("clipped page content unsupported")),
            "m"=>{ let p=point(m,pair(0)?); current=Some(p); start=Some(p); }
            "l"=>{ let p=point(m,pair(0)?); if let Some(c)=current { segments.push((c,p)); } else { bad=true; } current=Some(p); }
            "re"=>{ let p=pair(0)?; let wh=pair(2)?; let corners=[point(m,p),point(m,[p[0]+wh[0],p[1]]),point(m,[p[0]+wh[0],p[1]+wh[1]]),point(m,[p[0],p[1]+wh[1]])]; for i in 0..4 { segments.push((corners[i],corners[(i+1)%4])); } current=Some(corners[0]); start=current; }
            "h"=>{ if let (Some(c),Some(s))=(current,start) { if c!=s { segments.push((c,s)); } current=Some(s); } }
            "c"|"v"|"y"=>bad=true,
            "S"|"s"|"B"|"B*"|"b"|"b*"|"f"|"F"|"f*"|"n"=>{
                if matches!(op.operator.as_str(),"s"|"b"|"b*") { if let (Some(c),Some(s))=(current,start) { if c!=s { segments.push((c,s)); } } }
                if segments.len()>options.limits.archive_entries { return Err(Error::LimitExceeded("path segments".into())); }
                if segments.iter().any(|(a,b)|a.iter().chain(b.iter()).any(|v|!v.is_finite())) { return Err(invalid("path overflow")); }
                if !bad && matches!(op.operator.as_str(),"S"|"s") {
                    if let Some((xs,ys))=grid(&segments) {
                        if candidates.len()>=128.min(options.limits.blocks) { return Err(Error::LimitExceeded("table candidates".into())); }
                        candidates.push(PdfTableCandidate { candidate:candidates.len()+1,page,x_boundaries:xs,y_boundaries:ys,cell_spans:Vec::new() });
                    }
                }
                segments.clear(); current=None; start=None; bad=false;
            }
            _=>{}
        }
        if segments.len()>options.limits.archive_entries { return Err(Error::LimitExceeded("path segments".into())); }
    }
    if !stack.is_empty() { return Err(invalid("unbalanced q")); } Ok(candidates)
}
fn read(path:&Path,options:&Options)->Result<Vec<u8>> {
    let file=std::fs::File::open(path)?;
    if !file.metadata()?.is_file() { return Err(invalid("PDF input is not a regular file")); }
    let mut bytes=Vec::new(); file.take(options.limits.input_bytes.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    if bytes.len()>options.limits.input_bytes { return Err(Error::LimitExceeded("input bytes".into())); } Ok(bytes)
}
/// Discover page-local candidates, without running OCR or any recognition model.
pub fn discover_pdf_tables_bytes(name:&str,bytes:&[u8],page:u32,options:&Options)->Result<PdfTableDiscovery> {
    if bytes.len()>options.limits.input_bytes { return Err(Error::LimitExceeded("input bytes".into())); }
    let pdf=lopdf::Document::load_mem_with_options(bytes,lopdf::LoadOptions { max_decompressed_size:Some(options.limits.archive_bytes.min(usize::MAX as u64) as usize),..Default::default() }).map_err(|e|Error::Parse(format!("PDF: {e}")))?;
    if pdf.is_encrypted() { return Err(invalid("encrypted PDF unsupported")); }
    let pages=pdf.get_pages(); if pages.len()>options.limits.blocks { return Err(Error::LimitExceeded("PDF pages".into())); }
    let id=*pages.get(&page).ok_or_else(||invalid("page not found; pages are one-based"))?;
    let mut dictionary=pdf.get_dictionary(id).map_err(|e|Error::Parse(e.to_string()))?;
    for depth in 0..32 {
        for (key,expected) in [(b"Rotate".as_slice(),0.0),(b"UserUnit".as_slice(),1.0)] {
            if let Ok(v)=dictionary.get(key) { if number(v)?!=expected { return Err(invalid("non-default page rotation or UserUnit unsupported")); } }
        }
        match dictionary.get(b"Parent") {
            Ok(parent)=>{ if depth==31 { return Err(invalid("page ancestry limit")); } let parent=parent.as_reference().map_err(|e|Error::Parse(e.to_string()))?; dictionary=pdf.get_dictionary(parent).map_err(|e|Error::Parse(e.to_string()))?; }
            Err(_)=>break,
        }
    }
    let data=pdf.get_page_content_with_limit(id,options.limits.archive_bytes.min(usize::MAX as u64) as usize).map_err(|e|Error::Parse(e.to_string()))?;
    let content=Content::decode(&data).map_err(|e|Error::Parse(e.to_string()))?;
    let is_filled=filled_content(&content);
    let candidates=discover(content,page,options)?;
    let has_spans=candidates.iter().any(|c|!c.cell_spans.is_empty());
    let warning=if has_spans { "Open-sided filled-rule fallback; cell_spans are zero-based top-down anchors; outer x limits inferred from repeated horizontal-rule endpoints; row spans only; no column spans or cross-page joins; read discovery alongside conversion; paint visibility not verified" }
        else if is_filled { "Thin filled rectangles only (<=2 units), 0.8-unit join tolerance; rectangular clips must contain every band; section spans split into separate rectangular candidates; first row is formatting, not a semantic header; paint visibility not verified; no mixed strokes, XObjects or continuation" }
        else { "Page-content S/s grids only; no XObjects, clipping, continuation or normalized page geometry" };
    Ok(PdfTableDiscovery { schema_version:(if has_spans {"omniattach/pdf-table-discovery/0.2"} else {"omniattach/pdf-table-discovery/0.1"}).into(),source:name.into(),source_sha256:format!("{:x}",Sha256::digest(bytes)),page,candidates,warnings:vec![warning.into()] })
}
/// Path variant; reads input once with the configured size limit.
pub fn discover_pdf_tables(path:impl AsRef<Path>,page:u32,options:&Options)->Result<PdfTableDiscovery> {
    let path=path.as_ref(); let bytes=read(path,options)?; discover_pdf_tables_bytes(&path.to_string_lossy(),&bytes,page,options)
}
/// Re-discover the one-based page-local candidate and convert from the SAME bytes.
/// The number is not an inspection sequence and must be rediscovered if input changes.
pub fn convert_pdf_table_bytes(name:&str,bytes:&[u8],page:u32,candidate:usize,options:&Options)->Result<PdfTableConversion> {
    let index=candidate.checked_sub(1).ok_or_else(||invalid("candidate numbers are one-based"))?;
    let discovery=discover_pdf_tables_bytes(name,bytes,page,options)?;
    let selected=discovery.candidates.get(index).ok_or_else(||invalid("candidate not found"))?;
    let report=crate::inspect_pdf_bytes(name,bytes,options)?;
    let mut result=if selected.cell_spans.is_empty() { convert_inspection(&report,&PdfTableGrid { page,x_boundaries:selected.x_boundaries.clone(),y_boundaries:selected.y_boundaries.clone() },options)? }
        else { filled::convert_spans(&report,selected,options)? };
    result.warnings.extend(discovery.warnings);
    Ok(result)
}
/// Path variant of selected-candidate conversion, without writing any files.
pub fn convert_pdf_table(path:impl AsRef<Path>,page:u32,candidate:usize,options:&Options)->Result<PdfTableConversion> {
    let path=path.as_ref(); let bytes=read(path,options)?; convert_pdf_table_bytes(&path.to_string_lossy(),&bytes,page,candidate,options)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn transforms_grid() {
        let data=b"2 0 0 2 5 7 cm 0 0 m 20 0 l 0 10 m 20 10 l 0 20 m 20 20 l 0 0 m 0 20 l 10 0 m 10 20 l 20 0 m 20 20 l S";
        let g=discover(Content::decode(data).unwrap(),1,&Options::default()).unwrap();
        assert_eq!(g.len(),1); assert_eq!(g[0].x_boundaries,vec![5.,25.,45.]); assert_eq!(g[0].y_boundaries,vec![7.,27.,47.]);
    }
    #[test] fn unsupported_and_limits_fail() {
        for data in [b"/F Do".as_slice(),b"W n".as_slice()] { assert!(discover(Content::decode(data).unwrap(),1,&Options::default()).is_err()); }
        let mut o=Options::default(); o.limits.xml_nodes=0;
        assert!(discover(Content::decode(b"0 0 m").unwrap(),1,&o).is_err());
    }
    #[test] fn separate_strokes_are_not_combined() {
        assert!(discover(Content::decode(b"0 0 m 20 0 l S 0 10 m 20 10 l S").unwrap(),1,&Options::default()).unwrap().is_empty());
    }
    #[test] fn input_limit_checked_before_parse() {
        let mut o=Options::default(); o.limits.input_bytes=0;
        assert!(matches!(discover_pdf_tables_bytes("x",b"x",1,&o),Err(Error::LimitExceeded(_))));
    }
    #[test] fn zero_candidate_rejected() { assert!(convert_pdf_table_bytes("x",b"",1,0,&Options::default()).is_err()); }
    #[test] fn old_candidate_json_defaults_to_no_spans() {
        let c:PdfTableCandidate=serde_json::from_str(r#"{"candidate":1,"page":1,"x_boundaries":[0,10],"y_boundaries":[0,10]}"#).unwrap();
        assert!(c.cell_spans.is_empty());
    }
}

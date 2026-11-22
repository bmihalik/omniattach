//! Opt-in native-text table reconstruction, separate from ordinary extraction.
//! Grid coordinates use the inspection's unnormalized PDF user space. No model runs.
use crate::{Error, PdfElementKind, PdfInspection, Result};
use serde::{Deserialize, Serialize};
#[path = "pdf_tables/discovery.rs"]
pub mod discovery;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfTableGrid {
    pub page: u32,
    pub x_boundaries: Vec<f64>,
    pub y_boundaries: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfTableConversion {
    pub schema_version: String,
    pub source_name: String,
    pub source_sha256: String,
    pub grid: PdfTableGrid,
    pub source_sequences: Vec<usize>,
    pub first_row_is_header: bool,
    pub markdown: String,
    pub warnings: Vec<String>,
}
/// Low-level positioned text, shared with the discovery example.
#[derive(Clone, Debug)]
pub struct Run {
    pub bounds: [f64; 4],
    pub text: String,
    pub gap: bool,
    pub sequence: usize,
}
fn dimensions(xs: &[f64], ys: &[f64]) -> std::result::Result<(usize,usize),String> {
    for axis in [xs,ys] {
        if axis.len()<2 || axis.len()>257 || axis.iter().any(|n| !n.is_finite()) || axis.windows(2).any(|w| w[1]-w[0]<=0.5) {
            return Err("grid needs 2..=257 finite, ascending boundaries separated by more than 0.5 PDF units".into());
        }
    }
    let columns=xs.len()-1; let rows=ys.len()-1;
    if columns*rows>16384 { return Err("table cell limit exceeded".into()); }
    Ok((columns,rows))
}
/// Reconstruct Markdown from runs; first row is always the header.
/// Fixed 1-unit line tolerance and estimated boxes intentionally limit accuracy.
pub fn convert(xs: &[f64], ys: &[f64], runs: &[Run]) -> std::result::Result<String,String> {
    let (columns,rows)=dimensions(xs,ys)?;
    let mut cells: Vec<Vec<&Run>>=vec![Vec::new();columns*rows];
    let mut text_bytes=0usize;
    for run in runs {
        let [left,bottom,right,top]=run.bounds;
        if run.bounds.iter().any(|n| !n.is_finite()) || left>=right || bottom>=top { return Err(format!("invalid text geometry at sequence {}",run.sequence)); }
        if right<=xs[0] || left>=xs[columns] || top<=ys[0] || bottom>=ys[rows] || run.text.trim().is_empty() { continue; }
        text_bytes=text_bytes.checked_add(run.text.len()).ok_or("text size overflow")?;
        if text_bytes>8*1024*1024 { return Err("table text limit exceeded".into()); }
        let x=xs.windows(2).position(|w| left>=w[0] && right<=w[1]);
        let y=ys.windows(2).position(|w| bottom>=w[0] && top<=w[1]);
        let (Some(x),Some(y))=(x,y) else { return Err(format!("text crosses grid boundary at sequence {}; estimated bounds need review",run.sequence)); };
        cells[(rows-1-y)*columns+x].push(run);
    }
    let mut output=String::new();
    for row in 0..rows {
        output.push('|');
        for column in 0..columns {
            let cell=&mut cells[row*columns+column];
            cell.sort_by(|a,b| b.bounds[3].total_cmp(&a.bounds[3]).then(a.bounds[0].total_cmp(&b.bounds[0])).then(a.sequence.cmp(&b.sequence)));
            let mut text=String::new(); let mut previous: Option<&Run>=None;
            for run in cell.iter().copied() {
                if let Some(last)=previous {
                    if (last.bounds[3]-run.bounds[3]).abs()>1.0 { text.push_str("<br>"); }
                    else if run.gap || run.bounds[0]-last.bounds[2]>1.0 { text.push(' '); }
                }
                for c in run.text.chars() {
                    match c {
                        '&'=>text.push_str("&amp;"), '<'=>text.push_str("&lt;"), '>'=>text.push_str("&gt;"),
                        '|'=>text.push_str("&#124;"), '\\'=>text.push_str("&#92;"), '*'=>text.push_str("&#42;"),
                        '_'=>text.push_str("&#95;"), '`'=>text.push_str("&#96;"), '['=>text.push_str("&#91;"),
                        ']'=>text.push_str("&#93;"), '\n'=>text.push_str("<br>"), '\r'=>{}, c=>text.push(c),
                    }
                }
                previous=Some(run);
            }
            output.push(' '); output.push_str(&text); output.push_str(" |");
        }
        output.push('\n');
        if row==0 { output.push('|'); for _ in 0..columns { output.push_str(" --- |"); } output.push('\n'); }
    }
    Ok(output)
}
/// Convert a supplied grid against an inspection from the same source PDF.
/// Does not attest that the supplied grid is a real ruled table. Rejects unknown
/// text position on the selected page, since exclusion from the grid is unsafe.
pub fn convert_inspection(report: &PdfInspection, grid: &PdfTableGrid, options: &crate::Options) -> Result<PdfTableConversion> {
    let (columns,rows)=dimensions(&grid.x_boundaries,&grid.y_boundaries).map_err(Error::InvalidInput)?;
    if grid.page==0 || grid.page as usize>report.page_count { return Err(Error::InvalidInput("table page out of range".into())); }
    if columns*rows>options.limits.cells { return Err(Error::LimitExceeded("table cells".into())); }
    if report.elements.len()>options.limits.blocks { return Err(Error::LimitExceeded("inspection elements".into())); }
    let xs=&grid.x_boundaries; let ys=&grid.y_boundaries;
    let mut runs=Vec::new(); let mut sequences=Vec::new(); let mut total=0u64;
    for e in &report.elements {
        if e.anchor.page!=grid.page || e.kind!=PdfElementKind::Text { continue; }
        let b=e.bounds.ok_or_else(|| Error::InvalidInput("unpositioned text on selected page".into()))?;
        if [b.left,b.bottom,b.right,b.top].iter().any(|v| !v.is_finite()) || b.left>=b.right || b.bottom>=b.top { return Err(Error::InvalidInput("invalid text geometry".into())); }
        if b.right<=xs[0] || b.left>=xs[columns] || b.top<=ys[0] || b.bottom>=ys[rows] { continue; }
        let text=e.text.as_ref().ok_or_else(|| Error::InvalidInput("undecodable text in table".into()))?;
        if text.trim().is_empty() { continue; }
        total=total.checked_add(text.len() as u64).ok_or_else(|| Error::LimitExceeded("table text bytes".into()))?;
        if total>options.limits.archive_bytes { return Err(Error::LimitExceeded("table text bytes".into())); }
        sequences.push(e.sequence);
        runs.push(Run { bounds:[b.left,b.bottom,b.right,b.top],text:text.clone(),gap:e.word_gap_before,sequence:e.sequence });
    }
    if runs.is_empty() { return Err(Error::InvalidInput("selected grid has no decoded text".into())); }
    let markdown=convert(xs,ys,&runs).map_err(Error::InvalidInput)?;
    if markdown.len() as u64>options.limits.archive_bytes { return Err(Error::LimitExceeded("table Markdown bytes".into())); }
    Ok(PdfTableConversion { schema_version:"omniattach/pdf-table/0.1".into(),source_name:report.source.name.clone(),source_sha256:report.source.sha256.clone(),grid:grid.clone(),source_sequences:sequences,first_row_is_header:true,markdown,warnings:vec!["Estimated text bounds; unnormalized PDF user space; fixed 1-unit line tolerance; first row treated as header".into()] })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(bounds:[f64;4],text:&str)->Run { Run { bounds,text:text.into(),gap:false,sequence:1 } }
    #[test] fn reconstructs_top_down_and_preserves_empty_cells() {
        let runs=[run([1.,11.,9.,19.],"Header"),run([11.,1.,19.,9.],"Value")];
        assert_eq!(convert(&[0.,10.,20.],&[0.,10.,20.],&runs).unwrap(),"| Header |  |\n| --- | --- |\n|  | Value |\n");
    }
    #[test] fn rejects_crossing_text_and_invalid_grids() {
        assert!(convert(&[0.,10.,20.],&[0.,10.],&[run([9.,1.,11.,9.],"bad")]).is_err());
        assert!(convert(&[0.,0.],&[0.,10.],&[]).is_err());
        assert!(convert(&[0.,f64::NAN],&[0.,10.],&[]).is_err());
    }
    #[test] fn escapes_markdown_and_html() {
        assert!(convert(&[0.,10.],&[0.,10.],&[run([1.,1.,9.,9.],"a|<b>&_")]).unwrap().contains("a&#124;&lt;b&gt;&amp;&#95;"));
    }
    #[test] fn joins_fragments_and_multiple_lines() {
        let runs=[run([1.,11.,3.,19.],"A"),run([5.,11.,8.,19.],"B"),run([1.,1.,8.,9.],"C")];
        assert!(convert(&[0.,10.],&[0.,20.],&runs).unwrap().contains("A B<br>C"));
    }
    fn report()->PdfInspection {
        serde_json::from_value(serde_json::json!({"schema_version":"omniattach/pdf-inspection/0.1","source":{"name":"test.pdf","sha256":"test-hash","byte_length":1,"encoding":null},"page_count":1,"coordinate_space":"PDF user space","diagnostics":[],"elements":[{"sequence":7,"kind":"text","anchor":{"page":1,"operation_visit":1,"form_path":[],"object_id":null,"resource_name":null},"bounds":{"left":1.0,"bottom":1.0,"right":9.0,"top":9.0,"estimated":true},"text":"Cell","word_gap_before":false,"pixel_size":null,"derived_from":[],"evidence":[]}]})).unwrap()
    }
    #[test] fn typed_result_retains_provenance_and_serializes() {
        let result=convert_inspection(&report(),&PdfTableGrid { page:1,x_boundaries:vec![0.,10.],y_boundaries:vec![0.,10.] },&crate::Options::default()).unwrap();
        assert_eq!(result.source_sequences,vec![7]); assert_eq!(result.source_sha256,"test-hash");
        assert_eq!(result.markdown,"| Cell |\n| --- |\n");
        let round: PdfTableConversion=serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap(); assert_eq!(result,round);
    }
    #[test] fn typed_api_enforces_page_and_cell_limit() {
        let mut grid=PdfTableGrid { page:0,x_boundaries:vec![0.,10.],y_boundaries:vec![0.,10.] };
        assert!(convert_inspection(&report(),&grid,&crate::Options::default()).is_err());
        grid.page=1; let mut options=crate::Options::default(); options.limits.cells=0;
        assert!(matches!(convert_inspection(&report(),&grid,&options),Err(Error::LimitExceeded(_))));
    }
}

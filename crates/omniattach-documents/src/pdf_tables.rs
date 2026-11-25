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
/// Adjust assignment bounds only, never the original inspection geometry.
/// Caller validates the grid and input box. None means a small incursion from
/// outside this band, not permission to drop substantially overlapping text.
fn estimated_vertical(bottom:f64,top:f64,ys:&[f64])->std::result::Result<Option<(f64,f64)>,String> {
    let height=top-bottom;
    if !height.is_finite() || height<=0.0 { return Err("invalid estimated text height".into()); }
    let middle=bottom+height/2.0;
    let tolerance=1.5_f64.min(0.15*height);
    let last=ys[ys.len()-1];
    if middle<ys[0] || middle>last {
        let intrusion=(top.min(last)-bottom.max(ys[0])).max(0.0);
        if intrusion<=tolerance { return Ok(None); }
        return Err("estimated text substantially overlaps an adjacent band".into());
    }
    let row=ys.windows(2).find(|w| middle>w[0] && middle<w[1])
        .ok_or_else(||"estimated text centre lies on a row boundary".to_string())?;
    let spill=(row[0]-bottom).max(0.0)+(top-row[1]).max(0.0);
    if spill>tolerance { return Err("estimated vertical spill exceeds min(1.5 units, 15% of height)".into()); }
    Ok(Some((bottom.max(row[0]),top.min(row[1]))))
}
/// Reconstruct Markdown from runs; first row is always the header.
/// Strict containment remains the low-level contract; only the typed inspection
/// adapter below may adjust explicitly estimated vertical assignment bounds.
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
/// Explicitly estimated vertical boxes get a bounded row-assignment allowance;
/// horizontal geometry and non-estimated boxes retain strict containment.
pub fn convert_inspection(report: &PdfInspection, grid: &PdfTableGrid, options: &crate::Options) -> Result<PdfTableConversion> {
    let (columns,rows)=dimensions(&grid.x_boundaries,&grid.y_boundaries).map_err(Error::InvalidInput)?;
    if grid.page==0 || grid.page as usize>report.page_count { return Err(Error::InvalidInput("table page out of range".into())); }
    if columns*rows>options.limits.cells { return Err(Error::LimitExceeded("table cells".into())); }
    if report.elements.len()>options.limits.blocks { return Err(Error::LimitExceeded("inspection elements".into())); }
    let xs=&grid.x_boundaries; let ys=&grid.y_boundaries;
    let mut runs=Vec::new(); let mut sequences=Vec::new(); let mut total=0u64;
    let mut adjusted=0usize; let mut excluded=0usize;
    for e in &report.elements {
        if e.anchor.page!=grid.page || e.kind!=PdfElementKind::Text { continue; }
        let b=e.bounds.ok_or_else(|| Error::InvalidInput("unpositioned text on selected page".into()))?;
        if [b.left,b.bottom,b.right,b.top].iter().any(|v| !v.is_finite()) || b.left>=b.right || b.bottom>=b.top { return Err(Error::InvalidInput("invalid text geometry".into())); }
        if b.right<=xs[0] || b.left>=xs[columns] || b.top<=ys[0] || b.bottom>=ys[rows] { continue; }
        let text=e.text.as_ref().ok_or_else(|| Error::InvalidInput("undecodable text in table".into()))?;
        if text.trim().is_empty() { continue; }
        let (bottom,top)=if b.estimated {
            match estimated_vertical(b.bottom,b.top,ys).map_err(|message|Error::InvalidInput(format!("text crosses grid boundary at sequence {}; {message}",e.sequence)))? {
                Some(bounds)=>{ if bounds!=(b.bottom,b.top) { adjusted+=1; } bounds }
                None=>{ excluded+=1; continue; }
            }
        } else { (b.bottom,b.top) };
        total=total.checked_add(text.len() as u64).ok_or_else(|| Error::LimitExceeded("table text bytes".into()))?;
        if total>options.limits.archive_bytes { return Err(Error::LimitExceeded("table text bytes".into())); }
        sequences.push(e.sequence);
        runs.push(Run { bounds:[b.left,bottom,b.right,top],text:text.clone(),gap:e.word_gap_before,sequence:e.sequence });
    }
    if runs.is_empty() { return Err(Error::InvalidInput("selected grid has no decoded text".into())); }
    let markdown=convert(xs,ys,&runs).map_err(Error::InvalidInput)?;
    if markdown.len() as u64>options.limits.archive_bytes { return Err(Error::LimitExceeded("table Markdown bytes".into())); }
    let mut warnings=vec!["Estimated text bounds; unnormalized PDF user space; fixed 1-unit line tolerance; first row treated as header".into()];
    if adjusted>0 || excluded>0 {
        warnings.push(format!("Estimated vertical row assignment: adjusted {adjusted} text runs and excluded {excluded} small adjacent-band incursions; allowance min(1.5 PDF units, 15% of height); horizontal containment remains strict; original inspection bounds unchanged"));
    }
    Ok(PdfTableConversion { schema_version:"omniattach/pdf-table/0.1".into(),source_name:report.source.name.clone(),source_sha256:report.source.sha256.clone(),grid:grid.clone(),source_sequences:sequences,first_row_is_header:true,markdown,warnings })
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
    #[test] fn sample_heading_height_overhang_is_bounded() {
        let ys=[636.4686618993976,650.3226660120883];
        assert_eq!(estimated_vertical(639.7501144371943,651.4168149742009,&ys).unwrap(),Some((639.7501144371943,ys[1])));
    }
    #[test] fn adjacent_band_gets_no_duplicate_heading() {
        let ys=[473.13565165304607,500.1144604580768];
        assert_eq!(estimated_vertical(462.56299517397565,474.22969571098224,&ys).unwrap(),None);
    }
    #[test] fn large_or_ambiguous_vertical_crossings_still_fail() {
        assert!(estimated_vertical(5.,15.,&[0.,10.,20.]).is_err());
        assert!(estimated_vertical(1.,13.,&[0.,10.]).is_err());
        assert!(estimated_vertical(-8.,3.,&[0.,10.]).is_err());
        assert!(estimated_vertical(9.9,10.2,&[0.,10.]).is_err());
    }
    #[test] fn typed_vertical_allowance_is_estimated_only() {
        let mut r=report();
        r.elements[0].bounds.as_mut().unwrap().top=10.5;
        let grid=PdfTableGrid {page:1,x_boundaries:vec![0.,10.],y_boundaries:vec![0.,10.]};
        let result=convert_inspection(&r,&grid,&crate::Options::default()).unwrap();
        assert_eq!(result.source_sequences,vec![7]);
        assert!(result.warnings.iter().any(|w|w.contains("adjusted 1 text runs")));
        assert_eq!(r.elements[0].bounds.unwrap().top,10.5);
        r.elements[0].bounds.as_mut().unwrap().estimated=false;
        assert!(convert_inspection(&r,&grid,&crate::Options::default()).is_err());
    }
    #[test] fn typed_allowance_does_not_relax_column_containment() {
        let mut r=report();
        let b=r.elements[0].bounds.as_mut().unwrap(); b.left=9.; b.right=11.;
        let grid=PdfTableGrid {page:1,x_boundaries:vec![0.,10.,20.],y_boundaries:vec![0.,10.]};
        assert!(convert_inspection(&r,&grid,&crate::Options::default()).is_err());
    }
}

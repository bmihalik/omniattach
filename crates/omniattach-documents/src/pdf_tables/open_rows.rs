//! Bounded fallback for open-sided, filled-rule tables with vertical row spans.
use super::*;
use crate::pdf_tables::{Run,dimensions,estimated_vertical};
use crate::{PdfInspection,PdfElementKind};
fn coverage(lines:&[Line],horizontal:bool,axis:f64,lo:f64,hi:f64)->Option<bool> {
    let mut intervals:Vec<(f64,f64)>=lines.iter().filter(|l|l.horizontal==horizontal && (l.axis-axis).abs()<=T && l.hi>lo+T && l.lo<hi-T).map(|l|(l.lo,l.hi)).collect();
    if intervals.is_empty() { return Some(false); }
    intervals.sort_by(|a,b|a.0.total_cmp(&b.0));
    let mut end=lo;
    for (a,b) in intervals { if a>end+2.*T { return None; } end=end.max(b); }
    if end<hi-T { None } else { Some(true) }
}
fn checked_coverage(lines:&[Line],horizontal:bool,axis:f64,lo:f64,hi:f64,work:&mut usize)->Result<Option<bool>> {
    *work=work.saturating_add(lines.len());
    if *work>4_000_000 { return Err(Error::LimitExceeded("open table geometry work".into())); }
    Ok(coverage(lines,horizontal,axis,lo,hi))
}
pub(super) fn assemble(lines:&[Line],page:u32,options:&Options)->Result<Vec<PdfTableCandidate>> {
    let mut groups:Vec<Vec<Line>>=Vec::new();
    for h in lines.iter().filter(|l|l.horizontal) {
        if let Some(g)=groups.iter_mut().find(|g|(g[0].lo-h.lo).abs()<=T && (g[0].hi-h.hi).abs()<=T) { g.push(*h); } else { groups.push(vec![*h]); }
    }
    if groups.len()>128 { return Err(Error::LimitExceeded("open table rule groups".into())); }
    let mut result=Vec::new(); let mut work=0usize;
    for hs in groups {
        if hs.len()<3 { continue; }
        let left=hs[0].lo; let right=hs[0].hi;
        let bottom=hs.iter().map(|l|l.axis).min_by(f64::total_cmp).unwrap();
        let top=hs.iter().map(|l|l.axis).max_by(f64::total_cmp).unwrap();
        let vs:Vec<&Line>=lines.iter().filter(|l|!l.horizontal && l.hi>bottom+T && l.lo<top-T && l.axis>=left-T && l.axis<=right+T).collect();
        // Both outer sides must really be open, not damaged closed borders.
        if vs.iter().any(|l|(l.axis-left).abs()<=T || (l.axis-right).abs()<=T) { continue; }
        let mut xs=vec![left]; xs.extend(vs.iter().map(|l|l.axis)); xs.push(right);
        xs.sort_by(f64::total_cmp); xs.dedup_by(|a,b|(*a-*b).abs()<=T);
        if xs.len()<4 || xs.len()>257 || xs.windows(2).any(|w|w[1]-w[0]<=2.*T) { continue; }
        let mut complete=true;
        for x in &xs[1..xs.len()-1] { if checked_coverage(lines,false,*x,bottom,top,&mut work)?!=Some(true) { complete=false; break; } }
        if !complete { continue; }
        let relevant:Vec<Line>=lines.iter().filter(|l|l.horizontal && l.axis>=bottom-0.01 && l.axis<=top+0.01 && l.hi>left+T && l.lo<right-T).copied().collect();
        if relevant.iter().any(|l|l.lo<left-T || l.hi>right+T || !xs.iter().any(|x|(*x-l.lo).abs()<=T) || !xs.iter().any(|x|(*x-l.hi).abs()<=T)) { continue; }
        let mut ys:Vec<f64>=relevant.iter().map(|l|l.axis).collect();
        ys.sort_by(f64::total_cmp); ys.dedup_by(|a,b|(*a-*b).abs()<=T);
        if ys.len()<3 || ys.len()>257 || ys.windows(2).any(|w|w[1]-w[0]<=2.*T) { continue; }
        let (columns,rows)=dimensions(&xs,&ys).map_err(invalid)?;
        if columns*rows>options.limits.cells { return Err(Error::LimitExceeded("table cells".into())); }
        let mut spans=Vec::new(); let mut valid=true;
        for column in 0..columns {
            let mut start=0;
            for end in 1..=rows {
                match checked_coverage(lines,true,ys[rows-end],xs[column],xs[column+1],&mut work)? {
                    Some(true)=>{ spans.push(PdfTableSpan {row:start,column,row_span:end-start,column_span:1}); start=end; }
                    Some(false)=>{}, None=>{ valid=false; break; }
                }
            }
            if start!=rows { valid=false; }
            if !valid { break; }
        }
        if !valid { continue; }
        spans.sort_by_key(|s|(s.row,s.column));
        if result.iter().any(|c:&PdfTableCandidate|c.x_boundaries==xs && c.y_boundaries==ys) { continue; }
        if result.len()>=128.min(options.limits.blocks) { return Err(Error::LimitExceeded("table candidates".into())); }
        result.push(PdfTableCandidate {candidate:0,page,x_boundaries:xs,y_boundaries:ys,cell_spans:spans});
    }
    result.sort_by(|a,b|b.y_boundaries.last().unwrap().total_cmp(a.y_boundaries.last().unwrap()).then(a.x_boundaries[0].total_cmp(&b.x_boundaries[0])));
    // Overlapping fallback regions are ambiguous; never emit duplicate text.
    for (i,a) in result.iter().enumerate() { for b in result.iter().skip(i+1) {
        if a.x_boundaries[0]<*b.x_boundaries.last().unwrap()-T && b.x_boundaries[0]<*a.x_boundaries.last().unwrap()-T && a.y_boundaries[0]<*b.y_boundaries.last().unwrap()-T && b.y_boundaries[0]<*a.y_boundaries.last().unwrap()-T { return Err(invalid("overlapping open-sided table regions")); }
    }}
    for (i,c) in result.iter_mut().enumerate() { c.candidate=i+1; }
    Ok(result)
}
fn owners(c:&PdfTableCandidate,columns:usize,rows:usize)->Result<Vec<usize>> {
    if c.cell_spans.is_empty() || c.cell_spans.len()>columns*rows { return Err(invalid("invalid row-span partition")); }
    let mut owner=vec![usize::MAX;columns*rows];
    for (i,s) in c.cell_spans.iter().enumerate() {
        if s.row>=rows || s.column>=columns || s.row_span==0 || s.row_span>rows-s.row || s.column_span!=1 { return Err(invalid("invalid row span")); }
        for row in s.row..s.row+s.row_span {
            let slot=&mut owner[row*columns+s.column];
            if *slot!=usize::MAX { return Err(invalid("overlapping row spans")); } *slot=i;
        }
    }
    if owner.contains(&usize::MAX) { return Err(invalid("incomplete row-span partition")); }
    Ok(owner)
}
pub(super) fn convert_inspection(report:&PdfInspection,c:&PdfTableCandidate,options:&Options)->Result<PdfTableConversion> {
    let xs=&c.x_boundaries; let ys=&c.y_boundaries;
    let (columns,rows)=dimensions(xs,ys).map_err(invalid)?;
    if c.page==0 || c.page as usize>report.page_count { return Err(invalid("table page out of range")); }
    if columns*rows>options.limits.cells || report.elements.len()>options.limits.blocks { return Err(Error::LimitExceeded("table geometry or inspection elements".into())); }
    let owner=owners(c,columns,rows)?;
    let mut runs:Vec<Vec<Run>>=vec![Vec::new();c.cell_spans.len()];
    let mut sequences=Vec::new(); let mut seen=std::collections::BTreeSet::new(); let mut bytes=0u64; let mut adjusted=0; let mut excluded=0;
    for e in &report.elements {
        if e.anchor.page!=c.page || e.kind!=PdfElementKind::Text { continue; }
        let b=e.bounds.ok_or_else(||invalid("unpositioned text on selected page"))?;
        if [b.left,b.bottom,b.right,b.top].iter().any(|n|!n.is_finite()) || b.left>=b.right || b.bottom>=b.top { return Err(invalid("invalid text geometry")); }
        if b.right<=xs[0] || b.left>=xs[columns] || b.top<=ys[0] || b.bottom>=ys[rows] { continue; }
        let text=e.text.as_ref().ok_or_else(||invalid("undecodable text in table"))?;
        if text.trim().is_empty() { continue; }
        let middle=b.bottom+(b.top-b.bottom)/2.;
        if middle<ys[0] || middle>ys[rows] {
            if b.estimated && estimated_vertical(b.bottom,b.top,&[ys[0],ys[rows]]).map_err(invalid)?.is_none() { excluded+=1; continue; }
            return Err(invalid(format!("text crosses table edge at sequence {}",e.sequence)));
        }
        let column=xs.windows(2).position(|w|b.left>=w[0] && b.right<=w[1]).ok_or_else(||invalid(format!("text crosses column boundary at sequence {}",e.sequence)))?;
        let y=ys.windows(2).position(|w|middle>w[0] && middle<w[1]);
        // Text centred on a grid line is valid only if that line is absent in
        // this column, so both slots belong to the same spanning cell.
        let index=if let Some(y)=y { owner[(rows-1-y)*columns+column] } else {
            let boundary=ys.iter().position(|v|*v==middle).ok_or_else(||invalid("ambiguous text position"))?;
            if boundary==0 || boundary==rows { return Err(invalid("text centred on table edge")); }
            let a=owner[(rows-boundary-1)*columns+column]; let b=owner[(rows-boundary)*columns+column];
            if a!=b { return Err(invalid("text centred on cell boundary")); } a
        };
        let s=&c.cell_spans[index]; let bottom=ys[rows-s.row-s.row_span]; let top=ys[rows-s.row];
        let bounds=if b.estimated { estimated_vertical(b.bottom,b.top,&[bottom,top]).map_err(|error|invalid(format!("sequence {}: {error}",e.sequence)))?.ok_or_else(||invalid("text owner lies outside span"))? }
            else if b.bottom>=bottom && b.top<=top { (b.bottom,b.top) } else { return Err(invalid("non-estimated text crosses cell boundary")); };
        if bounds!=(b.bottom,b.top) { adjusted+=1; }
        bytes=bytes.checked_add(text.len() as u64).ok_or_else(||Error::LimitExceeded("table text bytes".into()))?;
        if bytes>options.limits.archive_bytes.min(8*1024*1024) { return Err(Error::LimitExceeded("table text bytes".into())); }
        if !seen.insert(e.sequence) { return Err(invalid("duplicate source sequence in inspection")); }
        sequences.push(e.sequence);
        runs[index].push(Run {bounds:[b.left,bounds.0,b.right,bounds.1],text:text.clone(),gap:e.word_gap_before,sequence:e.sequence});
    }
    if sequences.is_empty() { return Err(invalid("selected grid has no decoded text")); }
    let mut values=vec![String::new();columns*rows];
    for (s,runs) in c.cell_spans.iter().zip(&runs) {
        let cell=crate::pdf_tables::convert(&[xs[s.column],xs[s.column+1]],&[ys[rows-s.row-s.row_span],ys[rows-s.row]],runs).map_err(invalid)?;
        let text=cell.lines().next().and_then(|s|s.strip_prefix("| ")).and_then(|s|s.strip_suffix(" |")).ok_or_else(||invalid("invalid rendered cell"))?;
        values[s.row*columns+s.column]=text.to_string();
    }
    let mut markdown=String::new();
    for row in 0..rows {
        markdown.push('|');
        for column in 0..columns { markdown.push(' '); markdown.push_str(&values[row*columns+column]); markdown.push_str(" |"); }
        markdown.push('\n');
        if row==0 { markdown.push('|'); for _ in 0..columns { markdown.push_str(" --- |"); } markdown.push('\n'); }
    }
    if markdown.len() as u64>options.limits.archive_bytes { return Err(Error::LimitExceeded("table Markdown bytes".into())); }
    Ok(PdfTableConversion {schema_version:"omniattach/pdf-table/0.1".into(),source_name:report.source.name.clone(),source_sha256:report.source.sha256.clone(),grid:PdfTableGrid {page:c.page,x_boundaries:xs.clone(),y_boundaries:ys.clone()},source_sequences:sequences,first_row_is_header:true,markdown,warnings:vec![
        "Open-sided filled-rule table: outer x limits inferred from repeated horizontal-rule endpoints; full-height interior dividers required; vertical row spans only".into(),
        "Row-span topology is retained in pdf-tables candidate.cell_spans (zero-based, top-down); Markdown writes each cell at its anchor and leaves covered slots empty; read discovery alongside conversion".into(),
        "Page-local fragments only; no category propagation or cross-page row joining; first row is formatting, not semantic header inference".into(),
        format!("Estimated vertical assignment adjusted {adjusted} runs and excluded {excluded} small outer-edge incursions; original bounds unchanged; horizontal containment remains strict")
    ]})
}
#[cfg(test)] mod tests {
    use super::*;
    fn geometry()->Vec<Line> {
        let mut lines=Vec::new();
        for y in [0.,20.,60.,80.] { lines.push(Line {horizontal:true,axis:y,lo:0.,hi:100.}); }
        lines.push(Line {horizontal:true,axis:40.,lo:20.,hi:100.});
        for x in [20.,50.,70.] { lines.push(Line {horizontal:false,axis:x,lo:0.,hi:80.}); } lines
    }
    #[test] fn discovers_open_sides_and_keeps_row_span() {
        let found=assemble(&geometry(),1,&Options::default()).unwrap(); assert_eq!(found.len(),1);
        assert_eq!(found[0].x_boundaries,vec![0.,20.,50.,70.,100.]);
        assert!(found[0].cell_spans.iter().any(|s|s.row==1 && s.column==0 && s.row_span==2));
    }
    #[test] fn missing_divider_is_not_silently_filled() {
        let mut lines=geometry(); lines.last_mut().unwrap().hi=40.;
        assert!(assemble(&lines,1,&Options::default()).unwrap().is_empty());
    }
    #[test] fn partial_separator_is_ambiguous() {
        let mut lines=geometry(); lines.push(Line {horizontal:true,axis:30.,lo:30.,hi:100.});
        assert!(assemble(&lines,1,&Options::default()).unwrap().is_empty());
    }
    #[test] fn partition_rejects_overlap_gaps_and_bad_spans() {
        let mut c=assemble(&geometry(),1,&Options::default()).unwrap().remove(0);
        assert!(owners(&c,4,4).is_ok());
        c.cell_spans.push(c.cell_spans[0].clone()); assert!(owners(&c,4,4).is_err());
        c.cell_spans.pop(); c.cell_spans.remove(0); assert!(owners(&c,4,4).is_err());
    }
    fn inspection()->PdfInspection {
        let mut elements=Vec::new();
        for (sequence,text,bounds) in [(1,"Group",[1.,45.,15.,55.]),(2,"continued",[1.,35.,15.,45.]),(3,"A",[21.,45.,30.,55.]),(4,"B",[21.,25.,30.,35.])] {
            elements.push(serde_json::json!({"sequence":sequence,"kind":"text","anchor":{"page":1,"operation_visit":sequence,"form_path":[],"object_id":null,"resource_name":null},"bounds":{"left":bounds[0],"bottom":bounds[1],"right":bounds[2],"top":bounds[3],"estimated":true},"text":text,"word_gap_before":false,"pixel_size":null,"derived_from":[],"evidence":[]}));
        }
        serde_json::from_value(serde_json::json!({"schema_version":"omniattach/pdf-inspection/0.1","source":{"name":"test.pdf","sha256":"test","byte_length":1,"encoding":null},"page_count":1,"coordinate_space":"PDF user space","elements":elements,"diagnostics":[]})).unwrap()
    }
    #[test] fn text_across_absent_row_line_has_one_span_owner() {
        let c=assemble(&geometry(),1,&Options::default()).unwrap().remove(0);
        let result=convert_inspection(&inspection(),&c,&Options::default()).unwrap();
        assert!(result.markdown.contains("Group<br>continued"));
        assert_eq!(result.markdown.matches("continued").count(),1);
        assert_eq!(result.source_sequences,vec![1,2,3,4]);
    }
    #[test] fn column_crossings_still_fail() {
        let c=assemble(&geometry(),1,&Options::default()).unwrap().remove(0);
        let mut report=inspection(); report.elements[2].bounds.as_mut().unwrap().left=19.;
        assert!(convert_inspection(&report,&c,&Options::default()).is_err());
    }

}

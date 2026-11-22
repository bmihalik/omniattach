//! Narrow page-content grid discovery; no models or external tools.
#[cfg(feature = "pdf")]
#[path = "lfe_table/engine.rs"]
mod engine;
#[cfg(feature = "pdf")]
mod tables {
    use lopdf::{Object, content::Content};
    type Point = [f64; 2];
    type Matrix = [f64; 6];
    const ID: Matrix = [1.,0.,0.,1.,0.,0.];
    #[derive(Debug)]
    pub struct Grid { pub xs: Vec<f64>, pub ys: Vec<f64> }
    fn number(o: &Object) -> Result<f64, String> {
        let v = match o { Object::Integer(n) => *n as f64, Object::Real(n) => *n as f64, _ => return Err("non-numeric geometry".into()) };
        if !v.is_finite() { return Err("non-finite geometry".into()); } Ok(v)
    }
    fn point(m: Matrix, p: Point) -> Point { [m[0]*p[0]+m[2]*p[1]+m[4],m[1]*p[0]+m[3]*p[1]+m[5]] }
    fn compose(a: Matrix, b: Matrix) -> Matrix {
        [a[0]*b[0]+a[1]*b[2],a[0]*b[1]+a[1]*b[3],a[2]*b[0]+a[3]*b[2],a[2]*b[1]+a[3]*b[3],a[4]*b[0]+a[5]*b[2]+b[4],a[4]*b[1]+a[5]*b[3]+b[5]]
    }
    fn grid(segments: &[(Point,Point)]) -> Option<Grid> {
        if segments.is_empty() { return None; }
        let left = segments.iter().flat_map(|(a,b)| [a[0],b[0]]).min_by(f64::total_cmp)?;
        let right = segments.iter().flat_map(|(a,b)| [a[0],b[0]]).max_by(f64::total_cmp)?;
        let bottom = segments.iter().flat_map(|(a,b)| [a[1],b[1]]).min_by(f64::total_cmp)?;
        let top = segments.iter().flat_map(|(a,b)| [a[1],b[1]]).max_by(f64::total_cmp)?;
        if right-left <= 0.5 || top-bottom <= 0.5 { return None; }
        let mut xs = Vec::new(); let mut ys = Vec::new();
        for (a,b) in segments {
            if (a[1]-b[1]).abs() <= 0.01 && (a[0].min(b[0])-left).abs() <= 0.01 && (a[0].max(b[0])-right).abs() <= 0.01 { ys.push(a[1]); }
            else if (a[0]-b[0]).abs() <= 0.01 && (a[1].min(b[1])-bottom).abs() <= 0.01 && (a[1].max(b[1])-top).abs() <= 0.01 { xs.push(a[0]); }
            else { return None; }
        }
        for axis in [&mut xs,&mut ys] {
            axis.sort_by(f64::total_cmp); axis.dedup_by(|a,b| (*a-*b).abs() <= 0.01);
            if axis.len() < 3 || axis.len() > 257 || axis.windows(2).any(|w| w[1]-w[0] <= 0.5) { return None; }
        }
        if (xs[0]-left).abs()>0.01 || (xs[xs.len()-1]-right).abs()>0.01 || (ys[0]-bottom).abs()>0.01 || (ys[ys.len()-1]-top).abs()>0.01 { return None; }
        Some(Grid { xs, ys })
    }
    pub fn discover(content: Content, max_ops: usize, max_segments: usize) -> Result<Vec<Grid>,String> {
        if content.operations.len() > max_ops { return Err("content operation limit".into()); }
        let mut m = ID; let mut stack = Vec::new(); let mut segments = Vec::new();
        let mut current = None; let mut start = None; let mut invalid = false; let mut grids = Vec::new();
        for op in content.operations {
            let a = &op.operands;
            let pair = |i: usize| -> Result<Point,String> { Ok([number(a.get(i).ok_or("missing coordinate")?)?,number(a.get(i+1).ok_or("missing coordinate")?)?]) };
            match op.operator.as_str() {
                "q" => { if stack.len() >= 128 { return Err("graphics stack limit".into()); } stack.push(m); }
                "Q" => { m = stack.pop().ok_or("unbalanced Q")?; }
                "cm" => { if a.len()!=6 { return Err("invalid cm".into()); } let v: Vec<f64> = a.iter().map(number).collect::<Result<_,_>>()?; m = compose([v[0],v[1],v[2],v[3],v[4],v[5]],m); if m.iter().any(|v| !v.is_finite()) { return Err("matrix overflow".into()); } }
                "Do" => return Err("selected page invokes XObjects; this first discovery slice excludes Forms and images".into()),
                "W" | "W*" => return Err("clipped page content is unsupported".into()),
                "m" => { let p = point(m,pair(0)?); current = Some(p); start = Some(p); }
                "l" => { let p = point(m,pair(0)?); if let Some(c) = current { segments.push((c,p)); } else { invalid = true; } current = Some(p); }
                "re" => { let p = pair(0)?; let wh = pair(2)?; let corners = [point(m,p),point(m,[p[0]+wh[0],p[1]]),point(m,[p[0]+wh[0],p[1]+wh[1]]),point(m,[p[0],p[1]+wh[1]])]; for i in 0..4 { segments.push((corners[i],corners[(i+1)%4])); } current = Some(corners[0]); start = current; }
                "h" => { if let (Some(c),Some(s)) = (current,start) { if c != s { segments.push((c,s)); } current = Some(s); } }
                "c" | "v" | "y" => { invalid = true; }
                "S" | "s" | "B" | "B*" | "b" | "b*" | "f" | "F" | "f*" | "n" => {
                    if matches!(op.operator.as_str(),"s"|"b"|"b*") { if let (Some(c),Some(s)) = (current,start) { if c!=s { segments.push((c,s)); } } }
                    if !invalid && matches!(op.operator.as_str(),"S"|"s") {
                        if segments.iter().any(|(a,b)| a.iter().chain(b.iter()).any(|v| !v.is_finite())) { return Err("path overflow".into()); }
                        if let Some(g) = grid(&segments) { if grids.len()>=128 { return Err("table candidate limit".into()); } grids.push(g); }
                    }
                    segments.clear(); current = None; start = None; invalid = false;
                }
                _ => {}
            }
            if segments.len()>max_segments { return Err("path segment limit".into()); }
        }
        if !stack.is_empty() { return Err("unbalanced q".into()); }
        Ok(grids)
    }
    #[cfg(test)] mod tests {
        use super::*;
        #[test] fn finds_grid_and_transforms_coordinates() {
            let data = b"2 0 0 2 5 7 cm 0 0 m 20 0 l 0 10 m 20 10 l 0 20 m 20 20 l 0 0 m 0 20 l 10 0 m 10 20 l 20 0 m 20 20 l S";
            let g = discover(Content::decode(data).unwrap(),100,100).unwrap();
            assert_eq!(g.len(),1); assert_eq!(g[0].xs,vec![5.,25.,45.]); assert_eq!(g[0].ys,vec![7.,27.,47.]);
        }
        #[test] fn rejects_unsupported_and_limits() {
            assert!(discover(Content::decode(b"/F Do").unwrap(),100,100).is_err());
            assert!(discover(Content::decode(b"0 0 m 1 1 l").unwrap(),1,100).is_err());
            assert!(discover(Content::decode(b"W n").unwrap(),100,100).is_err());
        }
        #[test] fn separate_strokes_do_not_make_a_grid() {
            assert!(discover(Content::decode(b"0 0 m 20 0 l S 0 10 m 20 10 l S").unwrap(),100,100).unwrap().is_empty());
        }
    }
}
#[cfg(feature = "pdf")]
fn main() -> Result<(),Box<dyn std::error::Error>> {
    use omniattach_documents::{Options,inspect_pdf_bytes};
    use std::io::Read;
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len()!=3 { return Err("usage: pdf-ruled-table INPUT.pdf PAGE list|CANDIDATE_NUMBER".into()); }
    let page: u32 = a[1].parse()?;
    let options = Options::default();
    let mut bytes = Vec::new();
    std::fs::File::open(&a[0])?.take(options.limits.input_bytes.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    if bytes.len()>options.limits.input_bytes { return Err("input byte limit".into()); }
    let pdf = lopdf::Document::load_mem_with_options(&bytes,lopdf::LoadOptions { max_decompressed_size: Some(options.limits.archive_bytes.min(usize::MAX as u64) as usize), ..Default::default() })?;
    if pdf.is_encrypted() { return Err("encrypted PDF unsupported".into()); }
    let pages = pdf.get_pages();
    if pages.len()>options.limits.blocks { return Err("page limit".into()); }
    let id = *pages.get(&page).ok_or("page not found; pages are one-based")?;
    let mut dictionary = pdf.get_dictionary(id)?;
    for depth in 0..32 {
        for (key,expected) in [(b"Rotate".as_slice(),0.0),(b"UserUnit".as_slice(),1.0)] {
            if let Ok(value) = dictionary.get(key) {
                let actual = match value { lopdf::Object::Integer(v)=>*v as f64,lopdf::Object::Real(v)=>*v as f64,_=>return Err("indirect page geometry unsupported".into()) };
                if actual!=expected { return Err("non-default page rotation or UserUnit unsupported".into()); }
            }
        }
        match dictionary.get(b"Parent") { Ok(parent)=>{ if depth==31 { return Err("page ancestry limit".into()); } dictionary=pdf.get_dictionary(parent.as_reference()?)?; },Err(_)=>break }
    }
    let data = pdf.get_page_content_with_limit(id,options.limits.archive_bytes.min(usize::MAX as u64) as usize)?;
    let grids = tables::discover(lopdf::content::Content::decode(&data)?,options.limits.xml_nodes as usize,options.limits.archive_entries)?;
    if a[2]=="list" {
        let items: Vec<_> = grids.iter().enumerate().map(|(i,g)| serde_json::json!({"candidate":i+1,"page":page,"x_boundaries":g.xs,"y_boundaries":g.ys})).collect();
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"source":a[0],"page":page,"candidates":items}))?); return Ok(());
    }
    let selected: usize = a[2].parse()?;
    let grid = grids.get(selected.checked_sub(1).ok_or("candidate numbers are one-based")?).ok_or("candidate not found")?;
    let report = inspect_pdf_bytes(&a[0],&bytes,&options)?;
    let mut runs = Vec::new();
    for element in &report.elements {
        if element.anchor.page!=page || element.kind!=omniattach_documents::PdfElementKind::Text { continue; }
        let Some(b) = element.bounds else { return Err("unpositioned text on selected page; cannot safely exclude it".into()); };
        if b.right<=grid.xs[0] || b.left>=grid.xs[grid.xs.len()-1] || b.top<=grid.ys[0] || b.bottom>=grid.ys[grid.ys.len()-1] { continue; }
        let text = element.text.as_ref().ok_or("undecodable text in table")?;
        runs.push(engine::Run { bounds:[b.left,b.bottom,b.right,b.top],text:text.clone(),gap:element.word_gap_before,sequence:element.sequence });
    }
    if runs.is_empty() { return Err("selected grid has no decoded text".into()); }
    let markdown = engine::convert(&grid.xs,&grid.ys,&runs)?;
    eprintln!("page {page}, candidate {selected}; first row is header; estimated text bounds; source SHA-256 {}",report.source.sha256);
    print!("{markdown}"); Ok(())
}
#[cfg(not(feature = "pdf"))]
fn main() { eprintln!("requires --features pdf"); std::process::exit(1); }

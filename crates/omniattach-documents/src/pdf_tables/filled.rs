//! Filled thin-rectangle borders, with safe rectangular clips and band splitting.
use super::*;
type Rect=[f64;4];
const T:f64=0.8;
#[derive(Clone,Copy,Debug)]
struct Line { horizontal:bool, axis:f64, lo:f64, hi:f64 }
fn rectangle(edges:&[(Point,Point)])->Option<Rect> {
    if edges.len()!=4 { return None; }
    let x0=edges.iter().map(|(a,_)|a[0]).min_by(f64::total_cmp)?;
    let x1=edges.iter().map(|(a,_)|a[0]).max_by(f64::total_cmp)?;
    let y0=edges.iter().map(|(a,_)|a[1]).min_by(f64::total_cmp)?;
    let y1=edges.iter().map(|(a,_)|a[1]).max_by(f64::total_cmp)?;
    if x0>=x1 || y0>=y1 { return None; }
    let corners=[[x0,y0],[x1,y0],[x1,y1],[x0,y1]];
    for corner in corners { if edges.iter().filter(|(a,_)|*a==corner).count()!=1 { return None; } }
    for (i,(a,b)) in edges.iter().enumerate() {
        if *b!=edges[(i+1)%4].0 || (a[0]!=b[0] && a[1]!=b[1]) || a==b { return None; }
    }
    Some([x0,y0,x1,y1])
}
fn inside(a:Rect,b:Rect)->bool { a[0]>=b[0] && a[1]>=b[1] && a[2]<=b[2] && a[3]<=b[3] }
fn merge(mut lines:Vec<Line>)->Vec<Line> {
    lines.sort_by(|a,b|a.horizontal.cmp(&b.horizontal).then(a.axis.total_cmp(&b.axis)).then(a.lo.total_cmp(&b.lo)));
    let mut groups:Vec<Vec<Line>>=Vec::new();
    for l in lines {
        if let Some(g)=groups.last_mut() {
            if g[0].horizontal==l.horizontal && (g[0].axis-l.axis).abs()<=T { g.push(l); continue; }
        }
        groups.push(vec![l]);
    }
    let mut result:Vec<Line>=Vec::new();
    for mut g in groups {
        let axis=g.iter().map(|l|l.axis).sum::<f64>()/g.len() as f64;
        g.sort_by(|a,b|a.lo.total_cmp(&b.lo));
        let mut current=g[0]; current.axis=axis;
        for l in g.into_iter().skip(1) {
            if l.lo<=current.hi+T { current.hi=current.hi.max(l.hi); }
            else { result.push(current); current=Line {axis,..l}; }
        }
        result.push(current);
    }
    result
}
fn assemble(lines:Vec<Line>,page:u32,options:&Options)->Result<Vec<PdfTableCandidate>> {
    let lines=merge(lines);
    let mut groups:Vec<Vec<Line>>=Vec::new();
    for h in lines.iter().filter(|l|l.horizontal) {
        if let Some(g)=groups.iter_mut().find(|g|(g[0].lo-h.lo).abs()<=T && (g[0].hi-h.hi).abs()<=T) { g.push(*h); }
        else { groups.push(vec![*h]); }
    }
    let mut output=Vec::new();
    for mut hs in groups {
        if hs.len()<2 { continue; }
        hs.sort_by(|a,b|a.axis.total_cmp(&b.axis));
        if hs.len()>257 { return Err(Error::LimitExceeded("filled table rows".into())); }
        let left=hs[0].lo; let right=hs[0].hi;
        let mut pending:Option<(Vec<f64>,Vec<f64>)>=None;
        let flush=|pending:&mut Option<(Vec<f64>,Vec<f64>)>,output:&mut Vec<PdfTableCandidate>|->Result<()> {
            if let Some((xs,ys))=pending.take() {
                if output.len()>=128.min(options.limits.blocks) { return Err(Error::LimitExceeded("table candidates".into())); }
                if (xs.len()-1)*(ys.len()-1)>options.limits.cells.min(16384) { return Err(Error::LimitExceeded("table cells".into())); }
                output.push(PdfTableCandidate {candidate:0,page,x_boundaries:xs,y_boundaries:ys});
            }
            Ok(())
        };
        for ys in hs.windows(2) {
            let bottom=ys[0].axis; let top=ys[1].axis;
            if top-bottom<=2.*T { flush(&mut pending,&mut output)?; continue; }
            let verticals:Vec<&Line>=lines.iter().filter(|l|!l.horizontal && l.axis>=left-T && l.axis<=right+T && l.hi>bottom+T && l.lo<top-T).collect();
            let mut xs:Vec<f64>=verticals.iter().filter(|l|l.lo<=bottom+T && l.hi>=top-T).map(|l|l.axis).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup_by(|a,b|(*a-*b).abs()<=T);
            let partial=verticals.iter().any(|l|l.lo>bottom+T || l.hi<top-T);
            if partial || xs.len()<2 || (xs[0]-left).abs()>T || (xs[xs.len()-1]-right).abs()>T { flush(&mut pending,&mut output)?; continue; }
            if xs.len()>257 || xs.windows(2).any(|w|w[1]-w[0]<=2.*T) { return Err(invalid("ambiguous filled table columns")); }
            if let Some((old,axis))=&mut pending {
                if *old==xs && (axis[axis.len()-1]-bottom).abs()<0.01 { axis.push(top); continue; }
            }
            flush(&mut pending,&mut output)?;
            pending=Some((xs,vec![bottom,top]));
        }
        flush(&mut pending,&mut output)?;
    }
    output.sort_by(|a,b|b.y_boundaries.last().unwrap().total_cmp(a.y_boundaries.last().unwrap()).then(a.x_boundaries[0].total_cmp(&b.x_boundaries[0])));
    for (index,c) in output.iter_mut().enumerate() { c.candidate=index+1; }
    Ok(output)
}
pub(super) fn discover(content:Content,page:u32,options:&Options)->Result<Vec<PdfTableCandidate>> {
    if content.operations.len()>options.limits.xml_nodes as usize { return Err(Error::LimitExceeded("content operations".into())); }
    let mut m=ID; let mut clip:Option<Rect>=None; let mut stack=Vec::new();
    let mut edges=Vec::new(); let mut rects=Vec::new(); let mut current=None; let mut start=None;
    let mut bad=false; let mut pending_clip=false; let mut lines=Vec::new(); let mut clips=Vec::new();
    for op in content.operations {
        let a=&op.operands;
        let pair=|i:usize|->Result<Point> { Ok([number(a.get(i).ok_or_else(||invalid("missing coordinate"))?)?,number(a.get(i+1).ok_or_else(||invalid("missing coordinate"))?)?]) };
        match op.operator.as_str() {
            "q"=>{ if stack.len()>=128 { return Err(Error::LimitExceeded("graphics stack".into())); } stack.push((m,clip)); }
            "Q"=>{ let state=stack.pop().ok_or_else(||invalid("unbalanced Q"))?; m=state.0; clip=state.1; }
            "cm"=>{ if a.len()!=6 { return Err(invalid("invalid cm")); } let v:Vec<f64>=a.iter().map(number).collect::<Result<_>>()?; m=compose([v[0],v[1],v[2],v[3],v[4],v[5]],m); if m.iter().any(|n|!n.is_finite()) { return Err(invalid("matrix overflow")); } }
            "Do"=>return Err(invalid("selected page invokes XObjects; Forms and images unsupported")),
            "m"=>{ let p=point(m,pair(0)?); current=Some(p); start=Some(p); }
            "l"=>{ let p=point(m,pair(0)?); if let Some(c)=current { edges.push((c,p)); } else { bad=true; } current=Some(p); }
            "h"=>{ if let (Some(c),Some(s))=(current,start) { if c!=s { edges.push((c,s)); } current=Some(s); } }
            "re"=>{
                let p=pair(0)?; let wh=pair(2)?;
                let c=[point(m,p),point(m,[p[0]+wh[0],p[1]]),point(m,[p[0]+wh[0],p[1]+wh[1]]),point(m,[p[0],p[1]+wh[1]])];
                let e=[(c[0],c[1]),(c[1],c[2]),(c[2],c[3]),(c[3],c[0])];
                if let Some(r)=rectangle(&e) { rects.push(r); } else { bad=true; }
                edges.extend(e); current=Some(c[0]); start=current;
            }
            "c"|"v"|"y"=>bad=true,
            "W"|"W*"=>{ if bad || pending_clip { return Err(invalid("complex clipped page content unsupported")); } pending_clip=true; }
            "S"|"s"|"B"|"B*"|"b"|"b*"=>return Err(invalid("mixed stroked/filled table geometry unsupported")),
            "f"|"F"|"f*"|"n"=>{
                if edges.iter().any(|(a,b)|a.iter().chain(b.iter()).any(|n|!n.is_finite())) { return Err(invalid("path overflow")); }
                if pending_clip {
                    if op.operator!="n" { return Err(invalid("painted clipping path unsupported")); }
                    let mut r=rectangle(&edges).ok_or_else(||invalid("nonrectangular clipped page content unsupported"))?;
                    if let Some(old)=clip { r=[r[0].max(old[0]),r[1].max(old[1]),r[2].min(old[2]),r[3].min(old[3])]; }
                    clip=Some(r); clips.push(r);
                    if clips.len()>128 { return Err(Error::LimitExceeded("rectangular clips".into())); }
                } else if op.operator!="n" && !bad && rects.len()==1 && edges.len()==4 {
                    for r in &rects {
                        let w=r[2]-r[0]; let h=r[3]-r[1];
                        let line=if h<=2. && w>=3.*h && w>2.*T { Some(Line {horizontal:true,axis:(r[1]+r[3])/2.,lo:r[0],hi:r[2]}) }
                            else if w<=2. && h>=3.*w && h>2.*T { Some(Line {horizontal:false,axis:(r[0]+r[2])/2.,lo:r[1],hi:r[3]}) } else { None };
                        if let Some(line)=line {
                            if let Some(c)=clip {
                                if r[2]<=c[0] || r[0]>=c[2] || r[3]<=c[1] || r[1]>=c[3] { continue; }
                                if !inside(*r,c) { return Err(invalid("clipped border intersects table geometry")); }
                            }
                            lines.push(line);
                            if lines.len()>4096.min(options.limits.archive_entries) { return Err(Error::LimitExceeded("filled border segments".into())); }
                        }
                    }
                }
                edges.clear(); rects.clear(); current=None; start=None; bad=false; pending_clip=false;
            }
            _=>{}
        }
        if edges.len()>options.limits.archive_entries { return Err(Error::LimitExceeded("path segments".into())); }
    }
    if !stack.is_empty() || pending_clip { return Err(invalid("unfinished graphics state")); }
    let candidates=assemble(lines,page,options)?;
    for c in &candidates {
        let b=[c.x_boundaries[0],c.y_boundaries[0],*c.x_boundaries.last().unwrap(),*c.y_boundaries.last().unwrap()];
        if clips.iter().any(|clip|!inside(b,*clip)) { return Err(invalid("table not contained in every rectangular page clip")); }
    }
    Ok(candidates)
}
#[cfg(test)] mod tests {
    use super::*;
    fn filled()->String {
        let mut s=String::from("0 0 100 100 re W n ");
        for y in [10,30,50,70] { s+=&format!("10 {y} 60 0.5 re f "); }
        for x in [10,70] { s+=&format!("{x} 10 0.5 60.5 re f "); }
        s+"40 10 0.5 40 re f "
    }
    #[test] fn splits_section_heading_without_inventing_divider() {
        let g=discover(Content::decode(filled().as_bytes()).unwrap(),1,&Options::default()).unwrap();
        assert_eq!(g.len(),2); assert_eq!(g[0].x_boundaries.len(),2); assert_eq!(g[0].y_boundaries.len(),2);
        assert_eq!(g[1].x_boundaries.len(),3); assert_eq!(g[1].y_boundaries.len(),3);
    }
    #[test] fn rejects_cut_border_and_complex_clip() {
        for text in [filled().replace("0 0 100 100", "20 0 80 100"),filled().replace("0 0 100 100 re W", "0 0 m 50 0 l 25 100 l h W")] {
            assert!(discover(Content::decode(text.as_bytes()).unwrap(),1,&Options::default()).is_err());
        }
    }
    #[test] fn duplicate_rectangles_do_not_duplicate_tables() {
        let s=filled(); let twice=s.clone()+&s;
        assert_eq!(discover(Content::decode(s.as_bytes()).unwrap(),1,&Options::default()).unwrap(),discover(Content::decode(twice.as_bytes()).unwrap(),1,&Options::default()).unwrap());
    }
    #[test] fn honors_limits() {
        let mut o=Options::default(); o.limits.cells=0;
        assert!(matches!(discover(Content::decode(filled().as_bytes()).unwrap(),1,&o),Err(Error::LimitExceeded(_))));
    }
}

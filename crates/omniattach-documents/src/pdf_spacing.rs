//! Shared line-aware PDF interpreter with optional LFE candidate collection.
use crate::{Error, Options, Result};
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId, content::Content};
use std::{collections::BTreeMap, rc::Rc};
#[path = "pdf_candidates.rs"]
mod candidates;
pub use candidates::{PdfBounds, PdfElement, PdfElementAnchor, PdfElementKind};
use candidates::{Collector, PathTrace, bounds};

#[derive(Clone, Copy)]
struct Matrix([f64;6]);
impl Matrix {
    const ID: Self=Self([1.,0.,0.,1.,0.,0.]);
    fn translate(x:f64,y:f64)->Self {Self([1.,0.,0.,1.,x,y])}
    fn then(self,other:Self)->Self {let [a,b,c,d,e,f]=self.0; let [g,h,i,j,k,l]=other.0; Self([a*g+b*i,a*h+b*j,c*g+d*i,c*h+d*j,e*g+f*i+k,e*h+f*j+l])}
    fn origin(self)->(f64,f64) {(self.0[4],self.0[5])}
    fn point(self,x:f64,y:f64)->[f64;2] {let (x,y)=Self::translate(x,y).then(self).origin(); [x,y]}
    fn y_scale(self)->f64 {self.0[2].hypot(self.0[3])}
}
fn number(o:&Object)->Option<f64> {let n=match o {Object::Integer(n)=>*n as f64,Object::Real(n)=>*n as f64,_=>return None}; n.is_finite().then_some(n)}
fn resolve<'a>(pdf:&'a Document,o:&'a Object)->&'a Object {match o {Object::Reference(id)=>pdf.get_object(*id).unwrap_or(o),_=>o}}
fn dict<'a>(pdf:&'a Document,o:&'a Object)->Option<&'a Dictionary> {resolve(pdf,o).as_dict().ok()}
fn matrix(items:&[Object])->Option<Matrix> {if items.len()!=6 {return None;} let v:Vec<_>=items.iter().map(number).collect::<Option<_>>()?; Some(Matrix([v[0],v[1],v[2],v[3],v[4],v[5]]))}
fn xy(items:&[Object],index:usize)->Option<[f64;2]> {Some([number(items.get(index)?)?,number(items.get(index+1)?)?])}
fn resources(pdf:&Document,page:ObjectId)->Option<&Dictionary> {
    let mut current=pdf.get_dictionary(page).ok()?;
    for _ in 0..32 {if let Ok(r)=current.get(b"Resources") {return dict(pdf,r);} current=dict(pdf,current.get(b"Parent").ok()?)?;} None
}
fn resource<'a>(pdf:&'a Document,r:Option<&'a Dictionary>,category:&[u8],name:&[u8])->Option<&'a Object> {dict(pdf,r?.get(category).ok()?)?.get(name).ok()}
struct Font<'a> {encoding:Option<Encoding<'a>>,cid:bool,widths:BTreeMap<u32,f64>,ranges:BTreeMap<u32,(u32,f64)>,default_width:f64}
impl<'a> Font<'a> {
    fn new(pdf:&'a Document,d:&'a Dictionary)->Self {
        let cid=d.get(b"Subtype").and_then(Object::as_name).ok()==Some(b"Type0".as_slice());
        let mut font=Self {encoding:d.get_font_encoding(pdf).ok(),cid,widths:BTreeMap::new(),ranges:BTreeMap::new(),default_width:if cid {1000.}else{500.}};
        if cid {
            let descendant=d.get(b"DescendantFonts").ok().map(|o| resolve(pdf,o)).and_then(|o| o.as_array().ok()).and_then(|a| a.first()).and_then(|o| dict(pdf,o));
            if let Some(d)=descendant {
                font.default_width=d.get(b"DW").ok().map(|o| resolve(pdf,o)).and_then(number).unwrap_or(1000.);
                if let Some(w)=d.get(b"W").ok().map(|o| resolve(pdf,o)).and_then(|o| o.as_array().ok()) {
                    let mut i=0;
                    while i<w.len() {
                        let Some(start)=number(resolve(pdf,&w[i])).filter(|n| *n>=0. && *n<=u32::MAX as f64) else {break;}; let start=start as u32;
                        match w.get(i+1).map(|o| resolve(pdf,o)) {
                            Some(Object::Array(run))=>{for (k,n) in run.iter().enumerate() {if let (Some(code),Some(n))=(u32::try_from(k).ok().and_then(|k| start.checked_add(k)),number(resolve(pdf,n))) {font.widths.insert(code,n);}} i+=2;}
                            Some(last)=>{let (Some(end),Some(width))=(number(last),w.get(i+2).and_then(|o| number(resolve(pdf,o)))) else {break;}; if end>=start as f64 && end<=u32::MAX as f64 {font.ranges.insert(start,(end as u32,width));} i+=3;}
                            None=>break,
                        }
                    }
                }
            }
        } else {
            let first=d.get(b"FirstChar").ok().map(|o| resolve(pdf,o)).and_then(number).unwrap_or(0.) as u32;
            if let Some(w)=d.get(b"Widths").ok().map(|o| resolve(pdf,o)).and_then(|o| o.as_array().ok()) {
                for (k,n) in w.iter().enumerate() {if let (Some(code),Some(n))=(u32::try_from(k).ok().and_then(|k| first.checked_add(k)),number(resolve(pdf,n))) {font.widths.insert(code,n);}}
            }
        } font
    }
    fn width(&self,code:u32)->f64 {self.widths.get(&code).copied().or_else(|| self.ranges.range(..=code).next_back().and_then(|(_, (end,w))| (code<=*end).then_some(*w))).unwrap_or(self.default_width)}
    fn advance(&self,bytes:&[u8],state:&State<'_>)->f64 {
        let codes:Vec<u32>=if self.cid {bytes.as_chunks::<2>().0.iter().map(|c| u32::from(u16::from_be_bytes(*c))).collect()} else {bytes.iter().map(|c| u32::from(*c)).collect()};
        codes.into_iter().map(|code| self.width(code)/1000.*state.size+state.char_spacing+if !self.cid && code==32 {state.word_spacing}else{0.}).sum::<f64>()*state.scale
    }
}
#[derive(Clone)]
struct State<'a> {ctm:Matrix,font:Option<Rc<Font<'a>>>,size:f64,char_spacing:f64,word_spacing:f64,scale:f64,leading:f64,rise:f64}
impl Default for State<'_> {fn default()->Self {Self {ctm:Matrix::ID,font:None,size:0.,char_spacing:0.,word_spacing:0.,scale:1.,leading:0.,rise:0.}}}
struct Run {x:f64,end:f64,y:f64,height:f64,text:String,space:bool}
pub(super) struct Page {pub lines:Vec<String>,pub undecoded:usize,pub real_gaps:usize,pub elements:Vec<PdfElement>,pub unsupported_inline_images:usize}
struct Reader {runs:Vec<Run>,undecoded:usize,real_gaps:usize,pending_space:bool,content_left:usize,ops_left:usize,forms_left:usize,output_left:usize,collector:Option<Collector>,operation:usize,max_segments:usize,unsupported_inline_images:usize}
impl Reader {
    fn show(&mut self,bytes:&[u8],state:&State<'_>,tm:&mut Matrix,active:&[ObjectId])->Result<()> {
        let Some(font)=&state.font else {self.undecoded+=1; if let Some(c)=&mut self.collector {c.text(self.operation,active,None,None,self.pending_space)?;} return Ok(());};
        let text=font.encoding.as_ref().and_then(|e| Document::decode_text(e,bytes).ok());
        let device=Matrix::translate(0.,state.rise).then(*tm).then(state.ctm); let (x,y)=device.origin(); let height=(state.size*device.y_scale()).abs();
        *tm=Matrix::translate(font.advance(bytes,state),0.).then(*tm); let end=tm.then(state.ctm).origin().0;
        if ![x,y,height,end].iter().all(|n| n.is_finite()) {return Err(Error::Parse("non-finite PDF text geometry".into()));}
        if font.cid && !bytes.len().is_multiple_of(2) {self.undecoded+=1;}
        if let Some(text)=text {
            self.output_left=self.output_left.checked_sub(text.len()).ok_or_else(|| Error::LimitExceeded("PDF decoded text bytes".into()))?;
            if !text.is_empty() {
                if let Some(c)=&mut self.collector {c.text(self.operation,active,Some(&text),bounds([[x,y],[end,y+height.max(1.)]],true)?,self.pending_space)?;}
                else {self.runs.push(Run {x,end,y,height:height.max(1.),text,space:self.pending_space});}
                self.pending_space=false;
            }
        } else {self.undecoded+=1; if let Some(c)=&mut self.collector {c.text(self.operation,active,None,bounds([[x,y],[end,y+height.max(1.)]],true)?,self.pending_space)?;}} Ok(())
    }
    fn content<'a>(&mut self,pdf:&'a Document,bytes:&[u8],r:Option<&'a Dictionary>,mut state:State<'a>,depth:usize,active:&mut Vec<ObjectId>)->Result<()> {
        self.content_left=self.content_left.checked_sub(bytes.len()).ok_or_else(|| Error::LimitExceeded("PDF page and Form content bytes".into()))?;
        let content=Content::decode(bytes).map_err(|e| Error::Parse(format!("PDF content: {e}")))?;
        let mut stack=Vec::new(); let (mut tm,mut tlm)=(Matrix::ID,Matrix::ID);
        let mut path=self.collector.as_ref().map(|_| PathTrace::new(self.max_segments));
        for op in content.operations {
            self.ops_left=self.ops_left.checked_sub(1).ok_or_else(|| Error::LimitExceeded("PDF content operations".into()))?;
            if self.collector.is_some() {self.operation+=1;}
            let a=&op.operands; let first=a.first().and_then(number);
            match op.operator.as_str() {
                "q"=>{if stack.len()>=128 {return Err(Error::LimitExceeded("PDF graphics stack".into()));} stack.push(state.clone());}
                "Q"=>{if let Some(s)=stack.pop() {state=s;}}
                "cm"=>{if let Some(m)=matrix(a) {state.ctm=m.then(state.ctm);}}
                "BT"=>{tm=Matrix::ID; tlm=Matrix::ID;}
                "Tf"=>{state.font=a.first().and_then(|o| o.as_name().ok()).and_then(|n| resource(pdf,r,b"Font",n)).and_then(|o| dict(pdf,o)).map(|d| Rc::new(Font::new(pdf,d))); state.size=a.get(1).and_then(number).unwrap_or(state.size);}
                "Tc"=>state.char_spacing=first.unwrap_or(0.),
                "Tw"=>state.word_spacing=first.unwrap_or(0.),
                "Tz"=>state.scale=first.unwrap_or(100.)/100.,
                "TL"=>state.leading=first.unwrap_or(0.),
                "Ts"=>state.rise=first.unwrap_or(0.),
                "Td"|"TD"=>{if let (Some(x),Some(y))=(first,a.get(1).and_then(number)) {if op.operator=="TD" {state.leading=-y;} tlm=Matrix::translate(x,y).then(tlm); tm=tlm;}}
                "Tm"=>{if let Some(m)=matrix(a) {tlm=m; tm=m;}}
                "T*"=>{tlm=Matrix::translate(0.,-state.leading).then(tlm); tm=tlm;}
                "Tj"|"TJ"|"'"|"\""=>{
                    if op.operator=="'" || op.operator=="\"" {
                        if op.operator=="\"" {state.word_spacing=first.unwrap_or(0.); state.char_spacing=a.get(1).and_then(number).unwrap_or(0.);}
                        tlm=Matrix::translate(0.,-state.leading).then(tlm); tm=tlm;
                    }
                    let items:Vec<&Object>=match op.operator.as_str() {"TJ"=>a.first().and_then(|o| o.as_array().ok()).map(|v| v.iter().collect()).unwrap_or_default(),"\""=>a.get(2).into_iter().collect(),_=>a.first().into_iter().collect()};
                    for item in items {
                        self.ops_left=self.ops_left.checked_sub(1).ok_or_else(|| Error::LimitExceeded("PDF text operations".into()))?;
                        match item {Object::String(b,_)=>self.show(b,&state,&mut tm,active)?,_=>if let Some(n)=number(item) {tm=Matrix::translate(-n/1000.*state.size*state.scale,0.).then(tm); if n < -180. {self.pending_space=true; if matches!(item,Object::Real(_)) {self.real_gaps+=1;}}}}
                    }
                }
                "BMC"|"BDC"=>{if let Some(c)=&mut self.collector && let Some(tag)=a.first().and_then(|o| o.as_name().ok()) {c.tag(self.operation,active,tag)?;}}
                "BI"=>{if self.collector.is_some() {self.unsupported_inline_images+=1;}}
                "m"|"l"=>{if let Some(path)=&mut path {if let Some([x,y])=xy(a,0) {let p=state.ctm.point(x,y); if op.operator=="m" {path.move_to(p)?;} else {path.line_to(p)?;}} else {path.clear();}}}
                "re"=>{if let Some(path)=&mut path {if let (Some([x,y]),Some([w,h]))=(xy(a,0),xy(a,2)) {path.rectangle([state.ctm.point(x,y),state.ctm.point(x+w,y),state.ctm.point(x+w,y+h),state.ctm.point(x,y+h)])?;} else {path.clear();}}}
                "h"=>{if let Some(path)=&mut path {path.close()?;}}
                "c"|"v"|"y"=>{if let Some(path)=&mut path {if let Some([x,y])=xy(a,if op.operator=="c" {4}else{2}) {path.curve_end(state.ctm.point(x,y))?;} else {path.clear();}}}
                "S"|"s"|"B"|"B*"|"b"|"b*"|"f"|"F"|"f*"|"n"=>{
                    if let Some(path)=&mut path {
                        if matches!(op.operator.as_str(),"s"|"b"|"b*") {path.close()?;}
                        if matches!(op.operator.as_str(),"S"|"s"|"B"|"B*"|"b"|"b*") && let Some(area)=path.grid()? && let Some(c)=&mut self.collector {c.grid(self.operation,active,area)?;}
                        path.clear();
                    }
                }
                "Do"=>{
                    let Some(name)=a.first().and_then(|o| o.as_name().ok()) else {continue;};
                    let Some(o)=resource(pdf,r,b"XObject",name) else {continue;}; let Object::Stream(s)=resolve(pdf,o) else {continue;};
                    let subtype=s.dict.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"");
                    if subtype==b"Image" {
                        if let Some(c)=&mut self.collector {
                            let object=if let Object::Reference(id)=o {Some(*id)}else{None};
                            let size=|key:&[u8]| s.dict.get(key).ok().map(|o| resolve(pdf,o)).and_then(|o| o.as_i64().ok()).and_then(|n| u32::try_from(n).ok()).filter(|n| *n>0);
                            let pixels=size(b"Width").zip(size(b"Height")).map(|(w,h)| [w,h]);
                            c.image(self.operation,active,object,name,bounds([state.ctm.point(0.,0.),state.ctm.point(1.,0.),state.ctm.point(1.,1.),state.ctm.point(0.,1.)],false)?,pixels)?;
                        } continue;
                    }
                    if subtype!=b"Form" {continue;}
                    if depth>=32 {return Err(Error::LimitExceeded("PDF Form depth".into()));}
                    self.forms_left=self.forms_left.checked_sub(1).ok_or_else(|| Error::LimitExceeded("PDF Form invocations".into()))?;
                    let id=if let Object::Reference(id)=o {Some(*id)}else{None};
                    if id.is_some_and(|id| active.contains(&id)) {return Err(Error::InvalidInput("cyclic PDF Form invocation".into()));}
                    let data=s.get_plain_content_with_limit(self.content_left).map_err(|e| Error::Parse(format!("PDF Form content: {e}")))?;
                    let mut child=state.clone();
                    if let Some(m)=s.dict.get(b"Matrix").ok().map(|o| resolve(pdf,o)).and_then(|o| o.as_array().ok()).and_then(|a| matrix(a)) {child.ctm=m.then(child.ctm);}
                    let child_r=match s.dict.get(b"Resources") {Ok(o)=>dict(pdf,o),Err(_)=>r};
                    let pending=self.pending_space; self.pending_space=false; if let Some(id)=id {active.push(id);}
                    self.content(pdf,&data,child_r,child,depth+1,active)?;
                    if id.is_some() {active.pop();} self.pending_space=pending;
                }
                _=>{},
            }
        } Ok(())
    }
}
pub(super) fn page(pdf:&Document,id:ObjectId,options:&Options)->Result<Page> {
    let page=analyze_page(pdf,id,options,None)?;
    debug_assert!(page.elements.is_empty() && page.unsupported_inline_images==0); Ok(page)
}
pub(super) fn analyze_page(pdf:&Document,id:ObjectId,options:&Options,page_number:Option<u32>)->Result<Page> {
    if page_number==Some(0) {return Err(Error::InvalidInput("PDF inspection pages are one-based".into()));}
    let limit=options.limits.archive_bytes.min(usize::MAX as u64) as usize;
    let data=pdf.get_page_content_with_limit(id,limit).map_err(|e| Error::Parse(format!("PDF page content: {e}")))?;
    let mut reader=Reader {runs:Vec::new(),undecoded:0,real_gaps:0,pending_space:false,content_left:limit,ops_left:options.limits.xml_nodes as usize,forms_left:options.limits.archive_entries,output_left:limit,collector:page_number.map(|p| Collector::new(p,options.limits.blocks)),operation:0,max_segments:options.limits.archive_entries,unsupported_inline_images:0};
    reader.content(pdf,&data,resources(pdf,id),State::default(),0,&mut Vec::new())?;
    Ok(Page {lines:build_lines(&reader.runs),undecoded:reader.undecoded,real_gaps:reader.real_gaps,elements:reader.collector.map(Collector::finish).unwrap_or_default(),unsupported_inline_images:reader.unsupported_inline_images})
}
fn build_lines(runs:&[Run])->Vec<String> {
    let mut gaps=BTreeMap::<i64,usize>::new();
    for pair in runs.windows(2) {let dy=(pair[1].y-pair[0].y).abs(); let height=pair[0].height.max(pair[1].height); if dy>0.5*height && dy<=2.6*height {*gaps.entry((dy*2.).round() as i64).or_default()+=1;}}
    let common=gaps.iter().max_by(|(a,n),(b,m)| n.cmp(m).then_with(|| b.cmp(a))).map(|(g,_)| *g as f64/2.);
    let mut out=String::new(); let mut last:Option<&Run>=None;
    for run in runs {
        let mut new_line=false;
        if let Some(prev)=last {
            let dy=(run.y-prev.y).abs(); let size=prev.height.max(run.height);
            if dy>0.5*size {let len=out.trim_end_matches([' ','\t']).len(); out.truncate(len); if !out.is_empty() {out.push('\n'); if dy>2.6*size || common.is_some_and(|g| dy>1.45*g) {out.push('\n');}} new_line=true;}
            else if run.x-prev.end>0.18*size {push_space(&mut out);}
        }
        if run.space && !new_line {push_space(&mut out);}
        for c in run.text.chars() {if c.is_whitespace() {push_space(&mut out);} else if !c.is_control() {out.push(c);}} last=Some(run);
    } out.lines().map(|l| l.trim_end().to_owned()).collect()
}
fn push_space(out:&mut String) {if !out.is_empty() && !out.ends_with([' ','\n']) {out.push(' ');}}
fn edge(lines:&[String],first:bool)->Option<usize> {if first {lines.iter().position(|l| !l.trim().is_empty())} else {lines.iter().rposition(|l| !l.trim().is_empty())}}
fn page_number(line:&str)->bool {let s=line.trim().trim_matches(['-','–','—',' ']); !s.is_empty() && s.len()<=8 && (s.chars().all(|c| c.is_ascii_digit()) || s.chars().all(|c| "ivxlcdm".contains(c)))}
fn furniture_key(line:&str)->String {line.chars().filter(|c| !c.is_ascii_digit()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")}
pub(super) fn remove_furniture(pages:&mut [Page])->usize {
    let mut removed=0;
    for _ in 0..2 {
        for page in pages.iter_mut() {
            let mut drop=Vec::new();
            for first in [true,false] {if let Some(i)=edge(&page.lines,first) && page_number(&page.lines[i]) && !drop.contains(&i) {drop.push(i);}}
            drop.sort_unstable(); for i in drop.into_iter().rev() {page.lines.remove(i); removed+=1;}
        }
    }
    let threshold=(pages.len()/4).max(3);
    for _ in 0..2 {
        let mut counts:[BTreeMap<String,usize>;2]=Default::default();
        for page in pages.iter() {for (slot,first) in [true,false].into_iter().enumerate() {if let Some(i)=edge(&page.lines,first) {let key=furniture_key(&page.lines[i]); if !key.is_empty() && key.chars().count()<=80 {*counts[slot].entry(key).or_default()+=1;}}}}
        for page in pages.iter_mut() {
            let mut drop=Vec::new();
            for (slot,first) in [true,false].into_iter().enumerate() {if let Some(i)=edge(&page.lines,first) && counts[slot].get(&furniture_key(&page.lines[i])).is_some_and(|n| *n>=threshold) && !drop.contains(&i) {drop.push(i);}}
            let nonempty=page.lines.iter().filter(|l| !l.trim().is_empty()).count(); if nonempty>1 && drop.len()>=nonempty {continue;}
            drop.sort_unstable(); for i in drop.into_iter().rev() {page.lines.remove(i); removed+=1;}
        }
    } removed
}

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub schema_version:String, pub source:Source, pub title:Option<String>, pub format:String, pub extractor:String,
    pub blocks:Vec<Block>, pub tables:Vec<Table>, pub diagnostics:Vec<Diagnostic>,
    #[serde(default, skip_serializing_if="Vec::is_empty")] pub assets:Vec<ExtractedAsset>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {pub name:String,pub sha256:String,pub byte_length:usize,pub encoding:Option<String>}
/// In-memory asset bytes remain serialized as before; binary-file export is separate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedAsset {pub id:String,pub filename:String,pub media_type:String,pub sha256:String,pub byte_length:usize,pub bytes:Vec<u8>,pub source:SourceSpan}
/// One-based positions except zero-based, half-open decoded UTF-8 offsets.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSpan {
    #[serde(skip_serializing_if="Option::is_none")] pub member:Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] pub page:Option<u32>,
    #[serde(skip_serializing_if="Option::is_none")] pub slide:Option<u32>,
    #[serde(skip_serializing_if="Option::is_none")] pub sheet:Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] pub row:Option<u32>,
    #[serde(skip_serializing_if="Option::is_none")] pub column:Option<u32>,
    #[serde(skip_serializing_if="Option::is_none")] pub line_start:Option<u32>,
    #[serde(skip_serializing_if="Option::is_none")] pub line_end:Option<u32>,
    #[serde(skip_serializing_if="Option::is_none")] pub decoded_byte_start:Option<usize>,
    #[serde(skip_serializing_if="Option::is_none")] pub decoded_byte_end:Option<usize>,
    #[serde(skip_serializing_if="Option::is_none")] pub path:Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag="type",rename_all="snake_case")]
pub enum BlockKind {
    Paragraph, Heading {level:u8}, Code {language:Option<String>}, ListItem {ordered:bool}, Quote, Table {table_id:String},
    /// Block.text is an alt label, not an inferred visual description.
    Image {asset_id:String},
    ThematicBreak,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {pub id:String,pub parent_id:Option<String>,pub kind:BlockKind,pub text:String,pub section_path:Vec<String>,pub source:SourceSpan,pub keep_together:bool}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag="type",content="value",rename_all="snake_case")]
pub enum CellValue {Text(String),Integer(i64),Number(f64),Boolean(bool),SpreadsheetDate {serial:f64,calendar:Option<String>},DateTime(String),Error(String)}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cell {pub raw:Option<String>,pub value:Option<CellValue>,pub formula:Option<String>,pub source:SourceSpan}
impl Cell {
    pub fn text(text:impl Into<String>)->Self {let text=text.into(); Self {raw:Some(text.clone()),value:Some(CellValue::Text(text)),..Self::default()}}
    pub fn display(&self)->&str {self.raw.as_deref().unwrap_or("")}
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Table {pub id:String,pub name:Option<String>,pub columns:Vec<String>,pub has_header:bool,pub rows:Vec<Vec<Cell>>,pub source:SourceSpan}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {pub code:String,pub message:String,pub source:Option<SourceSpan>}
impl Document {
    pub fn children_of<'a>(&'a self,block_id:&'a str)->impl Iterator<Item=&'a Block>+'a {self.blocks.iter().filter(move |b| b.parent_id.as_deref()==Some(block_id))}
    pub(crate) fn push(&mut self,kind:BlockKind,text:String,source:SourceSpan) {
        let (section_path,parent_id)=self.blocks.last().map(|b| (b.section_path.clone(),b.parent_id.clone())).unwrap_or_default();
        let keep_together=matches!(kind,BlockKind::Code {..}|BlockKind::Table {..}|BlockKind::Image {..});
        self.blocks.push(Block {id:format!("{}:b{}",self.source.sha256,self.blocks.len()+1),parent_id,kind,text,section_path,source,keep_together});
    }
    pub(crate) fn heading(&mut self,level:u8,text:String,span:SourceSpan) {
        let parent=self.blocks.iter().rev().find(|b| matches!(b.kind,BlockKind::Heading {level:l} if l<level));
        let (mut path,parent_id)=parent.map(|b| (b.section_path.clone(),Some(b.id.clone()))).unwrap_or_default(); path.push(text.clone());
        self.push(BlockKind::Heading {level},text.clone(),span); let block=self.blocks.last_mut().unwrap(); block.section_path=path; block.parent_id=parent_id;
        if self.title.is_none() {self.title=Some(text);}
    }
    pub(crate) fn paragraph(&mut self,kind:BlockKind,text:String,span:SourceSpan) {
        self.push(kind,text,span);
        if let Some(heading)=self.blocks.iter().rev().skip(1).find(|b| matches!(b.kind,BlockKind::Heading {..})) {let parent=heading.id.clone(); self.blocks.last_mut().unwrap().parent_id=Some(parent);}
    }
    pub(crate) fn add_table(&mut self,mut table:Table) {table.id=format!("{}:t{}",self.source.sha256,self.tables.len()+1); self.paragraph(BlockKind::Table {table_id:table.id.clone()},String::new(),table.source.clone()); self.tables.push(table);}
    pub(crate) fn asset(&mut self,filename:String,media_type:&str,bytes:Vec<u8>,source:SourceSpan) {
        let sha256=format!("{:x}",Sha256::digest(&bytes)); let id=format!("{}:a{}",self.source.sha256,self.assets.len()+1);
        self.assets.push(ExtractedAsset {id,filename,media_type:media_type.into(),sha256,byte_length:bytes.len(),bytes,source});
    }
    pub(crate) fn warn(&mut self,code:&str,message:impl Into<String>) {self.diagnostics.push(Diagnostic {code:code.into(),message:message.into(),source:None});}
}

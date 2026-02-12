use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Presentation {
    pub slides: Vec<Slide>,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Slide {
    pub blocks: Vec<Block>,
    pub image: Option<ImageAsset>,
    pub warnings: Vec<String>,
    pub reveal_fragments: bool,
    pub line_spacing: Option<u8>,
}

#[derive(Debug, Clone)]
pub enum Block {
    BigText(String),
    Paragraph(String),
    Quote(String),
    List(ListData),
    Callout(CalloutData),
    Table(TableData),
    Code(CodeData),
}

#[derive(Debug, Clone)]
pub struct ListData {
    pub ordered: bool,
    pub items: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CalloutData {
    pub kind: CalloutKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub enum CalloutKind {
    Note,
    Tip,
    Warn,
}

#[derive(Debug, Clone)]
pub struct TableData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct CodeData {
    pub lang: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ImageAsset {
    pub path: PathBuf,
    pub alt: Option<String>,
    pub valign: VerticalAlign,
    pub halign: HorizontalAlign,
    pub frames: Vec<AsciiFrame>,
    pub delays_ms: Vec<u16>,
    pub cached_for: Option<(u16, u16)>,
    pub load_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct AsciiFrame {
    pub lines: Vec<String>,
    pub width: u16,
    pub height: u16,
}

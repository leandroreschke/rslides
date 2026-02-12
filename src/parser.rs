use std::fs;
use std::path::{Path, PathBuf};

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::model::{
    Block, CalloutData, CalloutKind, CodeData, ColumnAlign, CoverData, HorizontalAlign, ImageAsset,
    ImageMode, ListData, Presentation, Slide, TableData, VerticalAlign,
};

pub fn parse_presentation(path: &Path) -> Result<Presentation, String> {
    let content = fs::read_to_string(path)
        .map_err(|err| format!("failed to read markdown file {}: {err}", path.display()))?;
    parse_presentation_from_str(&content, path)
}

pub fn parse_presentation_from_str(
    content: &str,
    source_path: &Path,
) -> Result<Presentation, String> {
    let chunks = split_slides(content);
    if chunks.is_empty() {
        return Err("no slides found in markdown".to_string());
    }

    let mut slides = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        if idx == 0
            && let Some((cover, image)) = parse_cover_chunk(&chunk.content)
        {
            slides.push(Slide {
                blocks: Vec::new(),
                title: chunk.config.title.clone(),
                image,
                warnings: Vec::new(),
                reveal_fragments: false,
                line_spacing: None,
                column_ratios: chunk.config.column_ratios.clone(),
                image_mode: chunk.config.image_mode,
                cover: Some(cover),
            });
            continue;
        }
        slides.push(parse_slide_chunk(&chunk.content, &chunk.config)?);
    }

    if slides.is_empty() {
        return Err("no slides found in markdown".to_string());
    }

    Ok(Presentation {
        slides,
        source_path: source_path.to_path_buf(),
    })
}

#[derive(Debug, Clone, Default)]
pub struct SlideConfig {
    pub column_ratios: Option<Vec<u16>>,
    pub image_mode: Option<ImageMode>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SlideChunk {
    pub content: String,
    pub config: SlideConfig,
}

pub fn split_slides(markdown: &str) -> Vec<SlideChunk> {
    let mut slides = Vec::new();
    let mut current = String::new();
    let mut fence_char: Option<char> = None;
    let mut current_config = SlideConfig::default();

    for line in markdown.lines() {
        let trimmed = line.trim();

        if fence_char.is_none() {
            if trimmed.starts_with("```") {
                fence_char = Some('`');
            } else if trimmed.starts_with("~~~") {
                fence_char = Some('~');
            }
        } else if fence_char == Some('`') && trimmed.starts_with("```") {
            fence_char = None;
        } else if fence_char == Some('~') && trimmed.starts_with("~~~") {
            fence_char = None;
        }

        if fence_char.is_none() && trimmed.starts_with("---") {
            let next_config = parse_delimiter_config(trimmed).unwrap_or_default();
            if !current.trim().is_empty() || should_emit_empty_slide(&current_config) {
                slides.push(SlideChunk {
                    content: current.trim_end().to_string(),
                    config: current_config,
                });
            }
            current.clear();
            current_config = next_config;
            continue;
        }

        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() || should_emit_empty_slide(&current_config) {
        slides.push(SlideChunk {
            content: current.trim_end().to_string(),
            config: current_config,
        });
    }

    slides
}

fn should_emit_empty_slide(config: &SlideConfig) -> bool {
    config
        .title
        .as_ref()
        .is_some_and(|title| !title.trim().is_empty())
}

fn parse_delimiter_config(trimmed: &str) -> Option<SlideConfig> {
    if trimmed == "---" {
        return Some(SlideConfig::default());
    }
    let cfg = trimmed.strip_prefix("---")?.trim();
    let cfg = cfg.strip_prefix('{')?.strip_suffix('}')?;
    let mut out = SlideConfig::default();

    // Support: --- {columns: [1,3], image-mode: native}
    let cfg_lc = cfg.to_ascii_lowercase();
    if let Some(col_pos) = cfg_lc.find("columns:") {
        let rest = &cfg[col_pos + "columns:".len()..];
        if let Some(start) = rest.find('[')
            && let Some(end) = rest[start + 1..].find(']')
        {
            let inner = &rest[start + 1..start + 1 + end];
            let cols = inner
                .split(',')
                .filter_map(|v| v.trim().parse::<u16>().ok())
                .filter(|v| *v > 0)
                .collect::<Vec<_>>();
            if !cols.is_empty() {
                out.column_ratios = Some(cols);
            }
        }
    }
    if let Some(mode_pos) = cfg_lc.find("image-mode:") {
        let rest = cfg[mode_pos + "image-mode:".len()..]
            .split([',', '}'])
            .next()
            .unwrap_or("")
            .trim();
        if !rest.is_empty() {
            out.image_mode = parse_image_mode(rest);
        }
    }
    if let Some(title) = parse_delimiter_string_value(cfg, "title") {
        out.title = Some(title);
    }

    Some(out)
}

fn parse_delimiter_string_value(cfg: &str, key: &str) -> Option<String> {
    let cfg_lc = cfg.to_ascii_lowercase();
    let needle = format!("{key}:");
    let start = cfg_lc.find(&needle)?;
    let mut rest = cfg[start + needle.len()..].trim_start();
    if rest.is_empty() {
        return None;
    }
    if let Some(after_quote) = rest.strip_prefix('"') {
        let end = after_quote.find('"')?;
        let value = after_quote[..end].trim();
        return (!value.is_empty()).then(|| value.to_string());
    }
    let end = rest.find(',').unwrap_or(rest.len());
    rest = rest[..end].trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

fn parse_image_mode(value: &str) -> Option<ImageMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(ImageMode::Auto),
        "ascii" => Some(ImageMode::Ascii),
        "native" => Some(ImageMode::Native),
        _ => None,
    }
}

fn parse_cover_chunk(chunk: &str) -> Option<(CoverData, Option<ImageAsset>)> {
    let mut title: Option<String> = None;
    let mut subtitle: Option<String> = None;
    let mut author: Option<String> = None;
    let mut image_path: Option<PathBuf> = None;

    for line in chunk.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (key, value) = trimmed.split_once(':')?;
        let key = key.trim();
        let value = value.trim();
        let value = unquote_value(value);
        match key {
            "title" => title = Some(value),
            "sub_title" => subtitle = Some(value),
            "author" => author = Some(value),
            "image" => image_path = Some(PathBuf::from(value)),
            _ => return None,
        }
    }

    let title = title?;
    let cover = CoverData {
        title,
        subtitle,
        author,
    };
    let image = image_path.map(|path| ImageAsset {
        path,
        alt: None,
        valign: VerticalAlign::Middle,
        halign: HorizontalAlign::Center,
        frames: Vec::new(),
        delays_ms: Vec::new(),
        cached_for: None,
        load_error: None,
    });
    Some((cover, image))
}

fn unquote_value(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return trimmed[1..trimmed.len() - 1].trim().to_string();
    }
    trimmed.to_string()
}

fn parse_slide_chunk(chunk: &str, config: &SlideConfig) -> Result<Slide, String> {
    let directives = parse_slide_directives(chunk);
    let sanitized_chunk = directives.sanitized;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(&sanitized_chunk, options);

    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let mut image: Option<ImageAsset> = None;
    let mut first_h1_seen = false;

    let mut in_paragraph = false;
    let mut paragraph = String::new();

    let mut in_quote = false;
    let mut quote = String::new();

    let mut in_heading: Option<HeadingLevel> = None;
    let mut heading = String::new();

    let mut list = ListBuilder::default();

    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_body = String::new();

    let mut table = TableBuilder::default();

    let mut in_image = false;
    let mut pending_image_path: Option<PathBuf> = None;
    let mut pending_image_alt = String::new();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    in_paragraph = true;
                }
                Tag::Heading { level, .. } => {
                    in_heading = Some(level);
                    heading.clear();
                }
                Tag::BlockQuote(_) => {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    in_quote = true;
                    quote.clear();
                }
                Tag::List(start) => {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    list.depth += 1;
                    if list.depth == 1 {
                        list.ordered = start.is_some();
                        list.next_index = start.unwrap_or(1);
                    }
                }
                Tag::Item => {
                    list.in_item = true;
                    list.current_item.clear();
                    if list.depth > 1 {
                        list.current_item
                            .push_str(&"  ".repeat(list.depth.saturating_sub(1)));
                    }
                }
                Tag::CodeBlock(kind) => {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    in_code_block = true;
                    code_body.clear();
                    code_lang = match kind {
                        CodeBlockKind::Fenced(lang) => {
                            let trimmed = lang.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_string())
                            }
                        }
                        CodeBlockKind::Indented => None,
                    };
                }
                Tag::Table(_) => {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    table.reset();
                }
                Tag::TableHead => {
                    table.in_head = true;
                }
                Tag::TableRow => {
                    table.current_row.clear();
                    table.in_row = true;
                }
                Tag::TableCell => {
                    table.current_cell.clear();
                    table.in_cell = true;
                }
                Tag::Image { dest_url, .. } => {
                    in_image = true;
                    pending_image_path = Some(PathBuf::from(dest_url.to_string()));
                    pending_image_alt.clear();
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                }
                TagEnd::Heading(level) => {
                    let trimmed = heading.trim().to_string();
                    if !trimmed.is_empty() {
                        match level {
                            HeadingLevel::H1 if !first_h1_seen => {
                                blocks.push(Block::BigText(trimmed));
                                first_h1_seen = true;
                            }
                            HeadingLevel::H2
                            | HeadingLevel::H3
                            | HeadingLevel::H4
                            | HeadingLevel::H5
                            | HeadingLevel::H6 => {
                                let heading_level = match level {
                                    HeadingLevel::H2 => 1,
                                    HeadingLevel::H3 => 2,
                                    _ => 3,
                                };
                                blocks.push(Block::SectionHeading {
                                    level: heading_level,
                                    text: trimmed,
                                });
                            }
                            _ => {
                                blocks.push(Block::Paragraph(trimmed));
                            }
                        }
                    }
                    heading.clear();
                    in_heading = None;
                }
                TagEnd::BlockQuote(_) => {
                    in_quote = false;
                    let trimmed = quote.trim();
                    if !trimmed.is_empty() {
                        if let Some((kind, text)) = parse_callout(trimmed) {
                            blocks.push(Block::Callout(CalloutData { kind, text }));
                        } else {
                            blocks.push(Block::Quote(trimmed.to_string()));
                        }
                    }
                    quote.clear();
                }
                TagEnd::List(_) => {
                    if list.depth == 1 {
                        list.flush(&mut blocks);
                    }
                    list.depth = list.depth.saturating_sub(1);
                }
                TagEnd::Item => {
                    list.in_item = false;
                    let item = list.current_item.trim().to_string();
                    if !item.is_empty() {
                        list.items.push(item);
                        if list.ordered {
                            list.next_index += 1;
                        }
                    }
                    list.current_item.clear();
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    blocks.push(Block::Code(CodeData {
                        lang: code_lang.take(),
                        source: code_body.trim_end().to_string(),
                    }));
                    code_body.clear();
                }
                TagEnd::TableCell => {
                    table.in_cell = false;
                    table
                        .current_row
                        .push(table.current_cell.trim().to_string());
                }
                TagEnd::TableRow => {
                    table.in_row = false;
                    if table.in_head {
                        table.headers = table.current_row.clone();
                    } else if !table.current_row.is_empty() {
                        table.rows.push(table.current_row.clone());
                    }
                    table.current_row.clear();
                }
                TagEnd::TableHead => {
                    table.in_head = false;
                }
                TagEnd::Table => {
                    if !table.headers.is_empty() || !table.rows.is_empty() {
                        blocks.push(Block::Table(TableData {
                            headers: table.headers.clone(),
                            rows: table.rows.clone(),
                        }));
                    }
                    table.reset();
                }
                TagEnd::Image => {
                    in_image = false;
                    if let Some(path) = pending_image_path.take() {
                        if image.is_none() {
                            let meta = parse_image_alt_meta(&pending_image_alt);
                            image = Some(ImageAsset {
                                path,
                                alt: meta.alt,
                                valign: meta.valign,
                                halign: meta.halign,
                                frames: Vec::new(),
                                delays_ms: Vec::new(),
                                cached_for: None,
                                load_error: None,
                            });
                        } else {
                            warnings.push(
                                "only one image per slide is supported; additional image ignored"
                                    .to_string(),
                            );
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                push_text(
                    text.as_ref(),
                    &mut in_heading,
                    &mut heading,
                    in_code_block,
                    &mut code_body,
                    &mut table,
                    &mut paragraph,
                    in_image,
                    in_quote,
                    &mut quote,
                    &mut list,
                    &mut pending_image_alt,
                );
            }
            Event::Code(text) => {
                let decorated = format!("`{}`", text.as_ref());
                push_text(
                    &decorated,
                    &mut in_heading,
                    &mut heading,
                    in_code_block,
                    &mut code_body,
                    &mut table,
                    &mut paragraph,
                    in_image,
                    in_quote,
                    &mut quote,
                    &mut list,
                    &mut pending_image_alt,
                );
            }
            Event::Html(html) => {
                if let Some(column) = parse_column_directive(html.as_ref()) {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    blocks.push(Block::ColumnBreak(column));
                } else if let Some(align) = parse_align_directive(html.as_ref()) {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    blocks.push(Block::ColumnAlign(align));
                } else if let Some(spacing) = parse_inline_line_spacing(html.as_ref()) {
                    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);
                    if spacing > 0 {
                        blocks.push(Block::Spacer(spacing));
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_body.push('\n');
                } else if table.in_cell {
                    table.current_cell.push(' ');
                } else if in_heading.is_some() {
                    heading.push(' ');
                } else if list.in_item {
                    list.current_item.push(' ');
                } else if in_quote {
                    quote.push(' ');
                } else {
                    paragraph.push(' ');
                }
            }
            _ => {}
        }
    }

    flush_paragraph(&mut blocks, &mut paragraph, &mut in_paragraph);

    Ok(Slide {
        blocks,
        title: config.title.clone(),
        image,
        warnings,
        reveal_fragments: directives.reveal_fragments,
        line_spacing: directives.line_spacing,
        column_ratios: config.column_ratios.clone(),
        image_mode: config.image_mode,
        cover: None,
    })
}

struct SlideDirectives {
    sanitized: String,
    reveal_fragments: bool,
    line_spacing: Option<u8>,
}

fn parse_slide_directives(chunk: &str) -> SlideDirectives {
    let mut reveal_fragments = false;
    let mut line_spacing = None;
    let mut out = String::new();

    for line in chunk.lines() {
        let trimmed = line.trim().to_ascii_lowercase();
        if trimmed == "<!-- reveal: on -->" {
            reveal_fragments = true;
            continue;
        }
        if trimmed == "<!-- reveal: off -->" {
            reveal_fragments = false;
            continue;
        }
        if let Some(value) = trimmed
            .strip_prefix("<!-- line_spacing:")
            .and_then(|s| s.strip_suffix("-->"))
        {
            if let Ok(parsed) = value.trim().parse::<u8>() {
                line_spacing = Some(parsed.min(6));
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    SlideDirectives {
        sanitized: out,
        reveal_fragments,
        line_spacing,
    }
}

fn parse_inline_line_spacing(input: &str) -> Option<usize> {
    let trimmed = input.trim().to_ascii_lowercase();
    let value = trimmed
        .strip_prefix("<!-- line-spacing:")
        .or_else(|| trimmed.strip_prefix("<!-- line_spacing:"))?
        .strip_suffix("-->")?
        .trim();
    value.parse::<usize>().ok().map(|v| v.min(24))
}

fn parse_column_directive(input: &str) -> Option<usize> {
    let trimmed = input.trim().to_ascii_lowercase();
    let value = trimmed
        .strip_prefix("<!-- column:")
        .and_then(|s| s.strip_suffix("-->"))?
        .trim();
    value.parse::<usize>().ok()
}

fn parse_align_directive(input: &str) -> Option<ColumnAlign> {
    let trimmed = input.trim().to_ascii_lowercase();
    let value = trimmed
        .strip_prefix("<!-- align:")
        .and_then(|s| s.strip_suffix("-->"))?
        .trim();
    match value {
        "left" => Some(ColumnAlign::Left),
        "center" => Some(ColumnAlign::Center),
        "right" => Some(ColumnAlign::Right),
        _ => None,
    }
}

fn parse_callout(input: &str) -> Option<(CalloutKind, String)> {
    let upper = input.to_ascii_uppercase();
    let (kind, prefix_len) = if upper.starts_with("[!NOTE]") {
        (CalloutKind::Note, 7)
    } else if upper.starts_with("[!TIP]") {
        (CalloutKind::Tip, 6)
    } else if upper.starts_with("[!WARN]") || upper.starts_with("[!WARNING]") {
        let len = if upper.starts_with("[!WARNING]") {
            10
        } else {
            7
        };
        (CalloutKind::Warn, len)
    } else {
        return None;
    };

    let text = input[prefix_len..].trim().to_string();
    Some((kind, text))
}

fn flush_paragraph(blocks: &mut Vec<Block>, paragraph: &mut String, in_paragraph: &mut bool) {
    let trimmed = paragraph.trim();
    if !trimmed.is_empty() {
        if let Some((kind, text)) = parse_callout(trimmed) {
            blocks.push(Block::Callout(CalloutData { kind, text }));
        } else {
            blocks.push(Block::Paragraph(trimmed.to_string()));
        }
    }
    paragraph.clear();
    *in_paragraph = false;
}

#[allow(clippy::too_many_arguments)]
fn push_text(
    text: &str,
    in_heading: &mut Option<HeadingLevel>,
    heading: &mut String,
    in_code_block: bool,
    code_body: &mut String,
    table: &mut TableBuilder,
    paragraph: &mut String,
    in_image: bool,
    in_quote: bool,
    quote: &mut String,
    list: &mut ListBuilder,
    image_alt: &mut String,
) {
    if in_code_block {
        code_body.push_str(text);
    } else if table.in_cell {
        table.current_cell.push_str(text);
    } else if in_heading.is_some() {
        heading.push_str(text);
    } else if list.in_item {
        list.current_item.push_str(text);
    } else if in_quote {
        quote.push_str(text);
    } else if in_image {
        image_alt.push_str(text);
    } else {
        paragraph.push_str(text);
    }
}

struct ImageAltMeta {
    alt: Option<String>,
    valign: VerticalAlign,
    halign: HorizontalAlign,
}

fn parse_image_alt_meta(input: &str) -> ImageAltMeta {
    let mut meta = ImageAltMeta {
        alt: None,
        valign: VerticalAlign::Top,
        halign: HorizontalAlign::Center,
    };

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return meta;
    }

    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        meta.alt = Some(trimmed.to_string());
        return meta;
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    for part in inner.split(',') {
        let Some((raw_key, raw_value)) = part.split_once(':') else {
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let value = raw_value.trim().trim_matches('"').to_ascii_lowercase();

        match key.as_str() {
            "valign" => {
                meta.valign = match value.as_str() {
                    "top" => VerticalAlign::Top,
                    "bottom" => VerticalAlign::Bottom,
                    _ => VerticalAlign::Middle,
                };
            }
            "halign" => {
                meta.halign = match value.as_str() {
                    "left" => HorizontalAlign::Left,
                    "right" => HorizontalAlign::Right,
                    _ => HorizontalAlign::Center,
                };
            }
            "alt" => {
                let original = raw_value.trim().trim_matches('"').to_string();
                if !original.is_empty() {
                    meta.alt = Some(original);
                }
            }
            _ => {}
        }
    }

    meta
}

#[derive(Default)]
struct ListBuilder {
    ordered: bool,
    items: Vec<String>,
    in_item: bool,
    current_item: String,
    depth: usize,
    next_index: u64,
}

impl ListBuilder {
    fn flush(&mut self, blocks: &mut Vec<Block>) {
        if !self.items.is_empty() {
            blocks.push(Block::List(ListData {
                ordered: self.ordered,
                items: self.items.clone(),
            }));
        }
        self.items.clear();
        self.in_item = false;
        self.current_item.clear();
        self.ordered = false;
        self.next_index = 1;
    }
}

#[derive(Default)]
struct TableBuilder {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_head: bool,
    in_row: bool,
    in_cell: bool,
}

impl TableBuilder {
    fn reset(&mut self) {
        self.headers.clear();
        self.rows.clear();
        self.current_row.clear();
        self.current_cell.clear();
        self.in_head = false;
        self.in_row = false;
        self.in_cell = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;

    #[test]
    fn splits_on_delimiter_outside_fences() {
        let input = "# A\n---\n# B\n";
        let chunks = split_slides(input);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content, "# A");
        assert_eq!(chunks[1].content, "# B");
    }

    #[test]
    fn does_not_split_inside_fenced_code() {
        let input = "```\n---\n```\n---\n# Slide\n";
        let chunks = split_slides(input);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].content.contains("---"));
    }

    #[test]
    fn emits_empty_slide_when_delimiter_has_title_config() {
        let input = "--- {title: \"Only Title\"}\n---\n# Next\n";
        let chunks = split_slides(input);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content, "");
        assert_eq!(chunks[0].config.title.as_deref(), Some("Only Title"));
        assert_eq!(chunks[1].content, "# Next");
    }

    #[test]
    fn maps_h1_to_bigtext_and_captures_first_image() {
        let md = "# Hello\n\n![a](one.png)\n![b](two.png)\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md"))
            .expect("presentation should parse");
        let slide = &presentation.slides[0];

        assert!(matches!(slide.blocks.first(), Some(Block::BigText(_))));
        let image = slide.image.as_ref().expect("image should exist");
        assert_eq!(image.path, PathBuf::from("one.png"));
        assert_eq!(slide.warnings.len(), 1);
    }

    #[test]
    fn parses_table_and_fenced_code_language() {
        let md = "| A | B |\n| - | - |\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md"))
            .expect("presentation should parse");
        let slide = &presentation.slides[0];

        assert!(slide.blocks.iter().any(|b| matches!(b, Block::Table(_))));
        assert!(slide.blocks.iter().any(|b| {
            matches!(
                b,
                Block::Code(CodeData {
                    lang: Some(lang), ..
                }) if lang == "rust"
            )
        }));
    }

    #[test]
    fn parses_callout_quote_list_and_reveal() {
        let md = "<!-- reveal: on -->\n[!NOTE] remember this\n\n> quoted\n\n- one\n- two\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let slide = &presentation.slides[0];
        assert!(slide.reveal_fragments);
        assert!(slide.blocks.iter().any(|b| matches!(b, Block::Callout(_))));
        assert!(slide.blocks.iter().any(|b| matches!(b, Block::Quote(_))));
        assert!(slide.blocks.iter().any(|b| matches!(b, Block::List(_))));
    }

    #[test]
    fn parses_cover_first_slide() {
        let md = "title: Demo\nsub_title: Hello\nauthor: Me\nimage: assets/demo.png\n---\n# Next\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let first = &presentation.slides[0];
        assert!(first.cover.is_some());
        assert!(first.image.is_some());
    }

    #[test]
    fn parses_quoted_cover_values() {
        let md = "title: \"The future is LOVABLE.\"\nsub_title: \"This is super cool\"\nauthor: \"Me\"\nimage: \"./assets/lovable.svg\"\n---\n# Next\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let slide = &presentation.slides[0];
        let first = slide.cover.as_ref().expect("cover exists");
        assert_eq!(first.title, "The future is LOVABLE.");
        assert_eq!(first.subtitle.as_deref(), Some("This is super cool"));
        assert_eq!(first.author.as_deref(), Some("Me"));
        assert_eq!(
            slide.image.as_ref().map(|img| img.path.clone()),
            Some(PathBuf::from("./assets/lovable.svg"))
        );
    }

    #[test]
    fn leading_delimiter_config_applies_to_cover_slide() {
        let md = "--- {columns: [1,3], image-mode: native}\ntitle: Demo\nsub_title: Hello\nauthor: Me\n---\n# Next\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let first = &presentation.slides[0];
        assert!(first.cover.is_some());
        assert_eq!(first.column_ratios, Some(vec![1, 3]));
        assert_eq!(first.image_mode, Some(ImageMode::Native));
    }

    #[test]
    fn delimiter_config_applies_to_next_slide() {
        let md = "# One\n--- {columns: [1,3], image-mode: native, title: \"Slide Title\"}\n# Two\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        assert_eq!(presentation.slides.len(), 2);
        assert!(presentation.slides[0].column_ratios.is_none());
        assert_eq!(presentation.slides[1].column_ratios, Some(vec![1, 3]));
        assert_eq!(presentation.slides[1].image_mode, Some(ImageMode::Native));
        assert_eq!(presentation.slides[1].title.as_deref(), Some("Slide Title"));
    }

    #[test]
    fn inline_line_spacing_inserts_spacer_block() {
        let md = "# A\n\nBefore\n\n<!-- line-spacing: 10 -->\n\nAfter\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let slide = &presentation.slides[0];
        assert!(slide.blocks.iter().any(|b| matches!(b, Block::Spacer(10))));
    }

    #[test]
    fn parses_column_and_align_directives() {
        let md = "# A\n<!-- column: 1 -->\ntext\n<!-- align: center -->\nmore\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let slide = &presentation.slides[0];
        assert!(
            slide
                .blocks
                .iter()
                .any(|b| matches!(b, Block::ColumnBreak(1)))
        );
        assert!(
            slide
                .blocks
                .iter()
                .any(|b| matches!(b, Block::ColumnAlign(ColumnAlign::Center)))
        );
    }

    #[test]
    fn maps_h2_h3_to_section_heading_blocks() {
        let md = "## One\nBody\n### Two\nBody\n";
        let presentation = parse_presentation_from_str(md, Path::new("deck.md")).expect("parse ok");
        let slide = &presentation.slides[0];
        assert!(
            slide
                .blocks
                .iter()
                .any(|b| matches!(b, Block::SectionHeading { level: 1, .. }))
        );
        assert!(
            slide
                .blocks
                .iter()
                .any(|b| matches!(b, Block::SectionHeading { level: 2, .. }))
        );
    }
}

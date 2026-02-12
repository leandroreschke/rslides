pub mod bigtext;
pub mod code;
pub mod image_ascii;
pub mod layout;
pub mod table;
pub mod text;

use std::path::Path;
use std::time::Duration;

use crate::model::{
    Block, CalloutKind, ColumnAlign, CoverData, HorizontalAlign, Slide, VerticalAlign,
};
use crate::render::code::CodeHighlighter;
use crate::render::image_ascii::{current_frame, ensure_ascii_frames};

const RESET_COLOR: &str = "\x1b[39m";
const BOLD_ON: &str = "\x1b[1m";
const BOLD_OFF: &str = "\x1b[22m";
const TITLE_CONTENT_GAP_ROWS: usize = 2;
const COVER_SUBTITLE_IMAGE_GAP_ROWS: usize = 3;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub title_start: (u8, u8, u8),
    pub title_end: (u8, u8, u8),
    pub text: (u8, u8, u8),
    pub quote: (u8, u8, u8),
    pub table: (u8, u8, u8),
    pub code_bg: (u8, u8, u8),
    pub callout_note_bg: (u8, u8, u8),
    pub callout_tip_bg: (u8, u8, u8),
    pub callout_warn_bg: (u8, u8, u8),
    pub footer_muted: (u8, u8, u8),
    pub footer_accent: (u8, u8, u8),
    pub footer_warn: (u8, u8, u8),
    pub cover_subtitle: (u8, u8, u8),
    pub cover_author: (u8, u8, u8),
    pub image_caption: (u8, u8, u8),
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            title_start: (178, 102, 255),
            title_end: (102, 245, 255),
            text: (236, 240, 246),
            quote: (206, 214, 228),
            table: (182, 224, 192),
            code_bg: (34, 38, 54),
            callout_note_bg: (37, 64, 104),
            callout_tip_bg: (30, 85, 73),
            callout_warn_bg: (96, 57, 36),
            footer_muted: (170, 177, 191),
            footer_accent: (244, 208, 120),
            footer_warn: (255, 170, 170),
            cover_subtitle: (255, 170, 142),
            cover_author: (255, 210, 158),
            image_caption: (255, 195, 120),
        }
    }
}

impl Theme {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|err| format!("failed to read theme file {}: {err}", path.display()))?;
        let mut theme = Self::default();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let color = parse_rgb(value.trim())?;
            match key.trim() {
                "title_start" => theme.title_start = color,
                "title_end" => theme.title_end = color,
                "text" => theme.text = color,
                "quote" => theme.quote = color,
                "table" => theme.table = color,
                "code_bg" => theme.code_bg = color,
                "callout_note_bg" => theme.callout_note_bg = color,
                "callout_tip_bg" => theme.callout_tip_bg = color,
                "callout_warn_bg" => theme.callout_warn_bg = color,
                "footer_muted" => theme.footer_muted = color,
                "footer_accent" => theme.footer_accent = color,
                "footer_warn" => theme.footer_warn = color,
                "cover_subtitle" => theme.cover_subtitle = color,
                "cover_author" => theme.cover_author = color,
                "image_caption" => theme.image_caption = color,
                _ => {}
            }
        }

        Ok(theme)
    }
}

fn parse_rgb(value: &str) -> Result<(u8, u8, u8), String> {
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() != 3 {
        return Err(format!("invalid RGB value: {value}"));
    }
    let r = parts[0]
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("invalid red component: {value}"))?;
    let g = parts[1]
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("invalid green component: {value}"))?;
    let b = parts[2]
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("invalid blue component: {value}"))?;
    Ok((r, g, b))
}

pub struct RenderParams<'a> {
    pub slide: &'a mut Slide,
    pub slide_number: usize,
    pub total_slides: usize,
    pub term_width: u16,
    pub term_height: u16,
    pub ansi: bool,
    pub fps: u16,
    pub slide_elapsed: Duration,
    pub base_dir: &'a Path,
    pub highlighter: &'a CodeHighlighter,
    pub prefer_real_images: bool,
    pub visible_blocks: Option<usize>,
    pub theme: &'a Theme,
    pub line_spacing: u8,
    pub column_ratios: &'a [u16],
}

pub struct RenderOutput {
    pub lines: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeImageRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy)]
enum LineRole {
    Title { row: usize, total: usize },
    SectionHeading(u8),
    CoverSubtitle,
    CoverAuthor,
    ImageCaption,
    Paragraph,
    Quote,
    Table,
    Code,
    Callout(CalloutKind),
    Empty,
}

#[derive(Clone)]
struct RenderLine {
    text: String,
    role: LineRole,
}

enum BodyToken {
    Line(RenderLine),
    ColumnBreak(usize),
    Align(ColumnAlign),
}

pub fn render_slide(params: RenderParams<'_>) -> RenderOutput {
    let body_height = params.term_height.saturating_sub(1) as usize;
    if let Some(cover) = params.slide.cover.clone() {
        return render_cover_slide(params, &cover, body_height);
    }
    let image_focus = slide_image_focus_mode(params.slide);

    if image_focus {
        return render_image_focus_slide(params, body_height);
    }

    let has_image = params.slide.image.is_some();
    let ratios = if has_image {
        effective_ratios(params.column_ratios, true)
    } else {
        vec![1]
    };
    let widths = layout::compute_column_widths(params.term_width, &ratios);
    let text_col_count = if has_image {
        widths.len().saturating_sub(1).max(1)
    } else {
        1
    };
    let text_width = widths
        .iter()
        .take(text_col_count)
        .copied()
        .max()
        .unwrap_or(params.term_width) as usize;
    let body_tokens = render_body_tokens(
        params.slide,
        text_width,
        params.highlighter,
        params.ansi,
        params.visible_blocks,
        params.line_spacing,
    );
    let token_lines = body_tokens
        .iter()
        .filter_map(|token| match token {
            BodyToken::Line(line) => Some(line.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let image_rect = compute_native_image_rect(
        params.slide,
        params.term_width,
        params.term_height,
        params.column_ratios,
    );
    let mut image_lines: Vec<String> = Vec::new();
    if has_image && !params.prefer_real_images {
        if let Some(image) = params.slide.image.as_mut() {
            if let Some(rect) = image_rect {
                ensure_ascii_frames(image, params.base_dir, rect.width, rect.height, params.fps);

                if let Some(err) = &image.load_error {
                    image_lines.push(format!("[image load error: {err}]"));
                } else if let Some(frame) = current_frame(image, params.slide_elapsed) {
                    image_lines = frame.lines.clone();
                }
            }
        }
    }

    let (title_lines, text_cursor_start, center_title_block) =
        configured_or_leading_title_lines(params.slide, &token_lines, params.term_width as usize);

    let title_height = title_lines.len();
    let body_line_count = token_lines.len().saturating_sub(text_cursor_start);
    let title_gap = if title_height > 0 && !center_title_block {
        TITLE_CONTENT_GAP_ROWS
    } else {
        0
    };
    let text_total_rows = title_height + title_gap + body_line_count;

    let mut lines = vec![" ".repeat(params.term_width as usize); body_height];
    let mut truncated = text_total_rows > body_height;
    if !image_lines.is_empty() && image_lines.len() > body_height {
        truncated = true;
    }

    let title_start_row = if center_title_block {
        body_height.saturating_sub(title_height) / 2
    } else {
        0
    };
    for row in 0..title_height.min(body_height.saturating_sub(title_start_row)) {
        let (line, role) = title_lines
            .get(row)
            .map(|entry| (entry.text.as_str(), entry.role))
            .unwrap_or(("", LineRole::Empty));
        lines[title_start_row + row] = style_line(
            centered_line(line, params.term_width as usize),
            role,
            params.ansi,
            params.theme,
        );
    }

    let mut x_positions = Vec::new();
    let mut x = 0usize;
    for w in &widths {
        x_positions.push(x);
        x += *w as usize + 1;
    }

    let text_start_row = if center_title_block {
        body_height
    } else {
        title_start_row
            .saturating_add(title_height)
            .saturating_add(title_gap)
            .min(body_height)
    };
    let mut line_cursor = 0usize;
    let mut row_per_col = vec![text_start_row; text_col_count];
    let mut current_col = 0usize;
    let mut align_per_col = vec![ColumnAlign::Left; text_col_count];
    row_per_col.fill(text_start_row);
    for token in body_tokens {
        match token {
            BodyToken::ColumnBreak(target) => {
                current_col = target.min(text_col_count.saturating_sub(1));
            }
            BodyToken::Align(align) => {
                align_per_col[current_col] = align;
            }
            BodyToken::Line(entry) => {
                if line_cursor < text_cursor_start {
                    line_cursor += 1;
                    continue;
                }
                line_cursor += 1;
                let col_x = x_positions[current_col];
                let col_w = widths[current_col] as usize;
                let row = row_per_col[current_col];
                if row >= body_height {
                    truncated = true;
                    continue;
                }
                let aligned = if matches!(entry.role, LineRole::Code) {
                    // Code blocks are already width-bounded in render_code_window. Re-clipping ANSI
                    // content here can cut escape sequences and visibly truncate rows.
                    entry.text.clone()
                } else {
                    align_to_width(
                        &text::clip_to_width(&entry.text, col_w),
                        col_w,
                        align_per_col[current_col],
                    )
                };
                let styled = style_line(aligned, entry.role, params.ansi, params.theme);
                overlay_text_at(&mut lines[row], col_x, &styled);
                row_per_col[current_col] = row.saturating_add(1);
            }
        }
    }

    if let Some(rect) = image_rect {
        for (idx, img_line) in image_lines.iter().enumerate() {
            let row = rect.y as usize + idx;
            if row >= body_height {
                break;
            }
            overlay_text_at(
                &mut lines[row],
                rect.x as usize,
                &text::clip_to_width(img_line, rect.width as usize),
            );
        }
        if let Some(image) = params.slide.image.as_ref() {
            if let Some(caption) = image.alt.as_ref() {
                if !caption.trim().is_empty() {
                    let row = rect.y as usize + rect.height as usize;
                    if row < body_height {
                        let cap = style_line(
                            centered_line(caption, rect.width as usize),
                            LineRole::ImageCaption,
                            params.ansi,
                            params.theme,
                        );
                        overlay_text_at(&mut lines[row], rect.x as usize, &cap);
                    }
                }
            }
        }
    }

    let footer = build_footer(
        params.term_width as usize,
        params.slide_number + 1,
        params.total_slides,
        truncated,
        params.ansi,
        params.theme,
    );
    lines.push(footer);

    RenderOutput { lines, truncated }
}

fn render_cover_slide(
    params: RenderParams<'_>,
    cover: &CoverData,
    body_height: usize,
) -> RenderOutput {
    let mut lines = vec![" ".repeat(params.term_width as usize); body_height];

    let mut y = 1usize;
    let title = bigtext::render_big_text(&cover.title, params.term_width as usize);
    let title_total = title.len().max(1);
    for (title_row, line) in title.into_iter().enumerate() {
        if y >= body_height {
            break;
        }
        lines[y] = style_line(
            centered_line(&line, params.term_width as usize),
            LineRole::Title {
                row: title_row,
                total: title_total,
            },
            params.ansi,
            params.theme,
        );
        y += 1;
    }

    if let Some(subtitle) = &cover.subtitle {
        if y + TITLE_CONTENT_GAP_ROWS < body_height {
            y += TITLE_CONTENT_GAP_ROWS;
            lines[y] = style_line(
                centered_line(subtitle, params.term_width as usize),
                LineRole::CoverSubtitle,
                params.ansi,
                params.theme,
            );
        }
    }

    if !params.prefer_real_images {
        if let Some(image) = params.slide.image.as_mut() {
            let title_zone = y.saturating_add(COVER_SUBTITLE_IMAGE_GAP_ROWS) as u16;
            let author_rows = if cover.author.is_some() { 2u16 } else { 0u16 };
            let available = params
                .term_height
                .saturating_sub(1)
                .saturating_sub(title_zone)
                .saturating_sub(author_rows);
            if available > 3 {
                let rect = NativeImageRect {
                    x: 0,
                    y: title_zone,
                    width: params.term_width,
                    height: available,
                };
                ensure_ascii_frames(image, params.base_dir, rect.width, rect.height, params.fps);
                if let Some(frame) = current_frame(image, params.slide_elapsed) {
                    for (idx, frame_line) in frame.lines.iter().enumerate() {
                        let row = rect.y as usize + idx;
                        if row >= lines.len() {
                            break;
                        }
                        lines[row] = text::clip_to_width(frame_line, params.term_width as usize);
                    }
                }
            }
        }
    }

    if let Some(author) = &cover.author {
        if body_height >= 2 {
            let row = body_height.saturating_sub(2);
            lines[row] = style_line(
                centered_line(author, params.term_width as usize),
                LineRole::CoverAuthor,
                params.ansi,
                params.theme,
            );
        }
    }

    let footer = build_footer(
        params.term_width as usize,
        params.slide_number + 1,
        params.total_slides,
        false,
        params.ansi,
        params.theme,
    );
    lines.push(footer);

    RenderOutput {
        lines,
        truncated: false,
    }
}

fn render_image_focus_slide(params: RenderParams<'_>, body_height: usize) -> RenderOutput {
    let body_tokens = render_body_tokens(
        params.slide,
        params.term_width as usize,
        params.highlighter,
        params.ansi,
        params.visible_blocks,
        params.line_spacing,
    );
    let text_lines = body_tokens
        .iter()
        .filter_map(|token| match token {
            BodyToken::Line(line) => Some(line.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let (title_lines, _, _) =
        configured_or_leading_title_lines(params.slide, &text_lines, params.term_width as usize);
    let title_rows = if title_lines.is_empty() {
        0
    } else {
        title_lines
            .len()
            .saturating_add(TITLE_CONTENT_GAP_ROWS)
            .min(body_height)
    };

    let mut lines = vec![" ".repeat(params.term_width as usize); body_height];
    for row in 0..title_rows {
        let (line, role) = title_lines
            .get(row)
            .map(|entry| (entry.text.as_str(), entry.role))
            .unwrap_or(("", LineRole::Empty));
        let centered = centered_line(line, params.term_width as usize);
        lines[row] = style_line(centered, role, params.ansi, params.theme);
    }

    let rect = compute_native_image_rect(
        params.slide,
        params.term_width,
        params.term_height,
        params.column_ratios,
    );
    if !params.prefer_real_images {
        if let Some(image) = params.slide.image.as_mut() {
            if let Some(rect) = rect {
                ensure_ascii_frames(image, params.base_dir, rect.width, rect.height, params.fps);
                if let Some(err) = &image.load_error {
                    let row = rect.y as usize;
                    if row < lines.len() {
                        lines[row] = centered_line(
                            &format!("[image load error: {err}]"),
                            params.term_width as usize,
                        );
                    }
                } else if let Some(frame) = current_frame(image, params.slide_elapsed) {
                    for (idx, frame_line) in frame.lines.iter().enumerate() {
                        let row = rect.y as usize + idx;
                        if row >= lines.len() {
                            break;
                        }
                        let mut row_chars: Vec<char> = lines[row].chars().collect();
                        let mut x = rect.x as usize;
                        for ch in frame_line.chars() {
                            if x >= row_chars.len() {
                                break;
                            }
                            row_chars[x] = ch;
                            x += 1;
                        }
                        lines[row] = row_chars.into_iter().collect();
                    }
                }
            }
        }
    }

    if let Some(rect) = rect {
        if let Some(image) = params.slide.image.as_ref() {
            if let Some(caption) = image.alt.as_ref() {
                if !caption.trim().is_empty() {
                    let caption_row = rect.y as usize + rect.height as usize;
                    if caption_row < lines.len() {
                        let cap = style_line(
                            centered_line(caption, rect.width as usize),
                            LineRole::ImageCaption,
                            params.ansi,
                            params.theme,
                        );
                        overlay_text_at(&mut lines[caption_row], rect.x as usize, &cap);
                    }
                }
            }
        }
    }

    let footer = build_footer(
        params.term_width as usize,
        params.slide_number + 1,
        params.total_slides,
        false,
        params.ansi,
        params.theme,
    );
    lines.push(footer);

    RenderOutput {
        lines,
        truncated: false,
    }
}

pub fn compute_native_image_rect(
    slide: &Slide,
    term_width: u16,
    term_height: u16,
    column_ratios: &[u16],
) -> Option<NativeImageRect> {
    if slide.image.is_none() || term_width == 0 || term_height <= 1 {
        return None;
    }
    let image = slide.image.as_ref()?;
    let body_h = term_height.saturating_sub(1);

    if slide.cover.is_some() {
        let cover = slide.cover.as_ref()?;
        let title_rows = bigtext::render_big_text(&cover.title, term_width as usize).len() as u16;
        let mut y = 1u16.saturating_add(title_rows);
        if cover.subtitle.is_some() {
            y = y.saturating_add(TITLE_CONTENT_GAP_ROWS as u16);
        }
        y = y.saturating_add(COVER_SUBTITLE_IMAGE_GAP_ROWS as u16);
        let y = y.min(body_h.saturating_sub(1));
        let author_rows = if slide
            .cover
            .as_ref()
            .and_then(|cover| cover.author.as_ref())
            .is_some()
        {
            2u16
        } else {
            0u16
        };
        let h = body_h
            .saturating_sub(y)
            .saturating_sub(1)
            .saturating_sub(author_rows)
            .max(1);
        return Some(NativeImageRect {
            x: 0,
            y,
            width: term_width,
            height: h,
        });
    }

    if slide_image_focus_mode(slide) {
        let title_rows = title_reserved_rows(slide, term_width) as u16;
        let image_y = title_rows.min(body_h.saturating_sub(1));
        let available_h = body_h.saturating_sub(image_y);
        let width = term_width.saturating_sub(2).max(8).min(term_width);
        let height = available_h.saturating_sub(1).max(6).min(available_h.max(1));
        let x = match image.halign {
            HorizontalAlign::Left => 0,
            HorizontalAlign::Center => term_width.saturating_sub(width) / 2,
            HorizontalAlign::Right => term_width.saturating_sub(width),
        };
        let effective_valign = if slide.title.is_none() && slide.blocks.is_empty() {
            VerticalAlign::Middle
        } else {
            image.valign
        };
        let y = image_y
            + match effective_valign {
                VerticalAlign::Top => 0,
                VerticalAlign::Middle => available_h.saturating_sub(height) / 2,
                VerticalAlign::Bottom => available_h.saturating_sub(height),
            };
        return Some(NativeImageRect {
            x,
            y,
            width,
            height,
        });
    }

    let ratios = effective_ratios(column_ratios, true);
    let widths = layout::compute_column_widths(term_width, &ratios);
    if widths.len() < 2 {
        let width = term_width.saturating_sub(4).max(8).min(term_width);
        let height = body_h.saturating_sub(2).max(6).min(body_h.max(1));
        let x = term_width.saturating_sub(width) / 2;
        let y = body_h.saturating_sub(height) / 2;
        return Some(NativeImageRect {
            x,
            y,
            width,
            height,
        });
    }

    let image_col = widths.len() - 1;
    let image_col_x = widths
        .iter()
        .take(image_col)
        .map(|w| *w as usize + 1)
        .sum::<usize>() as u16;
    let image_col_w = widths[image_col];
    let title_rows = title_reserved_rows(slide, term_width) as u16;
    let content_top = title_rows.min(body_h.saturating_sub(1));
    let content_h = body_h.saturating_sub(content_top);
    let width = ((image_col_w as f32) * 0.96).round() as u16;
    let width = width.max(8).min(image_col_w);
    let height = ((content_h as f32) * 0.9).round() as u16;
    let height = height.max(6).min(content_h.max(1));
    let x = image_col_x
        + match image.halign {
            HorizontalAlign::Left => 0,
            HorizontalAlign::Center => image_col_w.saturating_sub(width) / 2,
            HorizontalAlign::Right => image_col_w.saturating_sub(width),
        };
    let y = content_top
        + match image.valign {
            VerticalAlign::Top => 0,
            VerticalAlign::Middle => content_h.saturating_sub(height) / 2,
            VerticalAlign::Bottom => content_h.saturating_sub(height),
        };
    Some(NativeImageRect {
        x,
        y,
        width,
        height,
    })
}

fn effective_ratios(input: &[u16], has_image: bool) -> Vec<u16> {
    let mut ratios: Vec<u16> = input.iter().copied().filter(|v| *v > 0).collect();
    if ratios.is_empty() {
        return if has_image { vec![6, 4] } else { vec![1] };
    }
    if has_image && ratios.len() < 2 {
        ratios.push(4);
    }
    ratios
}

fn leading_title_rows(lines: &[RenderLine]) -> usize {
    let mut rows = 0usize;
    for line in lines {
        match line.role {
            LineRole::Title { .. } => rows += 1,
            _ => break,
        }
    }
    rows
}

fn overlay_text_at(target: &mut String, start_col: usize, text: &str) {
    let target_chars: Vec<char> = target.chars().collect();
    let target_len = target_chars.len();
    let prefix_end = start_col.min(target_len);
    let text_width = text::visible_width(text);
    let suffix_start = prefix_end.saturating_add(text_width).min(target_len);

    let prefix: String = target_chars[..prefix_end].iter().collect();
    let suffix: String = target_chars[suffix_start..].iter().collect();
    *target = format!("{prefix}{text}{suffix}");
}

fn render_body_tokens(
    slide: &Slide,
    width: usize,
    highlighter: &CodeHighlighter,
    ansi: bool,
    visible_blocks: Option<usize>,
    base_line_spacing: u8,
) -> Vec<BodyToken> {
    let mut tokens = Vec::new();
    let line_spacing = usize::from(slide.line_spacing.unwrap_or(base_line_spacing).max(1));
    let mut section_indent = 0usize;
    let max_blocks = visible_blocks
        .unwrap_or(slide.blocks.len())
        .min(slide.blocks.len());

    for block in slide.blocks.iter().take(max_blocks) {
        match block {
            Block::ColumnBreak(col) => {
                tokens.push(BodyToken::ColumnBreak(*col));
                continue;
            }
            Block::ColumnAlign(align) => {
                tokens.push(BodyToken::Align(*align));
                continue;
            }
            _ => {}
        }

        let indent = "  ".repeat(section_indent);
        let indent_width = indent.chars().count();
        let content_width = width.saturating_sub(indent_width).max(1);

        let (block_lines, role) = match block {
            Block::BigText(text) => {
                section_indent = 0;
                (
                    bigtext::render_big_text(text, width),
                    LineRole::Title { row: 0, total: 1 },
                )
            }
            Block::SectionHeading { level, text } => {
                section_indent = usize::from((*level).min(3));
                (
                    vec![format!("▌ {text}")],
                    LineRole::SectionHeading((*level).min(3)),
                )
            }
            Block::Paragraph(text) => (text::wrap_text(text, content_width), LineRole::Paragraph),
            Block::Spacer(n) => (vec![String::new(); *n], LineRole::Empty),
            Block::Quote(text) => (
                text::wrap_text(text, content_width.saturating_sub(2))
                    .into_iter()
                    .map(|line| format!("| {line}"))
                    .collect(),
                LineRole::Quote,
            ),
            Block::List(list) => (render_list(list, content_width), LineRole::Paragraph),
            Block::Callout(callout) => (
                render_callout(callout, content_width),
                LineRole::Callout(callout.kind),
            ),
            Block::Table(table_data) => (
                table::render_table(table_data, content_width),
                LineRole::Table,
            ),
            Block::Code(code_data) => (
                render_code_window(code_data, highlighter, content_width, ansi),
                LineRole::Code,
            ),
            Block::ColumnBreak(_) | Block::ColumnAlign(_) => unreachable!(),
        };

        if block_lines.is_empty() {
            tokens.push(BodyToken::Line(RenderLine {
                text: String::new(),
                role: LineRole::Empty,
            }));
            continue;
        }

        let total = block_lines.len();
        for (idx, line) in block_lines.into_iter().enumerate() {
            let row_role = match role {
                LineRole::Title { .. } => LineRole::Title {
                    row: idx,
                    total: total.max(1),
                },
                _ => role,
            };
            let line_text = match block {
                Block::BigText(_) | Block::SectionHeading { .. } | Block::Spacer(_) => line,
                _ if indent_width > 0 && !line.is_empty() => format!("{indent}{line}"),
                _ => line,
            };
            tokens.push(BodyToken::Line(RenderLine {
                text: line_text,
                role: row_role,
            }));
        }

        if !matches!(block, Block::Spacer(_)) {
            for _ in 0..line_spacing {
                tokens.push(BodyToken::Line(RenderLine {
                    text: String::new(),
                    role: LineRole::Empty,
                }));
            }
        }
    }

    while matches!(
        tokens.last(),
        Some(BodyToken::Line(RenderLine { text, .. })) if text.is_empty()
    ) {
        tokens.pop();
    }

    tokens
}

fn render_list(list: &crate::model::ListData, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for (idx, item) in list.items.iter().enumerate() {
        let prefix = if list.ordered {
            format!("{}. ", idx + 1)
        } else {
            "- ".to_string()
        };
        let wrapped = text::wrap_text(item, width.saturating_sub(prefix.len()));
        if wrapped.is_empty() {
            out.push(prefix.clone());
            continue;
        }
        for (line_idx, line) in wrapped.iter().enumerate() {
            if line_idx == 0 {
                out.push(format!("{prefix}{line}"));
            } else {
                out.push(format!("{}{line}", " ".repeat(prefix.len())));
            }
        }
    }
    out
}

fn render_callout(callout: &crate::model::CalloutData, width: usize) -> Vec<String> {
    let label = match callout.kind {
        CalloutKind::Note => "NOTE",
        CalloutKind::Tip => "TIP",
        CalloutKind::Warn => "WARN",
    };
    let line = format!("[{label}] {}", callout.text);
    text::wrap_text(&line, width)
}

fn render_code_window(
    code_data: &crate::model::CodeData,
    highlighter: &CodeHighlighter,
    width: usize,
    ansi: bool,
) -> Vec<String> {
    let window_min = 14usize;
    let chrome = 2usize; // box borders
    let content_padding = 2usize; // one left + one right
    let max_content = width.saturating_sub(chrome + content_padding).max(1);

    let raw_lines = highlighter
        .render_code(code_data, max_content, ansi)
        .into_iter()
        .collect::<Vec<_>>();
    let max_visible = raw_lines
        .iter()
        .map(|line| text::visible_width(line))
        .max()
        .unwrap_or(1)
        .min(max_content);

    let code_area = max_visible.max(window_min.saturating_sub(chrome + content_padding));
    let outer = code_area + chrome + content_padding;

    let mut out = Vec::new();
    out.push(format!("╭{}╮", "─".repeat(outer.saturating_sub(2))));

    for line in raw_lines {
        let visible = text::visible_width(&line).min(code_area);
        let pad = code_area.saturating_sub(visible);
        out.push(format!("│ {line}{} │", " ".repeat(pad)));
    }

    out.push(format!("╰{}╯", "─".repeat(outer.saturating_sub(2))));
    out
}

fn style_line(line: String, role: LineRole, ansi: bool, theme: &Theme) -> String {
    if !ansi || line.is_empty() {
        return line;
    }

    match role {
        LineRole::Title { row, total } => {
            let (r, g, b) = gradient_color(row, total, theme.title_start, theme.title_end);
            format!("{BOLD_ON}\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}{BOLD_OFF}")
        }
        LineRole::SectionHeading(level) => {
            let (br, bg, bb, fr, fg, fb) = match level {
                1 => (54, 22, 36, 255, 146, 125), // neon salmon
                2 => (58, 30, 22, 255, 174, 96),  // sunset orange
                _ => (50, 24, 46, 255, 124, 204), // magenta neon
            };
            format!("\x1b[48;2;{br};{bg};{bb}m\x1b[38;2;{fr};{fg};{fb}m{line}{RESET_COLOR}\x1b[49m")
        }
        LineRole::Paragraph => {
            let (r, g, b) = theme.text;
            format!("\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}")
        }
        LineRole::CoverSubtitle => {
            let (r, g, b) = theme.cover_subtitle;
            format!("\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}")
        }
        LineRole::CoverAuthor => {
            let (r, g, b) = theme.cover_author;
            format!("{BOLD_ON}\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}{BOLD_OFF}")
        }
        LineRole::ImageCaption => {
            let (r, g, b) = theme.image_caption;
            format!("\x1b[3m\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}\x1b[23m")
        }
        LineRole::Quote => {
            let (r, g, b) = theme.quote;
            format!("\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}")
        }
        LineRole::Table => {
            let (r, g, b) = theme.table;
            format!("\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}")
        }
        LineRole::Code => {
            let (r, g, b) = theme.code_bg;
            if line.starts_with('╭') || line.starts_with('│') || line.starts_with('╰') {
                format!("\x1b[48;2;{r};{g};{b}m{line}\x1b[49m")
            } else {
                line
            }
        }
        LineRole::Callout(kind) => {
            let (r, g, b) = match kind {
                CalloutKind::Note => theme.callout_note_bg,
                CalloutKind::Tip => theme.callout_tip_bg,
                CalloutKind::Warn => theme.callout_warn_bg,
            };
            format!("\x1b[48;2;{r};{g};{b}m{line}\x1b[49m")
        }
        LineRole::Empty => line,
    }
}

fn gradient_color(
    index: usize,
    total: usize,
    start: (u8, u8, u8),
    end: (u8, u8, u8),
) -> (u8, u8, u8) {
    if total <= 1 {
        return start;
    }
    let t = index as f32 / (total.saturating_sub(1) as f32);
    let lerp = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };
    (
        lerp(start.0, end.0),
        lerp(start.1, end.1),
        lerp(start.2, end.2),
    )
}

fn align_to_width(input: &str, width: usize, align: ColumnAlign) -> String {
    let clipped = text::clip_to_width(input, width);
    let visible = text::visible_width(&clipped);
    if visible >= width {
        return clipped;
    }
    let gap = width - visible;
    match align {
        ColumnAlign::Left => format!("{clipped}{}", " ".repeat(gap)),
        ColumnAlign::Center => {
            let left = gap / 2;
            let right = gap - left;
            format!("{}{}{}", " ".repeat(left), clipped, " ".repeat(right))
        }
        ColumnAlign::Right => format!("{}{}", " ".repeat(gap), clipped),
    }
}

fn build_footer(
    width: usize,
    current: usize,
    total: usize,
    truncated: bool,
    ansi: bool,
    theme: &Theme,
) -> String {
    if width == 0 {
        return String::new();
    }

    let counter = format!("{current}/{total}");
    if counter.chars().count() >= width {
        return text::clip_to_width(&counter, width);
    }

    let mut left = if truncated {
        "[truncated: adjust slide content]".to_string()
    } else {
        String::new()
    };

    let counter_width = counter.chars().count();
    if left.chars().count() + counter_width > width {
        left = text::clip_to_width(&left, width.saturating_sub(counter_width + 1));
    }

    let spaces = width.saturating_sub(left.chars().count() + counter_width);
    if ansi {
        let left_colored = if left.is_empty() {
            String::new()
        } else if truncated {
            let (r, g, b) = theme.footer_warn;
            format!("\x1b[38;2;{r};{g};{b}m{left}{RESET_COLOR}")
        } else {
            let (r, g, b) = theme.footer_muted;
            format!("\x1b[38;2;{r};{g};{b}m{left}{RESET_COLOR}")
        };
        let (r, g, b) = theme.footer_accent;
        let counter_colored =
            format!("{BOLD_ON}\x1b[38;2;{r};{g};{b}m{counter}{RESET_COLOR}{BOLD_OFF}");
        format!("{left_colored}{}{counter_colored}", " ".repeat(spaces))
    } else {
        format!("{left}{}{counter}", " ".repeat(spaces))
    }
}

fn slide_image_focus_mode(slide: &Slide) -> bool {
    if slide.image.is_none() {
        return false;
    }
    slide
        .blocks
        .iter()
        .all(|block| matches!(block, Block::BigText(_)))
}

fn title_reserved_rows(slide: &Slide, term_width: u16) -> usize {
    if let Some(title) = slide.title.as_ref() {
        return bigtext::render_big_text(title, term_width as usize)
            .len()
            .saturating_add(TITLE_CONTENT_GAP_ROWS);
    }
    let title_rows = slide
        .blocks
        .iter()
        .filter_map(|block| {
            if let Block::BigText(text) = block {
                Some(bigtext::render_big_text(text, term_width as usize).len())
            } else {
                None
            }
        })
        .sum::<usize>();
    if title_rows == 0 {
        0
    } else {
        title_rows.saturating_add(TITLE_CONTENT_GAP_ROWS)
    }
}

fn centered_line(input: &str, width: usize) -> String {
    let clipped = text::clip_to_width(input, width);
    let len = clipped.chars().count();
    if len >= width {
        return clipped;
    }
    let left = (width - len) / 2;
    let right = width - len - left;
    format!("{}{}{}", " ".repeat(left), clipped, " ".repeat(right))
}

fn is_single_bigtext_slide(slide: &Slide) -> bool {
    if slide.title.is_some() {
        return false;
    }
    slide.image.is_none()
        && slide.blocks.len() == 1
        && matches!(slide.blocks.first(), Some(Block::BigText(_)))
}

fn configured_or_leading_title_lines(
    slide: &Slide,
    text_lines: &[RenderLine],
    width: usize,
) -> (Vec<RenderLine>, usize, bool) {
    if let Some(config_title) = slide.title.as_ref() {
        let rows = bigtext::render_big_text(config_title, width);
        let total = rows.len().max(1);
        let out = rows
            .into_iter()
            .enumerate()
            .map(|(idx, text)| RenderLine {
                text,
                role: LineRole::Title { row: idx, total },
            })
            .collect::<Vec<_>>();
        let center = slide.image.is_none() && slide.blocks.is_empty();
        return (out, 0, center);
    }

    let title_rows = leading_title_rows(text_lines);
    let mut body_start = title_rows;
    while body_start < text_lines.len() && matches!(text_lines[body_start].role, LineRole::Empty) {
        body_start += 1;
    }
    let out = text_lines
        .iter()
        .take(title_rows)
        .cloned()
        .collect::<Vec<_>>();
    (out, body_start, is_single_bigtext_slide(slide))
}

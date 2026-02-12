pub mod bigtext;
pub mod code;
pub mod image_ascii;
pub mod layout;
pub mod table;
pub mod text;

use std::path::Path;
use std::time::Duration;

use crate::model::{Block, CalloutKind, HorizontalAlign, Slide, VerticalAlign};
use crate::render::code::CodeHighlighter;
use crate::render::image_ascii::{current_frame, ensure_ascii_frames};

const RESET_COLOR: &str = "\x1b[39m";
const BOLD_ON: &str = "\x1b[1m";
const BOLD_OFF: &str = "\x1b[22m";

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
    Paragraph,
    Quote,
    Table,
    Code,
    Callout(CalloutKind),
    Empty,
}

struct RenderLine {
    text: String,
    role: LineRole,
}

pub fn render_slide(params: RenderParams<'_>) -> RenderOutput {
    let body_height = params.term_height.saturating_sub(1) as usize;
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
    let text_lines = render_blocks(
        params.slide,
        text_width,
        params.highlighter,
        params.ansi,
        params.visible_blocks,
        params.line_spacing,
    );

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

    let mut lines = vec![" ".repeat(params.term_width as usize); body_height];
    let mut truncated = text_lines.len() > body_height;
    if !image_lines.is_empty() && image_lines.len() > body_height {
        truncated = true;
    }

    let title_prefix = leading_title_rows(&text_lines);
    for row in 0..title_prefix.min(body_height) {
        let (line, role) = text_lines
            .get(row)
            .map(|entry| (entry.text.as_str(), entry.role))
            .unwrap_or(("", LineRole::Empty));
        lines[row] = style_line(
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

    let mut text_cursor = title_prefix;
    let text_start_row = title_prefix.min(body_height);
    for col in 0..text_col_count {
        let col_x = x_positions[col];
        let col_w = widths[col] as usize;
        for row in text_start_row..body_height {
            if text_cursor >= text_lines.len() {
                break;
            }
            let entry = &text_lines[text_cursor];
            text_cursor += 1;
            let styled = if matches!(entry.role, LineRole::Code) {
                // Code blocks are already width-bounded in render_code_window. Re-clipping ANSI
                // content here can cut escape sequences and visibly truncate rows.
                style_line(entry.text.clone(), entry.role, params.ansi, params.theme)
            } else {
                let raw = text::clip_to_width(&entry.text, col_w);
                style_line(
                    text::pad_to_width(&raw, col_w),
                    entry.role,
                    params.ansi,
                    params.theme,
                )
            };
            overlay_text_at(&mut lines[row], col_x, &styled);
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
                        let cap = centered_line(caption, rect.width as usize);
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

fn render_image_focus_slide(params: RenderParams<'_>, body_height: usize) -> RenderOutput {
    let text_lines = render_blocks(
        params.slide,
        params.term_width as usize,
        params.highlighter,
        params.ansi,
        params.visible_blocks,
        params.line_spacing,
    );
    let title_rows = title_reserved_rows(params.slide).min(body_height);

    let mut lines = vec![" ".repeat(params.term_width as usize); body_height];
    for row in 0..title_rows {
        let (line, role) = text_lines
            .get(row)
            .map(|entry| (entry.text.as_str(), entry.role))
            .unwrap_or(("", LineRole::Empty));
        let centered = centered_line(line, params.term_width as usize);
        lines[row] = style_line(centered, role, params.ansi, params.theme);
    }

    if !params.prefer_real_images {
        let rect = compute_native_image_rect(
            params.slide,
            params.term_width,
            params.term_height,
            params.column_ratios,
        );
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
                if let Some(caption) = image.alt.as_ref() {
                    if !caption.trim().is_empty() {
                        let caption_row = rect.y as usize + rect.height as usize;
                        if caption_row < lines.len() {
                            let cap = centered_line(caption, rect.width as usize);
                            overlay_text_at(&mut lines[caption_row], rect.x as usize, &cap);
                        }
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

    if slide_image_focus_mode(slide) {
        let title_rows = title_reserved_rows(slide) as u16;
        let image_y = title_rows.min(body_h.saturating_sub(1));
        let available_h = body_h.saturating_sub(image_y);
        let width = ((term_width as f32) * 0.9).round() as u16;
        let width = width.max(8).min(term_width);
        let height = available_h.saturating_sub(2).max(6).min(available_h.max(1));
        let x = match image.halign {
            HorizontalAlign::Left => 0,
            HorizontalAlign::Center => term_width.saturating_sub(width) / 2,
            HorizontalAlign::Right => term_width.saturating_sub(width),
        };
        let y = image_y
            + match image.valign {
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
    let title_rows = title_reserved_rows(slide) as u16;
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
    let mut seen_title = false;
    for line in lines {
        match line.role {
            LineRole::Title { .. } => {
                seen_title = true;
                rows += 1;
            }
            LineRole::Empty if seen_title => rows += 1,
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

fn render_blocks(
    slide: &Slide,
    width: usize,
    highlighter: &CodeHighlighter,
    ansi: bool,
    visible_blocks: Option<usize>,
    base_line_spacing: u8,
) -> Vec<RenderLine> {
    let mut lines = Vec::new();
    let line_spacing = usize::from(slide.line_spacing.unwrap_or(base_line_spacing).max(1));
    let max_blocks = visible_blocks
        .unwrap_or(slide.blocks.len())
        .min(slide.blocks.len());

    for block in slide.blocks.iter().take(max_blocks) {
        let block_lines = match block {
            Block::BigText(text) => bigtext::render_big_text(text, width),
            Block::Paragraph(text) => text::wrap_text(text, width),
            Block::Quote(text) => text::wrap_text(text, width.saturating_sub(2))
                .into_iter()
                .map(|line| format!("| {line}"))
                .collect(),
            Block::List(list) => render_list(list, width),
            Block::Callout(callout) => render_callout(callout, width),
            Block::Table(table_data) => table::render_table(table_data, width),
            Block::Code(code_data) => render_code_window(code_data, highlighter, width, ansi),
        };

        if block_lines.is_empty() {
            lines.push(RenderLine {
                text: String::new(),
                role: LineRole::Empty,
            });
            continue;
        }

        let total = block_lines.len();
        for (idx, line) in block_lines.into_iter().enumerate() {
            let role = match block {
                Block::BigText(_) => LineRole::Title {
                    row: idx,
                    total: total.max(1),
                },
                Block::Paragraph(_) => LineRole::Paragraph,
                Block::Quote(_) => LineRole::Quote,
                Block::List(_) => LineRole::Paragraph,
                Block::Callout(callout) => LineRole::Callout(callout.kind),
                Block::Table(_) => LineRole::Table,
                Block::Code(_) => LineRole::Code,
            };
            lines.push(RenderLine { text: line, role });
        }

        for _ in 0..line_spacing {
            lines.push(RenderLine {
                text: String::new(),
                role: LineRole::Empty,
            });
        }
        if matches!(block, Block::BigText(_)) {
            lines.push(RenderLine {
                text: String::new(),
                role: LineRole::Empty,
            });
            lines.push(RenderLine {
                text: String::new(),
                role: LineRole::Empty,
            });
        }
    }

    while lines.last().is_some_and(|line| line.text.is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        lines.push(RenderLine {
            text: String::new(),
            role: LineRole::Empty,
        });
    }

    lines
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
        LineRole::Paragraph => {
            let (r, g, b) = theme.text;
            format!("\x1b[38;2;{r};{g};{b}m{line}{RESET_COLOR}")
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

fn title_reserved_rows(slide: &Slide) -> usize {
    let title_count = slide
        .blocks
        .iter()
        .filter(|block| matches!(block, Block::BigText(_)))
        .count();
    if title_count == 0 { 0 } else { title_count * 8 }
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

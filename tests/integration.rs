use std::path::{Path, PathBuf};
use std::time::Duration;

use rslides::app::AppState;
use rslides::model::{
    AsciiFrame, Block, CodeData, HorizontalAlign, ImageAsset, Presentation, Slide, VerticalAlign,
};
use rslides::parser::parse_presentation_from_str;
use rslides::render::code::CodeHighlighter;
use rslides::render::image_ascii::current_frame;
use rslides::render::{RenderParams, Theme, render_slide};

#[test]
fn parse_and_render_smoke_test() {
    let markdown = "# Title\n\nParagraph text here.\n---\n# Two\n\n```rust\nfn main() {}\n```\n";
    let mut presentation = parse_presentation_from_str(markdown, Path::new("deck.md"))
        .expect("presentation should parse");
    assert_eq!(presentation.slides.len(), 2);
    let total_slides = presentation.slides.len();

    let highlighter = CodeHighlighter::new();
    let frame = render_slide(RenderParams {
        slide: &mut presentation.slides[0],
        slide_number: 0,
        total_slides,
        term_width: 80,
        term_height: 24,
        ansi: false,
        fps: 8,
        slide_elapsed: Duration::ZERO,
        base_dir: Path::new("."),
        highlighter: &highlighter,
        prefer_real_images: false,
        visible_blocks: None,
        theme: &Theme::default(),
        line_spacing: 1,
        column_ratios: &[6, 4],
    });

    assert!(!frame.lines.is_empty());
}

#[test]
fn footer_counter_is_rendered() {
    let markdown = "# One\n---\n# Two\n---\n# Three\n";
    let mut presentation = parse_presentation_from_str(markdown, Path::new("deck.md"))
        .expect("presentation should parse");
    let highlighter = CodeHighlighter::new();

    let frame = render_slide(RenderParams {
        slide: &mut presentation.slides[1],
        slide_number: 1,
        total_slides: 3,
        term_width: 40,
        term_height: 10,
        ansi: false,
        fps: 8,
        slide_elapsed: Duration::ZERO,
        base_dir: Path::new("."),
        highlighter: &highlighter,
        prefer_real_images: false,
        visible_blocks: None,
        theme: &Theme::default(),
        line_spacing: 1,
        column_ratios: &[6, 4],
    });

    let footer = frame.lines.last().expect("footer must exist");
    assert!(footer.ends_with("2/3"));
}

#[test]
fn app_navigation_state_transitions() {
    let mut state = AppState::new(2, false, (80, 24), 8);
    let presentation = Presentation {
        slides: vec![
            Slide {
                blocks: vec![Block::Paragraph("a".to_string())],
                title: None,
                image: None,
                warnings: vec![],
                reveal_fragments: false,
                line_spacing: None,
                column_ratios: None,
                image_mode: None,
                cover: None,
            },
            Slide {
                blocks: vec![Block::Paragraph("b".to_string())],
                title: None,
                image: None,
                warnings: vec![],
                reveal_fragments: false,
                line_spacing: None,
                column_ratios: None,
                image_mode: None,
                cover: None,
            },
        ],
        source_path: PathBuf::from("deck.md"),
    };
    assert_eq!(state.slide_counter(), (1, 2));
    assert!(state.advance_next(&presentation));
    assert_eq!(state.slide_counter(), (2, 2));
    assert!(state.advance_prev(&presentation));
    assert_eq!(state.slide_counter(), (1, 2));
}

#[test]
fn gif_frame_clock_progression_works() {
    let frame_a = AsciiFrame {
        lines: vec!["A".to_string()],
        width: 1,
        height: 1,
    };
    let frame_b = AsciiFrame {
        lines: vec!["B".to_string()],
        width: 1,
        height: 1,
    };

    let image = ImageAsset {
        path: PathBuf::from("demo.gif"),
        alt: None,
        valign: VerticalAlign::Middle,
        halign: HorizontalAlign::Center,
        frames: vec![frame_a, frame_b],
        delays_ms: vec![100, 100],
        cached_for: Some((1, 1)),
        load_error: None,
    };

    let first = current_frame(&image, Duration::from_millis(50)).expect("frame expected");
    let second = current_frame(&image, Duration::from_millis(150)).expect("frame expected");

    assert_eq!(first.lines[0], "A");
    assert_eq!(second.lines[0], "B");
}

#[test]
fn non_image_slide_does_not_apply_multi_column_split() {
    let mut slide = Slide {
        blocks: vec![Block::Code(CodeData {
            lang: Some("rust".to_string()),
            source: "fn fade_steps(total_ms: u64) -> Duration {".to_string(),
        })],
        title: None,
        image: None,
        warnings: vec![],
        reveal_fragments: false,
        line_spacing: None,
        column_ratios: None,
        image_mode: None,
        cover: None,
    };

    let highlighter = CodeHighlighter::new();
    let frame = render_slide(RenderParams {
        slide: &mut slide,
        slide_number: 0,
        total_slides: 1,
        term_width: 80,
        term_height: 20,
        ansi: false,
        fps: 8,
        slide_elapsed: Duration::ZERO,
        base_dir: Path::new("."),
        highlighter: &highlighter,
        prefer_real_images: false,
        visible_blocks: None,
        theme: &Theme::default(),
        line_spacing: 1,
        column_ratios: &[2, 8],
    });

    let joined = frame.lines.join("\n");
    assert!(joined.contains("fn fade_steps(total_ms: u64) -> Duration {"));
}

#[test]
fn ansi_code_line_keeps_full_visible_text() {
    let mut slide = Slide {
        blocks: vec![Block::Code(CodeData {
            lang: Some("rust".to_string()),
            source: "fn fade_steps(total_ms: u64) -> Duration::from_millis(total_ms / 12)"
                .to_string(),
        })],
        title: None,
        image: None,
        warnings: vec![],
        reveal_fragments: false,
        line_spacing: None,
        column_ratios: None,
        image_mode: None,
        cover: None,
    };

    let highlighter = CodeHighlighter::new();
    let frame = render_slide(RenderParams {
        slide: &mut slide,
        slide_number: 0,
        total_slides: 1,
        term_width: 120,
        term_height: 20,
        ansi: true,
        fps: 8,
        slide_elapsed: Duration::ZERO,
        base_dir: Path::new("."),
        highlighter: &highlighter,
        prefer_real_images: false,
        visible_blocks: None,
        theme: &Theme::default(),
        line_spacing: 1,
        column_ratios: &[2, 8],
    });

    let stripped = strip_ansi(&frame.lines.join("\n"));
    assert!(stripped.contains("fade_steps(total_ms: u64)"));
    assert!(stripped.contains("from_millis(total_ms / 12)"));
}

#[test]
fn bigtext_only_slide_is_vertically_centered() {
    let markdown = "# Center me\n";
    let mut presentation = parse_presentation_from_str(markdown, Path::new("deck.md"))
        .expect("presentation should parse");
    let highlighter = CodeHighlighter::new();

    let frame = render_slide(RenderParams {
        slide: &mut presentation.slides[0],
        slide_number: 0,
        total_slides: 1,
        term_width: 80,
        term_height: 24,
        ansi: false,
        fps: 8,
        slide_elapsed: Duration::ZERO,
        base_dir: Path::new("."),
        highlighter: &highlighter,
        prefer_real_images: false,
        visible_blocks: None,
        theme: &Theme::default(),
        line_spacing: 1,
        column_ratios: &[6, 4],
    });

    let body = &frame.lines[..frame.lines.len().saturating_sub(1)];
    let first_non_empty = body
        .iter()
        .position(|line| !line.trim().is_empty())
        .expect("bigtext should render");
    assert!(first_non_empty > 0, "bigtext should not render at top row");
}

fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut out = String::with_capacity(input.len());
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if (b as char).is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        let ch = input[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

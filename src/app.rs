use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use std::{env, fs};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use image::image_dimensions;
use resvg::{self, tiny_skia, usvg};

use crate::model::{HorizontalAlign, ImageMode, Presentation, VerticalAlign};
use crate::render::code::CodeHighlighter;
use crate::render::{RenderParams, Theme, compute_native_image_rect, render_slide};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub no_ansi: bool,
    pub fps: u16,
    pub theme: Theme,
    pub line_spacing: u8,
    pub image_mode: ImageMode,
    pub gif_mode: ImageMode,
}

#[derive(Debug)]
pub struct AppState {
    pub current_slide: usize,
    pub total_slides: usize,
    pub terminal_size: (u16, u16),
    pub no_ansi: bool,
    pub fps: u16,
    slide_started: Instant,
    transition_started: Option<Instant>,
    transition_duration: Duration,
    transition_steps: usize,
    anim_elapsed: Duration,
    anim_last_tick: Instant,
    revealed_blocks: usize,
}

impl AppState {
    pub fn new(total_slides: usize, no_ansi: bool, terminal_size: (u16, u16), fps: u16) -> Self {
        Self {
            current_slide: 0,
            total_slides,
            terminal_size,
            no_ansi,
            fps,
            slide_started: Instant::now(),
            transition_started: Some(Instant::now()),
            transition_duration: Duration::from_millis(260),
            transition_steps: 12,
            anim_elapsed: Duration::ZERO,
            anim_last_tick: Instant::now(),
            revealed_blocks: 1,
        }
    }

    pub fn set_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_size = (width, height);
    }

    pub fn slide_elapsed(&self) -> Duration {
        self.anim_elapsed
    }

    pub fn slide_counter(&self) -> (usize, usize) {
        (self.current_slide + 1, self.total_slides)
    }

    pub fn transition_step(&mut self) -> Option<usize> {
        let Some(start) = self.transition_started else {
            return None;
        };

        let elapsed = start.elapsed();
        if elapsed >= self.transition_duration {
            self.transition_started = None;
            return None;
        }

        let ratio = elapsed.as_secs_f32() / self.transition_duration.as_secs_f32();
        let step = (ratio * self.transition_steps as f32).ceil() as usize;
        Some(step.clamp(1, self.transition_steps))
    }

    pub fn is_transition_active(&self) -> bool {
        self.transition_started
            .is_some_and(|start| start.elapsed() < self.transition_duration)
    }

    pub fn tick_animation(&mut self) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.anim_last_tick);
        self.anim_last_tick = now;
        self.anim_elapsed = self.anim_elapsed.saturating_add(delta);
    }

    fn reset_slide_timers(&mut self) {
        self.slide_started = Instant::now();
        self.transition_started = Some(Instant::now());
        self.anim_elapsed = Duration::ZERO;
        self.anim_last_tick = Instant::now();
    }

    fn reset_reveal_for_slide(&mut self, presentation: &Presentation, full: bool) {
        if let Some(slide) = presentation.slides.get(self.current_slide) {
            if slide.reveal_fragments {
                self.revealed_blocks = if full { slide.blocks.len().max(1) } else { 1 };
            } else {
                self.revealed_blocks = slide.blocks.len().max(1);
            }
        }
    }

    pub fn visible_blocks_for_current(&self, presentation: &Presentation) -> Option<usize> {
        presentation
            .slides
            .get(self.current_slide)
            .and_then(|slide| {
                if slide.reveal_fragments {
                    Some(self.revealed_blocks.min(slide.blocks.len().max(1)))
                } else {
                    None
                }
            })
    }

    pub fn advance_next(&mut self, presentation: &Presentation) -> bool {
        if let Some(slide) = presentation.slides.get(self.current_slide) {
            if slide.reveal_fragments && self.revealed_blocks < slide.blocks.len().max(1) {
                self.revealed_blocks += 1;
                return true;
            }
        }

        if self.current_slide + 1 >= self.total_slides {
            return false;
        }

        self.current_slide += 1;
        self.reset_slide_timers();
        self.reset_reveal_for_slide(presentation, false);
        true
    }

    pub fn advance_prev(&mut self, presentation: &Presentation) -> bool {
        if let Some(slide) = presentation.slides.get(self.current_slide) {
            if slide.reveal_fragments && self.revealed_blocks > 1 {
                self.revealed_blocks -= 1;
                return true;
            }
        }

        if self.current_slide == 0 {
            return false;
        }

        self.current_slide -= 1;
        self.reset_slide_timers();
        self.reset_reveal_for_slide(presentation, true);
        true
    }
}

pub fn run_presentation(mut presentation: Presentation, config: AppConfig) -> Result<(), String> {
    if presentation.slides.is_empty() {
        return Err("presentation has no slides".to_string());
    }

    for (idx, slide) in presentation.slides.iter().enumerate() {
        for warning in &slide.warnings {
            eprintln!("warning (slide {}): {warning}", idx + 1);
        }
    }

    let ansi_mode = !config.no_ansi && io::stdout().is_terminal() && io::stdin().is_terminal();
    if ansi_mode {
        run_tui_mode(&mut presentation, config)
    } else {
        run_plain_mode(&mut presentation, config)
    }
}

fn run_tui_mode(presentation: &mut Presentation, config: AppConfig) -> Result<(), String> {
    let mut stdout = io::stdout();
    let _guard =
        TerminalGuard::enter(&mut stdout).map_err(|err| format!("terminal init failed: {err}"))?;
    execute!(
        stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(ClearType::All)
    )
    .map_err(|err| format!("terminal clear failed: {err}"))?;

    let size = terminal::size().map_err(|err| format!("failed to read terminal size: {err}"))?;
    let mut state = AppState::new(presentation.slides.len(), false, size, config.fps);
    state.reset_reveal_for_slide(presentation, false);

    let highlighter = CodeHighlighter::new();
    let mut previous_lines: Vec<String> = Vec::new();
    let image_backend = detect_image_backend();
    let mut last_native_image_signature: Option<String> = None;
    let mut native_image_active = false;

    let source_dir = presentation
        .source_path
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let mut dirty = true;

    loop {
        state.tick_animation();
        let geometry = compute_canvas_geometry(state.terminal_size.0, state.terminal_size.1);

        let current_image_path = presentation
            .slides
            .get(state.current_slide)
            .and_then(|slide| slide.image.as_ref())
            .map(|image| image.path.clone());
        let current_columns = presentation
            .slides
            .get(state.current_slide)
            .and_then(|slide| slide.column_ratios.clone())
            .unwrap_or_else(|| vec![6, 4]);
        let slide_image_mode = presentation
            .slides
            .get(state.current_slide)
            .and_then(|slide| slide.image_mode);
        let slide_has_image = current_image_path.is_some();
        let media_kind = current_image_path
            .as_ref()
            .map(|path| media_kind_for_path(path))
            .unwrap_or(MediaKind::StaticImage);
        let effective_image_mode = slide_image_mode.unwrap_or(config.image_mode);
        let effective_gif_mode = slide_image_mode.unwrap_or(config.gif_mode);
        let prefer_real_images = slide_has_image
            && image_backend.is_native()
            && match media_kind {
                MediaKind::StaticImage => should_use_native(effective_image_mode),
                MediaKind::Gif => should_use_native(effective_gif_mode),
                MediaKind::Svg => should_use_native(effective_image_mode),
                MediaKind::Video => false,
            };
        let animated_ascii = !prefer_real_images
            && presentation
                .slides
                .get(state.current_slide)
                .and_then(|slide| slide.image.as_ref())
                .is_some_and(|image| image.frames.len() > 1);
        let transition_active = state.is_transition_active() && !prefer_real_images;

        if !prefer_real_images && native_image_active {
            clear_native_images(&mut stdout, &image_backend);
            native_image_active = false;
            last_native_image_signature = None;
            previous_lines.clear();
            dirty = true;
        }

        if dirty || transition_active || animated_ascii {
            let visible_blocks = state.visible_blocks_for_current(presentation);
            let rendered = {
                let slide = &mut presentation.slides[state.current_slide];
                render_slide(RenderParams {
                    slide,
                    slide_number: state.current_slide,
                    total_slides: state.total_slides,
                    term_width: geometry.width,
                    term_height: geometry.height,
                    ansi: true,
                    fps: state.fps,
                    slide_elapsed: state.slide_elapsed(),
                    base_dir: source_dir,
                    highlighter: &highlighter,
                    prefer_real_images,
                    visible_blocks,
                    theme: &config.theme,
                    line_spacing: config.line_spacing,
                    column_ratios: &current_columns,
                })
            };

            let mut lines = rendered.lines;
            if transition_active {
                if let Some(step) = state.transition_step() {
                    lines = apply_fade(&lines, step, state.transition_steps, true);
                }
            }

            write_ansi_frame(
                &mut stdout,
                &lines,
                &mut previous_lines,
                geometry.origin_x,
                geometry.origin_y,
            )
            .map_err(|err| format!("failed to draw frame: {err}"))?;

            if prefer_real_images {
                let signature = format!(
                    "{}:{}:{}:{}:{}",
                    state.current_slide,
                    geometry.width,
                    geometry.height,
                    geometry.origin_x,
                    visible_blocks.unwrap_or(usize::MAX)
                );
                if dirty || last_native_image_signature.as_deref() != Some(&signature) {
                    if native_image_active {
                        clear_native_images(&mut stdout, &image_backend);
                    }
                    if let Some(slide) = presentation.slides.get(state.current_slide) {
                        if let Some(image) = &slide.image {
                            if let Some(rect) = compute_native_image_rect(
                                slide,
                                geometry.width,
                                geometry.height,
                                &current_columns,
                            ) {
                                let image_path = resolve_image_path(source_dir, &image.path);
                                let native_path = if media_kind == MediaKind::Svg {
                                    prepare_native_svg_png(
                                        &image_path,
                                        rect.width.max(1),
                                        rect.height.max(1),
                                    )
                                    .unwrap_or_else(|| image_path.clone())
                                } else {
                                    image_path.clone()
                                };
                                let (fit_x, fit_y, fit_w, fit_h) = fit_native_rect(
                                    &native_path,
                                    rect.x,
                                    rect.y,
                                    rect.width,
                                    rect.height,
                                    image.halign,
                                    if slide.title.is_none() && slide.blocks.is_empty() {
                                        VerticalAlign::Middle
                                    } else {
                                        image.valign
                                    },
                                );
                                let abs_x = geometry.origin_x + fit_x;
                                let abs_y = geometry.origin_y + fit_y;
                                draw_native_image(
                                    &mut stdout,
                                    &image_backend,
                                    &native_path,
                                    abs_x,
                                    abs_y,
                                    fit_w,
                                    fit_h,
                                );
                                native_image_active = true;
                            } else if native_image_active {
                                clear_native_images(&mut stdout, &image_backend);
                                native_image_active = false;
                            }
                        } else if native_image_active {
                            clear_native_images(&mut stdout, &image_backend);
                            native_image_active = false;
                        }
                    }
                    last_native_image_signature = Some(signature);
                }
            } else {
                last_native_image_signature = None;
            }

            dirty = false;
        }

        let timeout = if transition_active || animated_ascii {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(120)
        };

        if event::poll(timeout).map_err(|err| format!("event poll failed: {err}"))? {
            match event::read().map_err(|err| format!("event read failed: {err}"))? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Right | KeyCode::Down => {
                        if state.advance_next(presentation) {
                            if native_image_active {
                                clear_native_images(&mut stdout, &image_backend);
                                native_image_active = false;
                            }
                            last_native_image_signature = None;
                            previous_lines.clear();
                            dirty = true;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Left | KeyCode::Up => {
                        if state.advance_prev(presentation) {
                            if native_image_active {
                                clear_native_images(&mut stdout, &image_backend);
                                native_image_active = false;
                            }
                            last_native_image_signature = None;
                            previous_lines.clear();
                            dirty = true;
                        }
                    }
                    _ => {}
                },
                Event::Resize(width, height) => {
                    state.set_terminal_size(width, height);
                    if native_image_active {
                        clear_native_images(&mut stdout, &image_backend);
                        native_image_active = false;
                    }
                    previous_lines.clear();
                    last_native_image_signature = None;
                    dirty = true;
                }
                _ => {}
            }
        } else if transition_active || animated_ascii {
            dirty = true;
        }
    }

    Ok(())
}

fn resolve_image_path(base_dir: &Path, image_path: &Path) -> std::path::PathBuf {
    if image_path.is_absolute() {
        image_path.to_path_buf()
    } else {
        base_dir.join(image_path)
    }
}

#[derive(Clone, Copy)]
enum ImageBackend {
    Ascii,
    KittyGraphics,
    ItermInline,
}

impl ImageBackend {
    fn is_native(self) -> bool {
        matches!(self, Self::KittyGraphics | Self::ItermInline)
    }
}

fn detect_image_backend() -> ImageBackend {
    let term = env::var("TERM").unwrap_or_default();
    let term_program = env::var("TERM_PROGRAM").unwrap_or_default();

    if term_program == "iTerm.app" || env::var("ITERM_SESSION_ID").is_ok() {
        return ImageBackend::ItermInline;
    }

    if term.contains("kitty") || term_program == "ghostty" || env::var("KITTY_WINDOW_ID").is_ok() {
        return ImageBackend::KittyGraphics;
    }

    ImageBackend::Ascii
}

fn draw_native_image(
    stdout: &mut io::Stdout,
    backend: &ImageBackend,
    path: &Path,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
) {
    match backend {
        ImageBackend::KittyGraphics => {
            let encoded_path = base64_encode(path.to_string_lossy().as_bytes());
            let mut handle = stdout.lock();
            let _ = write!(
                handle,
                "\x1b[{};{}H\x1b_Ga=T,t=f,f=100,c={},r={};{}\x1b\\",
                y as usize + 1,
                x as usize + 1,
                width,
                height,
                encoded_path
            );
            let _ = handle.flush();
        }
        ImageBackend::ItermInline => {
            let Some(encoded_bytes) = encoded_image_bytes(path) else {
                return;
            };
            let mut handle = stdout.lock();
            let _ = write!(
                handle,
                "\x1b[{};{}H\x1b]1337;File=inline=1;width={};height={};preserveAspectRatio=1:{}\x07",
                y as usize + 1,
                x as usize + 1,
                width,
                height,
                encoded_bytes
            );
            let _ = handle.flush();
        }
        ImageBackend::Ascii => {}
    }
}

fn encoded_image_bytes(path: &Path) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some(value) = guard.get(path) {
            return Some(value.clone());
        }
    }

    let bytes = fs::read(path).ok()?;
    let encoded = base64_encode(&bytes);

    if let Ok(mut guard) = cache.lock() {
        guard.insert(path.to_path_buf(), encoded.clone());
    }

    Some(encoded)
}

fn fit_native_rect(
    path: &Path,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    halign: HorizontalAlign,
    valign: VerticalAlign,
) -> (u16, u16, u16, u16) {
    // Terminal cells are typically taller than they are wide. Adjusting for this avoids
    // overly narrow native images when fitting by aspect ratio.
    const CELL_W_OVER_H: f32 = 0.5;

    if width == 0 || height == 0 {
        return (x, y, width, height);
    }

    let Ok((src_w, src_h)) = image_dimensions(path) else {
        return (x, y, width, height);
    };
    if src_w == 0 || src_h == 0 {
        return (x, y, width, height);
    }

    let target_ratio_cells = width as f32 / height as f32;
    let source_ratio_cells = (src_w as f32 / src_h as f32) / CELL_W_OVER_H;

    let (fit_w, fit_h) = if source_ratio_cells >= target_ratio_cells {
        let w = width;
        let h = ((w as f32 / source_ratio_cells).round() as u16)
            .max(1)
            .min(height);
        (w, h)
    } else {
        let h = height;
        let w = ((h as f32 * source_ratio_cells).round() as u16)
            .max(1)
            .min(width);
        (w, h)
    };

    let dx = match halign {
        HorizontalAlign::Left => 0,
        HorizontalAlign::Center => width.saturating_sub(fit_w) / 2,
        HorizontalAlign::Right => width.saturating_sub(fit_w),
    };
    let dy = match valign {
        VerticalAlign::Top => 0,
        VerticalAlign::Middle => height.saturating_sub(fit_h) / 2,
        VerticalAlign::Bottom => height.saturating_sub(fit_h),
    };

    (x + dx, y + dy, fit_w, fit_h)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    StaticImage,
    Gif,
    Svg,
    Video,
}

fn media_kind_for_path(path: &Path) -> MediaKind {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "gif" => MediaKind::Gif,
        "svg" => MediaKind::Svg,
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" => MediaKind::Video,
        _ => MediaKind::StaticImage,
    }
}

fn should_use_native(mode: ImageMode) -> bool {
    matches!(mode, ImageMode::Native | ImageMode::Auto)
}

fn prepare_native_svg_png(
    path: &Path,
    target_cols: u16,
    target_rows: u16,
) -> Option<std::path::PathBuf> {
    let data = fs::read(path).ok()?;
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&data, &options).ok()?;
    let size = tree.size();
    let intrinsic_w = size.width().round().max(1.0) as u32;
    let intrinsic_h = size.height().round().max(1.0) as u32;

    // Render SVG at a higher resolution than terminal-cell size to keep edges sharp
    // when the terminal backend scales it into cell bounds.
    let target_w = u32::from(target_cols).saturating_mul(16);
    let target_h = u32::from(target_rows).saturating_mul(32);
    let width = intrinsic_w.max(target_w).clamp(1, 8192);
    let height = intrinsic_h.max(target_h).clamp(1, 8192);

    let mut key = path.to_string_lossy().to_string();
    key.push(':');
    key.push_str(&format!("{width}x{height}"));
    let hash = fnv1a_hash64(key.as_bytes());
    let out = env::temp_dir().join(format!("rslides-svg-{hash:016x}.png"));
    if out.exists() {
        return Some(out);
    }

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    let scale = sx.min(sy);
    let draw_w = size.width() * scale;
    let draw_h = size.height() * scale;
    let tx = (width as f32 - draw_w) * 0.5;
    let ty = (height as f32 - draw_h) * 0.5;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.save_png(&out).ok()?;
    Some(out)
}

fn fnv1a_hash64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn clear_native_images(stdout: &mut io::Stdout, backend: &ImageBackend) {
    match backend {
        ImageBackend::KittyGraphics => {
            let mut handle = stdout.lock();
            let _ = write!(handle, "\x1b_Ga=d,d=A\x1b\\");
            let _ = handle.flush();
        }
        ImageBackend::ItermInline => {
            let _ = execute!(
                stdout,
                cursor::MoveTo(0, 0),
                terminal::Clear(ClearType::All)
            );
        }
        ImageBackend::Ascii => {}
    }
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);

    let mut i = 0usize;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | input[i + 2] as u32;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
        i += 3;
    }

    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }

    out
}

fn run_plain_mode(presentation: &mut Presentation, config: AppConfig) -> Result<(), String> {
    let mut state = AppState::new(presentation.slides.len(), true, (100, 40), config.fps);
    state.reset_reveal_for_slide(presentation, false);

    let highlighter = CodeHighlighter::new();
    let source_dir = presentation
        .source_path
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let stdin = io::stdin();
    loop {
        let visible_blocks = state.visible_blocks_for_current(presentation);
        let current_columns = presentation
            .slides
            .get(state.current_slide)
            .and_then(|slide| slide.column_ratios.clone())
            .unwrap_or_else(|| vec![6, 4]);
        let rendered = {
            let slide = &mut presentation.slides[state.current_slide];
            render_slide(RenderParams {
                slide,
                slide_number: state.current_slide,
                total_slides: state.total_slides,
                term_width: state.terminal_size.0,
                term_height: state.terminal_size.1,
                ansi: false,
                fps: state.fps,
                slide_elapsed: state.slide_elapsed(),
                base_dir: source_dir,
                highlighter: &highlighter,
                prefer_real_images: false,
                visible_blocks,
                theme: &config.theme,
                line_spacing: config.line_spacing,
                column_ratios: &current_columns,
            })
        };

        println!(
            "\n=== Slide {}/{} ===",
            state.current_slide + 1,
            state.total_slides
        );
        for line in rendered.lines {
            println!("{line}");
        }

        print!("[n]ext [p]rev [q]uit > ");
        io::stdout()
            .flush()
            .map_err(|err| format!("flush failed: {err}"))?;

        let mut input = String::new();
        stdin
            .read_line(&mut input)
            .map_err(|err| format!("stdin read failed: {err}"))?;

        match input.trim().chars().next() {
            Some('q') | Some('Q') => break,
            Some('n') | Some('N') | Some('l') | Some('L') => {
                state.advance_next(presentation);
            }
            Some('p') | Some('P') | Some('k') | Some('K') => {
                state.advance_prev(presentation);
            }
            _ => {}
        }
    }

    Ok(())
}

fn write_ansi_frame(
    stdout: &mut io::Stdout,
    lines: &[String],
    previous_lines: &mut Vec<String>,
    origin_x: u16,
    origin_y: u16,
) -> io::Result<()> {
    let full_redraw = previous_lines.is_empty();
    let total_rows = if full_redraw {
        lines.len()
    } else {
        previous_lines.len().max(lines.len())
    };
    let mut handle = stdout.lock();
    for row in 0..total_rows {
        let next = lines.get(row).map(String::as_str).unwrap_or("");
        let prev = previous_lines.get(row).map(String::as_str).unwrap_or("");
        if full_redraw || next != prev {
            write!(
                handle,
                "\x1b[{};{}H\x1b[2K{next}",
                origin_y as usize + row + 1,
                origin_x as usize + 1
            )?;
        }
    }
    handle.flush()?;

    previous_lines.clear();
    previous_lines.extend_from_slice(lines);
    Ok(())
}

fn apply_fade(lines: &[String], step: usize, total_steps: usize, ansi: bool) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    let total_steps = total_steps.max(1);
    let step = step.clamp(1, total_steps);
    let progress = step as f32 / total_steps as f32;
    let row_count = lines.len().max(1);

    lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let row_delay = (idx as f32 / row_count as f32) * 0.2;
            let local_progress = ((progress - row_delay) / (1.0 - row_delay)).clamp(0.0, 1.0);

            if ansi {
                if local_progress <= 0.0 {
                    return String::new();
                }

                let has_ansi_codes = line.as_bytes().contains(&0x1b);
                if has_ansi_codes {
                    if local_progress < 0.55 {
                        String::new()
                    } else if local_progress < 0.9 {
                        format!("\x1b[2m{line}\x1b[22m")
                    } else {
                        line.clone()
                    }
                } else {
                    let visible_chars =
                        ((line.chars().count() as f32) * local_progress).ceil() as usize;
                    let partial: String = line.chars().take(visible_chars).collect();
                    if local_progress < 0.9 {
                        format!("\x1b[2m{partial}\x1b[22m")
                    } else {
                        partial
                    }
                }
            } else {
                let visible_chars =
                    ((line.chars().count() as f32) * local_progress).ceil() as usize;
                line.chars().take(visible_chars).collect()
            }
        })
        .collect()
}

struct TerminalGuard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanvasGeometry {
    origin_x: u16,
    origin_y: u16,
    width: u16,
    height: u16,
}

fn compute_canvas_geometry(term_width: u16, term_height: u16) -> CanvasGeometry {
    const H_PADDING_CELLS: u16 = 6; // wider side gutters.
    const V_PADDING_CELLS: u16 = 2; // shorter top/bottom gutters.
    let horizontal_padding = (H_PADDING_CELLS * 2).min(term_width.saturating_sub(1));
    let vertical_padding = (V_PADDING_CELLS * 2).min(term_height.saturating_sub(1));
    let width = term_width.saturating_sub(horizontal_padding).max(1);
    let height = term_height.saturating_sub(vertical_padding).max(1);
    CanvasGeometry {
        origin_x: H_PADDING_CELLS.min(term_width.saturating_sub(1)),
        origin_y: V_PADDING_CELLS.min(term_height.saturating_sub(1)),
        width,
        height,
    }
}

impl TerminalGuard {
    fn enter(stdout: &mut io::Stdout) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::model::{Presentation, Slide};

    #[test]
    fn app_state_navigation_respects_bounds() {
        let mut state = AppState::new(3, false, (80, 24), 8);
        let presentation = Presentation {
            slides: vec![
                Slide {
                    blocks: vec![],
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
                    blocks: vec![],
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
                    blocks: vec![],
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
            source_path: std::path::PathBuf::new(),
        };
        assert_eq!(state.slide_counter(), (1, 3));
        assert!(state.advance_next(&presentation));
        assert_eq!(state.slide_counter(), (2, 3));
        assert!(state.advance_prev(&presentation));
        assert_eq!(state.slide_counter(), (1, 3));
    }

    #[test]
    fn classifies_media_kinds() {
        assert_eq!(
            super::media_kind_for_path(std::path::Path::new("demo.gif")),
            super::MediaKind::Gif
        );
        assert_eq!(
            super::media_kind_for_path(std::path::Path::new("logo.svg")),
            super::MediaKind::Svg
        );
        assert_eq!(
            super::media_kind_for_path(std::path::Path::new("clip.mov")),
            super::MediaKind::Video
        );
        assert_eq!(
            super::media_kind_for_path(std::path::Path::new("photo.png")),
            super::MediaKind::StaticImage
        );
    }
}

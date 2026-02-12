use std::env;
use std::path::PathBuf;

use rslides::app::{AppConfig, run_presentation};
use rslides::model::ImageMode;
use rslides::parser::parse_presentation;
use rslides::render::Theme;

#[derive(Debug)]
struct Cli {
    markdown_path: PathBuf,
    no_ansi: bool,
    fps: u16,
    theme: Theme,
    line_spacing: u8,
    image_mode: ImageMode,
    gif_mode: ImageMode,
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(AppError { code, message }) => {
            eprintln!("{message}");
            code
        }
    };

    std::process::exit(code);
}

fn run() -> Result<(), AppError> {
    let cli = parse_args(env::args().skip(1))?;
    let presentation = parse_presentation(&cli.markdown_path).map_err(AppError::input)?;

    run_presentation(
        presentation,
        AppConfig {
            no_ansi: cli.no_ansi,
            fps: cli.fps,
            theme: cli.theme,
            line_spacing: cli.line_spacing,
            image_mode: cli.image_mode,
            gif_mode: cli.gif_mode,
        },
    )
    .map_err(AppError::runtime)
}

fn parse_args<I>(args: I) -> Result<Cli, AppError>
where
    I: Iterator<Item = String>,
{
    let mut no_ansi = false;
    let mut fps = 8u16;
    let mut theme = Theme::default();
    let mut line_spacing = 1u8;
    let mut image_mode = ImageMode::Native;
    let mut gif_mode = ImageMode::Native;
    let mut markdown_path: Option<PathBuf> = None;

    let mut pending = args.peekable();
    while let Some(arg) = pending.next() {
        if arg == "--no-ansi" {
            no_ansi = true;
            continue;
        }

        if arg == "--fps" {
            let Some(value) = pending.next() else {
                return Err(AppError::input("missing value for --fps"));
            };
            fps = parse_fps(&value)?;
            continue;
        }

        if let Some(value) = arg.strip_prefix("--fps=") {
            fps = parse_fps(value)?;
            continue;
        }

        if arg == "--theme" {
            let Some(value) = pending.next() else {
                return Err(AppError::input("missing value for --theme"));
            };
            theme = Theme::from_file(&PathBuf::from(value)).map_err(AppError::input)?;
            continue;
        }

        if let Some(value) = arg.strip_prefix("--theme=") {
            theme = Theme::from_file(&PathBuf::from(value)).map_err(AppError::input)?;
            continue;
        }

        if arg == "--line-spacing" {
            let Some(value) = pending.next() else {
                return Err(AppError::input("missing value for --line-spacing"));
            };
            line_spacing = parse_line_spacing(&value)?;
            continue;
        }

        if let Some(value) = arg.strip_prefix("--line-spacing=") {
            line_spacing = parse_line_spacing(value)?;
            continue;
        }

        if arg == "--image-mode" {
            let Some(value) = pending.next() else {
                return Err(AppError::input("missing value for --image-mode"));
            };
            image_mode = parse_render_mode(&value)?;
            continue;
        }

        if let Some(value) = arg.strip_prefix("--image-mode=") {
            image_mode = parse_render_mode(value)?;
            continue;
        }

        if arg == "--gif-mode" {
            let Some(value) = pending.next() else {
                return Err(AppError::input("missing value for --gif-mode"));
            };
            gif_mode = parse_render_mode(&value)?;
            continue;
        }

        if let Some(value) = arg.strip_prefix("--gif-mode=") {
            gif_mode = parse_render_mode(value)?;
            continue;
        }

        if arg.starts_with('-') {
            return Err(AppError::input(format!("unknown flag: {arg}")));
        }

        if markdown_path.is_some() {
            return Err(AppError::input("only one markdown file can be provided"));
        }
        markdown_path = Some(PathBuf::from(arg));
    }

    let Some(markdown_path) = markdown_path else {
        return Err(AppError::input(
            "usage: rslides [--no-ansi] [--fps <n>] [--image-mode auto|ascii|native] [--gif-mode auto|ascii|native] <file.md>",
        ));
    };

    Ok(Cli {
        markdown_path,
        no_ansi,
        fps,
        theme,
        line_spacing,
        image_mode,
        gif_mode,
    })
}

fn parse_fps(value: &str) -> Result<u16, AppError> {
    let fps = value
        .parse::<u16>()
        .map_err(|_| AppError::input(format!("invalid fps value: {value}")))?;
    if fps == 0 {
        return Err(AppError::input("fps must be greater than zero"));
    }
    Ok(fps)
}

fn parse_line_spacing(value: &str) -> Result<u8, AppError> {
    let spacing = value
        .parse::<u8>()
        .map_err(|_| AppError::input(format!("invalid line spacing: {value}")))?;
    if spacing == 0 || spacing > 6 {
        return Err(AppError::input("line spacing must be between 1 and 6"));
    }
    Ok(spacing)
}

fn parse_render_mode(value: &str) -> Result<ImageMode, AppError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(ImageMode::Auto),
        "ascii" => Ok(ImageMode::Ascii),
        "native" => Ok(ImageMode::Native),
        _ => Err(AppError::input(format!(
            "invalid render mode: {value} (expected auto|ascii|native)"
        ))),
    }
}

#[derive(Debug)]
struct AppError {
    code: i32,
    message: String,
}

impl AppError {
    fn input(message: impl Into<String>) -> Self {
        Self {
            code: 2,
            message: message.into(),
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }
}

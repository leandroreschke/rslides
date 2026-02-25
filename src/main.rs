use std::env;
use std::path::PathBuf;

use rslides::app::{AppConfig, run_presentation};
use rslides::parser::parse_presentation;
use rslides::render::Theme;

#[derive(Debug)]
struct Cli {
    markdown_path: PathBuf,
    theme: Theme,
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
            no_ansi: false,
            fps: 8,
            theme: cli.theme,
            line_spacing: 1,
            image_mode: rslides::model::ImageMode::Native,
            gif_mode: rslides::model::ImageMode::Native,
        },
    )
    .map_err(AppError::runtime)
}

fn parse_args<I>(args: I) -> Result<Cli, AppError>
where
    I: Iterator<Item = String>,
{
    let mut theme = Theme::default();
    let mut markdown_path: Option<PathBuf> = None;

    let mut pending = args.peekable();
    while let Some(arg) = pending.next() {
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
            "usage: rslides [--theme <file>] <file.md>",
        ));
    };

    Ok(Cli {
        markdown_path,
        theme,
    })
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

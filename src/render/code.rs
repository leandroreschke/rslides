use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

use crate::model::CodeData;
use crate::render::text::clip_to_width;

pub struct CodeHighlighter {
    ps: SyntaxSet,
    theme: Option<Theme>,
}

impl CodeHighlighter {
    pub fn new() -> Self {
        let ps = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| ts.themes.values().next().cloned());

        Self { ps, theme }
    }

    pub fn render_code(&self, code: &CodeData, width: usize, ansi: bool) -> Vec<String> {
        if width == 0 {
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut raw_lines = code.source.lines().peekable();

        if ansi {
            if let Some(theme) = &self.theme {
                let syntax = code
                    .lang
                    .as_deref()
                    .and_then(|lang| self.ps.find_syntax_by_token(lang))
                    .unwrap_or_else(|| self.ps.find_syntax_plain_text());

                let mut highlighter = HighlightLines::new(syntax, theme);

                while let Some(line) = raw_lines.next() {
                    let clipped = clip_to_width(line, width);
                    match highlighter.highlight_line(&clipped, &self.ps) {
                        Ok(ranges) => out.push(as_24_bit_terminal_escaped(&ranges, false)),
                        Err(_) => out.push(clipped),
                    }
                }

                if out.is_empty() {
                    out.push(String::new());
                }

                return out;
            }
        }

        while let Some(line) = raw_lines.next() {
            out.push(clip_to_width(line, width));
        }

        if out.is_empty() {
            out.push(String::new());
        }

        out
    }
}

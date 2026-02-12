use crate::model::TableData;
use crate::render::text::{clip_to_width, pad_to_width};

pub fn render_table(table: &TableData, width: usize) -> Vec<String> {
    if width < 4 {
        return vec!["[table omitted: width too small]".to_string()];
    }

    let cols = table
        .headers
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return vec!["[empty table]".to_string()];
    }

    let borders = cols + 1;
    let content_width = width.saturating_sub(borders);
    let base = (content_width / cols).max(1);
    let mut widths = vec![base; cols];

    let mut remainder = content_width.saturating_sub(base * cols);
    let mut idx = 0usize;
    while remainder > 0 {
        widths[idx] += 1;
        remainder -= 1;
        idx = (idx + 1) % cols;
    }

    let mut lines = Vec::new();
    lines.push(make_separator(&widths));

    if !table.headers.is_empty() {
        lines.push(make_row(&table.headers, &widths));
        lines.push(make_separator(&widths));
    }

    for row in &table.rows {
        lines.push(make_row(row, &widths));
    }

    lines.push(make_separator(&widths));
    lines
}

fn make_separator(widths: &[usize]) -> String {
    let mut out = String::from("+");
    for &w in widths {
        out.push_str(&"-".repeat(w));
        out.push('+');
    }
    out
}

fn make_row(cells: &[String], widths: &[usize]) -> String {
    let mut out = String::from("|");

    for (idx, &cell_width) in widths.iter().enumerate() {
        let raw = cells.get(idx).map(String::as_str).unwrap_or("");
        let clipped = clip_to_width(raw, cell_width);
        out.push_str(&pad_to_width(&clipped, cell_width));
        out.push('|');
    }

    out
}

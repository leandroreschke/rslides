pub fn wrap_text(input: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();

    for raw_line in input.lines() {
        if raw_line.trim().is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let word_len = word.chars().count();
            let current_len = current.chars().count();
            if current.is_empty() {
                if word_len > width {
                    lines.push(clip_to_width(word, width));
                } else {
                    current.push_str(word);
                }
            } else if current_len + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = String::new();
                if word_len > width {
                    lines.push(clip_to_width(word, width));
                } else {
                    current.push_str(word);
                }
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    lines
}

pub fn clip_to_width(input: &str, width: usize) -> String {
    input.chars().take(width).collect()
}

pub fn pad_to_width(input: &str, width: usize) -> String {
    let clipped = clip_to_width(input, width);
    let visible = visible_width(&clipped);
    if visible >= width {
        return clipped;
    }

    let mut out = clipped;
    out.push_str(&" ".repeat(width - visible));
    out
}

pub fn visible_width(input: &str) -> usize {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut width = 0usize;

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
        i += ch.len_utf8();
        width += 1;
    }

    width
}

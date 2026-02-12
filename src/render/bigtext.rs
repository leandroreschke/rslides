use crate::render::text::clip_to_width;

const GLYPH_HEIGHT: usize = 6;
const GLYPH_WIDTH: usize = 5;
const DEPTH: usize = 1;

pub fn render_big_text(input: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let char_cell = GLYPH_WIDTH + 2;
    let max_chars = (width / char_cell).max(1);
    let trimmed: String = input.chars().take(max_chars).collect();
    let upper = trimmed.to_uppercase();

    let mut glyphs = Vec::new();
    for ch in upper.chars() {
        if let Some(glyph) = glyph_for(ch) {
            glyphs.push(glyph);
        } else {
            return vec![clip_to_width(input, width)];
        }
    }

    let total_width = glyphs.len() * (GLYPH_WIDTH + 2) + DEPTH;
    let total_height = GLYPH_HEIGHT + DEPTH;
    let mut canvas = vec![vec![' '; total_width]; total_height];

    for (glyph_idx, glyph) in glyphs.iter().enumerate() {
        let base_x = glyph_idx * (GLYPH_WIDTH + 2);
        for (gy, row) in glyph.iter().enumerate() {
            let mask: Vec<char> = row.chars().collect();
            for gx in 0..GLYPH_WIDTH {
                if mask.get(gx).copied().unwrap_or(' ') == ' ' {
                    continue;
                }

                // Subtle shadow/extrusion layer.
                let sx = base_x + gx + DEPTH;
                let sy = gy + DEPTH;
                if sy < total_height && sx < total_width && canvas[sy][sx] == ' ' {
                    canvas[sy][sx] = '░';
                }

                // Crisp front face for readability.
                canvas[gy][base_x + gx] = '█';
            }
        }
    }

    let out = canvas
        .into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect::<Vec<_>>();

    out.into_iter()
        .map(|line| clip_to_width(&line, width))
        .collect()
}

fn glyph_for(ch: char) -> Option<[&'static str; GLYPH_HEIGHT]> {
    let glyph = match ch {
        'A' => ["  A  ", " A A ", "AAAAA", "A   A", "A   A", "A   A"],
        'B' => ["BBBB ", "B   B", "BBBB ", "B   B", "B   B", "BBBB "],
        'C' => [" CCC ", "C   C", "C    ", "C    ", "C   C", " CCC "],
        'D' => ["DDD  ", "D  D ", "D   D", "D   D", "D  D ", "DDD  "],
        'E' => ["EEEEE", "E    ", "EEE  ", "E    ", "E    ", "EEEEE"],
        'F' => ["FFFFF", "F    ", "FFF  ", "F    ", "F    ", "F    "],
        'G' => [" GGG ", "G    ", "G GG ", "G   G", "G   G", " GGG "],
        'H' => ["H   H", "H   H", "HHHHH", "H   H", "H   H", "H   H"],
        'I' => ["IIIII", "  I  ", "  I  ", "  I  ", "  I  ", "IIIII"],
        'J' => ["JJJJJ", "   J ", "   J ", "   J ", "J  J ", " JJ  "],
        'K' => ["K   K", "K  K ", "KKK  ", "K  K ", "K   K", "K   K"],
        'L' => ["L    ", "L    ", "L    ", "L    ", "L    ", "LLLLL"],
        'M' => ["M   M", "MM MM", "M M M", "M   M", "M   M", "M   M"],
        'N' => ["N   N", "NN  N", "N N N", "N  NN", "N   N", "N   N"],
        'O' => [" OOO ", "O   O", "O   O", "O   O", "O   O", " OOO "],
        'P' => ["PPPP ", "P   P", "PPPP ", "P    ", "P    ", "P    "],
        'Q' => [" QQQ ", "Q   Q", "Q   Q", "Q Q Q", "Q  QQ", " QQQQ"],
        'R' => ["RRRR ", "R   R", "RRRR ", "R R  ", "R  R ", "R   R"],
        'S' => [" SSS ", "S    ", " SSS ", "    S", "    S", " SSS "],
        'T' => ["TTTTT", "  T  ", "  T  ", "  T  ", "  T  ", "  T  "],
        'U' => ["U   U", "U   U", "U   U", "U   U", "U   U", " UUU "],
        'V' => ["V   V", "V   V", "V   V", "V   V", " V V ", "  V  "],
        'W' => ["W   W", "W   W", "W   W", "W W W", "WW WW", "W   W"],
        'X' => ["X   X", " X X ", "  X  ", "  X  ", " X X ", "X   X"],
        'Y' => ["Y   Y", " Y Y ", "  Y  ", "  Y  ", "  Y  ", "  Y  "],
        'Z' => ["ZZZZZ", "   Z ", "  Z  ", " Z   ", "Z    ", "ZZZZZ"],
        '0' => [" 000 ", "0   0", "0 0 0", "0 0 0", "0   0", " 000 "],
        '1' => ["  1  ", " 11  ", "  1  ", "  1  ", "  1  ", "11111"],
        '2' => [" 222 ", "2   2", "   2 ", "  2  ", " 2   ", "22222"],
        '3' => ["3333 ", "    3", " 333 ", "    3", "    3", "3333 "],
        '4' => ["4  4 ", "4  4 ", "44444", "   4 ", "   4 ", "   4 "],
        '5' => ["55555", "5    ", "5555 ", "    5", "    5", "5555 "],
        '6' => [" 666 ", "6    ", "6666 ", "6   6", "6   6", " 666 "],
        '7' => ["77777", "   7 ", "  7  ", " 7   ", "7    ", "7    "],
        '8' => [" 888 ", "8   8", " 888 ", "8   8", "8   8", " 888 "],
        '9' => [" 999 ", "9   9", " 9999", "    9", "   9 ", " 99  "],
        ' ' => ["     ", "     ", "     ", "     ", "     ", "     "],
        '.' => ["     ", "     ", "     ", "     ", "  .. ", "  .. "],
        ',' => ["     ", "     ", "     ", "  ,, ", "  ,, ", " ,,  "],
        ':' => ["     ", "  :: ", "  :: ", "     ", "  :: ", "  :: "],
        '-' => ["     ", "     ", "-----", "     ", "     ", "     "],
        '!' => ["  !  ", "  !  ", "  !  ", "  !  ", "     ", "  !  "],
        '?' => [" ??? ", "?   ?", "   ? ", "  ?  ", "     ", "  ?  "],
        '/' => ["    /", "   / ", "  /  ", " /   ", "/    ", "     "],
        _ => return None,
    };
    Some(glyph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_ascii_title() {
        let lines = render_big_text("AB", 80);
        assert!(lines.len() >= 6);
        assert!(lines[0].contains('█') || lines[0].contains('▓'));
    }

    #[test]
    fn falls_back_for_unsupported_chars() {
        let lines = render_big_text("@Hi", 40);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "@Hi");
    }
}

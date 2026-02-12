pub fn compute_column_widths(total_width: u16, ratios: &[u16]) -> Vec<u16> {
    if total_width == 0 {
        return Vec::new();
    }
    if ratios.is_empty() {
        return vec![total_width];
    }

    let gaps = (ratios.len().saturating_sub(1)) as u16;
    let content_width = total_width.saturating_sub(gaps);
    if content_width == 0 {
        return vec![1; ratios.len()];
    }

    let sum: u32 = ratios.iter().map(|v| *v as u32).sum::<u32>().max(1);
    let mut widths: Vec<u16> = ratios
        .iter()
        .map(|r| ((content_width as u32 * *r as u32) / sum) as u16)
        .collect();

    for w in &mut widths {
        if *w == 0 {
            *w = 1;
        }
    }

    let mut used: i32 = widths.iter().map(|w| *w as i32).sum();
    let target = content_width as i32;
    let mut idx = 0usize;
    while used < target {
        widths[idx] = widths[idx].saturating_add(1);
        used += 1;
        idx = (idx + 1) % widths.len();
    }
    while used > target {
        if widths[idx] > 1 {
            widths[idx] -= 1;
            used -= 1;
        }
        idx = (idx + 1) % widths.len();
    }

    widths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_custom_split() {
        let cols = compute_column_widths(100, &[2, 8]);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0] + cols[1] + 1, 100);
    }

    #[test]
    fn supports_three_columns() {
        let cols = compute_column_widths(90, &[2, 3, 5]);
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0] + cols[1] + cols[2] + 2, 90);
    }
}

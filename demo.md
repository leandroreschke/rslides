# rslides Feature Demo

Use `k/K` `l/L` or arrows to navigate. Use `q` or `Esc` to quit.

This deck is a usage reference for all currently implemented features.

---

# CLI Usage

Run:

```bash
cargo run -- demo.md
```

Common flags:

| Flag | Example | Notes |
| --- | --- | --- |
| `--fps` | `--fps 8` | GIF playback cap |
| `--no-ansi` | `--no-ansi` | Plain output mode |
| `--theme` | `--theme theme.example` | Custom colors |
| `--line-spacing` | `--line-spacing 2` | Global block spacing |
| `--columns` | `--columns 3,7` | Ratios for image slides |
| `--image-mode` | `--image-mode native` | Static image render mode |
| `--gif-mode` | `--gif-mode ascii` | GIF render mode |

`--columns` is ratio-based (not literal percent):
- `2,8` means 20% / 80%
- `3,4,3` means 30% / 40% / 30%

Note: non-image slides render full-width text by design.

Render modes:
- `auto`: prefer native on capable terminals
- `ascii`: force ASCII blocks
- `native`: force terminal-native image path

---

# Markdown Contract

Slides split on a line that is exactly:

```md
---
```

Supported blocks:
- H1 (`#`) => Big ASCII title
- Paragraph
- Quote (`>`)
- Lists (`-` and `1.`)
- Callouts (`[!NOTE]`, `[!TIP]`, `[!WARN]`)
- Table (pipe table)
- Code block (fenced + language)
- One image per slide (`![alt](path)`), including `.svg`

---

# Quote List Callout

> This is a quote block with quote styling.

1. Ordered list item one
2. Ordered list item two

- Bullet item one
- Bullet item two

[!NOTE] Note callout style.
[!TIP] Tip callout style.
[!WARN] Warning callout style.

---

# Fragments Reveal

<!-- reveal: on -->

This first paragraph appears first.

Second paragraph appears on next next/forward action.

Third paragraph appears after that.

---

# Line Spacing Per Slide

<!-- line_spacing: 2 -->

This slide overrides spacing.

Markdown normally collapses multiple blank lines.

Use `<!-- line_spacing: N -->` (N up to 6) when you want extra breathing room.

---

# Code Highlight

```rust
use std::time::Duration;

fn fade_steps(total_ms: u64, steps: usize) -> Duration {
    Duration::from_millis(total_ms / steps.max(1) as u64)
}

fn main() {
    println!("smooth transitions");
}
```

---

# Two Column Image

Text renders left and image renders right when a slide has one image.

Image metadata syntax:

```md
![ [valign: middle, halign: center, alt: "Caption text"] ](assets/demo.png)
```

![ [valign: middle, halign: center, alt: "Centered image with caption"] ](assets/demo.png)

---

# GIF

GIF can run in ASCII or native mode depending on `--gif-mode`.

![ [valign: middle, halign: center, alt: "Animated demo gif"] ](assets/demo.gif)

---

# SVG Image

SVG now renders in ASCII mode.

![ [valign: middle, halign: center, alt: "SVG demo asset"] ](assets/demo.svg)


---

# Hero Image (Centered)

![ [valign: middle, halign: center, alt: "Centered hero alignment"] ](assets/demo.png)

---

# Image Only Fullscreen

![ [valign: middle, halign: center, alt: "Fullscreen style image-only slide"] ](assets/demo.png)

---

# Done

You now have examples for:
BigText, paragraph, quote, list, callout, fragments, line spacing, tables, code highlight, static image, GIF, image metadata, and fullscreen image slides.

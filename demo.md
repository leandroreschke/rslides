--- {image-mode: native}

title: rslides Feature Demo
sub_title: Cover metadata slide (title, sub_title, author)
author: Me, I and Myself

--- {columns: [1,3], image-mode: native, title: "Markdown Contract"}

Slide delimiter supports per-slide config:

```md
--- {columns: [1,3], image-mode: native, title: "My Slide Title"}
```

This config applies to the slide below the delimiter.

---

# Quote List Callout

> This is a quote block with quote styling.

1. Ordered item one
2. Ordered item two

- Bullet item one
- Bullet item two

[!NOTE] Note callout style.
[!TIP] Tip callout style.
[!WARN] Warning callout style.

--- {columns: [1,2], image-mode: native}

# Inline Line Spacing

Text before spacing marker.

<!-- line-spacing: 10 -->

Text after spacing marker.

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

--- {columns: [2,3], image-mode: native}

# Two Column Image

![ [valign: middle, halign: center, alt: "Centered image with caption"] ](assets/demo.png)

--- {columns: [2,3], image-mode: native}

# GIF

![ [valign: middle, halign: center, alt: "Animated demo gif"] ](assets/demo.gif)

--- {columns: [2,3], image-mode: native}

# SVG Real Image

![ [valign: middle, halign: center, alt: "SVG demo asset"] ](assets/demo.svg)

--- {columns: [1,1], title: "Column Directives"}

<!-- column: 0 -->
<!-- align: left -->
## Left Panel
This text stays in the first column.
### Nested topic
Following text is indented by heading level.

<!-- column: 1 -->
<!-- align: center -->
## Right Panel
This whole block is centered inside column two.
### Another level
Indented and centered together.

---

# Done

Use `k/l` or arrows to navigate. `q` or `Esc` quits.

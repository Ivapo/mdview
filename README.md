# mdview

A minimal terminal markdown reader written in Rust.

```
mdview notes.md
```

## What it does

Renders a single markdown file in a centered, fixed-width column inside the alternate screen. Toggle between the rendered view and the raw source with `Tab`. Scroll with the trackpad/mouse wheel, or with `j`/`k` and arrow keys.

## Install

```sh
cargo install --git https://github.com/Ivapo/mdview
```

Or clone and build:

```sh
git clone https://github.com/Ivapo/mdview
cd mdview
cargo build --release
./target/release/mdview README.md
```

## Keys

| Key                 | Action                                  |
|---------------------|-----------------------------------------|
| `Tab`               | Toggle rendered ↔ raw view              |
| `j` / `↓`           | Scroll down one line                    |
| `k` / `↑`           | Scroll up one line                      |
| `Space` / `PgDn`    | Scroll down a page                      |
| `PgUp`              | Scroll up a page                        |
| `g` / `Home`        | Jump to top                             |
| `G` / `End`         | Jump to bottom                          |
| Mouse wheel         | Scroll                                  |
| `y`                 | Copy the file path to the clipboard     |
| `q` / `Esc`         | Quit                                    |

## Tweaks

A few constants near the top of `src/main.rs` control the look:

```rust
const CONTENT_WIDTH: u16 = 80;          // column width
const FRAME_COLOR:   Color = Color::DarkGray;
const TITLE_COLOR:   Color = Color::Green;
```

## Stack

- [`ratatui`](https://crates.io/crates/ratatui) + [`crossterm`](https://crates.io/crates/crossterm) — TUI rendering and input
- [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) — CommonMark parser
- [`syntect`](https://crates.io/crates/syntect) — code-block syntax highlighting
- [`unicode-width`](https://crates.io/crates/unicode-width) — cell widths for table layout
- [`arboard`](https://crates.io/crates/arboard) — clipboard
- [`anyhow`](https://crates.io/crates/anyhow) — error handling

The markdown renderer is a hand-rolled walk over the pulldown-cmark event stream — see `src/render.rs`. It handles headings (with `═`/`─` underbars on h1/h2), bold/italic/strikethrough/links, inline code, fenced code blocks (syntect-highlighted with a dark background and a language tag), ordered and unordered lists with nesting, blockquotes with a left bar, horizontal rules, and tables with box-drawing borders that shrink columns to fit the content width.

## License

MIT — see [LICENSE](LICENSE).

# mdview

[![crates.io](https://img.shields.io/crates/v/mdview-tui.svg)](https://crates.io/crates/mdview-tui)
[![license](https://img.shields.io/crates/l/mdview-tui.svg)](LICENSE)

A minimal terminal markdown reader written in Rust. Published on crates.io as [`mdview-tui`](https://crates.io/crates/mdview-tui); the installed binary is `mdview`.

```
mdview notes.md
```

```
mdview --help       # usage
mdview --version    # version
```

## What it does

Renders a single markdown file in a centered, fixed-width column inside the alternate screen. Toggle between the rendered view and the raw source with `Tab`. Scroll with the trackpad/mouse wheel, or with `j`/`k` and arrow keys — a thumb on the right border tracks your position. Press `?` for the full key list.

YAML front matter — the `---`-delimited block some files open with — renders as an aligned key/value header with a rule under it, not as markdown. Body paragraphs are justified to the column width. Standalone images render inline at up to 80% of the column width, or at their own size if they are smaller than that — as the real picture in terminals that support iTerm2's inline image protocol (iTerm2, WezTerm, Rio, including over ssh), and as half-block (`▀`) pixel previews everywhere else. Press `o` or click one (the bottom bar shows `click: <file>` while hovering) to open the original in your system viewer, which is also how SVG and PDF are handled since neither can be drawn inline. LaTeX math renders as Unicode: `$\gamma^d$` → *γᵈ*, `$h_{\min}$` → *hₘᵢₙ*, `\mathbb{E}[S]` → 𝔼[S], with `$$…$$` display equations centered on their own line. Complex TeX degrades to readable linear form (`\frac{a}{b}` → `(a)/(b)`) rather than raw source.

## Install

```sh
cargo install mdview-tui
```

The crate is published as `mdview-tui` (the `mdview` name on crates.io was already taken), but the binary it installs is `mdview` — so you still run `mdview notes.md` after installing.

Or build from source:

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
| `-` / `+`           | Narrow / widen the content column       |
| `o`                 | Open first visible image in system viewer |
| Left-click on image | Open that image in system viewer        |
| `y`                 | Copy the file path to the clipboard     |
| `?`                 | Toggle the keybinding overlay           |
| `q` / `Esc`         | Quit                                    |

## Tweaks

A few constants near the top of `src/main.rs` control the look:

```rust
const MAX_CONTENT_WIDTH:     u16 = 130;   // cap on column width
const MIN_CONTENT_WIDTH:     u16 = 80;    // floor for -/+ adjustments
const DEFAULT_CONTENT_WIDTH: u16 = 90;    // startup width target
const SIDE_MARGIN:           u16 = 4;     // breathing room left+right
const WIDTH_STEP:            u16 = 4;     // cells per -/+ press
const FRAME_COLOR:           Color = Color::DarkGray;
const TITLE_COLOR:           Color = Color::Green;
```

Startup column = `min(DEFAULT_CONTENT_WIDTH, terminal_width - SIDE_MARGIN)`. Adjust live with `-` and `+` (clamped to `[MIN_CONTENT_WIDTH, min(MAX_CONTENT_WIDTH, terminal_width - SIDE_MARGIN)]`); each press re-renders the cached markdown.

Images have their own constants near the top of `src/render.rs`:

```rust
const IMAGE_WIDTH_PCT:   u32 = 80;   // share of the column an image may fill
const IMAGE_CELL_PX:     u32 = 8;    // assumed cell width; decides when an image is "small"
const IMAGE_PX_PER_CELL: u32 = 8;    // inline-protocol resolution: sharpness vs bytes per frame
```

An image is drawn `min(IMAGE_WIDTH_PCT of the column, source_width / IMAGE_CELL_PX)` cells wide, with the height following the source aspect ratio — so large figures fit the column and small ones keep their own size rather than being blown up. Height is not capped: a tall image is simply taller than the viewport and you scroll through it.

`IMAGE_PX_PER_CELL` affects only the inline protocol. Raising it is sharper on a retina display, but there are no image ids in the protocol, so every scroll step resends the payload — push it too far and the half-block underlay starts showing through while scrolling. Set `MDVIEW_NO_INLINE_IMAGES=1` to turn inline rendering off entirely and use half-blocks everywhere.

## Stack

- [`ratatui`](https://crates.io/crates/ratatui) + [`crossterm`](https://crates.io/crates/crossterm) — TUI rendering and input
- [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) — CommonMark parser
- [`syntect`](https://crates.io/crates/syntect) — code-block syntax highlighting
- [`image`](https://crates.io/crates/image) — decoding and PNG re-encoding for inline images
- [`unicode-width`](https://crates.io/crates/unicode-width) — cell widths for table layout
- [`arboard`](https://crates.io/crates/arboard) — clipboard
- [`anyhow`](https://crates.io/crates/anyhow) — error handling

The markdown renderer is a hand-rolled walk over the pulldown-cmark event stream — see `src/render.rs`. It handles headings (with `═`/`─` underbars on h1/h2), bold/italic/strikethrough/links, inline code, fenced code blocks (syntect-highlighted with a dark background and a language tag), ordered and unordered lists with nesting, blockquotes with a left bar, horizontal rules, tables with box-drawing borders that shrink columns to fit the content width, justified paragraphs, half-block image previews, and TeX math via a small hand-rolled converter (`src/math.rs`). On terminals that speak iTerm2's inline image protocol, `src/graphics.rs` paints the real image over those half-block rows, cropped to whatever part of it is on screen; set `MDVIEW_NO_INLINE_IMAGES=1` to force the half-block fallback.

## License

MIT — see [LICENSE](LICENSE).

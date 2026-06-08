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
- [`tui-markdown`](https://crates.io/crates/tui-markdown) — markdown → ratatui `Text`
- [`arboard`](https://crates.io/crates/arboard) — clipboard
- [`anyhow`](https://crates.io/crates/anyhow) — error handling

Rendering is intentionally minimal — tui-markdown does not render tables and styles code blocks lightly. If you want syntax-highlighted code blocks, table layout, and richer block styling, [`md-tui`](https://github.com/henriklovhaug/md-tui) is a more featureful reader.

## License

MIT — see [LICENSE](LICENSE).

# CLAUDE.md

Notes for future Claude sessions on this repo.

## What this is

`mdview` is a small personal-use terminal markdown reader. One file, one job: open a `.md` file in a centered column inside the alternate screen, scroll it, toggle raw view, quit. Priority is **simplicity and clean code over features** — do not add abstractions, configuration layers, or feature flags unless the user explicitly asks for them.

## Layout

- `src/main.rs` — TUI shell: argv parsing, terminal lifecycle, event loop, frame layout. Don't grow this with markdown logic.
- `src/render.rs` — the markdown renderer: walks pulldown-cmark events and produces a `ratatui::text::Text<'static>`. All rendering decisions (colors, table layout, code-block style) live here.
- `sample.md` — scratch file for poking at rendering.

## Run

```sh
cargo run -- sample.md
```

The app uses the alternate screen + raw mode + mouse capture. A panic hook in `setup_terminal` restores the terminal on panic, and `restore_terminal` is called on normal exit.

## Verifying TUI changes

You cannot drive an interactive TUI from a non-tty shell — `enable_raw_mode` will fail. To smoke-test from the agent harness, wrap it in `expect` with a pty:

```sh
expect -c '
spawn ./target/debug/mdview sample.md
expect -re ".+"
sleep 0.3
send "j"; sleep 0.2
send "\t"; sleep 0.3
send "q"
expect eof
'
```

If you see the alt-screen enter (`[?1049h`) and leave (`[?1049l`) sequences with exit status 0, the boot/teardown path is fine. Visual correctness requires a real terminal — when you can't run one, say so rather than claiming success.

## Tweakable knobs

Constants at the top of `src/main.rs` are intentionally there so the user can edit them in one place. Don't bury them behind a config file.

- `MAX_CONTENT_WIDTH` — cap on column width (default 120). Actual width = `min(MAX_CONTENT_WIDTH, term_width - SIDE_MARGIN)`, computed once at `App::new` via `crossterm::terminal::size()` and stored on `App.content_width`. Terminal resize does NOT re-flow tables — re-launch for that.
- `SIDE_MARGIN` — left+right breathing room subtracted from terminal width
- `FRAME_COLOR` — border + bottom-hint color
- `TITLE_COLOR` — filename color and status-success color
- `SCROLL_STEP`, `PAGE_STEP`, `STATUS_TTL` — self-explanatory

## Renderer (`src/render.rs`)

We replaced `tui-markdown` with a hand-rolled `pulldown-cmark`-driven renderer on 2026-06-07. Roughly 400 lines. The shape:

- `render(source, width) -> Text<'static>` is the only public entry. Width matters because tables and code-block backgrounds pre-fit to it. `App` calls this once at startup and caches the result (no per-frame re-parse).
- `Renderer` holds: current line buffer (`cur`), a style stack (so nested emphasis composes), a list-context stack (for ordered counters / nesting depth), `quote_depth`, and `Option<CodeCtx>` / `Option<TableCtx>` for buffered block constructs.
- When inside a table, `push_span` redirects to the current cell instead of `cur`. Cells are buffered until `TagEnd::Table`, then laid out with box-drawing borders. Columns shrink proportionally if the natural width exceeds `width`; cells longer than their column truncate with `…`.
- Code blocks use `syntect`'s default `base16-ocean.dark` theme. Each highlighted line is padded with `CODE_BG`-styled spaces to the full column so the background fills.
- Headings h1/h2 emit a `═` / `─` underbar sized to the heading text. h3-h6 are color-only.
- The rendered `Paragraph` still has `Wrap { trim: false }` so long plain paragraphs wrap. Tables and code blocks are pre-fit to `CONTENT_WIDTH`, so wrap shouldn't visually break them — but if you grow features, keep this invariant.
- `rendered_line_count` (used for scroll bounds) counts logical lines, not wrapped visual lines. Scroll-to-bottom may stop slightly above the true end when paragraphs wrap. Acceptable for v1; fix with the `unstable-rendered-line-info` ratatui feature if it matters.

Color constants are at the top of `render.rs` (`CODE_BG`, `INLINE_CODE_BG`, `LINK_COLOR`, `RULE_COLOR`, `SYNTECT_THEME`, …). Keep them there so they're easy to tweak.

## Dependencies

Versions pinned to current majors (June 2026):

- `ratatui = "0.30"` — note the 0.30 split into `ratatui-core`/`ratatui-widgets` workspace
- `crossterm = "0.29"`
- `pulldown-cmark = "0.13"` — markdown parsing (default features off, only `html`)
- `syntect = "5.3"` — code-block syntax highlighting (default features on; pulls `onig`)
- `unicode-width = "0.2"` — cell widths for table column sizing
- `arboard = "3.6"` — clipboard for `y`
- `anyhow = "1.0"`

Edition is `2024`. `rust-version` is unset; tui-markdown needs ≥1.86.

## Conventions for this repo

- Single file (`main.rs`) is the rule, not an accident. Split only if a feature genuinely demands it.
- No comments unless the *why* is non-obvious — most code here is self-explanatory.
- Don't add error handling for impossible cases. Boundaries (file read, clipboard, terminal init) already have `anyhow` context.
- Don't introduce `clap` for CLI parsing — the manual `env::args_os()` block is fine for one positional argument.
- Don't add config files, themes, or plugin systems. If the user asks for theming, prefer "edit the constants" over a config file.

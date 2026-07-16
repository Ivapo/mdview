# CLAUDE.md

Notes for future Claude sessions on this repo.

## What this is

`mdview` is a small personal-use terminal markdown reader. One file, one job: open a `.md` file in a centered column inside the alternate screen, scroll it, toggle raw view, quit. Priority is **simplicity and clean code over features** — do not add abstractions, configuration layers, or feature flags unless the user explicitly asks for them.

## Layout

- `src/main.rs` — TUI shell: argv parsing, terminal lifecycle, event loop, frame layout. Don't grow this with markdown logic.
- `src/render.rs` — the markdown renderer: walks pulldown-cmark events and produces a `ratatui::text::Text<'static>`. All rendering decisions (colors, table layout, code-block style) live here.
- `src/math.rs` — pure `tex_to_unicode(&str) -> String`: best-effort TeX → Unicode (greek, sub/superscripts, `\frac` → `(a)/(b)`, symbol table). Unmappable scripts degrade to `^(...)`/`_(...)`, unknown commands pass through as their bare name (which is exactly right for `\min`, `\log`, …).
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

- `MAX_CONTENT_WIDTH` / `MIN_CONTENT_WIDTH` / `DEFAULT_CONTENT_WIDTH` — bounds and startup target. Startup uses `min(DEFAULT_CONTENT_WIDTH, term_width - SIDE_MARGIN)`, stored on `App.content_width`. `-`/`+` keys call `App::adjust_width(±WIDTH_STEP)` which clamps to `[MIN, min(MAX, term_width - SIDE_MARGIN)]`, re-renders the cached `Text`, and reclamps scroll. Each adjustment re-queries `crossterm::terminal::size()` so the cap tracks terminal resizes between presses, but the initial render does not re-flow on bare resize events.
- `SIDE_MARGIN` — left+right breathing room subtracted from terminal width
- `WIDTH_STEP` — cells per `-`/`+` press
- `FRAME_COLOR` — border + bottom-hint color
- `TITLE_COLOR` — filename color and status-success color
- `SCROLL_STEP`, `PAGE_STEP`, `STATUS_TTL` — self-explanatory

## Renderer (`src/render.rs`)

We replaced `tui-markdown` with a hand-rolled `pulldown-cmark`-driven renderer on 2026-06-07. Roughly 400 lines. The shape:

- `render(source, width, base) -> (Text<'static>, Vec<ImageRef>)` is the only public entry. Width matters because tables, code-block backgrounds, and images pre-fit to it; `base` is the `.md` file's directory, used to resolve image paths. `App` calls this once at startup and caches the result (no per-frame re-parse). Each `ImageRef` is `{ lines: (start, end), dest }` — a half-open range into the returned `Text` covering the image's pixel rows plus its caption line. Images inside blockquotes are dropped (the quote re-wrap invalidates indices) and images inside table cells are never recorded.
- Standalone images (own paragraph, not in quote/table) render as half-block pixel rows: each cell is `▀` with fg = top pixel, bg = bottom pixel, so 1 cell = 1×2 pixels. Downscaled to `IMAGE_WIDTH_PCT` (80%) of content width (`resize_exact`, Triangle filter), centered, alpha composited over black, with the dimmed `[image: dest]` caption kept underneath as the click target. Inline/quote/table images and failed decodes fall back to the caption-only placeholder. Decoding happens on every render, i.e. also on each width change — fine for paper-sized docs, revisit with a decoded-image cache if it ever feels slow.
- Math (`Options::ENABLE_MATH`): `$…$` renders inline via `math::tex_to_unicode`, italic in `MATH_COLOR`; `$$…$$` becomes its own centered line(s) (split on `\\`, wrapped if over width). Display math inside a table cell degrades to an inline span.
- Top-level paragraphs are justified: `justify_flush` pre-wraps the buffered paragraph with `wrap_spans` and pads the whitespace gaps so every line but the last is exactly content width (so the `Paragraph` `Wrap` never re-wraps them). Lists, quotes, headings, and segments cut off by hard breaks stay ragged-right. `justify_flush` also widens `ImageRef` ranges recorded inside the paragraph to cover all its wrapped lines.
- `Renderer` holds: current line buffer (`cur`), a style stack (so nested emphasis composes), a list-context stack (for ordered counters / nesting depth), `quote_depth`, and `Option<CodeCtx>` / `Option<TableCtx>` for buffered block constructs.
- When inside a table, `push_span` redirects to the current cell instead of `cur`. Cells are buffered until `TagEnd::Table`, then laid out with box-drawing borders. Columns shrink proportionally if the natural width exceeds `width`; cells longer than their column truncate with `…`.
- Code blocks use `syntect`'s default `base16-ocean.dark` theme. Each highlighted line is padded with `CODE_BG`-styled spaces to the full column so the background fills.
- Headings h1/h2 emit a `═` / `─` underbar sized to the heading text. h3-h6 are color-only.
- The rendered `Paragraph` still has `Wrap { trim: false }` so long plain paragraphs wrap. Tables and code blocks are pre-fit to `CONTENT_WIDTH`, so wrap shouldn't visually break them — but if you grow features, keep this invariant.
- Scroll bounds (`rendered_line_count` / `raw_line_count`) are exact wrapped-line counts from `Paragraph::line_count` (ratatui's `unstable-rendered-line-info` feature), so scroll-to-bottom reaches the true end. Both counts are recomputed on width change. Max scroll is `total - (viewport - 1)`, which intentionally leaves one blank row below the last line as an end-of-content marker.
- Opening images: `o` opens the first image at/below the top of the viewport; left-click on an image's row opens that image. Hovering the mouse over an image block shows `click: <dest>` in the bottom bar (`App.hover`, fed by `MouseEventKind::Moved`; a real pointer-shape change isn't possible in iTerm2 — OSC 22 is kitty/WezTerm-only). Paths resolve relative to the `.md` file's directory (http(s) URLs pass through) and launch via macOS `open`. The click/keybind → image mapping lives in `image_row_ranges` (`main.rs`): ratatui wraps each `Line` independently, so per-line `line_count` prefix sums give exact visual rows for each `ImageRef` — recomputed on width change. Keep that invariant if you touch wrapping.

Color constants are at the top of `render.rs` (`CODE_BG`, `INLINE_CODE_BG`, `LINK_COLOR`, `RULE_COLOR`, `SYNTECT_THEME`, …). Keep them there so they're easy to tweak.

## Dependencies

Versions pinned to current majors (June 2026):

- `ratatui = "0.30"` with the `unstable-rendered-line-info` feature (for `Paragraph::line_count`) — note the 0.30 split into `ratatui-core`/`ratatui-widgets` workspace
- `crossterm = "0.29"`
- `pulldown-cmark = "0.13"` — markdown parsing (default features off, only `html`)
- `syntect = "5.3"` — code-block syntax highlighting (default features on; pulls `onig`)
- `unicode-width = "0.2"` — cell widths for table column sizing
- `image = "0.25"` (default features off; `jpeg`, `png`, `gif`, `webp`) — decoding for half-block inline images
- `arboard = "3.6"` — clipboard for `y`
- `anyhow = "1.0"`

Edition is `2024`. `rust-version` is unset; tui-markdown needs ≥1.86.

## Conventions for this repo

- Single file (`main.rs`) is the rule, not an accident. Split only if a feature genuinely demands it.
- No comments unless the *why* is non-obvious — most code here is self-explanatory.
- Don't add error handling for impossible cases. Boundaries (file read, clipboard, terminal init) already have `anyhow` context.
- Don't introduce `clap` for CLI parsing — the manual `env::args_os()` block is fine for one positional argument.
- Don't add config files, themes, or plugin systems. If the user asks for theming, prefer "edit the constants" over a config file.

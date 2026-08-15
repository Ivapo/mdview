# CLAUDE.md

Notes for future Claude sessions on this repo.

## What this is

`mdview` is a small personal-use terminal markdown reader. One file, one job: open a `.md` file in a centered column inside the alternate screen, scroll it, toggle raw view, quit. Priority is **simplicity and clean code over features** — do not add abstractions, configuration layers, or feature flags unless the user explicitly asks for them.

## Layout

- `src/main.rs` — TUI shell: argv parsing, terminal lifecycle, event loop, frame layout. Don't grow this with markdown logic.
- `src/render.rs` — the markdown renderer: walks pulldown-cmark events and produces a `ratatui::text::Text<'static>`. All rendering decisions (colors, table layout, code-block style) live here.
- `src/graphics.rs` — iTerm2's inline image protocol: terminal detection, the OSC 1337 writer, and a hand-rolled base64 encoder. Knows nothing about markdown; it takes a bitmap, a cell box and a row range.
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

Most of the inline-image behaviour does not need a terminal at all, and is covered by `cargo test` instead: `draw_inline_images` writes to any `W: Write`, so the tests hand it a `Vec<u8>` and assert on the emitted `width=`/`height=` cell box. That covers crop geometry, the raw/help suppression, the not-resent-when-unmoved cache and its invalidation, and the small-image sizing rule. Fixtures are generated at test time with the `image` crate into a temp dir, so no binary blobs are committed. Two things to keep in mind when adding to them: build `App` by hand rather than via `App::new`, which asks crossterm for the terminal size and so renders at a different width under `cargo test` than under `cargo test -- --nocapture`; and check a new test actually fails when you break the thing it covers, since these all assert on absence of output in at least one direction.

What is *not* covered is event coalescing (it needs the real event loop) and the true end-to-end path. For those, or anything else that writes escape sequences, capture the session and assert on the bytes. For the inline images that means: log the whole run, count `\x1b]1337;File=` occurrences, and parse the `width=`/`height=` cell box out of each header. Two expect traps to know about — `log_file` needs `-a` when `log_user` is 0, and it re-logs its retained buffer if enabled mid-session, so log from `spawn` and assert on exact counts rather than segmenting. Expect only reads the pty inside an `expect` command, so follow every keypress with a drain (`expect -timeout 1 { -re ".+" { exp_continue } timeout {} }`). The transmitted payload can also be base64-decoded straight back into a PNG and compared against the source bitmap, which is how the crop geometry was verified without a display.

## Tweakable knobs

Constants at the top of `src/main.rs` are intentionally there so the user can edit them in one place. Don't bury them behind a config file.

- `MAX_CONTENT_WIDTH` / `MIN_CONTENT_WIDTH` / `DEFAULT_CONTENT_WIDTH` — bounds and startup target. Startup uses `min(DEFAULT_CONTENT_WIDTH, term_width - SIDE_MARGIN)`, stored on `App.content_width`. `-`/`+` keys call `App::adjust_width(±WIDTH_STEP)` which clamps to `[MIN, min(MAX, term_width - SIDE_MARGIN)]`, re-renders the cached `Text`, and reclamps scroll. Each adjustment re-queries `crossterm::terminal::size()` so the cap tracks terminal resizes between presses, but the initial render does not re-flow on bare resize events.
- `SIDE_MARGIN` — left+right breathing room subtracted from terminal width
- `WIDTH_STEP` — cells per `-`/`+` press
- `FRAME_COLOR` — border, bottom-hint, and scroll-thumb color
- `TITLE_COLOR` — filename color, status-success color, `?` overlay accent
- `BRAND` / `BRAND_COLOR` — bottom-right footer text and its amber (matching panex-tui); the leading spaces keep a clipped status off it
- `SCROLL_THUMB` — scroll-thumb glyph
- `IMAGE_CELL_PX` (in `render.rs`) — assumed cell width in pixels, used only to decide when an image is too small to fill the column. Layout, not quality, and deliberately separate from `IMAGE_PX_PER_CELL` so tuning sharpness doesn't silently resize small images. Raising it makes small images render smaller
- `IMAGE_PX_PER_CELL` (in `render.rs`) — horizontal resolution of the inline-image bitmap. At the default 8 a 72-cell image is 576 px wide and ~395 KB a frame, near 1:1 with the box's logical size; 12 is sharper on a retina display at ~900 KB. Raise it only as far as scrolling stays smooth — once a frame takes too long to write, the half-block underlay shows through while scrolling
- `SCROLL_STEP`, `PAGE_STEP`, `STATUS_TTL` — self-explanatory

## Shell details (`src/main.rs`)

- `parse_args` handles `-h/--help` and `-V/--version` by printing and `process::exit(0)` before the terminal enters raw mode, so `run` only ever sees a real path. Anything else starting with `-` is rejected. Still no `clap`.
- The footer (hints on the left, `BRAND` on the right) is drawn as two `Paragraph`s over the block's bottom border row rather than as two `title_bottom`s: ratatui renders right-aligned titles *before* left-aligned ones, so a long status would paint over the brand. The split rect clips the status instead.
- `render_scroll_thumb` is hand-rolled (same reasoning as panex-tui): `ratatui::Scrollbar` rounds the thumb's start and end independently, so the thumb visibly resizes by a cell mid-scroll. Length is computed once, only the position moves. It's drawn over the outer right border, spanning the content viewport rows only (not the block's vertical padding). Note `scroll_by` clamps to `total - (viewport - 1)`, one past the thumb's `max_offset = len - viewport`; the `offset.min(max_offset)` keeps it flush at the bottom.
- `App.help` drives the `?` overlay. While it's set the event loop swallows every key but `Esc`/`Enter`/`q`/`?` (which close it) and ignores mouse events entirely, so `q` can't quit out from under the overlay. Key lists live in `render_help` — update them when you add a binding, and the footer hint too if it's a common one.
- `draw_inline_images` runs *after* `terminal.draw` has flushed, writing to `terminal.backend_mut()`. It has to be after: the image lands in the terminal's own screen content, which ratatui's cell buffer knows nothing about. It is skipped in raw view and behind the `?` overlay, both of which the protocol would otherwise paint straight over. iTerm2 can't clip, and an image running past the last row would scroll the alternate screen, so partly-scrolled images are cropped to their visible rows and the emitted box shrinks to match. Nothing enforces that the box fits — it relies on the content area sitting inside the block's border and vertical padding, so there are always ≥2 rows below it for iTerm2's post-image cursor advance. Preserve that if you change the frame layout.
- The protocol has no image ids, so each emission resends the whole payload. `Drawn` records where each image was last drawn *and the encoded bytes*: an image that hasn't moved is not rewritten at all (which is what keeps mouse-move hover events, one redraw each, from pushing ~400 KB apiece), and one that moved without changing its crop — the common case when scrolling a fully visible image — is rewritten from the cached PNG without re-encoding. Skipping is only safe because ratatui repaints an image's rows whenever the content under them changed, and every such change also moves the placement; the two cases where that doesn't hold are handled explicitly — raw view / help clear the whole record, and `Event::Resize` clears it because a resize forces a full repaint without necessarily moving anything.
- The event loop applies every already-queued event before drawing again (`handle_event` in a `poll(Duration::ZERO)` loop). Without it, key repeat and mouse-move floods each get their own frame, the payload writes fall behind the input, and the half-block underlay is what you actually see while scrolling. This is also why the frame count in a test is not the keystroke count: keys delivered before the app reads them coalesce into one iteration, so an expect script must wait for the first frame before sending anything, and should drive the *release* binary — debug-build image decoding takes seconds and races the drains.
- `graphics::supported()` gates all of it: iTerm2, WezTerm or Rio, not under tmux (`TMUX`) or screen (`STY`) since neither passes the sequence through, and `MDVIEW_NO_INLINE_IMAGES` unset as an escape hatch. Everywhere else the half-blocks are what you see — the pre-0.4 behaviour, unchanged. It's an allowlist and not a probe on purpose: a terminal that doesn't parse OSC 1337 would dump the whole base64 payload on screen, so the cost of a false positive is much higher than the cost of a false negative. `LC_TERMINAL` is what carries this over ssh — iTerm2 sets it precisely because ssh forwards `LC_*` but not `TERM_PROGRAM`.

## Renderer (`src/render.rs`)

We replaced `tui-markdown` with a hand-rolled `pulldown-cmark`-driven renderer on 2026-06-07. Roughly 400 lines. The shape:

- `render(source, width, base) -> (Text<'static>, Vec<ImageRef>)` is the only public entry. Width matters because tables, code-block backgrounds, and images pre-fit to it; `base` is the `.md` file's directory, used to resolve image paths. `App` calls this once at startup and caches the result (no per-frame re-parse). Each `ImageRef` is `{ lines: (start, end), dest }` — a half-open range into the returned `Text` covering the image's pixel rows plus its caption line. Images inside blockquotes are dropped (the quote re-wrap invalidates indices) and images inside table cells are never recorded.
- Standalone images (own paragraph, not in quote/table) render as half-block pixel rows: each cell is `▀` with fg = top pixel, bg = bottom pixel, so 1 cell = 1×2 pixels. Sized to `IMAGE_WIDTH_PCT` (80%) of content width, but capped at `ow / IMAGE_CELL_PX` so an image smaller than the column renders at its own size rather than being stretched across it — a cell is one pixel to the half-blocks and roughly `IMAGE_CELL_PX` to the inline protocol, which is why the cap has to be in pixels-per-cell rather than the plain `.min(ow)` that was right when half-blocks were the only renderer. Then `resize_exact` (Triangle filter), centered, alpha composited over black, with the dimmed `[image: dest]` caption kept underneath as the click target. Inline/quote/table images and failed decodes fall back to the caption-only placeholder. Decoding happens on every render, i.e. also on each width change — fine for paper-sized docs, revisit with a decoded-image cache if it ever feels slow.
- On iTerm2 the half-blocks are only an underlay: `main.rs` paints the real image over them (see below). They stay in the buffer deliberately — they define the layout, they are the fallback everywhere else, and because each row's content differs, ratatui's diff always repaints the block when the document moves, so a stale image can't be left stranded behind a scroll. Every `ImageRef` that got half-blocks carries an `Art` alongside: the block's cell box (`cells`), its indent (`pad`), the line range of the half-block rows *without* the caption (`lines`), and `pixels`, a second copy of the image at `IMAGE_PX_PER_CELL` horizontal pixels per cell. `Art::pixels` height is forced to an exact multiple of `cells.1` (at least 2 px per cell row, what a half-block already carries), which is the invariant that makes cropping to a visible row range a plain slice of the pixel buffer — keep it if you touch that sizing.
- YAML front matter (`Options::ENABLE_YAML_STYLE_METADATA_BLOCKS`) arrives as `Tag::MetadataBlock`, buffered into `Renderer.meta` the same way code blocks buffer into `CodeCtx`, and emitted by `emit_metadata` as an aligned key/value header (dark-gray keys, green values, `─` rule underneath) — it's metadata *about* the file, not content. Without the option pulldown-cmark reads the opening `---` as a thematic break and the closing one as a setext h2, which turned the whole block into a rule + run-together paragraph. `split_meta_key` only splits top-level scalar keys; lines starting with space/tab/`-`/`#` stay verbatim under the value column so nested structures keep their shape. A `---` anywhere else in the document is still `Event::Rule`, and an unterminated leading `---` still parses as a rule.
- Math (`Options::ENABLE_MATH`): `$…$` renders inline via `math::tex_to_unicode`, italic in `MATH_COLOR`; `$$…$$` becomes its own centered line(s) (split on `\\`, wrapped if over width). Display math inside a table cell degrades to an inline span.
- Top-level paragraphs are justified: `justify_flush` pre-wraps the buffered paragraph with `wrap_spans` and pads the whitespace gaps so every line but the last is exactly content width (so the `Paragraph` `Wrap` never re-wraps them). Lists, quotes, headings, and segments cut off by hard breaks stay ragged-right. `justify_flush` also widens `ImageRef` ranges recorded inside the paragraph to cover all its wrapped lines.
- `Renderer` holds: current line buffer (`cur`), a style stack (so nested emphasis composes), a list-context stack (for ordered counters / nesting depth), `quote_depth`, and `Option<CodeCtx>` / `Option<TableCtx>` for buffered block constructs.
- When inside a table, `push_span` redirects to the current cell instead of `cur`. Cells are buffered until `TagEnd::Table`, then laid out with box-drawing borders. Columns shrink proportionally if the natural width exceeds `width`; cells longer than their column truncate with `…`.
- Code blocks use `syntect`'s default `base16-ocean.dark` theme. Each highlighted line is padded with `CODE_BG`-styled spaces to the full column so the background fills.
- Headings h1/h2 emit a `═` / `─` underbar sized to the heading text. h3-h6 are color-only.
- The rendered `Paragraph` still has `Wrap { trim: false }` so long plain paragraphs wrap. Tables and code blocks are pre-fit to `CONTENT_WIDTH`, so wrap shouldn't visually break them — but if you grow features, keep this invariant.
- Scroll bounds (`rendered_line_count` / `raw_line_count`) are exact wrapped-line counts from `Paragraph::line_count` (ratatui's `unstable-rendered-line-info` feature), so scroll-to-bottom reaches the true end. Both counts are recomputed on width change. Max scroll is `total - (viewport - 1)`, which intentionally leaves one blank row below the last line as an end-of-content marker.
- Opening images: `o` opens the first image at/below the top of the viewport; left-click on an image's row opens that image. Hovering the mouse over an image block shows `click: <dest>` in the bottom bar (`App.hover`, fed by `MouseEventKind::Moved`; a real pointer-shape change isn't possible in iTerm2 — OSC 22 is kitty/WezTerm-only). Paths resolve relative to the `.md` file's directory (http(s) URLs pass through) and launch via macOS `open`. The click/keybind → image mapping lives in `row_ranges` (`main.rs`): ratatui wraps each `Line` independently, so per-line `line_count` prefix sums give exact visual rows for each `ImageRef` — recomputed on width change. Keep that invariant if you touch wrapping.

Color constants are at the top of `render.rs` (`CODE_BG`, `INLINE_CODE_BG`, `LINK_COLOR`, `RULE_COLOR`, `SYNTECT_THEME`, …). Keep them there so they're easy to tweak.

## Dependencies

Versions pinned to current majors (June 2026):

- `ratatui = "0.30"` with the `unstable-rendered-line-info` feature (for `Paragraph::line_count`) — note the 0.30 split into `ratatui-core`/`ratatui-widgets` workspace
- `crossterm = "0.29"`
- `pulldown-cmark = "0.13"` — markdown parsing (default features off, only `html`)
- `syntect = "5.3"` — code-block syntax highlighting (default features on; pulls `onig`)
- `unicode-width = "0.2"` — cell widths for table column sizing
- `image = "0.25"` (default features off; `jpeg`, `png`, `gif`, `webp`) — decoding for half-block inline images, and the `png` feature's *encoder* for the iTerm2 path

The inline-image path deliberately added no dependencies: base64 is ~15 lines in `graphics.rs`, and PNG encoding was already available. Its encoder settings are load-bearing — `CompressionType::Fast` with `FilterType::Adaptive` is ~4x smaller than `NoFilter` on a photo for ~1 ms more, while `CompressionType::Default` costs 140 ms and is far too slow to run per scroll step. Alpha is composited over black up front so the payload is RGB, matching what the half-blocks show.

SVG and PDF are deliberately out: iTerm2 can't decode SVG and neither can the `image` crate (it would need `resvg`/`usvg`/`tiny-skia`), and PDF would only work by passing original file bytes through uncropped, which conflicts with the crop path. Both stay caption-only, openable with `o`.
- `arboard = "3.6"` — clipboard for `y`
- `anyhow = "1.0"`

Edition is `2024`. `rust-version` is unset; tui-markdown needs ≥1.86.

## Conventions for this repo

- Single file (`main.rs`) is the rule, not an accident. Split only if a feature genuinely demands it.
- No comments unless the *why* is non-obvious — most code here is self-explanatory.
- Don't add error handling for impossible cases. Boundaries (file read, clipboard, terminal init) already have `anyhow` context.
- Don't introduce `clap` for CLI parsing — the manual `env::args_os()` block is fine for one positional argument.
- Don't add config files, themes, or plugin systems. If the user asks for theming, prefer "edit the constants" over a config file.

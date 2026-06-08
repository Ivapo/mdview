# CLAUDE.md

Notes for future Claude sessions on this repo.

## What this is

`mdview` is a small personal-use terminal markdown reader. One file, one job: open a `.md` file in a centered column inside the alternate screen, scroll it, toggle raw view, quit. Priority is **simplicity and clean code over features** — do not add abstractions, configuration layers, or feature flags unless the user explicitly asks for them.

## Layout

- `src/main.rs` — the entire app (~250 lines). Keep it that way unless complexity genuinely justifies a split.
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

- `CONTENT_WIDTH` — column width (default 80)
- `FRAME_COLOR` — border + bottom-hint color
- `TITLE_COLOR` — filename color and status-success color
- `SCROLL_STEP`, `PAGE_STEP`, `STATUS_TTL` — self-explanatory

## Rendering limits (important context)

The rendering backend is `tui-markdown` 0.3, which is deliberately small. It does **not** render markdown tables and styles code blocks lightly. The user knows this and discussed it on 2026-06-07 — they may eventually want a custom `pulldown-cmark`-based renderer (with `syntect` for code blocks and a manual table layouter) to match `md-tui`-style output, but until they ask for it, don't pre-build it. If they do ask: scope is roughly 300–600 lines and should live in its own module (e.g. `src/render.rs`), not balloon `main.rs`.

## Dependencies

Versions pinned to current majors (June 2026):

- `ratatui = "0.30"` — note the 0.30 split into `ratatui-core`/`ratatui-widgets` workspace
- `crossterm = "0.29"`
- `tui-markdown = "0.3"` — uses `ratatui-core ^0.1`, compatible with ratatui 0.30
- `arboard = "3.6"` — clipboard for `y`
- `anyhow = "1.0"`

Edition is `2024`. `rust-version` is unset; tui-markdown needs ≥1.86.

## Conventions for this repo

- Single file (`main.rs`) is the rule, not an accident. Split only if a feature genuinely demands it.
- No comments unless the *why* is non-obvious — most code here is self-explanatory.
- Don't add error handling for impossible cases. Boundaries (file read, clipboard, terminal init) already have `anyhow` context.
- Don't introduce `clap` for CLI parsing — the manual `env::args_os()` block is fine for one positional argument.
- Don't add config files, themes, or plugin systems. If the user asks for theming, prefer "edit the constants" over a config file.

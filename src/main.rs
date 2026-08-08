use std::{
    env, fs,
    io::{self, Stdout},
    panic,
    path::{Path, PathBuf},
    process,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

mod math;
mod render;

const MAX_CONTENT_WIDTH: u16 = 130;
const MIN_CONTENT_WIDTH: u16 = 80;
const DEFAULT_CONTENT_WIDTH: u16 = 90;
const SIDE_MARGIN: u16 = 4;
const SCROLL_STEP: u16 = 1;
const PAGE_STEP: u16 = 10;
const WIDTH_STEP: u16 = 4;
const FRAME_COLOR: Color = Color::DarkGray;
const TITLE_COLOR: Color = Color::Green;
const STATUS_TTL: Duration = Duration::from_secs(2);
/// Padded so a footer hint clipped at the column edge can't butt up against it.
const BRAND: &str = "  mdview ";
/// Amber, matching panex-tui's footer brand.
const BRAND_COLOR: Color = Color::Rgb(255, 191, 0);
/// Half-width so it reads lighter than a full block while staying distinct
/// from the `│` border it is drawn over.
const SCROLL_THUMB: &str = "▐";

#[derive(Copy, Clone, PartialEq, Eq)]
enum Mode {
    Rendered,
    Raw,
}

struct Status {
    text: String,
    until: Instant,
    error: bool,
}

struct App {
    path: PathBuf,
    source: String,
    rendered: Text<'static>,
    images: Vec<render::ImageRef>,
    image_rows: Vec<(usize, usize)>,
    content_width: u16,
    mode: Mode,
    scroll: u16,
    raw_line_count: u16,
    rendered_line_count: u16,
    status: Option<Status>,
    hover: Option<usize>,
    help: bool,
}

impl App {
    fn new(path: PathBuf, source: String) -> Self {
        let term_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
        let content_width = DEFAULT_CONTENT_WIDTH
            .min(term_width.saturating_sub(SIDE_MARGIN))
            .max(20);
        let raw_line_count = visual_line_count(source.as_str(), content_width);
        let base = path.parent().unwrap_or(Path::new("")).to_path_buf();
        let (rendered, images) = render::render(&source, content_width, &base);
        let rendered_line_count = visual_line_count(rendered.clone(), content_width);
        let image_rows = image_row_ranges(&rendered, &images, content_width);
        Self {
            path,
            source,
            rendered,
            images,
            image_rows,
            content_width,
            mode: Mode::Rendered,
            scroll: 0,
            raw_line_count,
            rendered_line_count,
            status: None,
            hover: None,
            help: false,
        }
    }

    fn line_count(&self) -> u16 {
        match self.mode {
            Mode::Rendered => self.rendered_line_count,
            Mode::Raw => self.raw_line_count,
        }
    }

    fn toggle_mode(&mut self) {
        self.hover = None;
        self.mode = match self.mode {
            Mode::Rendered => Mode::Raw,
            Mode::Raw => Mode::Rendered,
        };
        self.scroll = self.scroll.min(self.line_count().saturating_sub(1));
    }

    fn adjust_width(&mut self, delta: i32) {
        let term_w = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
        let max = term_w
            .saturating_sub(SIDE_MARGIN)
            .min(MAX_CONTENT_WIDTH);
        let min = MIN_CONTENT_WIDTH.min(max);
        let next = (self.content_width as i32 + delta).clamp(min as i32, max as i32) as u16;
        if next == self.content_width {
            self.status = Some(Status {
                text: format!("width {} (limit)", self.content_width),
                until: Instant::now() + STATUS_TTL,
                error: false,
            });
            return;
        }
        self.content_width = next;
        let base = self.path.parent().unwrap_or(Path::new("")).to_path_buf();
        let (rendered, images) = render::render(&self.source, self.content_width, &base);
        self.rendered = rendered;
        self.images = images;
        self.image_rows =
            image_row_ranges(&self.rendered, &self.images, self.content_width);
        self.rendered_line_count = visual_line_count(self.rendered.clone(), self.content_width);
        self.raw_line_count = visual_line_count(self.source.as_str(), self.content_width);
        self.scroll = self
            .scroll
            .min(self.rendered_line_count.saturating_sub(1));
        self.status = Some(Status {
            text: format!("width {}", self.content_width),
            until: Instant::now() + STATUS_TTL,
            error: false,
        });
    }

    fn scroll_by(&mut self, delta: i32, viewport_height: u16) {
        let max = self
            .line_count()
            .saturating_sub(viewport_height.max(1).saturating_sub(1));
        let next = (self.scroll as i32).saturating_add(delta).clamp(0, max as i32);
        self.scroll = next as u16;
        self.hover = None;
    }

    fn open_image(&mut self, idx: usize) {
        let dest = self.images[idx].dest.clone();
        let (text, error) = self.launch_open(&dest);
        self.status = Some(Status {
            text,
            until: Instant::now() + STATUS_TTL,
            error,
        });
    }

    fn launch_open(&self, dest: &str) -> (String, bool) {
        let arg = if dest.starts_with("http://") || dest.starts_with("https://") {
            dest.to_string()
        } else {
            let p = Path::new(dest);
            let resolved = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.path.parent().unwrap_or(Path::new("")).join(p)
            };
            if !resolved.exists() {
                return (format!("not found: {dest}"), true);
            }
            resolved.display().to_string()
        };
        match process::Command::new("open").arg(&arg).spawn() {
            Ok(_) => (format!("opened {dest}"), false),
            Err(e) => (format!("open failed: {e}"), true),
        }
    }

    fn open_image_below(&mut self) {
        if self.mode != Mode::Rendered {
            self.status = Some(Status {
                text: "images open from rendered view (tab)".to_string(),
                until: Instant::now() + STATUS_TTL,
                error: false,
            });
            return;
        }
        let scroll = self.scroll as usize;
        match self.image_rows.iter().position(|&(_, end)| end > scroll) {
            Some(i) => self.open_image(i),
            None => {
                self.status = Some(Status {
                    text: "no image below".to_string(),
                    until: Instant::now() + STATUS_TTL,
                    error: false,
                });
            }
        }
    }

    fn image_at(&self, column: u16, row: u16, area: Rect) -> Option<usize> {
        if self.mode != Mode::Rendered || !area.contains(Position::new(column, row)) {
            return None;
        }
        let visual = self.scroll as usize + (row - area.y) as usize;
        self.image_rows
            .iter()
            .position(|&(start, end)| start <= visual && visual < end)
    }

    fn click(&mut self, column: u16, row: u16, area: Rect) {
        if let Some(i) = self.image_at(column, row, area) {
            self.open_image(i);
        }
    }

    fn yank_path(&mut self) {
        let path = self.path.display().to_string();
        let (text, error) = match arboard::Clipboard::new().and_then(|mut c| c.set_text(&path)) {
            Ok(()) => (format!("copied {path}"), false),
            Err(e) => (format!("clipboard error: {e}"), true),
        };
        self.status = Some(Status {
            text,
            until: Instant::now() + STATUS_TTL,
            error,
        });
    }

    fn current_status(&self) -> Option<&Status> {
        self.status
            .as_ref()
            .filter(|s| Instant::now() < s.until)
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("mdview: {err:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let path = parse_args()?;
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut app = App::new(path, source);

    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, &mut app);
    restore_terminal()?;
    result
}

/// Handles `--help`/`--version` before the terminal goes into raw mode; both
/// print and exit rather than returning, so `run` only ever sees a real path.
fn parse_args() -> Result<PathBuf> {
    let mut args = env::args_os().skip(1);
    let Some(arg) = args.next() else {
        bail!("usage: mdview <file.md>  (try --help)");
    };
    match arg.to_str() {
        Some("-h" | "-H" | "--help") => {
            print_help();
            process::exit(0);
        }
        Some("-V" | "-v" | "--version") => {
            println!("mdview {}", env!("CARGO_PKG_VERSION"));
            process::exit(0);
        }
        Some(other) if other.starts_with('-') => {
            bail!("unrecognized argument '{other}' (try --help)");
        }
        _ => {}
    }
    if args.next().is_some() {
        bail!("usage: mdview <file.md>  (try --help)");
    }
    Ok(PathBuf::from(arg))
}

fn print_help() {
    println!(
        "mdview {version}
A minimal terminal markdown reader.

USAGE:
    mdview <file.md>

OPTIONS:
    -h, --help       Print this help
    -V, --version    Print version

Press ? inside the app for keyboard shortcuts.",
        version = env!("CARGO_PKG_VERSION"),
    );
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original_hook(info);
    }));

    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("enter alternate screen")?;
    Terminal::new(CrosstermBackend::new(stdout)).context("create terminal")
}

fn restore_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    Ok(())
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut content_area = Rect::default();
    loop {
        terminal.draw(|frame| {
            content_area = draw(frame, app);
        })?;
        let viewport_height = content_area.height;

        if let Some(until) = app.current_status().map(|s| s.until) {
            if !event::poll(until.saturating_duration_since(Instant::now()))? {
                continue;
            }
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press && app.help => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?')
                ) {
                    app.help = false;
                }
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('?') => app.help = true,
                KeyCode::Tab => app.toggle_mode(),
                KeyCode::Char('y') => app.yank_path(),
                KeyCode::Char('o') => app.open_image_below(),
                KeyCode::Char('-') => app.adjust_width(-(WIDTH_STEP as i32)),
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    app.adjust_width(WIDTH_STEP as i32)
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    app.scroll_by(SCROLL_STEP as i32, viewport_height)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.scroll_by(-(SCROLL_STEP as i32), viewport_height)
                }
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    app.scroll_by(PAGE_STEP as i32, viewport_height)
                }
                KeyCode::PageUp => app.scroll_by(-(PAGE_STEP as i32), viewport_height),
                KeyCode::Home | KeyCode::Char('g') => app.scroll = 0,
                KeyCode::End | KeyCode::Char('G') => {
                    app.scroll_by(i32::MAX, viewport_height)
                }
                _ => {}
            },
            Event::Mouse(m) if !app.help => match m.kind {
                MouseEventKind::ScrollDown => {
                    app.scroll_by(3, viewport_height);
                }
                MouseEventKind::ScrollUp => {
                    app.scroll_by(-3, viewport_height);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    app.click(m.column, m.row, content_area);
                }
                MouseEventKind::Moved => {
                    app.hover = app.image_at(m.column, m.row, content_area);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn draw(frame: &mut ratatui::Frame, app: &App) -> Rect {
    let area = frame.area();

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            app.path.display().to_string(),
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ]);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FRAME_COLOR))
        .padding(Padding::vertical(1))
        .title(title);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let content_area = center_column(inner, app.content_width);

    match app.mode {
        Mode::Rendered => {
            let paragraph = Paragraph::new(app.rendered.clone())
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0));
            frame.render_widget(paragraph, content_area);
        }
        Mode::Raw => {
            let paragraph = Paragraph::new(app.source.as_str())
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0));
            frame.render_widget(paragraph, content_area);
        }
    }

    render_scroll_thumb(
        frame,
        Rect {
            x: area.right().saturating_sub(1),
            y: content_area.y,
            width: 1,
            height: content_area.height,
        },
        app.line_count() as usize,
        content_area.height as usize,
        app.scroll as usize,
    );
    render_footer(frame, app, area);
    if app.help {
        render_help(frame, area);
    }

    content_area
}

fn footer_hint(app: &App) -> Line<'static> {
    let key = Style::default()
        .fg(FRAME_COLOR)
        .add_modifier(Modifier::BOLD);
    let hint = Style::default().fg(FRAME_COLOR);
    if app.help {
        return Line::from(vec![
            Span::raw(" "),
            Span::styled("esc/q/?", key),
            Span::styled(" close ", hint),
        ]);
    }
    let mode_label = match app.mode {
        Mode::Rendered => "rendered",
        Mode::Raw => "raw",
    };
    let mut spans = vec![
        Span::raw(" "),
        Span::styled("tab", key),
        Span::styled(format!(" {mode_label}  "), hint),
        Span::styled("j/k", key),
        Span::styled(" scroll  ", hint),
        Span::styled("-/+", key),
        Span::styled(" width  ", hint),
        Span::styled("?", key),
        Span::styled(" keys  ", hint),
        Span::styled("q", key),
        Span::styled(" quit ", hint),
    ];
    if let Some(status) = app.current_status() {
        let color = if status.error { Color::Red } else { TITLE_COLOR };
        spans.push(Span::styled(
            format!(" • {} ", status.text),
            Style::default().fg(color),
        ));
    } else if let Some(i) = app.hover {
        spans.push(Span::styled(
            format!(" • click: {} ", app.images[i].dest),
            Style::default().fg(TITLE_COLOR),
        ));
    }
    Line::from(spans)
}

/// Draws the hints and the brand over the block's bottom border. Split into two
/// rects rather than a pair of block titles so a long status is clipped instead
/// of running under the brand.
fn render_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    if area.width < 4 || area.height < 1 {
        return;
    }
    let row = Rect {
        x: area.x + 1,
        y: area.bottom() - 1,
        width: area.width - 2,
        height: 1,
    };
    let [hints, brand] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(BRAND.chars().count() as u16),
    ])
    .areas(row);
    frame.render_widget(Paragraph::new(footer_hint(app)), hints);
    frame.render_widget(
        Paragraph::new(BRAND).style(
            Style::default()
                .fg(BRAND_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        brand,
    );
}

/// Hand-rolled rather than `ratatui::Scrollbar`, which rounds the thumb's start
/// and end independently and so visibly changes the thumb's length by a cell as
/// you scroll. Here the length is computed once and only the position moves.
fn render_scroll_thumb(
    frame: &mut ratatui::Frame,
    area: Rect,
    len: usize,
    viewport: usize,
    offset: usize,
) {
    let track = area.height as usize;
    if track == 0 || viewport == 0 || len <= viewport {
        return;
    }
    let thumb = (track * viewport / len).clamp(1, track);
    let travel = track - thumb;
    let max_offset = len - viewport;
    // Round to nearest so the thumb lands flush at the top and the bottom.
    let start = (offset.min(max_offset) * travel + max_offset / 2) / max_offset;

    let style = Style::default().fg(FRAME_COLOR);
    let buf = frame.buffer_mut();
    for i in start..(start + thumb).min(track) {
        // `cell_mut` returns None off-buffer; indexing would panic.
        if let Some(cell) = buf.cell_mut((area.x, area.y + i as u16)) {
            cell.set_symbol(SCROLL_THUMB).set_style(style);
        }
    }
}

fn help_lines(sections: &[(&str, &[(&str, &str)])]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (i, (title, items)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!(" {title}"),
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in *items {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key:<12}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled((*desc).to_string(), Style::default().fg(FRAME_COLOR)),
            ]));
        }
    }
    lines
}

fn render_help(frame: &mut ratatui::Frame, area: Rect) {
    let left = help_lines(&[
        (
            "Scrolling",
            &[
                ("j/k, ↑/↓", "scroll a line"),
                ("space, PgDn", "page down"),
                ("PgUp", "page up"),
                ("g, Home", "jump to top"),
                ("G, End", "jump to bottom"),
                ("wheel", "scroll"),
            ],
        ),
        (
            "View",
            &[("tab", "rendered ↔ raw"), ("- / +", "column width")],
        ),
    ]);
    let right = help_lines(&[
        (
            "Images",
            &[("o", "open first below"), ("click", "open under cursor")],
        ),
        (
            "Other",
            &[
                ("y", "copy file path"),
                ("?", "toggle this help"),
                ("q, esc", "quit"),
            ],
        ),
    ]);

    let height = (left.len().max(right.len()) as u16 + 2).min(area.height);
    let width = 68u16.min(area.width);
    let dialog = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(TITLE_COLOR))
        .title(Span::styled(
            " keys ",
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(inner);
    frame.render_widget(Paragraph::new(left), l);
    frame.render_widget(Paragraph::new(right), r);
}

fn visual_line_count<'a>(text: impl Into<Text<'a>>, width: u16) -> u16 {
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .line_count(width.max(1))
        .min(u16::MAX as usize) as u16
}

// ratatui wraps each Line independently, so per-line counts sum to exact visual offsets
fn image_row_ranges(
    text: &Text<'static>,
    images: &[render::ImageRef],
    width: u16,
) -> Vec<(usize, usize)> {
    if images.is_empty() {
        return vec![];
    }
    let mut offsets = Vec::with_capacity(text.lines.len() + 1);
    let mut acc = 0usize;
    offsets.push(0);
    for line in &text.lines {
        acc += visual_line_count(line.clone(), width) as usize;
        offsets.push(acc);
    }
    images
        .iter()
        .map(|img| {
            let start = offsets.get(img.lines.0).copied().unwrap_or(acc);
            let end = offsets
                .get(img.lines.1)
                .copied()
                .unwrap_or(acc)
                .max(start + 1);
            (start, end)
        })
        .collect()
}

fn center_column(area: Rect, width: u16) -> Rect {
    if area.width <= width {
        return area;
    }
    let side = (area.width - width) / 2;
    let [_, mid, _] = Layout::horizontal([
        Constraint::Length(side),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .areas(area);
    mid
}

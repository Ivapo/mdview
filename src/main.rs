use std::{
    env, fs,
    io::{self, Stdout},
    panic,
    path::PathBuf,
    process,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

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
    content_width: u16,
    mode: Mode,
    scroll: u16,
    raw_line_count: u16,
    rendered_line_count: u16,
    status: Option<Status>,
}

impl App {
    fn new(path: PathBuf, source: String) -> Self {
        let term_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
        let content_width = DEFAULT_CONTENT_WIDTH
            .min(term_width.saturating_sub(SIDE_MARGIN))
            .max(20);
        let raw_line_count = visual_line_count(source.as_str(), content_width);
        let rendered = render::render(&source, content_width);
        let rendered_line_count = visual_line_count(rendered.clone(), content_width);
        Self {
            path,
            source,
            rendered,
            content_width,
            mode: Mode::Rendered,
            scroll: 0,
            raw_line_count,
            rendered_line_count,
            status: None,
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Rendered => Mode::Raw,
            Mode::Raw => Mode::Rendered,
        };
        let total = match self.mode {
            Mode::Rendered => self.rendered_line_count,
            Mode::Raw => self.raw_line_count,
        };
        self.scroll = self.scroll.min(total.saturating_sub(1));
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
        self.rendered = render::render(&self.source, self.content_width);
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
        let total = match self.mode {
            Mode::Rendered => self.rendered_line_count,
            Mode::Raw => self.raw_line_count,
        };
        let max = total.saturating_sub(viewport_height.max(1).saturating_sub(1));
        let next = (self.scroll as i32 + delta).clamp(0, max as i32);
        self.scroll = next as u16;
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

fn parse_args() -> Result<PathBuf> {
    let mut args = env::args_os().skip(1);
    let Some(arg) = args.next() else {
        bail!("usage: mdview <file.md>");
    };
    if args.next().is_some() {
        bail!("usage: mdview <file.md>");
    }
    Ok(PathBuf::from(arg))
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
    let mut viewport_height: u16 = 1;
    loop {
        terminal.draw(|frame| {
            viewport_height = draw(frame, app);
        })?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Tab => app.toggle_mode(),
                KeyCode::Char('y') => app.yank_path(),
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
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollDown => {
                    app.scroll_by(3, viewport_height);
                }
                MouseEventKind::ScrollUp => {
                    app.scroll_by(-3, viewport_height);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn draw(frame: &mut ratatui::Frame, app: &App) -> u16 {
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
    let mode_label = match app.mode {
        Mode::Rendered => "rendered",
        Mode::Raw => "raw",
    };
    let key = Style::default()
        .fg(FRAME_COLOR)
        .add_modifier(Modifier::BOLD);
    let hint = Style::default().fg(FRAME_COLOR);
    let mut bottom_spans = vec![
        Span::raw(" "),
        Span::styled("tab", key),
        Span::styled(format!(" {mode_label}  "), hint),
        Span::styled("j/k", key),
        Span::styled(" scroll  ", hint),
        Span::styled("-/+", key),
        Span::styled(" width  ", hint),
        Span::styled("y", key),
        Span::styled(" copy path  ", hint),
        Span::styled("q", key),
        Span::styled(" quit ", hint),
    ];
    if let Some(status) = app.current_status() {
        let color = if status.error { Color::Red } else { TITLE_COLOR };
        bottom_spans.push(Span::styled(
            format!(" • {} ", status.text),
            Style::default().fg(color),
        ));
    }
    let bottom = Line::from(bottom_spans);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FRAME_COLOR))
        .padding(Padding::vertical(1))
        .title(title)
        .title_bottom(bottom);
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

    content_area.height
}

fn visual_line_count<'a>(text: impl Into<Text<'a>>, width: u16) -> u16 {
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .line_count(width.max(1))
        .min(u16::MAX as usize) as u16
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

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SynStyle, Theme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use unicode_width::UnicodeWidthStr;

const CODE_BG: Color = Color::Rgb(40, 42, 54);
const INLINE_CODE_BG: Color = Color::Rgb(60, 60, 70);
const INLINE_CODE_FG: Color = Color::Rgb(245, 245, 245);
const QUOTE_COLOR: Color = Color::Rgb(150, 150, 150);
const LINK_COLOR: Color = Color::Rgb(120, 170, 255);
const RULE_COLOR: Color = Color::DarkGray;
const CODE_LANG_COLOR: Color = Color::Rgb(180, 180, 100);
const SYNTECT_THEME: &str = "base16-ocean.dark";

pub fn render(source: &str, width: u16) -> Text<'static> {
    let parser = Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_SMART_PUNCTUATION,
    );
    let mut r = Renderer::new(width);
    for ev in parser {
        r.event(ev);
    }
    r.finish()
}

struct Renderer {
    width: u16,
    lines: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,
    style: Style,
    style_stack: Vec<Style>,
    lists: Vec<Option<u64>>,
    quote_depth: u16,
    code: Option<CodeCtx>,
    table: Option<TableCtx>,
    syntax: SyntaxSet,
    theme: Theme,
}

struct CodeCtx {
    lang: String,
    body: String,
}

struct TableCtx {
    _alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<Span<'static>>>>,
    cur_row: Vec<Vec<Span<'static>>>,
    cur_cell: Vec<Span<'static>>,
}

impl Renderer {
    fn new(width: u16) -> Self {
        let syntax = SyntaxSet::load_defaults_newlines();
        let theme = ThemeSet::load_defaults().themes[SYNTECT_THEME].clone();
        Self {
            width,
            lines: vec![],
            cur: vec![],
            style: Style::default(),
            style_stack: vec![],
            lists: vec![],
            quote_depth: 0,
            code: None,
            table: None,
            syntax,
            theme,
        }
    }

    fn finish(mut self) -> Text<'static> {
        if !self.cur.is_empty() {
            self.flush_line();
        }
        Text::from(self.lines)
    }

    fn push_style(&mut self, add: Style) {
        self.style_stack.push(self.style);
        self.style = self.style.patch(add);
    }

    fn pop_style(&mut self) {
        if let Some(s) = self.style_stack.pop() {
            self.style = s;
        }
    }

    fn push_span(&mut self, span: Span<'static>) {
        if let Some(t) = self.table.as_mut() {
            t.cur_cell.push(span);
            return;
        }
        if self.cur.is_empty() && self.quote_depth > 0 {
            let prefix = "▎ ".repeat(self.quote_depth as usize);
            self.cur
                .push(Span::styled(prefix, Style::default().fg(QUOTE_COLOR)));
        }
        self.cur.push(span);
    }

    fn push_text(&mut self, t: &str) {
        let span = Span::styled(t.to_string(), self.style);
        self.push_span(span);
    }

    fn flush_line(&mut self) {
        let spans = std::mem::take(&mut self.cur);
        self.lines.push(Line::from(spans));
    }

    fn ensure_line_break(&mut self) {
        if !self.cur.is_empty() {
            self.flush_line();
        }
    }

    fn blank_line(&mut self) {
        self.ensure_line_break();
        let last_blank = self
            .lines
            .last()
            .map(|l| l.spans.iter().all(|s| s.content.trim().is_empty()))
            .unwrap_or(true);
        if last_blank {
            return;
        }
        self.lines.push(Line::default());
    }

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(&t),
            Event::Code(c) => self.inline_code(&c),
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::FootnoteReference(name) => {
                self.push_text(&format!("[^{name}]"));
            }
            Event::SoftBreak => {
                if self.code.is_some() {
                    self.text("\n");
                } else {
                    self.push_text(" ");
                }
            }
            Event::HardBreak => {
                if self.code.is_some() {
                    self.text("\n");
                } else {
                    self.flush_line();
                }
            }
            Event::Rule => {
                self.ensure_line_break();
                let rule = "─".repeat(self.width as usize);
                self.lines.push(Line::from(Span::styled(
                    rule,
                    Style::default().fg(RULE_COLOR),
                )));
            }
            Event::TaskListMarker(b) => {
                let mark = if b { "[x] " } else { "[ ] " };
                self.push_text(mark);
            }
            _ => {}
        }
    }

    fn text(&mut self, t: &str) {
        if let Some(c) = self.code.as_mut() {
            c.body.push_str(t);
            return;
        }
        self.push_text(t);
    }

    fn inline_code(&mut self, c: &str) {
        let style = Style::default().bg(INLINE_CODE_BG).fg(INLINE_CODE_FG);
        self.push_span(Span::styled(format!(" {c} "), style));
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.ensure_line_break(),
            Tag::Heading { level, .. } => {
                self.blank_line();
                self.push_style(heading_style(level));
            }
            Tag::BlockQuote(_) => {
                self.blank_line();
                self.quote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.ensure_line_break();
                self.blank_line();
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some(CodeCtx {
                    lang,
                    body: String::new(),
                });
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    self.ensure_line_break();
                }
                self.lists.push(start);
            }
            Tag::Item => {
                self.ensure_line_break();
                let depth = self.lists.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{indent}{n}. ");
                        *n += 1;
                        m
                    }
                    _ => format!("{indent}• "),
                };
                self.cur
                    .push(Span::styled(marker, Style::default().fg(Color::Cyan)));
            }
            Tag::Emphasis => {
                self.push_style(Style::default().add_modifier(Modifier::ITALIC))
            }
            Tag::Strong => self.push_style(Style::default().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => {
                self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT))
            }
            Tag::Link { .. } => self.push_style(
                Style::default()
                    .fg(LINK_COLOR)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Tag::Image { dest_url, .. } => {
                self.push_text(&format!("[image: {dest_url}]"));
                self.push_style(Style::default().fg(Color::DarkGray));
            }
            Tag::Table(aligns) => {
                self.ensure_line_break();
                self.blank_line();
                self.table = Some(TableCtx {
                    _alignments: aligns,
                    rows: vec![],
                    cur_row: vec![],
                    cur_cell: vec![],
                });
            }
            Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.ensure_line_break();
                self.blank_line();
            }
            TagEnd::Heading(level) => {
                let text_width: usize = self
                    .cur
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                self.pop_style();
                self.ensure_line_break();
                let bar_width = text_width.min(self.width as usize);
                if bar_width > 0 {
                    let (ch, color) = match level {
                        HeadingLevel::H1 => ("═", Color::Magenta),
                        HeadingLevel::H2 => ("─", Color::Cyan),
                        _ => ("", Color::Reset),
                    };
                    if !ch.is_empty() {
                        let bar = ch.repeat(bar_width);
                        self.lines
                            .push(Line::from(Span::styled(bar, Style::default().fg(color))));
                    }
                }
                self.blank_line();
            }
            TagEnd::BlockQuote(_) => {
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.ensure_line_break();
                self.blank_line();
            }
            TagEnd::CodeBlock => {
                if let Some(ctx) = self.code.take() {
                    self.emit_code_block(&ctx);
                }
                self.blank_line();
            }
            TagEnd::List(_) => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.ensure_line_break();
                    self.blank_line();
                }
            }
            TagEnd::Item => self.ensure_line_break(),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Link => self.pop_style(),
            TagEnd::Image => self.pop_style(),
            TagEnd::Table => {
                if let Some(ctx) = self.table.take() {
                    self.emit_table(&ctx);
                }
                self.blank_line();
            }
            TagEnd::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    let row = std::mem::take(&mut t.cur_row);
                    if !row.is_empty() {
                        t.rows.push(row);
                    }
                }
            }
            TagEnd::TableRow => {
                if let Some(t) = self.table.as_mut() {
                    let row = std::mem::take(&mut t.cur_row);
                    if !row.is_empty() {
                        t.rows.push(row);
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(t) = self.table.as_mut() {
                    let cell = std::mem::take(&mut t.cur_cell);
                    t.cur_row.push(cell);
                }
            }
            _ => {}
        }
    }

    fn emit_code_block(&mut self, ctx: &CodeCtx) {
        let syntax = if ctx.lang.is_empty() {
            self.syntax.find_syntax_plain_text()
        } else {
            self.syntax
                .find_syntax_by_token(&ctx.lang)
                .unwrap_or_else(|| self.syntax.find_syntax_plain_text())
        };
        let mut hl = HighlightLines::new(syntax, &self.theme);
        let width = self.width as usize;

        if !ctx.lang.is_empty() {
            let label = format!(" {} ", ctx.lang);
            let pad = width.saturating_sub(UnicodeWidthStr::width(label.as_str()));
            self.lines.push(Line::from(vec![
                Span::styled(
                    label,
                    Style::default()
                        .bg(CODE_BG)
                        .fg(CODE_LANG_COLOR)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ".repeat(pad), Style::default().bg(CODE_BG)),
            ]));
        }

        let body = if ctx.body.is_empty() { "\n" } else { &ctx.body };
        for raw in LinesWithEndings::from(body) {
            let line = raw.trim_end_matches('\n');
            let ranges = hl
                .highlight_line(line, &self.syntax)
                .unwrap_or_else(|_| vec![(SynStyle::default(), line)]);
            let mut spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(s, t)| {
                    Span::styled(t.to_string(), syn_to_rt_style(s).bg(CODE_BG))
                })
                .collect();
            let content_w: usize = spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            if content_w < width {
                spans.push(Span::styled(
                    " ".repeat(width - content_w),
                    Style::default().bg(CODE_BG),
                ));
            }
            self.lines.push(Line::from(spans));
        }
    }

    fn emit_table(&mut self, ctx: &TableCtx) {
        if ctx.rows.is_empty() {
            return;
        }
        let cols = ctx.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if cols == 0 {
            return;
        }

        let mut col_w: Vec<usize> = vec![0; cols];
        for row in &ctx.rows {
            for (i, cell) in row.iter().enumerate() {
                let w = cell_width(cell);
                col_w[i] = col_w[i].max(w);
            }
        }

        let max_total = self.width as usize;
        let frame_overhead = cols + 1 + cols * 2;
        let avail = max_total.saturating_sub(frame_overhead);
        let total_content: usize = col_w.iter().sum();
        if total_content > avail && total_content > 0 {
            let ratio = avail as f64 / total_content as f64;
            for w in col_w.iter_mut() {
                *w = ((*w as f64) * ratio).floor().max(3.0) as usize;
            }
        }

        let border = Style::default().fg(RULE_COLOR);
        self.lines
            .push(make_table_border("┌", "┬", "┐", "─", &col_w, border));

        for (ri, row) in ctx.rows.iter().enumerate() {
            let mut spans: Vec<Span<'static>> =
                vec![Span::styled("│".to_string(), border)];
            for ci in 0..cols {
                let empty = vec![];
                let cell = row.get(ci).unwrap_or(&empty);
                let target = col_w[ci];
                spans.push(Span::raw(" ".to_string()));
                let cell_w = cell_width(cell);
                if cell_w <= target {
                    for s in cell {
                        spans.push(s.clone());
                    }
                    spans.push(Span::raw(" ".repeat(target - cell_w + 1)));
                } else {
                    let truncated = truncate_spans(cell, target.saturating_sub(1));
                    spans.extend(truncated);
                    spans.push(Span::raw("…".to_string()));
                    spans.push(Span::raw(" ".to_string()));
                }
                spans.push(Span::styled("│".to_string(), border));
            }
            // header style: bold
            if ri == 0 {
                for s in spans.iter_mut() {
                    if s.style.fg.is_none() && s.style.bg.is_none() {
                        s.style = s.style.add_modifier(Modifier::BOLD);
                    } else {
                        s.style = s.style.add_modifier(Modifier::BOLD);
                    }
                }
            }
            self.lines.push(Line::from(spans));

            if ri == 0 {
                self.lines
                    .push(make_table_border("├", "┼", "┤", "─", &col_w, border));
            }
        }

        self.lines
            .push(make_table_border("└", "┴", "┘", "─", &col_w, border));
    }
}

fn heading_style(level: HeadingLevel) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        HeadingLevel::H1 => base.fg(Color::Magenta),
        HeadingLevel::H2 => base.fg(Color::Cyan),
        HeadingLevel::H3 => base.fg(Color::Yellow),
        HeadingLevel::H4 => base.fg(Color::Green),
        HeadingLevel::H5 => base.fg(Color::Blue),
        HeadingLevel::H6 => base.fg(Color::Gray),
    }
}

fn syn_to_rt_style(s: SynStyle) -> Style {
    Style::default().fg(Color::Rgb(
        s.foreground.r,
        s.foreground.g,
        s.foreground.b,
    ))
}

fn cell_width(cell: &[Span<'static>]) -> usize {
    cell.iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

fn truncate_spans(cell: &[Span<'static>], target: usize) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = vec![];
    let mut taken = 0usize;
    for s in cell {
        let w = UnicodeWidthStr::width(s.content.as_ref());
        if taken + w <= target {
            out.push(s.clone());
            taken += w;
        } else {
            let remaining = target.saturating_sub(taken);
            let mut acc = String::new();
            let mut acc_w = 0usize;
            for ch in s.content.chars() {
                let cw = UnicodeWidthStr::width(ch.to_string().as_str());
                if acc_w + cw > remaining {
                    break;
                }
                acc.push(ch);
                acc_w += cw;
            }
            if !acc.is_empty() {
                out.push(Span::styled(acc, s.style));
            }
            break;
        }
    }
    out
}

fn make_table_border(
    left: &str,
    mid: &str,
    right: &str,
    fill: &str,
    col_w: &[usize],
    style: Style,
) -> Line<'static> {
    let mut s = String::from(left);
    for (i, w) in col_w.iter().enumerate() {
        for _ in 0..w + 2 {
            s.push_str(fill);
        }
        if i + 1 < col_w.len() {
            s.push_str(mid);
        }
    }
    s.push_str(right);
    Line::from(Span::styled(s, style))
}

use std::{error::Error, io, vec};

use chrono::FixedOffset;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};

use crate::client::{Client, Mail};

struct App {
    state: TableState,
    client: Client,
    subject_query: String,
    start_datetime: chrono::DateTime<FixedOffset>,
    end_datetime: chrono::DateTime<FixedOffset>,
    regex: bool,
    reserve: bool,
    mail_box: String,
    mails: Vec<Mail>,
}

impl App {
    pub fn new(
        client: Client,
        subject_query: String,
        start_datetime: chrono::DateTime<FixedOffset>,
        end_datetime: chrono::DateTime<FixedOffset>,
        regex: bool,
        reserve: bool,
        mail_box: String,
    ) -> App {
        App {
            state: TableState::default(),
            client,
            subject_query,
            start_datetime,
            end_datetime,
            regex,
            reserve,
            mail_box,
            mails: vec![],
        }
    }

    pub fn refresh(&mut self) {
        let mail_box = self.client.get(&self.mail_box).unwrap();
        self.mails = mail_box
            .filter(&self.subject_query, self.start_datetime)
            .end_date(self.end_datetime)
            .regex(self.regex)
            .reverse(self.reserve)
            .fetch();
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.mails.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.mails.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

pub fn run(
    client: Client,
    subject_query: String,
    start_datetime: chrono::DateTime<FixedOffset>,
    end_datetime: chrono::DateTime<FixedOffset>,
    regex: bool,
    reserve: bool,
    mail_box: String,
) -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut app = App::new(
        client,
        subject_query,
        start_datetime,
        end_datetime,
        regex,
        reserve,
        mail_box,
    );
    app.refresh();
    let res = run_app(&mut terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('r') => app.refresh(),
                KeyCode::Down => app.next(),
                KeyCode::Up => app.previous(),
                _ => {}
            }
        }
    }
}

fn draw_footer<B: Backend>(f: &mut Frame<B>, area: Rect) {
    let text = vec![Spans::from(vec![
        Span::raw("  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(": quit"),
        Span::raw("  "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(": refresh"),
    ])];
    let paragraph = Paragraph::new(text).style(Style::default().bg(Color::DarkGray));
    f.render_widget(paragraph, area);
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(f.size());

    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default().bg(Color::Blue);
    let header_cells = ["id", "Subject", "From", "To", "CC", "Date", "Attachments"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Red)));
    let header = Row::new(header_cells)
        .style(normal_style)
        .height(1)
        .bottom_margin(1);
    let rows = app.mails.iter().map(|item| {
        let mail_fields = [
            item.uid.to_string(),
            item.subject.to_string(),
            item.from.to_string(),
            item.to.join("\n"),
            item.cc.join("\n"),
            item.internal_date.format("%Y-%m-%dT%H:%M:%S").to_string(),
            item.attachments
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<String>>()
                .join("\n"),
        ];

        let height = mail_fields
            .iter()
            .map(|content| content.chars().filter(|c| *c == '\n').count())
            .max()
            .unwrap_or(0)
            + 1;
        let cells = mail_fields.iter().enumerate().map(|(idx, c)| {
            let style = match idx {
                0 => Style::default().fg(Color::DarkGray),
                _ => Style::default(),
            };
            Cell::from(c.clone()).style(style)
        });
        Row::new(cells).height(height as u16).bottom_margin(1)
    });
    let t = Table::new(rows)
        .header(header)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Mails",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .highlight_style(selected_style)
        .highlight_symbol(">> ")
        .widths(&[
            Constraint::Length(5),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(10),
            Constraint::Length(20),
            Constraint::Percentage(20),
        ]);

    f.render_stateful_widget(t, rects[0], &mut app.state);
    draw_footer(f, rects[1]);
}

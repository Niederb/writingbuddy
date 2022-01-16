use chrono::Utc;
use config::Config;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::cmp::max;
use std::error::Error;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

enum InputMode {
    Title,
    Writing,
}

/// App holds the state of the application
struct App {
    /// Current value of the title box
    title: String,
    /// Current value of the text box
    text: String,
    /// Current input mode
    input_mode: InputMode,
}

impl App {
    fn new(title: String) -> App {
        App {
            title,
            text: String::default(),
            input_mode: InputMode::Title,
        }
    }
}

fn get_text_position(text: &str) -> (u16, u16) {
    let last_line = text.lines().last().unwrap_or_default();
    let last_line_offset = if text.ends_with('\n') { 1 } else { 0 };
    let line_position = if last_line_offset == 1 {
        0
    } else {
        last_line.chars().count() as u16
    };
    (
        line_position,
        max(1, text.lines().count() + last_line_offset) as u16,
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut settings = Config::default();
    settings.merge(config::File::with_name("config")).unwrap();

    let now = Utc::now();
    let title_format_str = settings
        .get_str("title_string")
        .unwrap_or_else(|_| "%Y-%m-%d".to_string());
    let title = now.format(&title_format_str);
    let file_format_str = settings
        .get_str("file_string")
        .unwrap_or_else(|_| "%Y-%m.md".to_string());
    let filename = now.format(&file_format_str);

    let mut app = App::new(title.to_string());
    let res = run_app(&mut terminal, &mut app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if !app.text.is_empty() {
        println!("Storing text into: {}", &filename);
        let mut output = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&filename.to_string())
            .unwrap();
        writeln!(output, "## {}", app.title)?;
        writeln!(output, "{}", app.text)?;
        writeln!(output)?;
    }

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Title => match key.code {
                    KeyCode::Enter => {
                        app.input_mode = InputMode::Writing;
                    }
                    KeyCode::Esc => {
                        return Ok(());
                    }
                    KeyCode::Char(c) => {
                        app.title.push(c);
                    }
                    KeyCode::Backspace => {
                        app.title.pop();
                    }
                    _ => {}
                },
                InputMode::Writing => match key.code {
                    KeyCode::Enter => app.text.push('\n'),
                    KeyCode::Char(c) => {
                        app.text.push(c);
                    }
                    KeyCode::Backspace => {
                        app.text.pop();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Title;
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    let (msg, style) = match app.input_mode {
        InputMode::Title => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start writing."),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Writing => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to stop writing"),
            ],
            Style::default(),
        ),
    };
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[0]);

    let title = Paragraph::new(app.title.clone())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Title"));
    f.render_widget(title, chunks[1]);

    let terminal_width = f.size().width as usize - 6;
    let mut wrapped_text = textwrap::fill(&app.text, terminal_width);

    let trailing_whitespace = &app.text[app.text.trim_end_matches(' ').len()..];
    if !trailing_whitespace.is_empty() {
        // Trailing whitespace is discarded by
        // `textwrap::wrap`. We reinsert it here. If multiple
        // spaces are added, this can overflow the margins
        // which look a bit odd. Handling this would require
        // some more tinkering...
        wrapped_text = format!("{}{}", wrapped_text, trailing_whitespace);
    }
    match app.input_mode {
        InputMode::Title => {
            f.set_cursor(
                // Put cursor past the end of the title text
                chunks[1].x + app.title.chars().count() as u16 + 1,
                // Move one line down, from the border to the title line
                chunks[1].y + 1,
            )
        }
        InputMode::Writing => {
            let text_position = get_text_position(&wrapped_text);
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[2].x + 1 + text_position.0,
                // Move one line down, from the border to the input line
                chunks[2].y + text_position.1,
            )
        }
    }

    let messages = Paragraph::new(wrapped_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Title"));
    f.render_widget(messages, chunks[2]);

    let word_count = app.text.split_whitespace().count();
    let stats = Paragraph::new(format!("Word count: {word_count}"))
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Statistics"))
        .wrap(Wrap { trim: true });
    f.render_widget(stats, chunks[3]);
}

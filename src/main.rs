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
    Normal,
    Editing,
}

/// App holds the state of the application
struct App {
    title: String,
    /// Current value of the input box
    text: String,
    /// Current input mode
    input_mode: InputMode,
}

impl App {
    fn new(title: String) -> App {
        App {
            title,
            text: String::default(),
            input_mode: InputMode::Normal,
        }
    }
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

    // create app and run it
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
                InputMode::Normal => match key.code {
                    KeyCode::Char('e') => {
                        app.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        //app.messages.push(app.input.drain(..).collect());
                        app.text.push('\n')
                    }
                    KeyCode::Char(c) => {
                        app.text.push(c);
                    }
                    KeyCode::Backspace => {
                        app.text.pop();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
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
        InputMode::Normal => (
            vec![
                Span::raw("Press "),
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start editing."),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Editing => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to stop editing, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to record the message"),
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
    match app.input_mode {
        InputMode::Normal =>
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            {}

        InputMode::Editing => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[2].x
                    + app.text.lines().last().unwrap_or_default().chars().count() as u16
                    + 1,
                // Move one line down, from the border to the input line
                chunks[2].y + max(1, app.text.lines().count() as u16),
            )
        }
    }

    let messages = Paragraph::new(app.text.as_ref())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Title"))
        .wrap(Wrap { trim: true });
    f.render_widget(messages, chunks[2]);

    let word_count = app.text.split_whitespace().count();
    let stats = Paragraph::new(format!("Word count: {word_count}"))
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Statistics"))
        .wrap(Wrap { trim: true });
    f.render_widget(stats, chunks[3]);
}

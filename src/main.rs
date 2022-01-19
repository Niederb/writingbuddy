use chrono::Utc;
use config::Config;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::cmp::max;
use std::error::Error;
use std::time::Duration;
use stopwatch::Stopwatch;
use structopt::StructOpt;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

/// A basic example
#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct CliConfig {
    /// Path to config file. Can be a JSON, TOML, YAML, HJSON or INI file.
    #[structopt(short, long)]
    config_file: Option<String>,
}

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
    /// Whether the backspace key works while writing
    backspace_active: bool,

    writing_time: Stopwatch,

    time_goal: Option<i64>,

    word_goal: Option<i64>,

    strict_mode: bool,
}

impl App {
    fn new(
        title: String,
        time_goal: Option<i64>,
        word_goal: Option<i64>,
        backspace_active: bool,
        strict_mode: bool,
    ) -> App {
        App {
            title,
            text: String::default(),
            input_mode: InputMode::Title,
            backspace_active,
            time_goal,
            word_goal,
            writing_time: Stopwatch::new(),
            strict_mode,
        }
    }

    fn get_word_count_string(&self) -> String {
        let word_count = self.text.split_whitespace().count();
        match self.word_goal {
            Some(word_goal) => format!("{word_count}/{word_goal}"),
            None => format!("{word_count}"),
        }
    }

    fn get_time_string(&self) -> String {
        let duration = self.writing_time.elapsed().as_secs();
        match self.time_goal {
            Some(time_goal) => format!("{duration} s/{time_goal} s"),
            None => format!("{duration} s"),
        }
    }

    fn get_time_color(&self) -> Color {
        let duration = self.writing_time.elapsed().as_secs();
        match self.time_goal {
            Some(i) => {
                if duration as i64 >= i {
                    Color::Green
                } else {
                    Color::Yellow
                }
            }
            None => Color::DarkGray,
        }
    }

    fn get_word_count_color(&self) -> Color {
        let word_count = self.text.split_whitespace().count();
        match self.word_goal {
            Some(i) => {
                if word_count as i64 >= i {
                    Color::Green
                } else {
                    Color::Yellow
                }
            }
            None => Color::DarkGray,
        }
    }

    fn achieved_goals(&self) -> bool {
        let word_goal_achieved = match self.word_goal {
            Some(i) => self.text.split_whitespace().count() as i64 >= i,
            None => true,
        };
        let time_goal_achieved = match self.time_goal {
            Some(i) => self.writing_time.elapsed().as_secs() as i64 >= i,
            None => true,
        };
        word_goal_achieved && time_goal_achieved
    }

    fn get_title(&self) -> Vec<Span> {
        match self.input_mode {
            InputMode::Title => vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start the writing session."),
            ],
            InputMode::Writing => {
                if self.strict_mode && !self.achieved_goals() {
                    vec![Span::raw(
                        "Keep writing until you achieve your writing goal! ",
                    )]
                } else {
                    vec![
                        Span::raw("Press "),
                        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" to stop writing"),
                    ]
                }
            }
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
    let cli_config = CliConfig::from_args();
    let config_file = cli_config
        .config_file
        .unwrap_or_else(|| "config".to_string());
    println!("Config file: {:#?}", config_file);

    let mut settings = Config::default();
    if let Err(i) = settings.merge(config::File::with_name(&config_file)) {
        println!("Failed loading config file {:?}!", i);
        std::process::exit(1);
    }

    let now = Utc::now();
    let title_format_str = settings
        .get_str("title_string")
        .unwrap_or_else(|_| "%Y-%m-%d".to_string());
    let title = now.format(&title_format_str);
    let file_format_str = settings
        .get_str("file_string")
        .unwrap_or_else(|_| "%Y-%m.md".to_string());
    let filename = now.format(&file_format_str);
    let backspace_active = settings.get_bool("backspace_active").unwrap_or(true);
    let mut time_goal = settings.get_int("time_goal").ok();
    if let Some(0) = time_goal {
        time_goal = None;
    }
    let mut word_goal = settings.get_int("word_goal").ok();
    if let Some(0) = word_goal {
        word_goal = None;
    }
    let strict_mode = settings.get_bool("strict_mode").unwrap_or(true);

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(
        title.to_string(),
        time_goal,
        word_goal,
        backspace_active,
        strict_mode,
    );
    let res = run_app(&mut terminal, &mut app);

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
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

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match app.input_mode {
                    InputMode::Title => match key.code {
                        KeyCode::Enter => {
                            app.writing_time.start();
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
                            if app.backspace_active {
                                app.text.pop();
                            }
                        }
                        KeyCode::Esc => {
                            if app.achieved_goals() || !app.strict_mode {
                                app.writing_time.stop();
                                app.input_mode = InputMode::Title;
                            }
                        }
                        _ => {}
                    },
                }
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

    let text = Text::from(Spans::from(app.get_title()));
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[0]);

    let widget_colors = match app.input_mode {
        InputMode::Title => (Color::Yellow, Color::DarkGray),
        InputMode::Writing => (Color::DarkGray, Color::Yellow),
    };

    let title = Paragraph::new(app.title.clone())
        .style(Style::default().fg(widget_colors.0))
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

    let text = Paragraph::new(wrapped_text)
        .style(Style::default().fg(widget_colors.1))
        .block(Block::default().borders(Borders::ALL).title("Text"));
    f.render_widget(text, chunks[2]);

    let stat_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[3]);
    {
        let stats = Paragraph::new(app.get_word_count_string())
            .style(Style::default().fg(app.get_word_count_color()))
            .block(Block::default().borders(Borders::ALL).title("Word count"))
            .wrap(Wrap { trim: true });
        f.render_widget(stats, stat_chunks[0]);

        let stats = Paragraph::new(app.get_time_string())
            .style(Style::default().fg(app.get_time_color()))
            .block(Block::default().borders(Borders::ALL).title("Time"))
            .wrap(Wrap { trim: true });
        f.render_widget(stats, stat_chunks[1]);
    }
}

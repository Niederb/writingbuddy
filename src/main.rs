use chrono::Utc;
use config::Config;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fluent::{FluentBundle, FluentResource};
use fluent_langneg::negotiate_languages;
use fluent_langneg::NegotiationStrategy;
use std::cmp::max;
use std::collections::HashMap;
use std::error::Error;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};
use stopwatch::Stopwatch;
use structopt::StructOpt;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use unic_langid::langid;
use unic_langid::langids;
use unic_langid::LanguageIdentifier;

const ACTIVE_COLOR: Color = Color::Cyan;
const DONE_COLOR: Color = Color::Green;
const WARNING_COLOR: Color = Color::Yellow;
const DANGER_COLOR: Color = Color::Red;
const PASSIVE_COLOR: Color = Color::Gray;

/// `writingbuddy` is a tool to support your writing without distractions
#[derive(StructOpt, Debug)]
#[structopt(name = "writingbuddy")]
struct CliConfig {
    /// Path to config file. Can be a JSON, TOML, YAML, HJSON or INI file.
    /// If nothing is specified a file named `writingbuddy` will be searched for.
    /// If no config file is found default values will be used.
    #[structopt(short, long)]
    config_file: Option<String>,

    /// Create a new config file named writingbuddy.toml in case no config is found.
    /// This can be used in combination with the `config_file` option in which case
    /// the file will be created at the specified location if it does not exist.
    /// It can also be used without `config_file` in which case a config file will be
    /// created in the current directory if it does not exist.
    #[structopt(short, long)]
    initialize_config: bool,
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

    last_keystroke: Option<Instant>,

    keystroke_timeout: Option<i64>,
}

impl App {
    fn new(
        title: String,
        time_goal: Option<i64>,
        word_goal: Option<i64>,
        backspace_active: bool,
        strict_mode: bool,
        keystroke_timeout: Option<i64>,
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
            last_keystroke: None,
            keystroke_timeout,
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
                    DONE_COLOR
                } else {
                    ACTIVE_COLOR
                }
            }
            None => PASSIVE_COLOR,
        }
    }

    fn get_word_count_color(&self) -> Color {
        let word_count = self.text.split_whitespace().count();
        match self.word_goal {
            Some(i) => {
                if word_count as i64 >= i {
                    DONE_COLOR
                } else {
                    WARNING_COLOR
                }
            }
            None => PASSIVE_COLOR,
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

    fn get_instruction(&self, t: &Translator) -> Vec<Span> {
        match self.input_mode {
            InputMode::Title => {
                let save_and = if self.has_text() {
                    t.get_translated_message("exit-save")
                } else {
                    t.get_translated_message("exit-no-save")
                };
                vec![
                    Span::raw(save_and),
                    Span::raw(t.get_translated_message("start-writing")),
                ]
            }
            InputMode::Writing => {
                if self.strict_mode && !self.achieved_goals() {
                    vec![Span::raw(t.get_translated_message("keep-writing"))]
                } else {
                    vec![Span::raw(t.get_translated_message("stop-writing"))]
                }
            }
        }
    }

    fn get_widget_colors(&self) -> (Color, Color) {
        match self.input_mode {
            InputMode::Title => (ACTIVE_COLOR, PASSIVE_COLOR),
            InputMode::Writing => match (self.keystroke_timeout, self.last_keystroke) {
                (Some(timeout), Some(last_keystroke)) => {
                    if last_keystroke.elapsed().as_secs_f32() > 0.8 * timeout as f32 {
                        (PASSIVE_COLOR, DANGER_COLOR)
                    } else if last_keystroke.elapsed().as_secs_f32() > 0.5 * timeout as f32 {
                        (PASSIVE_COLOR, WARNING_COLOR)
                    } else {
                        (PASSIVE_COLOR, ACTIVE_COLOR)
                    }
                }
                _ => (PASSIVE_COLOR, ACTIVE_COLOR),
            },
        }
    }

    fn has_text(&self) -> bool {
        !self.text.is_empty()
    }

    fn get_paragraph_text(&self, paragraph_rows: usize, paragraph_cols: usize) -> String {
        let wrapped_text = textwrap::fill(&self.text, paragraph_cols);

        let total_lines = wrapped_text.lines().count();
        let mut final_text = String::default();
        let skip = if total_lines > max(1, paragraph_rows) - 1 {
            total_lines - paragraph_rows + 1
        } else {
            0
        };
        let line_iterator = wrapped_text.lines().skip(skip);
        for line in line_iterator {
            final_text = format!("{}{}\n", final_text, line);
        }
        final_text = final_text.trim_end_matches(char::is_whitespace).to_string();
        let trailing_whitespace =
            &self.text[self.text.trim_end_matches(char::is_whitespace).len()..];
        if !trailing_whitespace.is_empty() {
            // Trailing whitespace is discarded by
            // `textwrap::wrap`. We reinsert it here. If multiple
            // spaces are added, this can overflow the margins
            // which look a bit odd. Handling this would require
            // some more tinkering...
            final_text = format!("{}{}", final_text, trailing_whitespace);
        };
        final_text
    }
}

fn get_text_position(text: &str) -> (u16, u16) {
    let last_line = text.lines().last().unwrap_or_default();
    let last_line_offset = usize::from(text.ends_with('\n'));
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

fn create_default_config(t: &Translator) -> bool {
    if let Some(config_dir) = dirs::config_dir() {
        let config_directory = config_dir.join("writingbuddy/");
        let config_file = config_directory.join("writingbuddy.toml");
        return create_config_file(&config_file, t);
    }
    false
}

fn create_config_file(config_path: &Path, t: &Translator) -> bool {
    if let Some(directory) = config_path.parent() {
        if std::fs::create_dir_all(directory).is_err() {
            return false;
        }
    }
    let config_contents = include_str!("../default_config.toml");
    println!(
        "{}{:?}",
        t.get_translated_message("writing-config"),
        config_path
    );
    if std::fs::write(config_path, config_contents).is_ok() {
        return true;
    }
    false
}

struct Translator {
    bundle: FluentBundle<FluentResource>,
}

impl Translator {
    pub fn new(bundle: FluentBundle<FluentResource>) -> Self {
        Translator { bundle }
    }

    pub fn get_translated_message(&self, key: &str) -> String {
        let msg = self
            .bundle
            .get_message(key)
            .expect(&format!("Message doesn't exist: {}", key));
        let mut errors = vec![];
        let pattern = msg.value().expect("Message has no value.");
        self.bundle
            .format_pattern(&pattern, None, &mut errors)
            .to_string()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::from_args();

    let supported_languages = HashMap::from([
        ("en-US", include_str!("../resources/en-US.fluent")),
        ("de", include_str!("../resources/de.fluent")),
    ]);
    let requested_language = langids!("de-CH");
    let available_language = langids!("de", "en-US");
    let default_language: LanguageIdentifier = langid!("en-US");

    let selected_language = negotiate_languages(
        &requested_language,
        &available_language,
        Some(&default_language),
        NegotiationStrategy::Filtering,
    )[0];
    println!("{:?}", selected_language);
    let ftl_string = supported_languages[&selected_language.to_string() as &str];
    let res =
        FluentResource::try_new(ftl_string.to_string()).expect("Failed to parse an FTL string.");

    let mut bundle = FluentBundle::new(vec![selected_language.clone()]);

    bundle
        .add_resource(res)
        .expect("Failed to add FTL resources to the bundle.");

    let t = Translator::new(bundle);
    let value = t.get_translated_message("word-count");
    println!("{}", &value);

    let settings = get_settings(cli_config, &t);

    let now = Utc::now();
    let title_format_str = settings
        .get_string("title_string")
        .unwrap_or_else(|_| "## %Y-%m-%d".to_string());
    let title = now.format(&title_format_str);
    let file_format_str = settings
        .get_string("file_string")
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
    let mut keystroke_timeout = settings.get_int("keystroke_timeout").ok();
    if let Some(0) = keystroke_timeout {
        keystroke_timeout = None;
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
        keystroke_timeout,
    );
    let res = run_app(&mut terminal, &mut app, &t);

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if app.has_text() {
        println!("Storing text into: {}", &filename);
        let mut output = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&filename.to_string())
            .unwrap();
        if !app.title.is_empty() {
            writeln!(output, "{}", app.title)?;
        }
        writeln!(output, "{}", app.text)?;
        writeln!(output)?;
    }

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn get_settings(cli_config: CliConfig, t: &Translator) -> Config {
    if let Some(config_file) = cli_config.config_file {
        println!(
            "{}{:#?}",
            t.get_translated_message("read-specified-config"),
            config_file
        );
        let config_builder = Config::builder().add_source(config::File::with_name(&config_file));
        if let Ok(settings) = config_builder.build() {
            return settings;
        } else if cli_config.initialize_config {
            println!(
                "{}{:#?}",
                t.get_translated_message("create-config"),
                config_file
            );
            if create_config_file(Path::new(&config_file), t) {
                let config_builder =
                    Config::builder().add_source(config::File::with_name(&config_file));
                if let Ok(settings) = config_builder.build() {
                    return settings;
                } else {
                    println!("{}", t.get_translated_message("no-config-exit"));
                    std::process::exit(1);
                }
            }
        } else {
            println!("{}{:?}", t.get_translated_message(""), config_file);
            std::process::exit(1);
        }
    } else {
        println!("{}", t.get_translated_message("no-config-current-dir"));
        let config_builder = Config::builder().add_source(config::File::with_name("writingbuddy"));
        if let Ok(settings) = config_builder.build() {
            return settings;
        } else {
            println!("{}", t.get_translated_message("no-config-current-dir"));
            if cli_config.initialize_config {
                println!("{}", t.get_translated_message("create-config-current-dir"));
                if create_config_file(Path::new("writingbuddy.toml"), t) {
                    let config_builder =
                        Config::builder().add_source(config::File::with_name("writingbuddy.toml"));
                    if let Ok(settings) = config_builder.build() {
                        return settings;
                    } else {
                        println!("{}", t.get_translated_message("no-config-exit"));
                        std::process::exit(1);
                    }
                }
            } else if let Some(config_dir) = dirs::config_dir() {
                let config_file = config_dir
                    .join("writingbuddy/writingbuddy")
                    .to_str()
                    .unwrap()
                    .to_string();
                let config_builder =
                    Config::builder().add_source(config::File::with_name(&config_file));
                if let Ok(settings) = config_builder.build() {
                    return settings;
                } else {
                    println!(
                        "{}{:?}",
                        t.get_translated_message("create-default-because-error"),
                        config_file
                    );
                    if create_default_config(t) {
                        let config_builder =
                            Config::builder().add_source(config::File::with_name(&config_file));
                        if let Ok(settings) = config_builder.build() {
                            return settings;
                        } else {
                            println!("{}", t.get_translated_message("no-config-exit"));
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }
    Config::default()
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: &mut App,
    t: &Translator,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app, t))?;

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
                            app.last_keystroke = Some(Instant::now());
                            if !app.writing_time.is_running() {
                                app.writing_time.start();
                            }
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
                                app.last_keystroke = None;
                                app.input_mode = InputMode::Title;
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
        if let (Some(keystroke_timeout), Some(last_keystroke)) =
            (app.keystroke_timeout, app.last_keystroke)
        {
            if last_keystroke.elapsed().as_secs() > keystroke_timeout as u64 {
                app.last_keystroke = None;
                app.writing_time.reset();
                app.text.clear();
            }
        };
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App, t: &Translator) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    let text = Text::from(Spans::from(app.get_instruction(t)));
    let help_message = Paragraph::new(text)
        .style(Style::default().fg(PASSIVE_COLOR))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t.get_translated_message("instructions")),
        );
    f.render_widget(help_message, chunks[0]);

    let widget_colors = app.get_widget_colors();

    let title = Paragraph::new(app.title.clone())
        .style(Style::default().fg(widget_colors.0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t.get_translated_message("title")),
        );
    f.render_widget(title, chunks[1]);

    let paragraph_cols = max(6, f.size().width as usize) - 6; // subtract 6 for border
    let paragraph_rows = max(2, chunks[2].height as usize) - 2; // subtract 2 for border

    let wrapped_text = app.get_paragraph_text(paragraph_rows, paragraph_cols);

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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t.get_translated_message("text")),
        );
    f.render_widget(text, chunks[2]);

    let stat_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[3]);
    {
        let stats = Paragraph::new(app.get_word_count_string())
            .style(Style::default().fg(app.get_word_count_color()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t.get_translated_message("word-count")),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(stats, stat_chunks[0]);

        let stats = Paragraph::new(app.get_time_string())
            .style(Style::default().fg(app.get_time_color()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t.get_translated_message("time")),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(stats, stat_chunks[1]);
    }
}

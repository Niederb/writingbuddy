use chrono::{Datelike, Utc};
use config::Config;
use crossterm::terminal::EnableLineWrap;
use crossterm::{
    execute,
    terminal::{size, SetSize},
    Result,
};
use std::fs::OpenOptions;
use std::io;
use std::io::{stdout, Write};

fn main() -> Result<()> {
    let (cols, rows) = size()?;
    // Resize terminal and scroll up.
    execute!(stdout(), SetSize(60, 25), EnableLineWrap)?;

    let mut settings = Config::default();
    settings.merge(config::File::with_name("config")).unwrap();

    let folder = settings.get_str("folder").unwrap();
    let now = Utc::now();
    let title = format!("## {}-{:02}-{:02}", now.year(), now.month(), now.day());
    let filename = format!("{}/{}-{:02}.md", folder, now.year(), now.month());

    let mut text = String::new();
    println!("Enter your text now: ");
    io::stdin()
        .read_line(&mut text)
        .expect("Couldn't read line");
    println!("Storing text into: {}", &filename);
    let mut output = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(&filename)
        .unwrap();
    writeln!(output, "{}", title)?;
    writeln!(output, "{}", text)?;
    writeln!(output)?;

    // Be a good citizen, cleanup
    execute!(stdout(), SetSize(cols, rows))?;
    Ok(())
}

use chrono::{Datelike, Utc};
use std::fs::OpenOptions;
use std::io;
use std::io::{Error, Write};

fn main() -> Result<(), Error> {
    let now = Utc::now();
    let title = format!("## {}-{:02}-{:02}", now.year(), now.month(), now.day());
    let filename = format!("{}-{:02}.md", now.year(), now.month());

    let mut text = String::new();
    println!("Enter your text now: ");
    io::stdin()
        .read_line(&mut text)
        .expect("Couldn't read line");

    let mut output = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(filename)
        .unwrap();
    writeln!(output, "{}", title)?;
    writeln!(output, "{}", text)?;
    writeln!(output)?;

    Ok(())
}

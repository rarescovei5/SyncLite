use colored::Colorize;
use std::io::{self, Write};

pub fn confirm(
    message: &str,
    padding_left: Option<usize>,
) -> Result<String, Box<dyn std::error::Error>> {
    let pad = " ".repeat(padding_left.unwrap_or(0));
    print!("{}🤔 {}", pad, message.bright_cyan());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read user input: {}", e))?;

    Ok(input.trim().to_string())
}

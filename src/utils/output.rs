use chrono::Utc;
use colored::*;
use std::fmt::Display;

/// CLI output utilities with consistent styling
pub struct CliOutput;

// Presets
impl CliOutput {
    pub fn banner() {
        let ascii_art = r#"
                            .__  .__  __          
  _________.__. ____   ____ |  | |__|/  |_  ____  
 /  ___<   |  |/    \_/ ___\|  | |  \   __\/ __ \ 
 \___ \ \___  |   |  \  \___|  |_|  ||  | \  ___/ 
/____  >/ ____|___|  /\___  >____/__||__|  \___  >
     \/ \/         \/     \/                   \/                      
"#;
        println!("{}", ascii_art.bright_yellow().bold());
    }

    pub fn usage() {
        println!();
        println!("{}", "Usage:".bright_white().bold());
        println!(
            "  {} {} {}",
            "synclite".bright_cyan(),
            "<command>".bright_yellow(),
            "<path>".bright_magenta()
        );
        println!();
        println!("{}", "Commands:".bright_white().bold());
        println!(
            "  {}  {}",
            "serve".bright_green().bold(),
            "Start a sync server in the specified directory".dimmed()
        );
        println!(
            "  {}  {}",
            "connect".bright_green().bold(),
            "Connect to a sync server".dimmed()
        );
        println!();
        println!("{}", "Options:".bright_white().bold());
        println!(
            "  {}, {}  {}",
            "-p".bright_yellow(),
            "--port".bright_yellow(),
            "Specify port number (default: 8080)".dimmed()
        );
        println!();
    }
}

// Utilities
impl CliOutput {
    pub fn success(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}✅ {}", pad, message);
    }

    pub fn log(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!(
            "{}[{}]: {}",
            pad,
            Utc::now().time().to_string().bright_black().bold(),
            message
        );
    }

    pub fn wrench(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}🔧 {}", pad, message);
    }

    pub fn error(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        eprintln!("{}❌ {}", pad, message);
    }

    pub fn warning(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}⚠️ {}", pad, message);
    }

    pub fn info(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}ℹ️  {}", pad, message);
    }

    pub fn question(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}🤔 {}", pad, message);
    }
    pub fn cancelled(message: impl Display, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}🚫 {}", pad, message);
    }
}

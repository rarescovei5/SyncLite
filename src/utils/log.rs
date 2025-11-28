use chrono::Utc;
use colored::*;
use std::fmt::Display;

/// CLI output utilities with consistent styling
pub struct Log;

// Presets
impl Log {
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
impl Log {
    pub fn log(message: impl Display, indent: Option<usize>) {
        let time_str = Utc::now().time().to_string();
        let time = &time_str[..13];

        println!(
            "{}[{}]: {}",
            " ".repeat(indent.unwrap_or(0)),
            time.bright_black().bold(),
            message
        );
    }

    pub fn info(message: impl Display, indent: Option<usize>) {
        println!("{}ℹ️  {}", " ".repeat(indent.unwrap_or(0)), message);
    }
    pub fn wrench(message: impl Display, indent: Option<usize>) {
        println!("{}🔧 {}", " ".repeat(indent.unwrap_or(0)), message);
    }

    pub fn error(message: impl Display, indent: Option<usize>) {
        eprintln!("{}❌ {}", " ".repeat(indent.unwrap_or(0)), message);
    }
    pub fn warning(message: impl Display, indent: Option<usize>) {
        println!("{}⚠️ {}", " ".repeat(indent.unwrap_or(0)), message);
    }
    pub fn success(message: impl Display, indent: Option<usize>) {
        println!("{}✅ {}", " ".repeat(indent.unwrap_or(0)), message);
    }
    pub fn question(message: impl Display, indent: Option<usize>) {
        println!("{}🤔 {}", " ".repeat(indent.unwrap_or(0)), message);
    }
}

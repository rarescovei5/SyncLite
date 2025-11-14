use colored::*;

/// CLI output utilities with consistent styling
pub struct CliOutput;

// Presets
impl CliOutput {
    pub fn banner() {
        println!();
        println!("{}", "🚀 SyncLite".bright_cyan().bold());
        println!("   {} {}", "Version".dimmed(), "v0.1.0".bright_white());
        println!("   {}", "A lightweight file synchronization tool".dimmed());
        println!();
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

    pub fn step(current: usize, total: usize, message: &str) {
        println!(
            "{} {} {}",
            format!("[{}/{}]", current, total).bright_black().bold(),
            "→".bright_cyan(),
            message.bright_white()
        );
    }

    pub fn initializing(path: &str) {
        println!();
        println!("🔧 {}", "Initializing SyncLite...".bright_yellow().bold());
        println!(
            "📁 {}",
            format!("Target directory: {}", path).bright_white()
        );
        println!();
    }
}

// Utilities
impl CliOutput {
    pub fn success(message: &str, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}✅ {}", pad, message.bright_green());
    }

    pub fn error(message: &str, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        eprintln!("{}❌ {}", pad, message.bright_red());
    }

    pub fn warning(message: &str, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}⚠️ {}", pad, message.bright_yellow());
    }

    pub fn info(message: &str, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}ℹ️  {}", pad, message.bright_blue());
    }

    pub fn question(message: &str, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}🤔 {}", pad, message.bright_cyan());
    }
    pub fn cancelled(message: &str, padding_left: Option<usize>) {
        let pad = " ".repeat(padding_left.unwrap_or(0));

        println!("{}🚫 {}", pad, message.bright_red());
    }
}

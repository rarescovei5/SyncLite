use std::path::{Path, PathBuf};

use super::types::{Args, Command};
use crate::utils::Log;

pub enum ParseArgsError {
    InvalidArguments,
    InvalidCommand(String),
    InvalidPath(String),
    InvalidPortNumber(String),
}

// Parse Function
pub fn parse_args() -> Result<Args, ParseArgsError> {
    // Get the arguments from the command line
    let args = std::env::args().skip(1).collect::<Vec<String>>();

    // Improper usage
    if args.len() < 2 || args[0] == "-h" || args[0] == "--help" {
        return Err(ParseArgsError::InvalidArguments);
    }

    // Get the command
    let command = match args[0].as_str() {
        "serve" => Command::Serve,
        "connect" => Command::Connect,
        _ => return Err(ParseArgsError::InvalidCommand(args[0].clone())),
    };

    // Get the path and validate it
    let abs_workspace_path = match Path::new(&args[1]).canonicalize() {
        Ok(mut canonical_workspace) => {
            let canonical_str = canonical_workspace.to_string_lossy();
            if canonical_str.starts_with(r"\\?\") {
                canonical_workspace = PathBuf::from(&canonical_str[4..]);
            }
            canonical_workspace
        }
        Err(_) => {
            return Err(ParseArgsError::InvalidPath(args[1].clone()));
        }
    };

    // Other arguments
    let port = match args.get(2).unwrap_or(&"8080".to_string()).parse::<u16>() {
        Ok(port) => port,
        Err(_) => {
            Log::error(
                &format!("Invalid port number: '{}'", args.get(2).unwrap()),
                None,
            );
            std::process::exit(1);
        }
    };

    // Return the arguments
    Ok(Args {
        command,
        abs_workspace_path,
        port,
    })
}

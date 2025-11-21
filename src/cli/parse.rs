use crate::cli::types::{CliArguments, Command, FlagType};
use crate::utils::output::CliOutput;

// Parse Function
pub fn parse_args() -> Result<CliArguments, Box<dyn std::error::Error>> {
    // Get the arguments from the command line
    let mut args = std::env::args().skip(1);

    // Look if the command is "serve" or "connect", otherwise return an error
    let command: Command = match args.next().as_deref() {
        Some("serve") => Command::Serve,
        Some("connect") => Command::Connect,
        _ => {
            CliOutput::usage();
            std::process::exit(0);
        }
    };

    // Look if the path is provided and validate it
    let path: String = match args.next() {
        Some(path) => path,
        None => {
            CliOutput::error("No path provided", None);
            std::process::exit(1);
        }
    };
    if !std::path::Path::new(&path).is_dir() {
        CliOutput::error(&format!("Path is not a directory: '{}'", path), None);
        std::process::exit(1);
    }

    // Other arguments
    let mut port = 8080;

    // Look for flags
    let mut pending_flag: Option<FlagType> = None;
    for arg in args {
        match pending_flag {
            Some(FlagType::Port) => {
                port = match arg.parse::<u16>() {
                    Ok(port) => port,
                    Err(_) => {
                        CliOutput::error(&format!("Invalid port number: '{}'", arg), None);
                        std::process::exit(1);
                    }
                };
                pending_flag = None;
                continue;
            }
            None => (),
        }

        match arg.as_str() {
            "-p" | "--port" => {
                pending_flag = Some(FlagType::Port);
            }
            _ => {
                CliOutput::error(&format!("Unknown flag: '{}'", arg), None);
                std::process::exit(1);
            }
        }
    }

    // Return the arguments
    Ok(CliArguments {
        command,
        path,
        port,
    })
}

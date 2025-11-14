use crate::cli::types::{Args, Command, FlagType, ParseError, ParseErrorCode};

// Parse Function
pub fn parse_args() -> Result<Args, ParseError> {
    // Get the arguments from the command line
    let mut args = std::env::args().skip(1);

    // Look if the command is "serve" or "connect", otherwise return an error
    let command: Command = match args.next().as_deref() {
        Some("serve") => Command::Serve,
        Some("connect") => Command::Connect,
        Some(other) => {
            return Err(ParseError {
                message: format!("Invalid command: '{}'", other),
                code: ParseErrorCode::UnknownCommand,
            });
        }
        None => {
            return Err(ParseError {
                message: "No command provided".to_string(),
                code: ParseErrorCode::NoCommand,
            });
        }
    };

    // Look if the path is provided and validate it
    let path: String = match args.next() {
        Some(path) => path,
        None => {
            return Err(ParseError {
                message: "No path provided".to_string(),
                code: ParseErrorCode::NoPath,
            });
        }
    };
    if !std::path::Path::new(&path).is_dir() {
        return Err(ParseError {
            message: format!("Path is not a directory: '{}'", path),
            code: ParseErrorCode::InvalidPath,
        });
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
                        return Err(ParseError {
                            message: format!("Invalid port number: '{}'", arg),
                            code: ParseErrorCode::InvalidPort,
                        });
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
                return Err(ParseError {
                    message: format!("Unknown flag: '{}'", arg),
                    code: ParseErrorCode::InvalidFlag,
                });
            }
        }
    }

    // Return the arguments
    Ok(Args {
        command,
        path,
        port,
    })
}

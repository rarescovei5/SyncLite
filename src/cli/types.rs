use std::fmt;

// Base Types
#[derive(Debug, Clone)]
pub enum Command {
    Serve,
    Connect,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Serve => write!(f, "serve"),
            Command::Connect => write!(f, "connect"),
        }
    }
}

#[derive(Debug)]
pub struct Args {
    pub command: Command,
    pub path: String,
    pub port: u16,
}

// Utility Types
#[derive(Debug, Copy, Clone)]
pub enum FlagType {
    Port,
}

// Error Types
#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum ParseErrorCode {
    NoCommand = 1,
    NoPath = 2,
    UnknownCommand = 3,
    InvalidPath = 4,
    InvalidFlag = 5,
    InvalidPort = 6,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub code: ParseErrorCode,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug)]
pub enum Command {
    Serve,
    Connect,
}

pub struct CliArguments {
    pub command: Command,
    pub path: String,
    pub port: u16,
}

// Utility Types
#[derive(Debug, Copy, Clone)]
pub enum FlagType {
    Port,
}

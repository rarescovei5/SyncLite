use std::path::PathBuf;

#[derive(Debug)]
pub enum Command {
    Serve,
    Connect,
}

pub struct Args {
    pub command: Command,
    pub abs_workspace_path: PathBuf,
    pub port: u16,
}

use synclite::cli::{
    self,
    output::CliOutput,
    types::{Command, ParseErrorCode},
};
use synclite::storage::initialise_storage;

#[tokio::main]
async fn main() {
    CliOutput::banner();

    // Parse the arguments
    let args = match cli::parse_args() {
        Ok(args) => args,
        Err(e) => {
            CliOutput::error(&e.message, None);
            match e.code {
                ParseErrorCode::NoCommand | ParseErrorCode::NoPath => {
                    CliOutput::usage();
                }
                _ => (),
            }
            std::process::exit(e.code as i32);
        }
    };

    CliOutput::info(
        &format!("Starting {} mode for: {}", args.command, args.path),
        None,
    );

    let result = match args.command {
        Command::Serve => {
            CliOutput::initializing(&args.path);
            initialise_storage(&args.path, args.port)
        }
        Command::Connect => {
            CliOutput::info("Connect mode not yet implemented", None);
            Ok(())
        }
    };

    if let Err(e) = result {
        CliOutput::error(&format!("{}", e), None);
        std::process::exit(1);
    }
}

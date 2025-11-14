use synclite::cli::{self, types::ParseErrorCode};
use synclite::storage::initialise_storage;
use synclite::sync::initialise_state;
use synclite::utils::output::CliOutput;

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
        &format!("Starting {} mode for: {}\n", args.command, args.path),
        None,
    );

    initialise_storage(&args.path);
    initialise_state(&args.path);
}

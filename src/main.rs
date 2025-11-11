use synclite::cli::{ParseErrorCode, parse_args};

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{}", e.message);
            match e.code {
                ParseErrorCode::NoCommand | ParseErrorCode::NoPath => {
                    eprintln!("🧭 Usage: synclite <serve|connect> <path>")
                }
                _ => (),
            }
            std::process::exit(e.code as i32);
        }
    };
}

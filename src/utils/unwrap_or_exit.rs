use crate::utils::output::CliOutput;

pub fn unwrap_or_exit<T>(result: Result<T, String>) -> T {
    if let Err(error) = result {
        CliOutput::error(&error, None);
        std::process::exit(1);
    }
    result.unwrap()
}

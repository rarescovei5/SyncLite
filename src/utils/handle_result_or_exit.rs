use crate::utils::output::CliOutput;

/// Helper function to handle Result types - exit on error, continue on success
pub fn handle_result_or_exit<T>(result: Result<T, String>) -> T {
    if let Err(error_msg) = result {
        CliOutput::error(&error_msg, None);
        std::process::exit(1);
    }
    result.unwrap()
}

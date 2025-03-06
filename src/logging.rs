use colored::*;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Initialize the logging system
pub fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .without_time()
        .compact()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}

/// Log a message with a colored tag
pub fn log(tag: &str, message: &str, level: Level) {
    match level {
        Level::INFO => info!("{} {}", format!("[{}]", tag).bright_cyan(), message),
        Level::WARN => warn!("{} {}", format!("[{}]", tag).bright_yellow(), message),
        Level::ERROR => error!("{} {}", format!("[{}]", tag).bright_red(), message),
        _ => info!("{} {}", format!("[{}]", tag).bright_white(), message),
    }
}

/// Convenience functions for different log levels
pub mod log_utils {
    use super::*;
    use tracing::Level;

    pub fn info(tag: &str, message: &str) {
        log(tag, message, Level::INFO);
    }

    pub fn warn(tag: &str, message: &str) {
        log(tag, message, Level::WARN);
    }

    pub fn error(tag: &str, message: &str) {
        log(tag, message, Level::ERROR);
    }
}

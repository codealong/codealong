use slog::{Discard, Logger};

pub fn build_test_logger() -> Logger {
    let drain = Discard;
    Logger::root(drain, o!())
}

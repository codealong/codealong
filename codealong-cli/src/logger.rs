use std::str::FromStr;

use sloggers::file::FileLoggerBuilder;
use sloggers::types::Severity;
use sloggers::Build;

pub fn build_logger(matches: &clap::ArgMatches) -> slog::Logger {
    let mut builder = FileLoggerBuilder::new(matches.value_of("log_file").unwrap());
    builder.level(Severity::from_str(matches.value_of("log_level").unwrap()).unwrap());
    builder.build().unwrap()
}

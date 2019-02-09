use std::str::FromStr;

use slog::{Drain, Duplicate, Logger};
use sloggers::file::FileLoggerBuilder;
use sloggers::terminal::TerminalLoggerBuilder;
use sloggers::types::Severity;
use sloggers::Build;

pub fn build_logger(matches: &clap::ArgMatches) -> slog::Logger {
    let mut builder = FileLoggerBuilder::new(matches.value_of("log_file").unwrap());
    builder.level(Severity::from_str(matches.value_of("log_level").unwrap()).unwrap());
    let file_logger = builder.build().unwrap();
    // If we are in progress mode we do not show terminal log output for those
    // subcommands which support it.
    if !matches.subcommand_matches("analyze").is_some() || !matches.is_present("progress") {
        let verbosity = Severity::from_str(matches.value_of("verbosity").unwrap()).unwrap();
        let mut builder = TerminalLoggerBuilder::new();
        builder.level(verbosity);
        let terminal_logger = builder.build().unwrap();
        Logger::root(Duplicate::new(file_logger, terminal_logger).fuse(), o!())
    } else {
        file_logger
    }
}

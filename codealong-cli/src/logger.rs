use std::str::FromStr;

use slog::{Drain, Duplicate, Logger};
use sloggers::file::FileLoggerBuilder;
use sloggers::terminal::TerminalLoggerBuilder;
use sloggers::types::Severity;
use sloggers::Build;

#[derive(Clone, Copy, PartialEq)]
pub enum OutputMode {
    Progress,
    Log,
    None,
}

impl OutputMode {
    pub fn from_str(s: &str) -> Option<OutputMode> {
        match s {
            "progress" => Some(OutputMode::Progress),
            "log" => Some(OutputMode::Log),
            "none" => Some(OutputMode::None),
            _ => None,
        }
    }
}

pub fn build_logger(matches: &clap::ArgMatches) -> (slog::Logger, OutputMode) {
    let mut builder = FileLoggerBuilder::new(matches.value_of("log_file").unwrap());
    builder.level(Severity::from_str(matches.value_of("log_level").unwrap()).unwrap());
    let file_logger = builder.build().unwrap();
    let output_mode = OutputMode::from_str(matches.value_of("output_mode").unwrap()).unwrap();
    let logger = if let OutputMode::Log = output_mode {
        let verbosity = Severity::from_str(matches.value_of("verbosity").unwrap()).unwrap();
        let mut builder = TerminalLoggerBuilder::new();
        builder.level(verbosity);
        let terminal_logger = builder.build().unwrap();
        Logger::root(Duplicate::new(file_logger, terminal_logger).fuse(), o!())
    } else {
        file_logger
    };
    (logger, output_mode)
}

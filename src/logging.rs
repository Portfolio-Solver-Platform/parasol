use crate::args::DebugVerbosityLevel;
use std::sync::atomic::{AtomicU8, Ordering};

static CURRENT_VERBOSITY: AtomicU8 = AtomicU8::new(LEVEL_WARNING);

pub fn init(verbosity: DebugVerbosityLevel) {
    CURRENT_VERBOSITY.store(verbosity as u8, Ordering::Relaxed);
}

pub(crate) fn log_msg_impl(
    verbosity: u8,
    level: &str,
    args: std::fmt::Arguments,
    file: &str,
    line: u32,
) {
    let current_level = CURRENT_VERBOSITY.load(Ordering::Relaxed);

    if current_level >= verbosity {
        eprintln!("{level}: [{file}:{line}] {args}");
    }
}

macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logging::log_msg_impl(
            $crate::logging::LEVEL_INFO,
            "INFO",
            format_args!($($arg)*),
            file!(),
            line!()
        )
    };
}

macro_rules! warning {
    ($($arg:tt)*) => {
        $crate::logging::log_msg_impl(
            $crate::logging::LEVEL_WARNING,
            "WARNING",
            format_args!($($arg)*),
            file!(),
            line!()
        )
    };
}

macro_rules! error_msg {
    ($($arg:tt)*) => {
        $crate::logging::log_msg_impl(
            $crate::logging::LEVEL_ERROR,
            "ERROR",
            format_args!($($arg)*),
            file!(),
            line!()
        )
    };
}

// This function is purely used to force the anyhow::Error type
// to avoid forgetting to convert it to that type before printing
pub(crate) fn log_error_impl(e: &anyhow::Error, file: &str, line: u32) {
    log_msg_impl(LEVEL_ERROR, "ERROR", format_args!("{e:#}"), file, line);
}

macro_rules! error {
    ($e:expr) => {
        $crate::logging::log_error_impl(&$e, file!(), line!())
    };
}

pub(crate) use error;
pub(crate) use error_msg;
pub(crate) use info;
pub(crate) use warning;

#[allow(dead_code)]
pub(crate) const LEVEL_QUIET: u8 = 0;
pub(crate) const LEVEL_ERROR: u8 = 1;
pub(crate) const LEVEL_WARNING: u8 = 2;
pub(crate) const LEVEL_INFO: u8 = 3;

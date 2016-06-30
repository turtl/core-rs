use ::fern;
use ::log;
use ::time;
use ::error::{TResult, TError};

pub fn setup_logger(level: log::LogLevelFilter) -> TResult<()> {
    let logger_config = fern::DispatchConfig {
        format: Box::new(|msg: &str, level: &log::LogLevel, _location: &log::LogLocation| {
            format!("[{}][{}] {}", time::now().strftime("%Y-%m-%d][%H:%M:%S").unwrap(), level, msg)
        }),
        output: vec![fern::OutputConfig::stdout()],
        level: log::LogLevelFilter::Trace,
    };
    try_t!(fern::init_global_logger(logger_config, level));
    Ok(())
}


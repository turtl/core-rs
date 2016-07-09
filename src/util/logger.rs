use ::config;
use ::fern;
use ::log;
use ::time;
use ::error::{TResult, TError};

/// a simple wrapper (pretty much direct from documentation) that sets up
/// logging to STDOUT via fern/log
pub fn setup_logger() -> TResult<()> {
    let levelstr: String = try!(config::get(&["loglevel"]));
    let level = match levelstr.to_lowercase().as_ref() {
        "off" => log::LogLevelFilter::Off,
        "error" => log::LogLevelFilter::Error,
        "warn" => log::LogLevelFilter::Warn,
        "info" => log::LogLevelFilter::Info,
        "debug" => log::LogLevelFilter::Debug,
        "trace" => log::LogLevelFilter::Trace,
        _ => {
            println!("turtl: config: bad `loglevel` value (\"{}\"), defaulting to \"warn\"", levelstr);
            log::LogLevelFilter::Warn
        }
    };
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


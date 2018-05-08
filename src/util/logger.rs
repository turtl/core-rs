use ::config;
use ::fern;
use ::log;
use ::time;
use ::error::TResult;
use ::std::{self, env};

/// a simple wrapper (pretty much direct from documentation) that sets up
/// logging to STDOUT via fern/log
pub fn setup_logger() -> TResult<()> {
    let levelstr: String = match env::var("TURTL_LOGLEVEL") {
        Ok(x) => x,
        Err(_) => config::get(&["loglevel"])?
    };
    let level = match levelstr.to_lowercase().as_ref() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => {
            println!("turtl: config: bad `loglevel` value (\"{}\"), defaulting to \"warn\"", levelstr);
            log::LevelFilter::Warn
        }
    };
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                time::now().strftime("%Y-%m-%d][%H:%M:%S").unwrap(),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}


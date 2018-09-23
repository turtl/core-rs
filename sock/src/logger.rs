use ::fern;
use ::log;
use ::time;
use ::std::{self, env};

/// A fun and lighthearted logger
pub fn setup_logger() {
    let levelstr: String = match env::var("TURTL_LOGLEVEL") {
        Ok(x) => x,
        Err(_) => String::from("info"),
    };
    let level = match levelstr.to_lowercase().as_ref() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        "off" => log::LevelFilter::Off,
        _ => {
            println!("logger::setup_logger() -- bad `log.level` value (\"{}\"), defaulting to \"warn\"", levelstr);
            log::LevelFilter::Warn
        }
    };
    let config = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} - [{}][{}] {}",
                time::now().strftime("%Y-%m-%dT%H:%M:%S").expect("sock::setup_logger() -- failed to parse time format"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout());
    match config.apply() {
        Ok(_) => {}
        Err(e) => {
            trace!("logger::setup_logger() -- looks like the logger was already init: {}", e);
        }
    }
}


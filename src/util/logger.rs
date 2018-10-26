use ::config;
use ::fern;
use ::log;
use ::time;
use ::error::{TResult, TError};
use ::std::{self, env};
use ::std::fs::{self, File};
use ::std::io::BufReader;
use ::std::io::prelude::*;
use ::std::sync::{Mutex, RwLock};
use ::glob;
use ::std::path::PathBuf;

lazy_static! {
    static ref LOG_SETUP_DONE: RwLock<bool> = RwLock::new(false);
}

/// grab the current logfile from the config. quite hypnotic.
pub fn get_logfile() -> Option<String> {
    let filedest: String = match config::get(&["logging", "file"]) {
        Ok(x) => x,
        Err(_) => return None,
    };
    // if our log file doesn't start with "/" or "x:" then we assume a path
    // relative to the data_folder
    let has_slash = filedest.find("/").unwrap_or(999) == 0;
    let has_colon = filedest.find(":").unwrap_or(999) < 2;
    let filedest = if !has_slash && !has_colon {
        if let Ok(data_folder) = config::get::<String>(&["data_folder"]) {
            format!("{}/{}", data_folder, filedest)
        } else {
            filedest
        }
    } else {
        filedest
    };
    Some(filedest)
}

/// read the logfile's contents to a string and return. if logging is not set up
/// then we throw a tantrum. Set `num_lines` to -1 to grab everything.
pub fn read_log(num_lines: i32) -> TResult<String> {
    let logfile = match get_logfile() {
        Some(x) => Ok(x),
        None => TErr!(TError::MissingField(String::from("logging not set up in config"))),
    }?;
    let mut file = File::open(logfile)?;
    if num_lines < 0 {
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(contents)
    } else {
        let num_lines = num_lines as usize;
        let reader = BufReader::new(file);
        let mut lines: Vec<String> = Vec::with_capacity(1024);
        for line in reader.lines() {
            lines.push(line?);
        }
        let num_log_lines = lines.len();
        if num_log_lines > num_lines {
            lines = lines.split_off(num_log_lines - num_lines);
        }
        Ok(lines.join("\n"))
    }
}

/// rotate a logfile:
///   - move file.log.3 -> file.log.4, file.log.2 -> file.log.3, etc
///   - copy file.log -> file.log.1
///   - truncate file.log
///   - dispose of file.log.4
fn rotate(logfile: &String) -> TResult<()> {
    let max_size: u64 = config::get(&["logging", "rotation", "size"]).unwrap_or(10485760);
    let keep_logs: u8 = config::get(&["logging", "rotation", "keep"]).unwrap_or(3);
    let metadata = match fs::metadata(&logfile) {
        Ok(meta) => meta,
        Err(e) => {
            warn!("logger::rotate() -- can't stat logfile: {}", e);
            return Ok(())
        }
    };
    if metadata.len() < max_size {
        return Ok(())
    }
    for i in (1..keep_logs).rev() {
        let oldlog = format!("{}.{}", logfile, i);
        let newlog = format!("{}.{}", logfile, i + 1);
        match fs::metadata(&oldlog) {
            Ok(_) => {
                match fs::rename(&oldlog, &newlog) {
                    Ok(_) => info!("logging::rotate() -- rotated {} -> {}", oldlog, newlog),
                    Err(e) => warn!("logging::rotate() -- failed to rename {} -> {}: {}", oldlog, newlog, e),
                }
            }
            // doesn't exist, oh well
            Err(_) => {}
        }
    }
    // now search all log files, removing ones that aren't part of the keep
    // rotation
    let files = glob::glob(format!("{}*", logfile).as_str())?;
    for file in files {
        let file = match file {
            Ok(x) => x,
            Err(_) => continue,
        };

        let mut keep = file == PathBuf::from(&logfile);
        if !keep {
            for i in (1..keep_logs).rev() {
                if file == PathBuf::from(format!("{}.{}", logfile, i)) {
                    keep = true;
                }
            }
        }
        if !keep {
            match fs::remove_file(file.clone()) {
                Ok(_) => info!("logging::rotate() -- removed {:?}", file),
                Err(e) => warn!("logging::rotate() -- failed to remove {:?}: {}", file, e),
            }
        }
    }
    match fs::copy(&logfile, &format!("{}.1", logfile)) {
        Ok(_) => {
            let truncate = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&logfile);
            match truncate {
                Ok(_) => {}
                Err(e) => warn!("logging::rotate() -- failed to truncate {}: {}", logfile, e),
            }
        }
        Err(e) => {
            warn!("logging::rotate() -- failed to copy {} to {}.1: {}", logfile, logfile, e);
        }
    }
    Ok(())
}

/// if we are logging to a logfile, make sure that if it's over a certain size,
/// we cut it down a bit.
lazy_static! {
    static ref PRUNE_COUNTER: Mutex<u32> = Mutex::new(0);
}
fn prune_logfile() -> TResult<()> {
    {
        let mut count_guard = lock!(*PRUNE_COUNTER);
        *count_guard += 1;
        if *count_guard < 1000 { return Ok(()); }
        *count_guard = 0;
        drop(count_guard);
    }
    let logfile = match get_logfile() {
        Some(x) => x,
        None => return Ok(()),
    };
    rotate(&logfile)
}

/// a simple wrapper (pretty much direct from documentation) that sets up
/// logging to STDOUT (and file if config allows) via fern/log
pub fn setup_logger() -> TResult<()> {
    let levelstr: String = match env::var("TURTL_LOGLEVEL") {
        Ok(x) => x,
        Err(_) => config::get(&["logging", "level"])?
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
    let mut config = fern::Dispatch::new()
        .format(|out, message, record| {
            match prune_logfile() {
                Ok(_) => {},
                Err(e) => {
                    println!("logger::setup_logger() -- prune error: {}", e);
                }
            }
            out.finish(format_args!(
                "{} - [{}][{}] {}",
                time::now().strftime("%Y-%m-%dT%H:%M:%S").expect("turtl::logger::setup_logger() -- failed to parse time or something"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(level)
        .level_for("tokio_reactor", if level < log::LevelFilter::Info { level } else { log::LevelFilter::Info })
        .level_for("hyper", if level < log::LevelFilter::Info { level } else { log::LevelFilter::Info })
        .level_for("jni", if level < log::LevelFilter::Info { level } else { log::LevelFilter::Info })
        .chain(std::io::stdout());
    if let Some(filedest) = get_logfile() {
        config = config.chain(fern::log_file(filedest)?);
    }
    match config.apply() {
        Ok(_) => {}
        Err(e) => {
            trace!("logger::setup_logger() -- looks like the logger was already init: {}", e);
        }
    }

    let mut init_guard = lockw!(*LOG_SETUP_DONE);
    *init_guard = true;
    drop(init_guard);
    Ok(())
}

/// Whether or not logging has been set up
pub fn has_init() -> bool {
    let init_guard = lockr!(*LOG_SETUP_DONE);
    *init_guard
}


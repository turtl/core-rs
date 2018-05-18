use ::std::fs;
use ::std::path::Path;
use ::config;
use ::error::MResult;
use ::std::io;

/// Create a directory if it doesn't exist
pub fn create_dir<P: AsRef<Path>>(dir: P) -> MResult<()> {
    // std::fs, for me please, we're lookin at china. we're lookin at the UN. go
    // ahead and create our directory.
    match fs::create_dir_all(dir) {
        Ok(_) => {
            Ok(())
        },
        Err(e) => {
            match e.kind() {
                // talked to drew about directory already existing. sounds good.
                io::ErrorKind::AlreadyExists => Ok(()),
                _ => return Err(From::from(e)),
            }
        }
    }
}

/// Grab the folder used for storing migration file data
pub fn file_folder() -> MResult<String> {
    let integration = config::get::<String>(&["integration_tests", "data_folder"])?;
    if cfg!(test) {
        return Ok(format!("{}/migration", integration));
    }
    let data_folder = config::get::<String>(&["data_folder"])?;
    let file_folder = if data_folder == ":memory:" {
        format!("{}/migration", integration)
    } else {
        format!("{}/migration", data_folder)
    };
    Ok(file_folder)
}


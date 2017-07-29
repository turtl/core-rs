use ::jedi::Value;
use ::error::{TResult, TError};
use ::storage::Storage;
use ::models::model::Model;
use ::models::sync_record::SyncAction;
use ::models::protected::{Keyfinder, Protected};
use ::models::note::Note;
use ::sync::sync_model::SyncModel;
use ::turtl::Turtl;
use ::std::mem;
use ::crypto;
use ::config;
use ::util;
use ::std::fs;
use ::std::io::prelude::*;
use ::std::path::PathBuf;
use ::glob;

/// Stores the name of our file directory under the data folder
const FILEDIR: &'static str = "files";

protected! {
    /// Defines the object we find inside of Note.File (a description of the
    /// note's file with no actual file data...name, mime type, etc).
    #[derive(Serialize, Deserialize)]
    pub struct File {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub size: Option<u64>,
        #[serde(default)]
        #[protected_field(public)]
        pub has_data: i8,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub name: Option<String>,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub type_: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub meta: Option<Value>,
    }
}

protected! {
    /// Defines the object that holds actual file body data separately from the
    /// metadata that lives in the Note object.
    #[derive(Serialize, Deserialize)]
    #[protected_modeltype(file)]
    pub struct FileData {
        #[serde(with = "::util::ser::base64_converter")]
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        #[protected_field(private)]
        pub data: Option<Vec<u8>>,
    }
}

make_storable!(FileData, "files");
make_basic_sync_model!{ FileData,
    // NOP - we do not want to sync to db LOOL
    fn db_save(&self, _db: &mut Storage) -> TResult<()> {
        Ok(())
    }

    // remove the file
    fn db_delete(&self, _db: &mut Storage) -> TResult<()> {
        let id = match self.id().as_ref() {
            Some(id) => id.clone(),
            None => return Err(TError::MissingField(String::from("FileData.db_delete() -- `self.id` is None, cannot delete file =["))),
        };

        let data_folder = config::get::<String>(&["data_folder"])?;
        let search = format!("{}/{}/{}", data_folder, FILEDIR, FileData::filebuilder(&String::from("*"), &id));
        let files = glob::glob(&search)?;
        for file in files {
            fs::remove_file(&file?)?;
        }
        Ok(())
    }
}

impl Keyfinder for FileData {}

impl FileData {
    /// Builds a standard filename
    fn filebuilder(user_id: &String, note_id: &String) -> String {
        format!("u_{}.n_{}.enc", user_id, note_id)
    }

    /// Load a note's file, if we have one.
    pub fn load_file(turtl: &Turtl, note: &Note) -> TResult<Vec<u8>> {
        let note_id = match note.id().as_ref() {
            Some(id) => id.clone(),
            None => return Err(TError::MissingField(format!("FileData::load_file() -- `note.id` is None when saving file...tsk tsk"))),
        };
        let note_key = match note.key() {
            Some(key) => key.clone(),
            None => return Err(TError::MissingField(format!("FileData::load_file() -- `note.key` is None when saving file...shame, shame"))),
        };

        let data_folder = config::get::<String>(&["data_folder"])?;
        let mut filepath = PathBuf::from(data_folder);
        filepath.push(FILEDIR);
        filepath.push(FileData::filebuilder(&String::from("*"), &note_id));
        let pathstr = match filepath.to_str() {
            Some(x) => x,
            None => return Err(TError::BadValue(format!("FileData::load_file() -- invalid path: {:?}", filepath))),
        };
        let mut files = glob::glob(pathstr)?;
        let filename = match files.nth(0) {
            Some(x) => x,
            None => return Err(TError::NotFound(format!("FileData::load_file() -- file for note {} not found", note_id))),
        };
        let enc = {
            let mut file = fs::File::open(filename?)?;
            let mut enc = Vec::new();
            file.read_to_end(&mut enc)?;
            enc
        };

        // encrypt the file using the turtl standard serialization format
        let data = turtl.work.run(move || {
            crypto::decrypt(&note_key, enc)
                .map_err(|e| From::from(e))
        })?;

        Ok(data)
    }

    /// Encrypt/save this file
    pub fn save(&mut self, turtl: &Turtl, note: &mut Note) -> TResult<()> {
        // grab some items we'll need to do our work (user_id/note_id for the
        // filename, note_key for encrypting the file).
        let user_id = {
            let isengard = turtl.user_id.read().unwrap();
            match isengard.as_ref() {
                Some(id) => id.clone(),
                None => return Err(TError::MissingField(format!("FileData.save() -- `turtl.user_id` is None when saving file... =["))),
            }
        };
        let note_id = match note.id().as_ref() {
            Some(id) => id.clone(),
            None => return Err(TError::MissingField(format!("FileData.save() -- `note.id` is None when saving file...tsk tsk"))),
        };
        let note_key = match note.key() {
            Some(key) => key.clone(),
            None => return Err(TError::MissingField(format!("FileData.save() -- `note.key` is None when saving file...shame, shame"))),
        };

        // the file id should ref the note
        self.id = Some(note_id.clone());

        // rip the `data` field out of the FileData object
        let mut data: Option<Vec<u8>> = None;
        mem::swap(&mut data, &mut self.data);

        // unwrap our data
        let data = match data {
            Some(x) => x,
            None => return Err(TError::MissingField(format!("FileData.save() -- `file.data` is None when saving file...HOW CAN YOU HAVE A FILE IF YOU DON'T GIVE IT DATA?!"))),
        };

        // encrypt the file using the turtl standard serialization format
        let enc = turtl.work.run(move || {
            crypto::encrypt(&note_key, data, crypto::CryptoOp::new("chacha20poly1305")?)
                .map_err(|e| From::from(e))
        })?;

        // now, save the encrypted file data to disk
        let data_folder = config::get::<String>(&["data_folder"])?;
        let mut filepath = PathBuf::from(data_folder);
        filepath.push(FILEDIR);
        util::create_dir(&filepath)?;
        filepath.push(FileData::filebuilder(&user_id, &note_id));
        let mut fs_file = fs::File::create(&filepath)?;
        fs_file.write_all(enc.as_slice())?;

        // phew, now that all went smoothly, create a sync record for the saved
        // file (which will let the sync system know to upload our heroic file)
        let create_sync = move || -> TResult<()> {
            let mut db_guard = turtl.db.write().unwrap();
            let db = match db_guard.as_mut() {
                Some(x) => x,
                None => return Err(TError::MissingField(format!("FileData.save() -- `turtl.db` is None when saving file...can't save sync record (deleting file)"))),
            };
            // run the sync.
            self.outgoing(SyncAction::Add, &user_id, db, false)?;
            Ok(())
        };
        match create_sync() {
            Ok(_) => (),
            Err(e) => {
                match fs::remove_file(&filepath) {
                    Ok(_) => {},
                    Err(e) => {
                        error!("FileData.save() -- error removing saved file: {}", e);
                    }
                }
                return Err(e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi;

    #[test]
    fn filedata_serializes_to_from_base64() {
        let filedata: Vec<u8> = vec![73, 32, 67, 65, 78, 39, 84, 32, 66, 69, 76, 73, 69, 86, 69, 32, 73, 84, 39, 83, 32, 78, 79, 84, 32, 71, 79, 78, 79, 82, 82, 72, 69, 65, 33, 33];
        let mut file: FileData = Default::default();
        file.data = Some(filedata.clone());

        let ser = jedi::stringify(&file).unwrap();
        assert_eq!(ser, r#"{"body":null,"data":"SSBDQU4nVCBCRUxJRVZFIElUJ1MgTk9UIEdPTk9SUkhFQSEh"}"#);

        let file2: FileData = jedi::parse(&ser).unwrap();
        assert_eq!(file2.data.as_ref().unwrap(), &filedata);
    }

    #[test]
    fn can_save_and_load_files() {
        ::process_runtime_config(String::from("")).unwrap();
        let turtl = ::turtl::tests::with_test(true);
        config::set(&["data_folder"], &String::from("/tmp")).unwrap();
        let user_id = {
            let user_guard = turtl.user_id.read().unwrap();
            user_guard.as_ref().unwrap().clone()
        };

        let mut note: Note = jedi::from_val(json!({
            "space_id": "6969",
            "user_id": user_id.clone(),
        })).unwrap();
        note.generate_id().unwrap();
        note.generate_key().unwrap();

        let filedata = jedi::stringify(&json!({
            "name": "flippy",
            "likes": "slippy",
            "dislikes": "slappy",
            "age": 42,
            "lives": {
                "city": "santa cruz brahhhh"
            }
        })).unwrap();

        let mut file: FileData = Default::default();
        file.data = Some(Vec::from(filedata.as_bytes()));

        // talked to drew about encrypting and saving the file. sounds good.
        file.save(&turtl, &mut note).unwrap();
        let loaded = FileData::load_file(&turtl, &note).unwrap();

        assert_eq!(String::from_utf8(loaded).unwrap(), r#"{"age":42,"dislikes":"slappy","likes":"slippy","lives":{"city":"santa cruz brahhhh"},"name":"flippy"}"#);

        let mut db_guard = turtl.db.write().unwrap();
        let db = db_guard.as_mut().unwrap();
        file.db_delete(db).unwrap();

        match FileData::load_file(&turtl, &note) {
            Ok(_) => panic!("Found file for note {}, should be deleted", note.id().as_ref().unwrap()),
            Err(e) => {
                match e {
                    // great.
                    TError::NotFound(_) => {},
                    _ => panic!("{}", e),
                }
            },
        }
    }
}


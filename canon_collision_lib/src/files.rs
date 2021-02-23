use std::fs::{DirBuilder, File};
use std::fs;
use std::path::{PathBuf, Path};

use dirs_next;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;
use serde_cbor;

pub fn build_version() -> String { String::from(env!("BUILD_VERSION")) }

pub fn engine_version() -> u64 { 20 }

pub fn save_struct_json<T: Serialize>(filename: &Path, object: &T) {
    // ensure parent directories exists
    DirBuilder::new().recursive(true).create(filename.parent().unwrap()).unwrap();

    // save
    let json = serde_json::to_string_pretty(object).unwrap();
    std::fs::write(filename, &json.as_bytes()).unwrap();
}

pub fn load_struct_json<T: DeserializeOwned>(filename: &Path) -> Result<T, String> {
    let json = load_file(filename)?;
    serde_json::from_str(&json).map_err(|x| format!("{:?}", x))
}

pub fn load_json(filename: &Path) -> Result<serde_json::Value, String> {
    let json = load_file(filename)?;
    serde_json::from_str(&json).map_err(|x| format!("{:?}", x))
}

pub fn save_struct_cbor<T: Serialize>(filename: &Path, object: &T) {
    // ensure parent directories exists
    DirBuilder::new().recursive(true).create(filename.parent().unwrap()).unwrap();

    // save
    let file = File::create(filename).unwrap();
    serde_cbor::to_writer(file, object).unwrap();
}

pub fn load_cbor(filename: &Path) -> Result<serde_cbor::Value, String> {
    let file = File::open(filename).map_err(|x| format!("{:?}", x))?;
    serde_cbor::from_reader(&file).map_err(|x| format!("{:?}", x))
}

pub fn save_struct_bincode<T: Serialize>(filename: &Path, object: &T) {
    // ensure parent directories exists
    DirBuilder::new().recursive(true).create(filename.parent().unwrap()).unwrap();

    // save
    let file = File::create(filename).unwrap();
    bincode::serialize_into(file, object).unwrap();
}

pub fn load_struct_bincode<T: DeserializeOwned>(filename: &Path) -> Result<T, String> {
    let file = File::open(filename).map_err(|x| format!("{:?}", x))?;
    bincode::deserialize_from(file).map_err(|x| format!("{:?}", x))
}

pub fn load_file(filename: &Path) -> Result<String, String> {
    std::fs::read_to_string(&filename)
        .map_err(|x| format!("Failed to open file: {} because: {}", filename.to_str().unwrap(), x))
}

/// deletes all files in the passed directory
/// if the directory does not exist it is created
pub fn nuke_dir(path: &Path) {
    fs::remove_dir_all(path).ok();
    fs::create_dir_all(path).unwrap();
}

pub fn has_ext(path: &Path, check_ext: &str) -> bool {
    if let Some(ext) = path.extension() {
        if let Some(ext) = ext.to_str() {
            if ext.to_lowercase().as_str() == check_ext {
                return true
            }
        }
    }
    false
}

pub fn get_path() -> PathBuf {
    let mut data_local = dirs_next::data_local_dir().expect("Could not get data_local_dir");
    data_local.push("CanonCollision");
    data_local
}

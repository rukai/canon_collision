use std::fs::{DirBuilder, File};
use std::fs;
use std::io::{Read, Write};
use std::path::{PathBuf, Path};

use dirs;
use reqwest::Url;
use reqwest;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json::Value;
use serde_json;

pub fn build_version() -> String { String::from(env!("BUILD_VERSION")) }

pub fn engine_version() -> u64 { 15 }

pub fn save_struct<T: Serialize>(filename: PathBuf, object: &T) {
    // ensure parent directories exists
    DirBuilder::new().recursive(true).create(filename.parent().unwrap()).unwrap();

    // save
    let json = serde_json::to_string_pretty(object).unwrap();
    File::create(filename).unwrap().write_all(&json.as_bytes()).unwrap();
}

pub fn load_struct<T: DeserializeOwned>(filename: PathBuf) -> Result<T, String> {
    let json = load_file(filename)?;
    serde_json::from_str(&json).map_err(|x| format!("{:?}", x))
}

pub fn load_json(filename: PathBuf) -> Result<Value, String> {
    let json = load_file(filename)?;
    serde_json::from_str(&json).map_err(|x| format!("{:?}", x))
}

pub fn save_struct_cbor<T: Serialize>(filename: PathBuf, object: &T) {
    // ensure parent directories exists
    DirBuilder::new().recursive(true).create(filename.parent().unwrap()).unwrap();

    // save
    let file = File::create(filename).unwrap();
    serde_cbor::to_writer(file, object).unwrap();
}

pub fn save_struct_bincode<T: Serialize>(filename: PathBuf, object: &T) {
    // ensure parent directories exists
    DirBuilder::new().recursive(true).create(filename.parent().unwrap()).unwrap();

    // save
    let file = File::create(filename).unwrap();
    bincode::serialize_into(file, object).unwrap();
}

pub fn load_struct_bincode<T: DeserializeOwned>(filename: PathBuf) -> Result<T, String> {
    let file = File::open(filename).map_err(|x| format!("{:?}", x))?;
    bincode::deserialize_from(file).map_err(|x| format!("{:?}", x))
}

pub fn load_file(filename: PathBuf) -> Result<String, String> {
    let mut file = match File::open(&filename) {
        Ok(file) => file,
        Err(err) => return Err(format!("Failed to open file: {} because: {}", filename.to_str().unwrap(), err))
    };

    let mut contents = String::new();
    if let Err(err) = file.read_to_string(&mut contents) {
        return Err(format!("Failed to read file {} because: {}", filename.to_str().unwrap(), err))
    };
    Ok(contents)
}

/// Load the json file at the passed URL directly into a struct
pub fn load_struct_from_url<T: DeserializeOwned>(url: Url) -> Option<T> {
    if let Ok(mut response) = reqwest::get(url) {
        if response.status().is_success() {
            return response.json().ok();
        }
    }
    None
}

/// Returns the bytes of the file stored at the url
pub fn load_bin_from_url(url: Url) -> Option<Vec<u8>> {
    if let Ok(mut response) = reqwest::get(url) {
        if response.status().is_success() {
            let mut buf: Vec<u8> = vec!();
            if let Ok(_) = response.read_to_end(&mut buf) {
                return Some(buf);
            }
        }
    }
    None
}

/// deletes all files in the passed directory
/// if the directory does not exist it is created
pub fn nuke_dir(path: &Path) {
    fs::remove_dir_all(path).ok();
    fs::create_dir_all(path).unwrap();
}

pub fn has_ext(path: &PathBuf, check_ext: &str) -> bool {
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
    let mut data_local = dirs::data_local_dir().expect("Could not get data_local_dir");
    data_local.push("CanonCollision");
    data_local
}

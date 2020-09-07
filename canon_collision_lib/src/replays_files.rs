use std::fs;
use std::path::PathBuf;
use std::cmp::Ordering;

use chrono::DateTime;

use crate::files;

pub fn get_replay_names() -> Vec<String> {
    let mut result: Vec<String> = vec!();
    
    if let Ok(files) = fs::read_dir(get_replays_dir_path()) {
        for file in files {
            if let Ok(file) = file {
                let file_name = file.file_name().into_string().unwrap();
                if let Some(split_point) = file_name.rfind('.') {
                    let (name, ext) = file_name.split_at(split_point);
                    if ext.to_lowercase() == ".zip" {
                        result.push(name.to_string());
                    }
                }
            }
        }
    }

    // Most recent dates come first
    // Dates come before non-dates
    // Non-dates are sorted alphabetically
    result.sort_by(
        |a, b| {
            let a_dt = DateTime::parse_from_rfc2822(a);
            let b_dt = DateTime::parse_from_rfc2822(b);
            if a_dt.is_err() && b_dt.is_err() {
                a.cmp(b)
            } else {
                if let Ok(a_dt) = a_dt {
                    if let Ok(b_dt) = b_dt {
                        a_dt.cmp(&b_dt).reverse()
                    } else {
                        Ordering::Less
                    }
                } else {
                    Ordering::Greater
                }
            }
        }
    );
    result
}

fn get_replays_dir_path() -> PathBuf {
    let mut replays_path = files::get_path();
    replays_path.push("replays");
    replays_path
}

pub fn get_replay_path(name: &str) -> PathBuf {
    let mut replay_path = get_replays_dir_path();
    replay_path.push(name.to_string());
    replay_path
}

pub fn delete_replay(name: &str) {
    fs::remove_file(get_replay_path(&format!("{}.zip", name))).ok();
}

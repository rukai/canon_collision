use std::borrow::Cow;
use std::env;
use std::panic::PanicInfo;
use std::panic;
use std::path::{Path, PathBuf};
use std::process::Command;

use backtrace::Backtrace;
use os_type;
use uuid::Uuid;
use toml;
use crate::files;

/// Enables the panic handler.
/// Only use on Canon Collision applications with a gui
/// Automatically provides the build_version and crate_name
#[macro_export]
macro_rules! setup_panic_handler {
    () => {
        // implemented with a macro so that BUILD_VERSION/CARGO_PKG_NAME are from the actual crate
        use canon_collision_lib::panic_handler;
        panic_handler::setup(env!("BUILD_VERSION"), env!("CARGO_PKG_NAME"));
    }
}

/// Enables the panic handler.
/// Only use on Canon Collision applications with a gui
pub fn setup(build_version: &'static str, crate_name: &'static str) {
    // only enable if the handler exists, this ensures we get regular panics if run in a dev
    // environment and the panic handler isnt built.
    //
    // I have considered immediately quitting Canon Collision if it cant locate the panic handler i.e. the user has moved
    // the exe to a different folder. However that would break the above case.
    let pfs_dev_not_true = env::var("CC_DEV").map(|x| x.to_lowercase() != "true").unwrap_or(true);
    let handler_exists = path_to_handler().map(|x| x.exists()).unwrap_or(false);
    if pfs_dev_not_true && handler_exists {
        panic::set_hook(Box::new(move |panic_info: &PanicInfo| {
            let (location_file, location_line, location_column) = match panic_info.location() {
                Some(loc) => (Some(loc.file().to_string()), Some(loc.line()), Some(loc.column())),
                None      => (None, None, None)
            };

            let operating_system = if cfg!(windows) {
                "windows".into()
            } else {
                let platform = os_type::current_platform();
                format!("unix:{:?}", platform.os_type).into()
            };

            let report = Report {
                backtrace:     format!("{:#?}", Backtrace::new()),
                build_version: build_version.into(),
                crate_name:    crate_name.into(),
                payload:       panic_info.payload().downcast_ref::<&str>().map(|x| x.to_string()),
                operating_system,
                location_file,
                location_line,
                location_column,
            };

            match report.to_file() {
                Ok(file_path) => call_helper(file_path),
                Err(err) => {
                    match toml::to_string_pretty(&report) {
                        Ok(report)   => eprintln!("Failed to save report to file: {}\nReport:\n{}", err, report),
                        Err(ser_err) => eprintln!("Failed to save report to file: {}\nFailed to serialize report: {}", err, ser_err)
                    }
                }
            }
        }));
    }
}

fn path_to_handler() -> Result<PathBuf, String> {
    match env::current_exe() {
        Ok(mut path) => {
            path.pop();
            if cfg!(target_os = "windows") {
                path.push("panic_handler.exe");
            } else {
                path.push("panic_handler");
            }
            Ok(path)
        }
        Err(err) => Err(format!("Failed to get path of executable: {}", err))
    }
}

fn call_helper(dump_path: PathBuf) {
    match path_to_handler() {
        Ok(path) => {
            match (dump_path.into_os_string().into_string(), path.into_os_string().into_string()) {
                (Ok(dump_path), Ok(path)) => {
                    if let Err(err) = Command::new(path).arg(dump_path).status() {
                        eprintln!("Failed to run the panic handler: {}", err);
                    }
                }
                (a, b) => {
                    eprintln!("Failed to convert paths to string:\n{:?}\n{:?}", a, b);
                }
            }
        }
        Err(err) => eprintln!("{}", err),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub crate_name:       String,
    pub build_version:    String,
    pub payload:          Option<String>,
    pub location_file:    Option<String>,
    pub location_line:    Option<u32>,
    pub location_column:  Option<u32>,
    pub backtrace:        String,
    pub operating_system: Cow<'static, str>,
}

impl Report {
    /// Write the report to disk
    pub fn to_file(&self) -> Result<PathBuf, String> {
        let uuid = Uuid::new_v4().to_hyphenated().to_string();
        let tmp_dir = env::temp_dir();
        if let Some(tmp_dir) = tmp_dir.to_str() {
            let file_name = format!("canon-collision-panic-{}.toml", &uuid);
            let file_path = Path::new(tmp_dir).join(file_name);
            let toml = toml::to_string_pretty(&self).map_err(|x| format!("{:?}", x))?;
            std::fs::write(&file_path, toml.as_bytes()).map_err(|x| format!("{:?}", x))?;
            Ok(file_path)
        } else {
            Err(String::from("Couldnt get a temp directory"))
        }
    }

    /// Read a report from disk
    pub fn from_file(file_name: &str) -> Result<Report, String> {
        let text = files::load_file(Path::new(file_name))?;
        toml::from_str(&text).map_err(|x| format!("{:?}", x))
    }
}

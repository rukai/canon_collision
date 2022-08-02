use crate::files;
use crate::files::engine_version;

use std::path::PathBuf;

use serde_json;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct ControllerMaps {
    pub engine_version: u64,
    pub maps: Vec<ControllerMap>,
}

impl ControllerMaps {
    fn get_path() -> PathBuf {
        let mut path = files::get_path();
        path.push("controller_maps.json");
        path
    }

    pub fn load() -> ControllerMaps {
        if let Ok(json) = files::load_json(&ControllerMaps::get_path()) {
            if let Ok(maps) = serde_json::from_value::<ControllerMaps>(json) {
                return maps;
            }
        }

        warn!(
            "{:?} is invalid or does not exist, loading default values",
            ControllerMaps::get_path()
        );
        let maps = include_str!("controller_maps.json");
        serde_json::from_str(maps).unwrap()
    }

    pub fn save(&self) {
        files::save_struct_json(&ControllerMaps::get_path(), self);
    }
}

impl Default for ControllerMaps {
    fn default() -> ControllerMaps {
        ControllerMaps {
            engine_version: engine_version(),
            maps: vec![],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ControllerMap {
    pub os: OS,
    pub uuid: Uuid,
    pub name: String,
    pub analog_maps: Vec<AnalogMap>,
    pub digital_maps: Vec<DigitalMap>,
}

impl ControllerMap {
    pub fn get_digital_maps(&self, dest: DigitalDest) -> Vec<(usize, DigitalMap)> {
        let mut result = vec![];
        for (index, map) in self.digital_maps.iter().enumerate() {
            if dest == map.dest {
                result.push((index, map.clone()));
            }
        }
        result
    }

    pub fn get_analog_maps(&self, dest: AnalogDest) -> Vec<(usize, AnalogMap)> {
        let mut result = vec![];
        for (index, map) in self.analog_maps.iter().enumerate() {
            if dest == map.dest {
                result.push((index, map.clone()));
            }
        }
        result
    }

    pub fn get_fullname(&self) -> String {
        format!("{} - {}", self.name, self.uuid)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum OS {
    Windows,
    Linux,
}

impl OS {
    pub fn get_current() -> OS {
        if cfg!(target_os = "linux") {
            OS::Linux
        } else if cfg!(target_os = "windows") {
            OS::Windows
        } else {
            panic!("OS not supported");
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AnalogMap {
    pub source: usize,
    pub dest: AnalogDest,
    pub filter: AnalogFilter,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DigitalMap {
    pub source: usize,
    pub dest: DigitalDest,
    pub filter: DigitalFilter,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AnalogFilter {
    FromDigital { value: f32 }, // is value when true otherwise unchanged, starting at 0 (can stack multiple AnalogMap's in this way)
    FromAnalog { min: i32, max: i32, flip: bool }, // map the analog value from [min, max] to [-1.0, 1.0], flipping if enabled.
}

impl AnalogFilter {
    pub fn default_digital() -> AnalogFilter {
        AnalogFilter::FromDigital { value: 1.0 }
    }

    pub fn default_analog() -> AnalogFilter {
        AnalogFilter::FromAnalog {
            min: -1,
            max: 1,
            flip: false,
        }
    }

    pub fn is_digital_source(&self) -> bool {
        match self {
            AnalogFilter::FromDigital { .. } => true,
            AnalogFilter::FromAnalog { .. } => false,
        }
    }

    pub fn set_min(&mut self, new_min: i32) {
        match self {
            AnalogFilter::FromAnalog { min, .. } => *min = new_min,
            AnalogFilter::FromDigital { .. } => unreachable!(),
        }
    }

    pub fn set_max(&mut self, new_max: i32) {
        match self {
            AnalogFilter::FromAnalog { max, .. } => *max = new_max,
            AnalogFilter::FromDigital { .. } => unreachable!(),
        }
    }

    pub fn set_flip(&mut self, new_flip: bool) {
        match self {
            AnalogFilter::FromAnalog { flip, .. } => *flip = new_flip,
            AnalogFilter::FromDigital { .. } => unreachable!(),
        }
    }

    pub fn set_value(&mut self, new_value: f32) {
        match self {
            AnalogFilter::FromDigital { value, .. } => *value = new_value,
            AnalogFilter::FromAnalog { .. } => unreachable!(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum DigitalFilter {
    FromDigital,
    FromAnalog { min: i32, max: i32 }, // true if between min and max false otherwise
}

impl DigitalFilter {
    pub fn default_digital() -> DigitalFilter {
        DigitalFilter::FromDigital
    }

    pub fn default_analog() -> DigitalFilter {
        DigitalFilter::FromAnalog { min: 1, max: 2 }
    }

    pub fn is_digital_source(&self) -> bool {
        match self {
            DigitalFilter::FromDigital => true,
            DigitalFilter::FromAnalog { .. } => false,
        }
    }

    pub fn set_min(&mut self, new_min: i32) {
        match self {
            DigitalFilter::FromAnalog { min, .. } => *min = new_min,
            DigitalFilter::FromDigital => unreachable!(),
        }
    }

    pub fn set_max(&mut self, new_max: i32) {
        match self {
            DigitalFilter::FromAnalog { max, .. } => *max = new_max,
            DigitalFilter::FromDigital => unreachable!(),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum AnalogDest {
    StickX,
    StickY,
    CStickX,
    CStickY,
    RTrigger,
    LTrigger,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum DigitalDest {
    A,
    B,
    X,
    Y,
    Left,
    Right,
    Down,
    Up,
    Start,
    Z,
    R,
    L,
}

#[test]
pub fn controller_maps_file_is_valid() {
    let maps = include_str!("controller_maps.json");
    let _controller_maps: ControllerMaps = serde_json::from_str(maps).unwrap();
}

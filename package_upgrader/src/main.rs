//use serde_json::{Value, Number};
use canon_collision_lib::package::Package;

/// This code is checked in to:
/// *   refer back to past changes
/// *   copy paste from previous similar transforms
/// *   allow for review of transforms run in a PR
/// *   makes it possible to restore old deleted deleted files to the current format (via per file versioning)
fn main() {
    if std::env::args().any(|x| x.to_lowercase() == "init") {
        let path = std::env::current_dir().unwrap().join("package");
        Package::generate_base(path);
        println!("Created an empty package in the current directory.");
        return;
    }

    if let Some(_path) = Package::find_package_in_parent_dirs() {
        // TODO: Call upgrade_to_latest_fighter etc on each cbor file
    } else {
        println!("Could not find package in current directory or any of its parent directories.");
    }
}

//fn get_engine_version(object: &Value) -> u64 {
//    if let &Value::Object (ref object) = object {
//        if let Some (engine_version) = object.get("engine_version") {
//            if let Some (value) = engine_version.as_u64() {
//                return value
//            }
//        }
//    }
//    engine_version()
//}
//
//fn upgrade_engine_version(meta: &mut Value) {
//    if let &mut Value::Object (ref mut object) = meta {
//        object.insert(String::from("engine_version"), engine_version_json());
//    }
//}
//
//pub(crate) fn upgrade_to_latest_fighter(fighter: &mut Value, file_name: &str) {
//    let fighter_engine_version = get_engine_version(fighter);
//    if fighter_engine_version > engine_version() {
//        println!("Fighter: {} is newer than this version of PF Sandbox. Please upgrade to the latest version.", file_name);
//    }
//    else if fighter_engine_version < engine_version() {
//        for upgrade_from in fighter_engine_version..engine_version() {
//            match upgrade_from {
//                //3  => { upgrade_fighter3(fighter) }
//                //2  => { upgrade_fighter2(fighter) }
//                //1  => { upgrade_fighter1(fighter) }
//                //0  => { upgrade_fighter0(fighter) }
//                _ => { }
//            }
//        }
//        upgrade_engine_version(fighter);
//    }
//}
//
//pub(crate) fn upgrade_to_latest_stage(stage: &mut Value, file_name: &str) {
//    let stage_engine_version = get_engine_version(stage);
//    if stage_engine_version > engine_version() {
//        println!("Stage: {} is newer than this version of PF Sandbox. Please upgrade to the latest version.", file_name);
//    }
//    else if stage_engine_version < engine_version() {
//        // TODO: Handle upgrades here
//        upgrade_engine_version(stage);
//    }
//}
//
//pub(crate) fn upgrade_to_latest_rules(rules: &mut Value) {
//    let rules_engine_version = get_engine_version(rules);
//    if rules_engine_version > engine_version() {
//        println!("rules.json is newer than this version of PF Sandbox. Please upgrade to the latest version.");
//    }
//    else if rules_engine_version < engine_version() {
//        // TODO: Handle upgrades here
//        upgrade_engine_version(rules);
//    }
//}
//
//pub(crate) fn upgrade_to_latest_meta(meta: &mut Value) {
//    let meta_engine_version = get_engine_version(meta);
//    if meta_engine_version > engine_version() {
//        println!("meta.json is newer than this version of PF Sandbox. Please upgrade to the latest version.");
//    }
//    else if meta_engine_version < engine_version() {
//        // TODO: Handle upgrades here
//        upgrade_engine_version(meta);
//    }
//}

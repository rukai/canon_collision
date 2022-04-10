use serde_cbor::{value, Value};
use strum::IntoEnumIterator;

use canon_collision_lib::entity_def::player::PlayerAction;
use canon_collision_lib::entity_def::EntityDef;
use canon_collision_lib::files::{engine_version, load_cbor, save_struct_cbor};
use canon_collision_lib::package::Package;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// This code is checked in to:
/// *   refer back to past changes
/// *   copy paste from previous similar transforms
/// *   allow for review of transforms run in a PR
/// *   makes it possible to restore old deleted files to the current format (via per file versioning)
fn main() {
    if std::env::args().any(|x| x.to_lowercase() == "init") {
        let path = std::env::current_dir().unwrap().join("package");
        Package::generate_base(path);
        println!("Created an empty package in the current directory.");
        return;
    }

    if std::env::args().any(|x| x.to_lowercase() == "action_indexes") {
        for action in PlayerAction::iter() {
            println!("{:?} {}", &action, action.clone() as usize);
        }
        return;
    }

    let dry_run = std::env::args().any(|x| x.to_lowercase() == "dryrun");

    if let Some(package_path) = Package::find_package_in_parent_dirs() {
        if let Ok(dir) = fs::read_dir(package_path.join("Entities")) {
            for path in dir {
                let full_path = path.unwrap().path();
                upgrade_to_latest_entity(&full_path, dry_run);
            }
        }
    } else {
        println!("Could not find package in current directory or any of its parent directories.");
    }
}

fn get_engine_version(object: &Value) -> u64 {
    if let &Value::Map(ref map) = object {
        if let Some(engine_version) = map.get(&Value::Text("engine_version".into())) {
            if let Value::Integer(value) = engine_version {
                return *value as u64;
            }
        }
    }
    engine_version()
}

fn upgrade_engine_version(meta: &mut Value) {
    if let &mut Value::Map(ref mut map) = meta {
        map.insert(
            Value::Text(String::from("engine_version")),
            Value::Integer(engine_version() as i128),
        );
    }
}

fn get_vec<'a>(parent: &'a mut Value, member: &str) -> Option<&'a mut Vec<Value>> {
    if let &mut Value::Map(ref mut map) = parent {
        if let Some(array) = map.get_mut(&Value::Text(member.into())) {
            if let &mut Value::Array(ref mut array) = array {
                return Some(array);
            }
        }
    }
    return None;
}

fn new_object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        map.insert(Value::Text(key.into()), value);
    }
    Value::Map(map)
}

fn upgrade_to_latest_entity(path: &Path, dry_run: bool) {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let mut entity = load_cbor(path).unwrap();
    let entity_engine_version = get_engine_version(&entity);
    if entity_engine_version > engine_version() {
        panic!(
            "EntityDef: {} is newer than this version of Canon Collision.",
            path.file_name().unwrap().to_str().unwrap()
        );
    } else if entity_engine_version < engine_version() {
        for upgrade_from in entity_engine_version..engine_version() {
            match upgrade_from {
                19 => upgrade_entity19(&mut entity),
                18 => upgrade_entity18(&mut entity, file_name),
                17 => upgrade_entity17(&mut entity),
                16 => upgrade_entity16(&mut entity),
                15 => upgrade_entity15(&mut entity),
                _ => {}
            }
        }
        upgrade_engine_version(&mut entity);
    }

    // convert to EntityDef to ensure result is deserializable before writing to disk
    let entity: EntityDef = value::from_value(entity.into()).unwrap();

    if dry_run {
        print!("dry run: ");
    } else {
        save_struct_cbor(path, &entity);
    }

    println!(
        "Upgraded entity from version {} to version {}.",
        entity_engine_version,
        engine_version()
    );
}

fn upgrade_entity19(entity: &mut Value) {
    if let Value::Map(entity) = entity {
        entity.insert(Value::Text("css_action".into()), Value::Text("Idle".into()));
        if let Some(entity_type) = entity.get_mut(&Value::Text("ty".into())) {
            if let Value::Map(entity_type) = entity_type {
                if let Some(fighter) = entity_type.get_mut(&Value::Text("Fighter".into())) {
                    if let Value::Map(fighter) = fighter {
                        fighter.insert(Value::Text("ty".into()), Value::Text("Toriel".into()));
                    }
                }
            }
        }
    }
}

fn upgrade_entity18(entity: &mut Value, file_name: &str) {
    let item_action_names = ["Spawn", "Idle", "Fall", "Held", "Thrown", "Dropped"];

    let projectile_action_names = ["Spawn", "Travel", "Hit"];

    let fighter_action_names = [
        "Spawn",
        "ReSpawn",
        "ReSpawnIdle",
        "Idle",
        "Crouch",
        "LedgeIdle",
        "Teeter",
        "TeeterIdle",
        "MissedTechIdle",
        // Movement
        "Fall",
        "AerialFall",
        "Land",
        "JumpSquat",
        "JumpF",
        "JumpB",
        "JumpAerialF",
        "JumpAerialB",
        "TiltTurn",
        "RunTurn",
        "SmashTurn",
        "Dash",
        "Run",
        "RunEnd",
        "Walk",
        "PassPlatform",
        "Damage",
        "DamageFly",
        "DamageFall",
        "LedgeGrab",
        "LedgeJump",
        "LedgeJumpSlow",
        "LedgeGetup",
        "LedgeGetupSlow",
        "LedgeIdleChain",
        // Defense
        "PowerShield",
        "ShieldOn",
        "Shield",
        "ShieldOff",
        "RollF",
        "RollB",
        "SpotDodge",
        "AerialDodge",
        "SpecialFall",
        "SpecialLand",
        "TechF",
        "TechN",
        "TechB",
        "MissedTechGetupF",
        "MissedTechGetupN",
        "MissedTechGetupB",
        "Rebound",
        "LedgeRoll",
        "LedgeRollSlow",
        // Vulnerable
        "ShieldBreakFall",
        "ShieldBreakGetup",
        "Stun",
        "MissedTechStart",
        // Attack",
        "Jab",
        "Jab2",
        "Jab3",
        "Utilt",
        "Dtilt",
        "Ftilt",
        "DashAttack",
        "Usmash",
        "Dsmash",
        "Fsmash",
        // Grabs
        "Grab",
        "DashGrab",
        "GrabbingIdle",
        "GrabbingEnd",
        "GrabbedIdleAir",
        "GrabbedIdle",
        "GrabbedEnd",
        // Throws
        "Uthrow",
        "Dthrow",
        "Fthrow",
        "Bthrow",
        // Items
        "ItemGrab",
        "ItemEat",
        "ItemThrowU",
        "ItemThrowD",
        "ItemThrowF",
        "ItemThrowB",
        "ItemThrowAirU",
        "ItemThrowAirD",
        "ItemThrowAirF",
        "ItemThrowAirB",
        // Getup attacks
        "LedgeAttack",
        "LedgeAttackSlow",
        "MissedTechAttack",
        // Aerials
        "Uair",
        "Dair",
        "Fair",
        "Bair",
        "Nair",
        "UairLand",
        "DairLand",
        "FairLand",
        "BairLand",
        "NairLand",
        // Taunts
        "TauntUp",
        "TauntDown",
        "TauntLeft",
        "TauntRight",
        // Crouch
        "CrouchStart",
        "CrouchEnd",
        "Eliminated",
        "DummyFramePreStart",
    ];

    let action_names = match file_name {
        "PerfectlyGenericProjectile.cbor" => projectile_action_names.as_ref(),
        "PerfectlyGenericObject.cbor" => item_action_names.as_ref(),
        "TorielFireball.cbor" => projectile_action_names.as_ref(),
        _ => fighter_action_names.as_ref(),
    };

    if let Some(actions) = get_vec(entity, "actions") {
        let mut new_actions = BTreeMap::new();
        new_actions.insert(
            Value::Text("keys".into()),
            Value::Array(
                action_names
                    .iter()
                    .map(|x| Value::Text(x.to_string()))
                    .collect(),
            ),
        );
        new_actions.insert(
            Value::Text("vector".into()),
            Value::Array(actions[0..action_names.len()].to_vec()),
        );

        if let Value::Map(entity) = entity {
            entity.insert(Value::Text("actions".into()), Value::Map(new_actions));
        }
    }
}

fn upgrade_entity17(entity: &mut Value) {
    let action = new_object(vec![
        (
            "frames",
            Value::Array(vec![new_object(vec![
                (
                    "ecb",
                    new_object(vec![
                        ("top", Value::Float(16.0)),
                        ("left", Value::Float(-4.0)),
                        ("right", Value::Float(4.0)),
                        ("bottom", Value::Float(0.0)),
                    ]),
                ),
                ("colboxes", Value::Array(vec![])),
                ("item_hold_x", Value::Float(4.0)),
                ("item_hold_y", Value::Float(11.0)),
                ("grabbing_x", Value::Float(8.0)),
                ("grabbing_y", Value::Float(11.0)),
                ("grabbed_x", Value::Float(4.0)),
                ("grabbed_y", Value::Float(11.0)),
                ("pass_through", Value::Bool(true)),
                ("ledge_cancel", Value::Bool(true)),
                ("use_platform_angle", Value::Bool(false)),
                ("ledge_grab_box", Value::Null),
                ("force_hitlist_reset", Value::Bool(false)),
                ("x_vel_modify", Value::Text("None".into())),
                ("y_vel_modify", Value::Text("None".into())),
                ("x_vel_temp", Value::Float(0.0)),
                ("y_vel_temp", Value::Float(0.0)),
            ])]),
        ),
        ("iasa", Value::Integer(0)),
    ]);

    let action_indexes = [78, 79, 80, 81, 82, 83, 84, 85, 86, 87];
    if let Some(actions) = get_vec(entity, "actions") {
        for action_index in &action_indexes {
            actions.insert(*action_index, action.clone());
        }
    }
}

fn upgrade_entity16(entity: &mut Value) {
    if let Value::Map(entity) = entity {
        entity.insert(Value::Text("ty".into()), Value::Text("Generic".into()));
    }
}

fn upgrade_entity15(entity: &mut Value) {
    if let Value::Map(entity) = entity {
        entity.insert(Value::Text("ledge_grab_x".into()), Value::Float(-2.0));
        entity.insert(Value::Text("ledge_grab_y".into()), Value::Float(-24.0));
    }

    for action in get_vec(entity, "actions").unwrap() {
        for frame in get_vec(action, "frames").unwrap() {
            if let Value::Map(frame) = frame {
                frame.insert(Value::Text("grabbing_x".into()), Value::Float(8.0));
                frame.insert(Value::Text("grabbing_y".into()), Value::Float(11.0));
                frame.insert(Value::Text("grabbed_x".into()), Value::Float(4.0));
                frame.insert(Value::Text("grabbed_y".into()), Value::Float(11.0));
            }
        }
    }

    let action = new_object(vec![
        (
            "frames",
            Value::Array(vec![new_object(vec![
                (
                    "ecb",
                    new_object(vec![
                        ("top", Value::Float(16.0)),
                        ("left", Value::Float(-4.0)),
                        ("right", Value::Float(4.0)),
                        ("bottom", Value::Float(0.0)),
                    ]),
                ),
                ("colboxes", Value::Array(vec![])),
                ("item_hold_x", Value::Float(4.0)),
                ("item_hold_y", Value::Float(11.0)),
                ("grabbing_x", Value::Float(8.0)),
                ("grabbing_y", Value::Float(11.0)),
                ("grabbed_x", Value::Float(4.0)),
                ("grabbed_y", Value::Float(11.0)),
                ("pass_through", Value::Bool(true)),
                ("ledge_cancel", Value::Bool(true)),
                ("use_platform_angle", Value::Bool(false)),
                ("ledge_grab_box", Value::Null),
                ("force_hitlist_reset", Value::Bool(false)),
                ("x_vel_modify", Value::Text("None".into())),
                ("y_vel_modify", Value::Text("None".into())),
                ("x_vel_temp", Value::Float(0.0)),
                ("y_vel_temp", Value::Float(0.0)),
            ])]),
        ),
        ("iasa", Value::Integer(0)),
    ]);

    let action_indexes = [33, 69, 70, 71, 72, 73, 74, 75, 76, 77];
    if let Some(actions) = get_vec(entity, "actions") {
        for action_index in &action_indexes {
            actions.insert(*action_index, action.clone());
        }
    }
}

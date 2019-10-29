mod cli;

use canon_collision_lib::package::Package;
use canon_collision_lib::fighter::{Action, ActionDef};
use cli::CLIResults;
use cgmath::Vector3;
use std::collections::HashMap;

fn main() {
    let cli = cli::cli();

    if let Some(fighter_key) = &cli.fighter_name {
        let mut package = if let Some(path) = Package::find_package_in_parent_dirs() {
            if let Some(package) = Package::open(path) {
                package
            } else {
                println!("Could not load package");
                return;
            }
        }
        else {
            println!("Could not find package/ in current directory or any of its parent directories.");
            return;
        };

        let hurtboxes = get_hurtboxes();

        if let Some(ref mut fighter) = package.fighters.key_to_value_mut(&fighter_key) {
            if let Some(hurtboxes) = hurtboxes.get(fighter_key) {
                for (i, ref mut action) in (*fighter.actions).iter_mut().enumerate() {
                    let key = Action::action_index_to_string(i);
                    if cli.action_names.len() == 0 || cli.action_names.contains(&key) {
                        regenerate_action(action, &cli, &hurtboxes);
                    }
                }
                package.save();
            }
            else {
                println!("Hurtboxes hashmap does not contain fighter: {}", fighter_key);
            }
        }
        else {
            println!("Package does not contain fighter: {}", fighter_key);
        }
    }
}

fn get_hurtboxes() -> HashMap<String, Vec<HurtBox>> {
    let mut hurtboxes = HashMap::new();

    hurtboxes.insert(
        "Toriel.cbor".into(),
        vec!(
            HurtBox::new("Head", 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("ArmL", 1.0, 0.0, 0.0, 0.0),
        )
    );

    hurtboxes.insert(
        "Dave.cbor".into(),
        vec!(
            HurtBox::new("Head", 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("ArmL", 1.0, 0.0, 0.0, 0.0),
        )
    );

    hurtboxes
}

fn regenerate_action(action: &mut ActionDef, cli: &CLIResults, hurtboxes: &[HurtBox]) {
    println!("Hello, world!");
}

struct HurtBox {
    pub bone:   String,
    pub radius: f32,
    pub offset: Vector3<f32>,
    //pub length: f32, // repeats the circle along this axis, calculate some sensible overlap ratio based on this and the radius
}

impl HurtBox {
    fn new(bone: &str, radius: f32, offset_x: f32, offset_y: f32, offset_z: f32) -> HurtBox {
        HurtBox {
            bone: bone.into(),
            radius,
            offset: Vector3::new(offset_x, offset_y, offset_z),
        }
    }
}

mod cli;
mod model;
mod hurtbox;
mod animation;
// TODO: Move duplicate code in hurtbox and animation modules into canon_collision_lib

use canon_collision_lib::assets::Assets;
use canon_collision_lib::fighter::{Action, ActionDef, ActionFrame, CollisionBoxRole, CollisionBox};
use canon_collision_lib::package::Package;
use cli::CLIResults;
use model::{Model3D, Joint, Animation};
use hurtbox::HurtBox;

use cgmath::{Point3, Vector3, Matrix4, SquareMatrix, Transform, VectorSpace};

fn main() {
    let cli = cli::cli();

    let mut assets = Assets::new().unwrap();

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

        let hurtboxes = hurtbox::get_hurtboxes();

        if let Some(ref mut fighter) = package.fighters.key_to_value_mut(&fighter_key) {
            let model_name = fighter.name.replace(" ", "");
            let model = if let Some(data) = assets.get_model(&model_name) {
                Model3D::from_gltf(&data, &model_name)
            } else {
                println!("Model does not exist for fighter: {}", fighter_key);
                return;
            };

            let hurtboxes = if let Some(hurtboxes) = hurtboxes.get(fighter_key) {
                hurtboxes
            } else {
                println!("Hurtboxes hashmap does not contain fighter: {}", fighter_key);
                return;
            };

            for (i, ref mut action) in (*fighter.actions).iter_mut().enumerate() {
                let action_key = Action::action_index_to_string(i);
                if cli.action_names.len() == 0 || cli.action_names.contains(&action_key) {
                    if let Some(animation) = model.animations.get(&action_key) {
                        regenerate_action(action, &model.root_joint, animation, &cli, &hurtboxes);
                    }
                    else {
                        println!("Action '{}' does not have a corresponding animation, skipping.", action_key);
                    }
                }
            }
            package.save();
        }
        else {
            println!("Package does not contain fighter: {}", fighter_key);
        }
    }
}

fn regenerate_action(action: &mut ActionDef, root_joint: &Joint, animation: &Animation, cli: &CLIResults, hurtboxes: &[HurtBox]) {
    if cli.resize {
        let frames = animation.len().max(1);
        while action.frames.len() > frames {
            action.frames.pop();
        }
        while action.frames.len() < frames {
            action.frames.push(ActionFrame::default());
        }
    }

    for frame in action.frames.iter_mut() {
        if cli.delete_hitboxes {
            frame.colboxes.clear();
        }
        else {
            for i in (0..frame.colboxes.len()).rev() {
                if let CollisionBoxRole::Hurt(_) = frame.colboxes[i].role {
                    frame.colboxes.remove(i);
                }
            }
        }
    }

    for (i, frame) in action.frames.iter_mut().enumerate() {
        let mut root_joint = root_joint.clone();
        let animation_frame = i as f32;
        animation::set_animated_joints(animation, animation_frame, &mut root_joint, Matrix4::identity());
        for hurtbox in hurtboxes {
            generate_hurtbox(frame, &root_joint, hurtbox);
        }
    }
}

fn generate_hurtbox(frame: &mut ActionFrame, root_joint: &Joint, hurtbox: &HurtBox) {
    for child in &root_joint.children {
        generate_hurtbox(frame, child, hurtbox);
    }

    if root_joint.name == hurtbox.bone {
        let role = CollisionBoxRole::Hurt(Default::default());
        let radius = hurtbox.radius; // TODO: Multiply by scale of transform

        let count = (hurtbox.bone_length / radius) as usize;
        let transform = &root_joint.transform;
        let o = &hurtbox.offset;
        let point1 = transform.transform_point(Point3::new(o.x, o.y, o.z));
        let point2 = transform.transform_point(Point3::new(o.x, o.y + hurtbox.bone_length, o.z));

        if count > 1 {
            for i in 0..count {
                let point1 = Vector3::new(point1.x, point1.y, point1.z);
                let point2 = Vector3::new(point2.x, point2.y, point2.z);
                let lerped = point1.lerp(point2, i as f32 / count as f32);
                let point = (lerped.z, lerped.y);
                let role = role.clone();
                frame.colboxes.push(CollisionBox { point, radius, role });
            }
        }
        else {
            let point = (point1.z, point1.y);
            frame.colboxes.push(CollisionBox { point, radius, role });
        }
    }
}

mod cli;
mod model;
mod hurtbox;
mod animation;
// TODO: Move duplicate code in hurtbox and animation modules into canon_collision_lib

use canon_collision_lib::assets::Assets;
use canon_collision_lib::entity_def::{Action, ActionDef, ActionFrame, CollisionBoxRole, CollisionBox, ItemHold};
use canon_collision_lib::package::Package;
use cli::CLIResults;
use model::{Model3D, Joint, Animation};
use hurtbox::HurtBox;

use cgmath::{Point3, Vector3, Matrix4, SquareMatrix, Transform, VectorSpace, Rad};
use std::f32;

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

        if let Some(ref mut fighter) = package.entities.key_to_value_mut(&fighter_key) {
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

        generate_item_hold(frame, &root_joint, "Hand.R");
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

fn generate_item_hold(frame: &mut ActionFrame, root_joint: &Joint, bone_name: &str) {
    for child in &root_joint.children {
        generate_item_hold(frame, child, bone_name);
    }

    if root_joint.name == bone_name {
        let transform = Matrix4::from_angle_y(Rad(f32::consts::PI / 2.0)) * &root_joint.transform;
        let point = transform.transform_point(Point3::new(0.0, 0.0, 0.0));
        let quaternion = matrix_to_quaternion(&transform);

        if frame.item_hold.is_some() {
            frame.item_hold = Some(ItemHold {
                translation_x: point.x,
                translation_y: point.y,
                translation_z: point.z,
                quaternion_x: quaternion.3,
                quaternion_y: quaternion.1,
                quaternion_z: quaternion.2,
                quaternion_rotation: quaternion.0,
            });
        }
    }
}

/// copied from three.js
/// which copied from http://www.euclideanspace.com/maths/geometry/rotations/conversions/matrixToQuaternion/index.htm
/// assumes the upper 3x3 of m is a pure rotation matrix (i.e, unscaled)
fn matrix_to_quaternion(m: &Matrix4<f32>) -> (f32, f32, f32, f32) {
    let trace = m.x.x + m.y.y + m.z.z;

    if trace > 0.0 {
        let s = 0.5 / (trace + 1.0).sqrt();

        (
            0.25 / s,
            (m.z.y - m.y.z) * s,
            (m.x.z - m.z.x) * s,
            (m.y.x - m.x.y) * s,
        )
    } else if m.x.x > m.y.y && m.x.x > m.z.z {
        let s = 2.0 * (1.0 + m.x.x - m.y.y - m.z.z).sqrt();

        (
            (m.z.y - m.y.z) / s,
            0.25 * s,
            (m.x.y + m.y.x) / s,
            (m.x.z + m.z.x) / s,
        )
    } else if m.y.y > m.z.z {
        let s = 2.0 * (1.0 + m.y.y - m.x.x - m.z.z).sqrt();

        (
            (m.x.z - m.z.x) / s,
            (m.x.y + m.y.x) / s,
            0.25 * s,
            (m.y.z + m.z.y) / s,
        )
    } else {
        let s = 2.0 * (1.0 + m.z.z - m.x.x - m.y.y).sqrt();

        (
            (m.y.x - m.x.y) / s,
            (m.x.z + m.z.x) / s,
            (m.y.z + m.z.y) / s,
            0.25 * s,
        )
    }
}

use crate::wgpu::model3d::{Animation, Joint, ChannelOutputs};

use cgmath::Matrix4;
use gltf::animation::Interpolation;

pub fn set_animated_joints(animation: &Animation, frame: f32, root_joint: &mut Joint, parent_transform: Matrix4<f32>) {
    let mut translation = root_joint.translation.clone();
    let mut rotation = root_joint.rotation.clone();
    let mut scale = root_joint.scale.clone();

    for channel in &animation.channels {
        for input in &channel.inputs {
            println!("{}", input);
        }
        if root_joint.node_index == channel.target_node_index {
            match (&channel.outputs, &channel.interpolation) {
                (ChannelOutputs::Translations (translations), Interpolation::Linear) => {
                    translation = translations[0];
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Translations (translations), Interpolation::Step) => {
                    translation = translations[0];
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Translations (translations), Interpolation::CubicSpline) => {
                    translation = translations[0];
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Rotations (rotations), Interpolation::Linear) => {
                    rotation = rotations[0].into();
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Rotations (rotations), Interpolation::Step) => {
                    rotation = rotations[0].into();
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Rotations (rotations), Interpolation::CubicSpline) => {
                    rotation = rotations[0].into();
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Scales (scales), Interpolation::Linear) => {
                    scale = scales[0];
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Scales (scales), Interpolation::Step) => {
                    scale = scales[0];
                    error!("unimplemented channel output type + interpolation type");
                }
                (ChannelOutputs::Scales (scales), Interpolation::CubicSpline) => {
                    scale = scales[0];
                    error!("unimplemented channel output type + interpolation type");
                }
                (_, Interpolation::CatmullRomSpline) => unimplemented!("This will be deleted in next gltf version"),
            }
        }
    }

    let rotation: Matrix4<f32> = rotation.into();
    let transform: Matrix4<f32> = parent_transform *
        Matrix4::from_translation(translation) *
        rotation *
        Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z);

    root_joint.transform = transform * root_joint.ibm;

    for child in &mut root_joint.children {
        set_animated_joints(animation, frame, child, transform.clone());
    }
}

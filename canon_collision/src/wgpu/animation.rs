use crate::wgpu::model3d::{Animation, Joint, ChannelOutputs, Channel};

use cgmath::{Matrix4, VectorSpace};
use gltf::animation::Interpolation;
use splines::{Interpolation as SplineInterpolation, Key, Spline};

pub fn set_animated_joints(animation: &Animation, frame: f32, root_joint: &mut Joint, parent_transform: Matrix4<f32>) {
    let mut translation = root_joint.translation.clone();
    let mut rotation = root_joint.rotation.clone();
    let mut scale = root_joint.scale.clone();
    println!("pre  scale: {:?}", scale);

    for channel in &animation.channels {
        if root_joint.node_index == channel.target_node_index {
            match (&channel.outputs, &channel.interpolation) {
                (ChannelOutputs::Translations (translations), Interpolation::Linear) => {
                    let (index_pre, index_next, amount) = index_linear(channel, frame);
                    let pre = translations[index_pre];
                    let next = translations[index_next];
                    translation = pre.lerp(next, amount);
                }
                (ChannelOutputs::Translations (translations), Interpolation::Step) => {
                    let output_index = index_step(channel, frame);
                    translation = translations[output_index];
                }
                (ChannelOutputs::Translations (translations), Interpolation::CubicSpline) => {
                    let seconds = frame / 60.0; // 60fps
                    let mut points = vec!();
                    for (input, outputs) in channel.inputs.iter().zip(translations.chunks(3)) {
                        points.push(Key::new(*input, outputs[1], SplineInterpolation::StrokeBezier(outputs[0], outputs[2])));
                    }
                    let spline = Spline::from_vec(points);
                    if let Some(result) = spline.clamped_sample(seconds) {
                        translation = result;
                    }
                    else {
                        error!("Failed to interpolate translation spline");
                    }
                }
                (ChannelOutputs::Rotations (rotations), Interpolation::Linear) => {
                    let (index_pre, index_next, amount) = index_linear(channel, frame);
                    let pre = rotations[index_pre];
                    let next = rotations[index_next];
                    rotation = pre.slerp(next, amount);
                }
                (ChannelOutputs::Rotations (rotations), Interpolation::Step) => {
                    let output_index = index_step(channel, frame);
                    rotation = rotations[output_index].into();
                }
                (ChannelOutputs::Rotations (rotations), Interpolation::CubicSpline) => {
                    let seconds = frame / 60.0; // 60fps
                    let mut points = vec!();
                    for (input, outputs) in channel.inputs.iter().zip(rotations.chunks(3)) {
                        points.push(Key::new(*input, outputs[1], SplineInterpolation::StrokeBezier(outputs[0], outputs[2])));
                    }
                    let spline = Spline::from_vec(points);
                    if let Some(result) = spline.clamped_sample(seconds) {
                        rotation = result;
                    }
                    else {
                        error!("Failed to interpolate rotation spline");
                    }
                }
                (ChannelOutputs::Scales (scales), Interpolation::Linear) => {
                    let (index_pre, index_next, amount) = index_linear(channel, frame);
                    let pre = scales[index_pre];
                    let next = scales[index_next];
                    scale = pre.lerp(next, amount);
                }
                (ChannelOutputs::Scales (scales), Interpolation::Step) => {
                    let output_index = index_step(channel, frame);
                    scale = scales[output_index];
                }
                (ChannelOutputs::Scales (scales), Interpolation::CubicSpline) => {
                    let seconds = frame / 60.0; // 60fps
                    let mut points = vec!();
                    for (input, outputs) in channel.inputs.iter().zip(scales.chunks(3)) {
                        points.push(Key::new(*input, outputs[1], SplineInterpolation::StrokeBezier(outputs[0], outputs[2])));
                        println!("spline vertex: {:?}, input tangent: {:?}, output tangent: {:?}", outputs[1], outputs[0], outputs[2]);
                    }
                    let spline = Spline::from_vec(points);
                    if let Some(result) = spline.clamped_sample(seconds) {
                        scale = result;
                    }
                    else {
                        error!("Failed to interpolate scale spline");
                    }
                }
                (_, Interpolation::CatmullRomSpline) => unimplemented!("This will be deleted in next gltf version"),
            }
        }
    }
    println!("post scale: {:?}", scale);

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

fn index_step(channel: &Channel, frame: f32) -> usize {
    let seconds = frame / 60.0; // 60fps

    if seconds < *channel.inputs.first().unwrap() || channel.inputs.len() < 2 {
        return 0;
    }

    for (i, window) in channel.inputs.windows(2).enumerate() {
        let input_pre  = window[0];
        let input_next = window[1];
        if seconds >= input_pre && seconds < input_next {
            return i;
        }
    }

    // seconds must be larger than highest input, so return the last index
    channel.inputs.len() - 1
}

fn index_linear(channel: &Channel, frame: f32) -> (usize, usize, f32) {
    let seconds = frame / 60.0; // 60fps

    if seconds < *channel.inputs.first().unwrap() || channel.inputs.len() < 2 {
        return (0, 0, 0.0);
    }

    for (i, window) in channel.inputs.windows(2).enumerate() {
        let input_pre  = window[0];
        let input_next = window[1];
        if seconds >= input_pre && seconds < input_next {
            let amount = (seconds - input_pre) / (input_next - input_pre);
            return (i, i + 1, amount);
        }
    }

    // seconds must be larger than highest input, so return the last index
    let last = channel.inputs.len() - 1;
    (last, last, 0.0)
}

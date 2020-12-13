/// TODO: Share common code with canon_collision/wgpu module.
/// Current differences:
/// * ibm not applied to transformation
/// * transformation written directly to Joint struct

use crate::model::{Animation, Joint, ChannelOutputs, Channel};

use cgmath::{Matrix4, VectorSpace, InnerSpace};
use gltf::animation::Interpolation;

// Cubicspline interpolation implemented as per:
// https://github.com/KhronosGroup/glTF/blob/master/specification/2.0/README.md#appendix-c-spline-interpolation

pub fn set_animated_joints(animation: &Animation, frame: f32, root_joint: &mut Joint, parent_transform: Matrix4<f32>) {
    let mut translation = root_joint.translation.clone();
    let mut rotation = root_joint.rotation.clone();
    let mut scale = root_joint.scale.clone();

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
                    translation = match index_cubicspline(channel, frame) {
                        CubicSplineIndex::Clamped { index } => {
                            translations[index * 3 + 1]
                        }
                        CubicSplineIndex::Interpolate { index_pre, index_next, t, range } => {
                            let p0 =         translations[index_pre  * 3 + 1]; // previous spline vertex
                            let p1 =         translations[index_next * 3 + 1]; // next spline vertex
                            let m0 = range * translations[index_pre  * 3 + 2]; // previous output tangent
                            let m1 = range * translations[index_next * 3 + 0]; // next input tangent

                            let tpow2 = t * t;
                            let tpow3 = tpow2 * t;

                            (2.0 * tpow3 - 3.0 * tpow2 + 1.0) * p0 +
                            (tpow3 - 2.0 * tpow2 + t) * m0 +
                            (-2.0 * tpow3 + 3.0 * tpow2) * p1 +
                            (tpow3 - tpow2) * m1
                        }
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
                    rotation = match index_cubicspline(channel, frame) {
                        CubicSplineIndex::Clamped { index } => {
                            rotations[index * 3 + 1]
                        }
                        CubicSplineIndex::Interpolate { index_pre, index_next, t, range } => {
                            let p0 =         rotations[index_pre  * 3 + 1]; // previous spline vertex
                            let p1 =         rotations[index_next * 3 + 1]; // next spline vertex
                            let m0 = range * rotations[index_pre  * 3 + 2]; // previous output tangent
                            let m1 = range * rotations[index_next * 3 + 0]; // next input tangent

                            let tpow2 = t * t;
                            let tpow3 = tpow2 * t;

                            (2.0 * tpow3 - 3.0 * tpow2 + 1.0) * p0 +
                            (tpow3 - 2.0 * tpow2 + t) * m0 +
                            (-2.0 * tpow3 + 3.0 * tpow2) * p1 +
                            (tpow3 - tpow2) * m1
                        }
                    };
                    rotation = rotation.normalize();
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
                    scale = match index_cubicspline(channel, frame) {
                        CubicSplineIndex::Clamped { index } => {
                            scales[index * 3 + 1]
                        }
                        CubicSplineIndex::Interpolate { index_pre, index_next, t, range } => {
                            let p0 =         scales[index_pre  * 3 + 1]; // previous spline vertex
                            let p1 =         scales[index_next * 3 + 1]; // next spline vertex
                            let m0 = range * scales[index_pre  * 3 + 2]; // previous output tangent
                            let m1 = range * scales[index_next * 3 + 0]; // next input tangent

                            let tpow2 = t * t;
                            let tpow3 = tpow2 * t;

                            (2.0 * tpow3 - 3.0 * tpow2 + 1.0) * p0 +
                            (tpow3 - 2.0 * tpow2 + t) * m0 +
                            (-2.0 * tpow3 + 3.0 * tpow2) * p1 +
                            (tpow3 - tpow2) * m1
                        }
                    };
                }
            }
        }
    }

    let rotation: Matrix4<f32> = rotation.into();
    let transform: Matrix4<f32> = parent_transform *
        Matrix4::from_translation(translation) *
        rotation *
        Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z);

    root_joint.transform = transform;

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

enum CubicSplineIndex {
    Clamped { index: usize },
    Interpolate { index_pre: usize, index_next: usize, t: f32, range: f32 }
}

fn index_cubicspline(channel: &Channel, frame: f32) -> CubicSplineIndex {
    let seconds = frame / 60.0; // 60fps

    if seconds < *channel.inputs.first().unwrap() || channel.inputs.len() < 2 {
        return CubicSplineIndex::Clamped { index: 0 }
    }

    for (i, window) in channel.inputs.windows(2).enumerate() {
        let input_pre  = window[0];
        let input_next = window[1];
        if seconds >= input_pre && seconds < input_next {
            let range = input_next - input_pre;
            let t = (seconds - input_pre) / range;
            return CubicSplineIndex::Interpolate {
                index_pre: i,
                index_next: i + 1,
                t,
                range
            };
        }
    }

    // seconds must be larger than highest input, so return the last index
    let index = channel.inputs.len() - 1;
    CubicSplineIndex::Clamped { index }
}

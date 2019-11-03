use std::collections::HashMap;

use cgmath::{Matrix4, Quaternion, Vector3, SquareMatrix};
use gltf::Gltf;
use gltf::buffer::Source as BufferSource;
use gltf::scene::{Node, Transform};
use gltf::animation::{Interpolation};
use gltf::animation::util::ReadOutputs;

pub struct Model3D  {
    pub root_joint: Joint,
    pub animations: HashMap<String, Animation>
}

impl Model3D {
    pub fn from_gltf(data: &[u8], model_name: &str) -> Model3D {
        let gltf = Gltf::from_slice(&data).unwrap();
        let blob = gltf.blob.as_ref().unwrap();

        let node = gltf
            .nodes()
            .find(|x| x.name() == Some(model_name))
            .expect(&format!("Model must contain a node named {}", model_name));

        let mut root_joint = None;
        if let Some(skin) = node.skin() {
            // You might think that skin.skeleton() would return the root_node, but you would be wrong.
            let joints: Vec<_> = skin.joints().collect();
            if joints.len() > 0 {
                let reader = skin.reader(|buffer| {
                    match buffer.source() {
                        BufferSource::Bin => { }
                        _ => unimplemented!("It is assumed that gltf buffers use only bin source.")
                    }
                    Some(&blob)
                });
                let ibm: Vec<Matrix4<f32>> = reader.read_inverse_bind_matrices().unwrap().map(|x| x.into()).collect();
                let node_to_joints_lookup: Vec<_> = joints.iter().map(|x| x.index()).collect();
                root_joint = Some(skeleton_from_gltf_node(&joints[0], blob, &node_to_joints_lookup, &ibm, Matrix4::identity()));
            }
        }
        let root_joint = root_joint.expect("Could not find root_joint in model");

        let mut animations = HashMap::new();
        for animation in gltf.animations() {
            if let Some(name) = animation.name() {
                let mut channels = vec!();

                for channel in animation.channels() {
                    let target = channel.target();
                    let target_node_index = target.node().index();

                    let sampler = channel.sampler();
                    let interpolation = sampler.interpolation();

                    let reader = channel.reader(|buffer| {
                        match buffer.source() {
                            BufferSource::Bin => { }
                            _ => unimplemented!("It is assumed that gltf buffers use only bin source.")
                        }
                        Some(&blob)
                    });
                    let inputs: Vec<_> = reader.read_inputs().unwrap().collect();
                    let outputs = match reader.read_outputs().unwrap() {
                        ReadOutputs::Translations (translations) => {
                            ChannelOutputs::Translations (translations.map(|x| x.into()).collect())
                        }
                        ReadOutputs::Rotations (rotations) => {
                            ChannelOutputs::Rotations (rotations.into_f32().map(|r|
                                Quaternion::new(r[3], r[0], r[1], r[2])
                            ).collect())
                        }
                        ReadOutputs::Scales (scales) => {
                            ChannelOutputs::Scales (scales.map(|x| x.into()).collect())
                        }
                        ReadOutputs::MorphTargetWeights (_) => unimplemented!("gltf Property::MorphTargetWeights is unimplemented."),
                    };
                    channels.push(Channel { target_node_index, inputs, outputs, interpolation });
                }

                let name = name.to_string();
                animations.insert(name, Animation { channels });
            }
            else {
                panic!("A gltf animation could not be loaded as it has no name.");
            }
        }
        Model3D { root_joint, animations }
    }
}

#[derive(Debug, Clone)]
pub struct Joint {
    pub name: String,
    pub node_index: usize,
    pub index: usize,
    pub children: Vec<Joint>,
    // TODO: delete this and write to the buffer directly
    pub transform:   Matrix4<f32>,
    pub ibm:         Matrix4<f32>,
    // default transform
    pub translation: Vector3<f32>,
    pub rotation:    Quaternion<f32>,
    pub scale:       Vector3<f32>,
}

fn skeleton_from_gltf_node(node: &Node, blob: &[u8], node_to_joints_lookup: &[usize], ibms: &[Matrix4<f32>], parent_transform: Matrix4<f32>) -> Joint {
    let mut children = vec!();
    let node_index = node.index();
    let index = node_to_joints_lookup.iter().enumerate().find(|(_, x)| **x == node_index).unwrap().0;
    let name = node.name().unwrap_or("").to_string();

    let ibm = &ibms[index];
    let pose_transform = parent_transform * transform_to_matrix4(node.transform());

    for child in node.children() {
        children.push(skeleton_from_gltf_node(&child, blob, node_to_joints_lookup, ibms, pose_transform.clone()));
    }

    let transform = pose_transform; // TODO: Modified to remove the IBM, how to handle when merging ????
    let ibm = ibm.clone();

    let (translation, rotation, scale) = match node.transform() {
        Transform::Matrix { .. } => unimplemented!("It is assumed that gltf node transforms only use decomposed form."),
        Transform::Decomposed { translation, rotation, scale } => {
            let translation: Vector3<f32> = translation.into();
            let rotation = Quaternion::new(rotation[3], rotation[0], rotation[1], rotation[2]).into();
            let scale: Vector3<f32> = scale.into();
            (translation, rotation, scale)
        }
    };

    Joint { node_index, index, name, children, transform, ibm, translation, rotation, scale }
}

fn transform_to_matrix4(transform: Transform) -> Matrix4<f32> {
    match transform {
        Transform::Matrix { .. } => unimplemented!("It is assumed that gltf node transforms only use decomposed form."),
        Transform::Decomposed { translation, rotation, scale } => {
            let translation = Matrix4::from_translation(translation.into());
            let rotation: Matrix4<f32> = Quaternion::new(rotation[3], rotation[0], rotation[1], rotation[2]).into();
            let scale = Matrix4::from_nonuniform_scale(scale[0], scale[1], scale[2]);
            translation * rotation * scale
        }
    }
}

pub struct Animation {
    pub channels: Vec<Channel>,
}

impl Animation {
    pub fn len(&self) -> usize {
        0
    }
}

pub struct Channel {
    pub target_node_index: usize,
    pub inputs: Vec<f32>,
    pub outputs: ChannelOutputs,
    pub interpolation: Interpolation,
}

pub enum ChannelOutputs {
    Translations (Vec<Vector3<f32>>),
    Rotations    (Vec<Quaternion<f32>>),
    Scales       (Vec<Vector3<f32>>),
}

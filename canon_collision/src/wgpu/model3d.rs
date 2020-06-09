use canon_collision_lib::assets::Assets;
use crate::game::{RenderEntity, RenderGame};
use crate::wgpu::buffers::Buffers;

use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use cgmath::{Matrix4, Quaternion, SquareMatrix, Vector3};
use gltf::Gltf;
use gltf::animation::util::ReadOutputs;
use gltf::animation::{Interpolation};
use gltf::buffer::Source as BufferSource;
use gltf::image::Source as ImageSource;
use gltf::mesh::Mode;
use gltf::scene::{Node, Transform};
use png_decoder::color::ColorType as PNGColorType;
use png_decoder::png;
use wgpu::{Device, Texture, CommandEncoder};
use zerocopy::AsBytes;

pub struct Models {
    assets:           Assets,
    models:           HashMap<String, Model3D>,
    stage_model_name: Option<String>,
}

impl Models {
    pub fn new() -> Self {
        Models {
            assets:           Assets::new().unwrap(),
            models:           HashMap::new(),
            stage_model_name: None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Model3D> {
        self.models.get(&key.replace(" ", ""))
    }

    // TODO: Refactor this so the game logic sends a message to the graphics logic requesting it to
    // preload models before it needs them.
    // Maybe load fighters immediately when selected on the CSS.
    pub fn load(&mut self, device: &Device, encoder: &mut CommandEncoder, render: &RenderGame) {
        // hotreload current models
        for reload in self.assets.models_reloads() {
            // only reload if its still in memory
            if self.models.contains_key(&reload.name) {
                self.models.insert(reload.name.clone(), Model3D::from_gltf(device, encoder, &reload.data));
            }
        }

        // load current stage
        // if a new stage is used, unload old stage and load new stage
        let new_name = render.stage_model_name.replace(" ", "");
        if let Some(ref old_name) = self.stage_model_name {
            if old_name != &new_name {
                self.models.remove(old_name);
                self.load_stage(device, encoder, new_name);
            }
        }
        else {
            self.load_stage(device, encoder, new_name);
        }

        // load current fighters
        for entity in render.entities.iter() {
            if let RenderEntity::Player(ref player) = entity {
                let fighter_model_name = player.frames[0].model_name.replace(" ", "");
                if !self.models.contains_key(&fighter_model_name) {
                    // TODO: Dont reload every frame if the model doesnt exist, probs just do another hashmap
                    if let Some(data) = self.assets.get_model(&fighter_model_name) {
                        self.models.insert(fighter_model_name.clone(), Model3D::from_gltf(device, encoder, &data));
                    }
                }
            }
        }
    }

    fn load_stage(&mut self, device: &Device, encoder: &mut CommandEncoder, new_name: String) {
        if let Some(data) = self.assets.get_model(&new_name) {
            self.models.insert(new_name.clone(), Model3D::from_gltf(device, encoder, &data));
        }
        self.stage_model_name = Some(new_name);
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, AsBytes)]
pub struct ModelVertexAnimated {
    pub position: [f32; 4],
    pub uv:       [f32; 2],
    pub joints:   [u32; 4],
    pub weights:  [f32; 4],
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, AsBytes)]
pub struct ModelVertexStatic {
    pub position: [f32; 4],
    pub uv:       [f32; 2],
}

pub enum ModelVertexType {
    Animated,
    Static,
}

pub struct Model3D {
    pub meshes: Vec<Mesh>,
    pub textures: Vec<Texture>,
    pub animations: HashMap<String, Animation>
}

pub struct Mesh {
    pub primitives:  Vec<Primitive>,
    pub transform:   Matrix4<f32>,
    pub root_joint:   Option<Joint>,
}

pub struct Primitive {
    pub vertex_type: ModelVertexType,
    pub buffers:     Arc<Buffers>,
    pub texture:     Option<usize>,
}

pub struct Animation {
    pub channels: Vec<Channel>,
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

impl Model3D {
    pub fn from_gltf(device: &Device, encoder: &mut CommandEncoder, data: &[u8]) -> Model3D {
        let gltf = Gltf::from_slice(&data).unwrap();
        let blob = gltf.blob.as_ref().unwrap();
        let scene = gltf.default_scene().unwrap();

        let mut meshes = vec!();
        for node in scene.nodes() {
            meshes.extend(Model3D::mesh_from_gltf_node(device, blob, &node, Matrix4::identity()));
        }

        let mut textures = vec!();
        for texture in gltf.textures() {
            match texture.source().source() {
                ImageSource::View { view, mime_type } => {
                    assert!(view.stride().is_none(), "It is assumed that gltf texture stride is None.");
                    assert_eq!(mime_type, "image/png", "It is assumed that gltf texture mime_type is image/png.");

                    // read png data
                    let slice = &blob[view.offset() .. view.offset() + view.length()-1];
                    let png = png::decode_no_check(slice).unwrap();
                    let data = match png.color_type {
                        PNGColorType::RGB => {
                            let mut data = Vec::with_capacity(png.data.len()*2);
                            for bytes in png.data.chunks(3) {
                                data.extend(bytes);
                                data.push(0xFF);
                            }
                            data
                        }
                        PNGColorType::RGBA => png.data,
                        _ => unimplemented!("It is assumed that gltf png textures are in RGB or RGBA format.")
                    };
                    assert_eq!(data.len(), png.width * png.height * 4);

                    // create buffer and texture
                    let texture_buffer = device.create_buffer_with_data(&data, wgpu::BufferUsage::COPY_SRC);
                    let size = wgpu::Extent3d {
                        width: png.width as u32,
                        height: png.height as u32,
                        depth: 1,
                    };
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
                    });

                    // copy buffer to texture
                    let texture_buffer_copy_view = wgpu::BufferCopyView {
                        buffer: &texture_buffer,
                        layout: wgpu::TextureDataLayout {
                            offset: 0,
                            bytes_per_row: png.width as u32 * 4,
                            rows_per_image: 0,
                        }
                    };
                    let texture_copy_view = wgpu::TextureCopyView {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                    };
                    encoder.copy_buffer_to_texture(texture_buffer_copy_view, texture_copy_view, size);

                    textures.push(texture);
                }
                _ => unimplemented!("It is assumed that gltf textures are embedded in the glb file.")
            }
        }

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
                error!("A gltf animation could not be loaded as it has no name.");
            }
        }

        Model3D { meshes, textures, animations }
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

    fn mesh_from_gltf_node(device: &Device, blob: &[u8], node: &Node, parent_transform: Matrix4<f32>) -> Vec<Mesh> {
        let mut meshes = vec!();

        let transform = parent_transform * Model3D::transform_to_matrix4(node.transform());

        if let Some(mesh) = node.mesh() {
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
                    root_joint = Some(Model3D::skeleton_from_gltf_node(&joints[0], blob, &node_to_joints_lookup, &ibm, Matrix4::identity()));
                }
            }

            let mut primitives = vec!();
            for primitive in mesh.primitives() {
                match primitive.mode() {
                    Mode::Triangles => { }
                    _ => unimplemented!("It is assumed that gltf primitives use only triangle topology.")
                }
                let reader = primitive.reader(|buffer| {
                    match buffer.source() {
                        BufferSource::Bin => { }
                        _ => unimplemented!("It is assumed that gltf buffers use only bin source.")
                    }
                    Some(&blob)
                });

                let index: Vec<u16> = reader
                    .read_indices()
                    .unwrap()
                    .into_u32()
                    .map(|x| x.try_into().unwrap())
                    .collect();

                let positions = reader.read_positions();
                let uvs = reader.read_tex_coords(0);
                let joints = reader.read_joints(0);
                let weights = reader.read_weights(0);
                let (buffers, vertex_type) = match (positions, uvs, joints, weights) {
                    (Some(positions), Some(uvs), Some(joints), Some(weights)) => {
                        let vertices: Vec<_> = positions
                            .zip(uvs.into_f32())
                            .zip(joints.into_u16())
                            .zip(weights.into_f32())
                            .map(|(((pos, uv), joints), weights)| ModelVertexAnimated {
                                position: [pos[0], pos[1], pos[2], 1.0],
                                uv,
                                joints: [joints[0] as u32, joints[1] as u32, joints[2] as u32, joints[3] as u32],
                                weights,
                            })
                            .collect();

                        let buffers = Buffers::new(device, vertices.as_bytes(), &index);
                        (buffers, ModelVertexType::Animated)
                    }
                    (Some(positions), Some(uvs), None, None) => {
                        let vertices: Vec<_> = positions
                            .zip(uvs.into_f32())
                            .map(|(pos, uv)| ModelVertexStatic {
                                position: [pos[0], pos[1], pos[2], 1.0],
                                uv,
                            })
                            .collect();

                        let buffers = Buffers::new(device, vertices.as_bytes(), &index);
                        (buffers, ModelVertexType::Static)
                    }
                    (positions, uvs, joints, weights) => unimplemented!("Unexpected combination of vertex data - positions: {:?}, uvs: {:?}, joints: {:?}, weights: {:?}", positions.is_some(), uvs.is_some(), joints.is_some(), weights.is_some()),
                };

                let texture = primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_texture()
                    .map(|x| x.texture().index());

                primitives.push(Primitive { vertex_type, buffers, texture });
            }

            meshes.push(Mesh { primitives, transform, root_joint });
        }

        for child in node.children() {
            meshes.extend(Model3D::mesh_from_gltf_node(device, blob, &child, transform));
        }

        meshes
    }

    fn skeleton_from_gltf_node(node: &Node, blob: &[u8], node_to_joints_lookup: &[usize], ibms: &[Matrix4<f32>], parent_transform: Matrix4<f32>) -> Joint {
        let mut children = vec!();
        let node_index = node.index();
        let index = node_to_joints_lookup.iter().enumerate().find(|(_, x)| **x == node_index).unwrap().0;
        let name = node.name().unwrap_or("").to_string();

        let ibm = &ibms[index];
        let pose_transform = parent_transform * Model3D::transform_to_matrix4(node.transform());

        for child in node.children() {
            children.push(Model3D::skeleton_from_gltf_node(&child, blob, node_to_joints_lookup, ibms, pose_transform.clone()));
        }

        let transform = pose_transform * ibm;
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
}

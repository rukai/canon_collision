use crate::assets::Assets;
use crate::game::{RenderEntity, RenderGame};

use std::collections::HashMap;

use gltf::Gltf;
use gltf::mesh::Mode;
use gltf::image::Source as ImageSource;
use gltf::buffer::Source as BufferSource;
use gltf::scene::{Node, Transform};
use png_decoder::png;
use png_decoder::color::ColorType as PNGColorType;
use wgpu::{Device, Buffer, Texture, CommandEncoder};
use cgmath::{Matrix4, Quaternion};

use std::convert::TryInto;

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

#[derive(Default, Debug, Clone, Copy)]
pub struct ModelVertex {
    pub position: [f32; 4],
    pub uv:       [f32; 2],
}

pub struct Mesh {
    pub transform:   Matrix4<f32>,
    pub vertex:      Buffer,
    pub index:       Buffer,
    pub index_count: u32,
    pub texture:     Option<usize>,
}

impl Mesh {
    fn new(device: &Device, vertices: &[ModelVertex], indices: &[u16], transform: Matrix4<f32>, texture: Option<usize>) -> Mesh {
        let vertex = device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices);

        let index = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&indices);

        let index_count = indices.len() as u32;

        Mesh { transform, vertex, index, index_count, texture }
    }
}

pub struct Model3D {
    pub meshes: Vec<Mesh>,
    pub textures: Vec<Texture>,
    //animation: Vec<Animation>
}

impl Model3D {
    pub fn from_gltf(device: &Device, encoder: &mut CommandEncoder, data: &[u8]) -> Model3D {
        let gltf = Gltf::from_slice(&data).unwrap();
        let blob = gltf.blob.as_ref().unwrap();
        let scene = gltf.default_scene().unwrap();

        let mut meshes = vec!();
        let mut textures = vec!();
        for node in scene.nodes() {
            let child_model = Model3D::from_gltf_node(device, &node, blob);
            for mesh in child_model.meshes {
                meshes.push(mesh);
            }
        }

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
                    let texture_buffer = device
                        .create_buffer_mapped(data.len(), wgpu::BufferUsage::COPY_SRC)
                        .fill_from_slice(&data);
                    let size = wgpu::Extent3d {
                        width: png.width as u32,
                        height: png.height as u32,
                        depth: 1,
                    };
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        size,
                        array_layer_count: 1,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
                    });

                    // copy buffer to texture
                    let texture_buffer_copy_view = wgpu::BufferCopyView {
                        buffer: &texture_buffer,
                        offset: 0,
                        row_pitch: 0,
                        image_height: size.height,
                    };
                    let texture_copy_view = wgpu::TextureCopyView {
                        texture: &texture,
                        mip_level: 0,
                        array_layer: 0,
                        origin: wgpu::Origin3d { x: 0.0, y: 0.0, z: 0.0 },
                    };
                    encoder.copy_buffer_to_texture(texture_buffer_copy_view, texture_copy_view, size);

                    textures.push(texture);
                }
                _ => unimplemented!("It is assumed that gltf textures are embedded in the glb file.")
            }
        }

        Model3D { meshes, textures }
    }

    fn from_gltf_node(device: &Device, node: &Node, blob: &[u8]) -> Model3D {
        let mut meshes = vec!();
        let textures = vec!();

        if let Some(mesh) = node.mesh() {
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

                let positions = reader.read_positions().unwrap();
                let vertices: Vec<_> = if let Some(uvs) = reader.read_tex_coords(0) {
                    positions
                        .zip(uvs.into_f32())
                        .map(|(pos, uv)| ModelVertex {
                            position: [pos[0], pos[1], pos[2], 1.0],
                            uv
                        })
                        .collect()
                } else {
                    warn!("Model loaded without uv's.");
                    positions
                        .map(|pos| ModelVertex {
                            position: [pos[0], pos[1], pos[2], 1.0],
                            uv: [0.0, 0.0]
                        })
                        .collect()
                };

                let indices: Vec<u16> = reader
                    .read_indices()
                    .unwrap()
                    .into_u32()
                    .map(|x| x.try_into().unwrap())
                    .collect();

                let transform = match node.transform() {
                    Transform::Matrix { .. } => unimplemented!("It is assumed that gltf node transforms only use decomposed form."),
                    Transform::Decomposed { translation, rotation, scale } => {
                        let translation = Matrix4::from_translation(translation.into());
                        let rotation: Matrix4<f32> = Quaternion::new(rotation[3], rotation[0], rotation[1], rotation[2]).into();
                        let scale = Matrix4::from_nonuniform_scale(scale[0], scale[1], scale[2]);
                        translation * rotation * scale
                    }
                };

                let texture = primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_texture()
                    .map(|x| x.texture().index());

                meshes.push(Mesh::new(device, &vertices, &indices, transform, texture));
            }
        }

        for child in node.children() {
            let child_model = Model3D::from_gltf_node(device, &child, blob);
            for mesh in child_model.meshes {
                meshes.push(mesh);
            }
        }

        Model3D { meshes, textures }
    }
}

use gltf::Gltf;
use gltf::mesh::Mode;
use gltf::buffer::Source;
use gltf::scene::{Node, Transform};
use wgpu::{Device, Buffer, Texture};
use cgmath::{Matrix4, Quaternion};

use std::convert::TryInto;

#[derive(Default, Debug, Clone, Copy)]
pub struct ModelVertex {
    pub position: [f32; 4],
    // pub uvs: [f32; 2],
}

pub struct Mesh {
    pub transform:   Matrix4<f32>,
    pub vertex:      Buffer,
    pub index:       Buffer,
    pub index_count: u32,
}

impl Mesh {
    fn new(device: &Device, vertices: &[ModelVertex], indices: &[u16], transform: Matrix4<f32>) -> Mesh {
        let vertex = device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices);

        let index = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&indices);

        let index_count = indices.len() as u32;

        Mesh { transform, vertex, index, index_count }
    }
}

pub struct Model3D {
    pub meshes: Vec<Mesh>,
    pub _textures: Vec<Texture>,
    //animation: Vec<Animation>
}

impl Model3D {
    pub fn from_gltf(device: &Device, data: &[u8]) -> Model3D {
        let gltf = Gltf::from_slice(&data).unwrap();
        let blob = gltf.blob.as_ref().unwrap();
        let scene = gltf.default_scene().unwrap();

        let mut meshes = vec!();
        let _textures = vec!();
        for node in scene.nodes() {
            let child_model = Model3D::from_gltf_node(device, &node, blob);
            for mesh in child_model.meshes {
                meshes.push(mesh);
            }
        }

        Model3D { meshes, _textures }
    }

    fn from_gltf_node(device: &Device, node: &Node, blob: &[u8]) -> Model3D {
        let mut meshes = vec!();
        let _textures = vec!();

        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                match primitive.mode() {
                    Mode::Triangles => { }
                    _ => unimplemented!("It is assumed that gltf primitives use only triangle topology.")
                }
                let reader = primitive.reader(|buffer| {
                    match buffer.source() {
                        Source::Bin => { }
                        _ => unimplemented!("It is assumed that gltf buffers use only bin source.")
                    }
                    Some(&blob)
                });

                let vertices: Vec<ModelVertex> = reader
                    .read_positions()
                    .unwrap()
                    .map(|x| ModelVertex { position: [x[0], x[1], x[2], 1.0] })
                    .collect();
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

                meshes.push(Mesh::new(device, &vertices, &indices, transform));
            }
        }
        for child in node.children() {
            let child_model = Model3D::from_gltf_node(device, &child, blob);
            for mesh in child_model.meshes {
                meshes.push(mesh);
            }
        }

        Model3D { meshes, _textures }
    }
}

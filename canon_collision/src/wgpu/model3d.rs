use gltf::Gltf;
use gltf::mesh::Mode;
use gltf::buffer::Source;
use wgpu::{Device, Buffer, Texture};

use std::convert::TryInto;

#[derive(Default, Debug, Clone, Copy)]
pub struct ModelVertex {
    pub position: [f32; 4],
    // pub uvs: [f32; 2],
}

pub struct Mesh {
    pub vertex: Buffer,
    pub index:  Buffer,
    pub index_count: u32,
}

impl Mesh {
    fn new(device: &Device, vertices: &[ModelVertex], indices: &[u16]) -> Mesh {
        let vertex = device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices);

        let index = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&indices);

        let index_count = indices.len() as u32;

        Mesh { vertex, index, index_count }
    }
}

pub struct Model3D {
    pub meshes: Vec<Mesh>,
    pub _textures: Vec<Texture>,
    //animation: Vec<Animation>
}

impl Model3D {
    pub fn from_gltf(device: &Device, data: &[u8]) -> Model3D {
        let mut meshes = vec!();
        let _textures = vec!();

        let gltf = Gltf::from_slice(&data).unwrap();
        let blob = gltf.blob.as_ref().unwrap();
        for mesh in gltf.meshes() {
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

                meshes.push(Mesh::new(device, &vertices, &indices));
            }
        }

        Model3D { meshes, _textures }
    }
}

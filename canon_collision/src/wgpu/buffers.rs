use crate::entity::fighters::player::RenderShield;
use crate::game::{RenderRect, SurfaceSelection};
use crate::graphics;
use canon_collision_lib::entity_def::{CollisionBox, ECB};
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::package::Package;
use canon_collision_lib::stage::Surface;

use bytemuck::{Pod, Zeroable};
use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};
use wgpu::util::DeviceExt;
use wgpu::{Buffer, Device};

use std::collections::HashSet;
use std::f32::consts;
use std::rc::Rc;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub edge: f32,
    pub render_id: u32,
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, Pod, Zeroable)]
pub struct ColorVertex {
    pub position: [f32; 4],
    pub color: [f32; 4],
}

fn colorvertex(x: f32, y: f32, color: [f32; 4]) -> ColorVertex {
    ColorVertex {
        position: [x, y, 0.0, 1.0],
        color,
    }
}

struct StageVertexConstructor;
impl FillVertexConstructor<ColorVertex> for StageVertexConstructor {
    fn new_vertex(&mut self, fill_vertex: FillVertex) -> ColorVertex {
        let position = fill_vertex.position();
        ColorVertex {
            position: [position.x, position.y, 0.0, 1.0],
            color: [0.16, 0.16, 0.16, 1.0],
        }
    }
}

pub struct Buffers {
    pub vertex: Buffer,
    pub index: Buffer,
    pub index_count: u32,
}

impl Buffers {
    pub fn new<T>(device: &Device, vertices: &[T], indices: &[u16]) -> Rc<Buffers>
    where
        T: Pod,
    {
        let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let index_count = indices.len() as u32;

        Rc::new(Buffers {
            vertex,
            index,
            index_count,
        })
    }

    pub fn gen_colbox(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u16>,
        colbox: &CollisionBox,
        index_count: &mut u16,
        render_id: u32,
    ) {
        let triangles = 60;
        // triangles are drawn meeting at the centre, forming a circle
        let point = &colbox.point;
        vertices.push(Vertex {
            position: [point.0, point.1],
            edge: 0.0,
            render_id,
        });
        for i in 0..triangles {
            let angle = i as f32 * 2.0 * consts::PI / (triangles as f32);
            let (sin, cos) = angle.sin_cos();
            let x = point.0 + cos * colbox.radius;
            let y = point.1 + sin * colbox.radius;
            vertices.push(Vertex {
                position: [x, y],
                edge: 1.0,
                render_id,
            });
            indices.push(*index_count);
            indices.push(*index_count + i + 1);
            indices.push(*index_count + (i + 1) % triangles + 1);
        }
        *index_count += triangles + 1;
    }

    pub fn new_fighter_frame_colboxes(
        device: &Device,
        package: &Package,
        fighter: &str,
        action: &str,
        frame: usize,
        selected: &HashSet<usize>,
    ) -> Rc<Buffers> {
        let mut vertices: Vec<Vertex> = vec![];
        let mut indices: Vec<u16> = vec![];
        let mut index_count = 0;

        let colboxes = &package.entities[fighter].actions[action].frames[frame].colboxes;
        for (i, colbox) in colboxes.iter().enumerate() {
            if selected.contains(&i) {
                Buffers::gen_colbox(&mut vertices, &mut indices, colbox, &mut index_count, 0);
            }
        }

        Buffers::new(device, &vertices, &indices)
    }

    pub fn new_fighter_frame(
        device: &Device,
        package: &Package,
        fighter: &str,
        action: &str,
        frame: usize,
    ) -> Option<Rc<Buffers>> {
        let frames = &package.entities[fighter].actions[action].frames;
        if let Some(frame) = frames.get(frame) {
            let mut vertices: Vec<Vertex> = vec![];
            let mut indices: Vec<u16> = vec![];
            let mut index_count = 0;

            for colbox in frame.colboxes.iter() {
                let render_id = graphics::get_render_id(&colbox.role);
                Buffers::gen_colbox(
                    &mut vertices,
                    &mut indices,
                    colbox,
                    &mut index_count,
                    render_id,
                );
            }

            Some(Buffers::new(device, &vertices, &indices))
        } else {
            None
        }
    }

    pub fn new_selected_surfaces(
        device: &Device,
        surfaces: &[Surface],
        selected_surfaces: &HashSet<SurfaceSelection>,
    ) -> Option<Rc<Buffers>> {
        if surfaces.is_empty() {
            return None;
        }

        let mut vertices: Vec<ColorVertex> = vec![];
        let mut indices: Vec<u16> = vec![];
        let mut indice_count = 0;
        let color = [0.0, 1.0, 0.0, 1.0];
        for (i, surface) in surfaces.iter().enumerate() {
            let x_mid = (surface.x1 + surface.x2) / 2.0;
            let y_mid = (surface.y1 + surface.y2) / 2.0;

            let angle = surface.render_angle() - 90f32.to_radians();
            let d_x = angle.cos() / 4.0;
            let d_y = angle.sin() / 4.0;

            if selected_surfaces.contains(&SurfaceSelection::P1(i)) {
                vertices.push(colorvertex(x_mid + d_x, y_mid + d_y, color));
                vertices.push(colorvertex(surface.x1 + d_x, surface.y1 + d_y, color));
                vertices.push(colorvertex(surface.x1 - d_x, surface.y1 - d_y, color));
                vertices.push(colorvertex(x_mid - d_x, y_mid - d_y, color));

                indices.push(indice_count + 0);
                indices.push(indice_count + 1);
                indices.push(indice_count + 2);
                indices.push(indice_count + 0);
                indices.push(indice_count + 2);
                indices.push(indice_count + 3);
                indice_count += 4;
            }
            if selected_surfaces.contains(&SurfaceSelection::P2(i)) {
                vertices.push(colorvertex(x_mid + d_x, y_mid + d_y, color));
                vertices.push(colorvertex(surface.x2 + d_x, surface.y2 + d_y, color));
                vertices.push(colorvertex(surface.x2 - d_x, surface.y2 - d_y, color));
                vertices.push(colorvertex(x_mid - d_x, y_mid - d_y, color));

                indices.push(indice_count + 0);
                indices.push(indice_count + 1);
                indices.push(indice_count + 2);
                indices.push(indice_count + 0);
                indices.push(indice_count + 2);
                indices.push(indice_count + 3);
                indice_count += 4;
            }
        }

        Some(Buffers::new(device, &vertices, &indices))
    }

    pub fn new_surfaces(device: &Device, surfaces: &[Surface]) -> Option<Rc<Buffers>> {
        if surfaces.is_empty() {
            return None;
        }

        let mut vertices: Vec<ColorVertex> = vec![];
        let mut indices: Vec<u16> = vec![];
        let mut indice_count = 0;

        for surface in surfaces {
            let r = if surface.is_pass_through() {
                0.4
            } else if surface.floor.is_some() {
                0.6
            } else {
                0.0
            };
            let g = if surface.ceiling { 0.5 } else { 0.0 };
            let b = if surface.wall { 0.5 } else { 0.0 };
            let color = [1.0 - g - b, 1.0 - r - b, 1.0 - r - g, 1.0];

            let angle = surface.render_angle() - 90f32.to_radians();
            let d_x = angle.cos() / 4.0;
            let d_y = angle.sin() / 4.0;

            vertices.push(colorvertex(surface.x1 + d_x, surface.y1 + d_y, color));
            vertices.push(colorvertex(surface.x2 + d_x, surface.y2 + d_y, color));
            vertices.push(colorvertex(surface.x2 - d_x, surface.y2 - d_y, color));
            vertices.push(colorvertex(surface.x1 - d_x, surface.y1 - d_y, color));

            indices.push(indice_count + 0);
            indices.push(indice_count + 1);
            indices.push(indice_count + 2);
            indices.push(indice_count + 0);
            indices.push(indice_count + 2);
            indices.push(indice_count + 3);
            indice_count += 4;
        }

        Some(Buffers::new(device, &vertices, &indices))
    }

    // TODO: Combine new_surfaces(..) and new_surfaces_fill(..), waiting on: https://github.com/nical/lyon/issues/224
    pub fn new_surfaces_fill(device: &Device, surfaces: &[Surface]) -> Option<Rc<Buffers>> {
        if surfaces.is_empty() {
            return None;
        }

        let mut builder = Path::svg_builder();
        let mut used: Vec<usize> = vec![];
        let mut cant_loop: Vec<usize> = vec![]; // optimization, so we dont have to keep rechecking surfaces that will never loop

        for (i, surface) in surfaces.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }

            fn f32_equal(a: f32, b: f32) -> bool {
                (a - b).abs() < 0.0000001
            }

            let mut loop_elements: Vec<usize> = vec![i];
            let mut found_loop = false;
            let mut prev_surface = surface;
            if !cant_loop.contains(&i) {
                'loop_search: loop {
                    for (j, check_surface) in surfaces.iter().enumerate() {
                        if i != j
                            && !loop_elements.contains(&j)
                            && !used.contains(&j)
                            && (f32_equal(check_surface.x1, prev_surface.x1)
                                && f32_equal(check_surface.y1, prev_surface.y1)
                                || f32_equal(check_surface.x1, prev_surface.x2)
                                    && f32_equal(check_surface.y1, prev_surface.y2)
                                || f32_equal(check_surface.x2, prev_surface.x1)
                                    && f32_equal(check_surface.y2, prev_surface.y1)
                                || f32_equal(check_surface.x2, prev_surface.x2)
                                    && f32_equal(check_surface.y2, prev_surface.y2))
                        {
                            loop_elements.push(j);
                            if loop_elements.len() > 2
                                && (f32_equal(check_surface.x1, surface.x1)
                                    && f32_equal(check_surface.y1, surface.y1)
                                    || f32_equal(check_surface.x1, surface.x2)
                                        && f32_equal(check_surface.y1, surface.y2)
                                    || f32_equal(check_surface.x2, surface.x1)
                                        && f32_equal(check_surface.y2, surface.y1)
                                    || f32_equal(check_surface.x2, surface.x2)
                                        && f32_equal(check_surface.y2, surface.y2))
                            {
                                found_loop = true;
                                break 'loop_search; // completed a loop
                            } else {
                                prev_surface = check_surface;
                                continue 'loop_search; // found a loop element, start the loop_search again to find the next loop element.
                            }
                        }
                    }
                    break 'loop_search; // loop search exhausted
                }
            }

            if found_loop {
                let mut loop_elements_iter = loop_elements.iter().cloned();
                let first_surface_i = loop_elements_iter.next().unwrap();
                used.push(first_surface_i);

                let first_surface = &surfaces[first_surface_i];
                let second_surface = &surfaces[loop_elements[1]];
                let start_p1 = f32_equal(first_surface.x1, second_surface.x1)
                    && f32_equal(first_surface.y1, second_surface.y1)
                    || f32_equal(first_surface.x1, second_surface.x2)
                        && f32_equal(first_surface.y1, second_surface.y2);
                let mut prev_x = if start_p1 {
                    first_surface.x1
                } else {
                    first_surface.x2
                };
                let mut prev_y = if start_p1 {
                    first_surface.y1
                } else {
                    first_surface.y2
                };
                builder.move_to(point(prev_x, prev_y));

                for j in loop_elements_iter {
                    let surface = &surfaces[j];
                    if f32_equal(surface.x1, prev_x) && f32_equal(surface.y1, prev_y) {
                        prev_x = surface.x2;
                        prev_y = surface.y2;
                    } else {
                        prev_x = surface.x1;
                        prev_y = surface.y1;
                    }
                    builder.line_to(point(prev_x, prev_y));
                    used.push(j);
                }
                builder.close();
            } else {
                for j in loop_elements {
                    cant_loop.push(j);
                }
            }
            used.push(i);
        }

        let path = builder.build();
        let mut tessellator = FillTessellator::new();
        let mut mesh = VertexBuffers::new();
        tessellator
            .tessellate(
                path.iter(),
                &FillOptions::tolerance(0.01),
                &mut BuffersBuilder::new(&mut mesh, StageVertexConstructor),
            )
            .unwrap();

        Some(Buffers::new(device, &mesh.vertices, &mesh.indices))
    }

    /// TODO: Set individual corner vertex colours to show which points of the ecb are selected
    pub fn new_ecb(device: &Device, ecb: &ECB) -> Rc<Buffers> {
        let color = [1.0, 1.0, 1.0, 1.0];
        let mid_y = (ecb.top + ecb.bottom) / 2.0;
        let vertices: [ColorVertex; 12] = [
            // ecb
            colorvertex(0.0, ecb.bottom, color),
            colorvertex(ecb.left, mid_y, color),
            colorvertex(ecb.right, mid_y, color),
            colorvertex(0.0, ecb.top, color),
            // horizontal bps
            colorvertex(-4.0, -0.15, color),
            colorvertex(-4.0, 0.15, color),
            colorvertex(4.0, -0.15, color),
            colorvertex(4.0, 0.15, color),
            // vertical bps
            colorvertex(-0.15, -4.0, color),
            colorvertex(0.15, -4.0, color),
            colorvertex(-0.15, 4.0, color),
            colorvertex(0.15, 4.0, color),
        ];

        let indices: [u16; 18] = [
            1, 2, 0, // 1
            1, 2, 3, // 2
            4, 5, 6, // 3
            7, 6, 5, // 4
            8, 9, 10, // 5
            11, 10, 9, // 6
        ];

        Buffers::new(device, &vertices, &indices)
    }

    pub fn new_arrow(device: &Device, color: [f32; 4]) -> Rc<Buffers> {
        let vertices: [ColorVertex; 7] = [
            // stick
            colorvertex(-0.7, 0.0, color),
            colorvertex(0.7, 0.0, color),
            colorvertex(-0.7, 10.0, color),
            colorvertex(0.7, 10.0, color),
            // head
            colorvertex(0.0, 12.0, color),
            colorvertex(-2.2, 10.0, color),
            colorvertex(2.2, 10.0, color),
        ];

        let indices: [u16; 9] = [
            // stick
            0, 1, 2, // 1
            1, 2, 3, // 2
            //head
            4, 5, 6, // 3
        ];

        Buffers::new(device, &vertices, &indices)
    }

    pub fn rect_buffers(device: &Device, rect: Rect, color: [f32; 4]) -> Rc<Buffers> {
        let left = rect.left();
        let right = rect.right();
        let bot = rect.bot();
        let top = rect.top();

        let vertices: [ColorVertex; 4] = [
            colorvertex(left, bot, color),
            colorvertex(right, bot, color),
            colorvertex(right, top, color),
            colorvertex(left, top, color),
        ];

        let indices: [u16; 6] = [
            0, 1, 2, // 1
            0, 2, 3, // 2
        ];

        Buffers::new(device, &vertices, &indices)
    }

    pub fn rect_outline_buffers(device: &Device, rect: &RenderRect) -> Rc<Buffers> {
        let width = 0.5;
        let left = rect.rect.left();
        let right = rect.rect.right();
        let bot = rect.rect.bot();
        let top = rect.rect.top();

        let vertices: [ColorVertex; 8] = [
            // outer rectangle
            colorvertex(left, bot, rect.color),
            colorvertex(right, bot, rect.color),
            colorvertex(right, top, rect.color),
            colorvertex(left, top, rect.color),
            // inner rectangle
            colorvertex(left + width, bot + width, rect.color),
            colorvertex(right - width, bot + width, rect.color),
            colorvertex(right - width, top - width, rect.color),
            colorvertex(left + width, top - width, rect.color),
        ];

        let indices: [u16; 24] = [
            0, 4, 1, 1, 4, 5, // bottom edge
            1, 5, 2, 2, 5, 6, // right edge
            2, 6, 3, 3, 7, 6, // top edge
            3, 7, 0, 0, 4, 7, // left edge
        ];

        Buffers::new(device, &vertices, &indices)
    }

    /// Creates a single triangle with sides of length 1
    pub fn new_triangle(device: &Device, color: [f32; 4]) -> Rc<Buffers> {
        let h = ((3.0 / 4.0) as f32).sqrt();
        let vertices = [
            colorvertex(0.0, h, color),
            colorvertex(h / -2.0, 0.0, color),
            colorvertex(h / 2.0, 0.0, color),
        ];

        let indices = [0, 1, 2];
        Buffers::new(device, &vertices, &indices)
    }

    pub fn new_shield(device: &Device, shield: &RenderShield, color: [f32; 4]) -> Rc<Buffers> {
        let mut vertices: Vec<ColorVertex> = vec![];
        let mut indices: Vec<u16> = vec![];
        let mut index_count = 0;

        let segments = match shield.distort {
            0 => 50,
            1 => 10,
            2 => 7,
            3 => 6,
            4 => 5,
            5 => 4,
            _ => 3,
        };

        let mut grid = vec![];
        for iy in 0..segments + 1 {
            let mut vertices_row = vec![];
            let v = iy as f32 / segments as f32;

            for ix in 0..segments + 1 {
                let u = ix as f32 / segments as f32;
                let sin_v_pi = (v * consts::PI).sin();
                let position = [
                    shield.radius * (u * consts::PI * 2.0).cos() * sin_v_pi,
                    shield.radius * (v * consts::PI).cos(),
                    shield.radius * (u * consts::PI * 2.0).sin() * sin_v_pi,
                    1.0,
                ];
                vertices.push(ColorVertex { position, color });
                vertices_row.push(index_count);
                index_count += 1;
            }
            grid.push(vertices_row);
        }

        for iy in 0..segments {
            for ix in 0..segments {
                let a = grid[iy][ix + 1];
                let b = grid[iy][ix];
                let c = grid[iy + 1][ix];
                let d = grid[iy + 1][ix + 1];

                indices.extend(&[d, b, a]);
                indices.extend(&[d, c, b]);
            }
        }

        Buffers::new(device, &vertices, &indices)
    }

    pub fn new_spawn_point(device: &Device, color: [f32; 4]) -> Rc<Buffers> {
        let vertices: [ColorVertex; 11] = [
            // vertical bar
            colorvertex(-0.15, -4.0, color),
            colorvertex(0.15, -4.0, color),
            colorvertex(-0.15, 4.0, color),
            colorvertex(0.15, 4.0, color),
            // horizontal bar
            colorvertex(-4.0, -0.15, color),
            colorvertex(-4.0, 0.15, color),
            colorvertex(4.0, -0.15, color),
            colorvertex(4.0, 0.15, color),
            // arrow head
            colorvertex(4.2, 0.0, color),
            colorvertex(3.0, -1.0, color),
            colorvertex(3.0, 1.0, color),
        ];

        let indices: [u16; 15] = [
            // vertical bar
            0, 1, 2, // 1
            3, 2, 1, // 2
            // horizontal bar
            4, 5, 6, // 3
            7, 6, 5, // 4
            // arrow head
            8, 9, 10, // 5
        ];

        Buffers::new(device, &vertices, &indices)
    }

    /// Creates a single circle with radius 1 around the origin
    pub fn new_circle(device: &Device, color: [f32; 4]) -> Rc<Buffers> {
        let mut vertices: Vec<ColorVertex> = vec![];
        let mut indices: Vec<u16> = vec![];

        let iterations = 40;

        vertices.push(colorvertex(0.0, 0.0, color));
        for i in 0..iterations {
            let angle = i as f32 * 2.0 * consts::PI / (iterations as f32);
            let (sin, cos) = angle.sin_cos();
            vertices.push(colorvertex(cos, sin, color));
            indices.push(0);
            indices.push(i + 1);
            indices.push((i + 1) % iterations + 1);
        }

        Buffers::new(device, &vertices, &indices)
    }
}

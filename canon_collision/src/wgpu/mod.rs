mod buffers;
mod model3d;
mod animation;

use buffers::{ColorVertex, Vertex, Buffers};
use model3d::{Models, Model3D, ModelVertexType, ModelVertexAnimated, ShaderType, ModelVertexStatic};
use crate::audio::BGMMetadata;
use crate::entity::{RenderEntityType, RenderEntityFrame};
use crate::game::{GameState, RenderObject, RenderGame};
use crate::graphics::{self, GraphicsMessage, Render, RenderType};
use crate::menu::{RenderMenu, RenderMenuState, PlayerSelect, PlayerSelectUi};
use crate::particle::ParticleType;
use crate::results::PlayerResult;
use crate::camera::Camera;
use canon_collision_lib::entity_def::CollisionBoxRole;
use canon_collision_lib::entity_def::player::PlayerAction;
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::package::{Package, PackageUpdate};

use std::rc::Rc;
use std::sync::mpsc::{Sender, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use std::{mem, f32};
use std::borrow::Cow;
use std::num::{NonZeroU8, NonZeroU64};
use std::str::FromStr;

use bytemuck::{Pod, Zeroable};
use cgmath::Rad;
use cgmath::prelude::*;
use cgmath::{Matrix4, Vector3};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, Surface, BindGroupLayout, RenderPipeline, TextureView, Sampler, Texture, Buffer, ShaderSource, BufferBinding};
use wgpu_glyph::ab_glyph::FontArc;
use wgpu_glyph::{Section, GlyphBrush, GlyphBrushBuilder, FontId, Text};

use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;
use winit::event::{Event, WindowEvent};
use winit::window::Fullscreen;

pub struct WgpuGraphics {
    package:                      Option<Package>,
    models:                       Models,
    uniforms_buffer:              Buffer,
    uniforms_buffer_len:          usize,
    glyph_brush:                  GlyphBrush<()>,
    hack_font_id:                 FontId,
    window:                       Window,
    event_tx:                     Sender<WindowEvent<'static>>,
    render_rx:                    Receiver<GraphicsMessage>,
    device:                       Device,
    queue:                        Queue,
    surface:                      Surface,
    wsd:                          WindowSizeDependent,
    staging_belt:                 StagingBelt,
    pipeline_color_2d:            RenderPipeline,
    pipeline_color_3d:            RenderPipeline,
    pipeline_hitbox:              RenderPipeline,
    pipeline_debug:               RenderPipeline,
    pipeline_model3d_static:      RenderPipeline,
    pipeline_model3d_static_lava: RenderPipeline,
    pipeline_model3d_animated:    RenderPipeline,
    pipeline_model3d_fireball:    RenderPipeline,
    bind_group_layout_generic:    BindGroupLayout,
    bind_group_layout_model3d:    BindGroupLayout,
    sampler:                      Sampler,
    prev_fullscreen:              Option<bool>,
    frame_durations:              Vec<Duration>,
    fps:                          String,
    bgm_metadata:                 Option<(BGMMetadata, Instant)>,
    width:                        u32,
    height:                       u32,
}

const SAMPLE_COUNT: u32 = 4;

impl WgpuGraphics {
    pub async fn new(event_loop: &EventLoop<()>, event_tx: Sender<WindowEvent<'static>>, render_rx: Receiver<GraphicsMessage>) -> WgpuGraphics {
        let window = Window::new(event_loop).unwrap();
        window.set_title("Canon Collision");

        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let surface = unsafe { instance.create_surface(&window) };

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }
        ).await.unwrap();

        let (mut device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                limits: wgpu::Limits {
                    max_uniform_buffer_binding_size: 32068, // Needed for AnimatedUniform
                    ..wgpu::Limits::default()
                },
                label: None,
            },
            None,
        ).await.unwrap();

        let color_module = WgpuGraphics::create_shader(&mut device, include_str!("../shaders/color.wgsl"));

        let bind_group_layout_generic = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::all(),
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ]
            }
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout_generic],
            push_constant_ranges: &[],
        });

        let primitive_back_face_culling = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            .. Default::default()
        };

        let primitive = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            .. Default::default()
        };

        let targets = [wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Bgra8Unorm,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        }];
        let depth_stencil = Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: Default::default(),
        });

        let multisample = wgpu::MultisampleState {
            count: SAMPLE_COUNT,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        let pipeline_color_2d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &color_module,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ColorVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x4  // color
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &color_module,
                entry_point: "fs_main",
                targets: &targets,
            }),
            primitive,
            depth_stencil: depth_stencil.clone(),
            multisample,
        });

        let pipeline_color_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &color_module,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ColorVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x4  // color
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &color_module,
                entry_point: "fs_main",
                targets: &targets,
            }),
            primitive: primitive_back_face_culling,
            depth_stencil: depth_stencil.clone(),
            multisample,
        });

        let depth_stencil_disable = Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        });

        let pipeline_debug = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &color_module,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ColorVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x4  // color
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &color_module,
                entry_point: "fs_main",
                targets: &targets,
            }),
            primitive,
            depth_stencil: depth_stencil_disable.clone(),
            multisample,
        });

        let hitbox_module = WgpuGraphics::create_shader(&mut device, include_str!("../shaders/hitbox.wgsl"));

        let pipeline_hitbox = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &hitbox_module,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // position
                        1 => Float32,   // edge
                        2 => Uint32     // render_id
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &hitbox_module,
                entry_point: "fs_main",
                targets: &targets,
            }),
            primitive,
            depth_stencil: depth_stencil_disable,
            multisample,
        });

        let model3d_standard_fs = vk_shader_macros::include_glsl!("src/shaders/model3d-standard-fragment.glsl", kind: frag);
        let model3d_standard_fs_module = WgpuGraphics::create_shader_glsl(&mut device, model3d_standard_fs);

        let model3d_lava_fs = vk_shader_macros::include_glsl!("src/shaders/model3d-lava-fragment.glsl", kind: frag);
        let model3d_lava_fs_module = WgpuGraphics::create_shader_glsl(&mut device, model3d_lava_fs);

        let model3d_static_vs = vk_shader_macros::include_glsl!("src/shaders/model3d-static-vertex.glsl", kind: vert);
        let model3d_static_vs_module = WgpuGraphics::create_shader_glsl(&mut device, model3d_static_vs);

        let model3d_animated_vs = vk_shader_macros::include_glsl!("src/shaders/model3d-animated-vertex.glsl", kind: vert);
        let model3d_animated_vs_module = WgpuGraphics::create_shader_glsl(&mut device, model3d_animated_vs);

        let model3d_fireball_vs = vk_shader_macros::include_glsl!("src/shaders/model3d-fireball-vertex.glsl", kind: vert);
        let model3d_fireball_vs_module = WgpuGraphics::create_shader_glsl(&mut device, model3d_fireball_vs);

        // TODO: wgsl cant even handle the multiply yet.
        //let model3d_module = WgpuGraphics::create_shader(&mut device, include_str!("../shaders/model3d.wgsl"));

        let bind_group_layout_model3d = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::all(),
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                ]
            }
        );
        let pipeline_model3d_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout_model3d],
            push_constant_ranges: &[],
        });

        let pipeline_model3d_static = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_model3d_layout),
            vertex: wgpu::VertexState {
                module: &model3d_static_vs_module,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ModelVertexStatic>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x2  // uv
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &model3d_standard_fs_module,
                entry_point: "main",
                targets: &targets,
            }),
            primitive: primitive_back_face_culling,
            depth_stencil: depth_stencil.clone(),
            multisample,
        });

        let pipeline_model3d_static_lava = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_model3d_layout),
            vertex: wgpu::VertexState {
                module: &model3d_static_vs_module,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ModelVertexStatic>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x2  // uv
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &model3d_lava_fs_module,
                entry_point: "main",
                targets: &targets,
            }),
            primitive: primitive_back_face_culling,
            depth_stencil: depth_stencil.clone(),
            multisample,
        });

        let pipeline_model3d_animated = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_model3d_layout),
            vertex: wgpu::VertexState {
                module: &model3d_animated_vs_module,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ModelVertexAnimated>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x2, // uv
                        2 => Uint32x4,  // joints
                        3 => Float32x4  // weights
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &model3d_standard_fs_module,
                entry_point: "main",
                targets: &targets,
            }),
            primitive: primitive_back_face_culling,
            depth_stencil: depth_stencil.clone(),
            multisample,
        });

        let pipeline_model3d_fireball = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_model3d_layout),
            vertex: wgpu::VertexState {
                module: &model3d_fireball_vs_module,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ModelVertexAnimated>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, // position
                        1 => Float32x2, // uv
                        2 => Uint32x4,  // joints
                        3 => Float32x4  // weights
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &model3d_standard_fs_module,
                entry_point: "main",
                targets: &targets,
            }),
            primitive: primitive_back_face_culling,
            depth_stencil,
            multisample,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: Some(NonZeroU8::new(16).unwrap()),
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let dejavu = FontArc::try_from_slice(include_bytes!("../fonts/DejaVuSans.ttf")).unwrap();
        let hack = FontArc::try_from_slice(include_bytes!("../fonts/Hack-Regular.ttf")).unwrap();

        let mut glyph_brush_builder = GlyphBrushBuilder::using_font(dejavu);
        let hack_font_id = glyph_brush_builder.add_font(hack);
        let glyph_brush = glyph_brush_builder
            .initial_cache_size((512, 512))
            .build(&device, wgpu::TextureFormat::Bgra8Unorm);

        let width = size.width;
        let height = size.height;
        let wsd = WindowSizeDependent::new(&device, &surface, width, height);

        let models = Models::new();
        let uniforms_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &[],
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let uniforms_buffer_len = 0;

        WgpuGraphics {
            package: None,
            models,
            uniforms_buffer,
            uniforms_buffer_len,
            glyph_brush,
            hack_font_id,
            window,
            event_tx,
            render_rx,
            surface,
            device,
            queue,
            wsd,
            staging_belt: StagingBelt::new(),
            pipeline_color_2d,
            pipeline_color_3d,
            pipeline_hitbox,
            pipeline_debug,
            pipeline_model3d_static,
            pipeline_model3d_static_lava,
            pipeline_model3d_animated,
            pipeline_model3d_fireball,
            bind_group_layout_generic,
            bind_group_layout_model3d,
            sampler,
            prev_fullscreen: None,
            frame_durations: vec!(),
            fps: "".into(),
            bgm_metadata: None,
            width,
            height,
        }
    }

    fn create_shader_glsl(device: &mut Device, shader: &[u32]) -> wgpu::ShaderModule {
        device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::SpirV(Cow::Borrowed(shader)),
        })
    }

    fn create_shader(device: &mut Device, shader: &str) -> wgpu::ShaderModule {
        device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(Cow::Borrowed(shader)),
        })
    }

    pub fn update(&mut self, event: Event<()>, control_flow: &mut ControlFlow) {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::MainEventsCleared => {
                let frame_start = Instant::now();

                // get the most recent render
                let mut render = None;
                loop {
                    match self.render_rx.try_recv() {
                        Ok(message) => {
                            // we want only the last render message
                            render = Some(self.read_message(message));
                        }
                        Err(TryRecvError::Empty) => {
                            if render.is_none() {
                                // restart loop so we can send more window events to the app thread
                                return;
                            }
                            else {
                                break;
                            }
                        }
                        Err(TryRecvError::Disconnected) => {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                    }
                }
                let render = render.expect("Guaranteed by logic above");

                let resolution: (u32, u32) = self.window.inner_size().into();
                self.window_resize(resolution.0, resolution.1);

                self.render(render);
                self.frame_durations.push(frame_start.elapsed());
            }
            Event::WindowEvent { event, .. } => {
                if let Some(event) = event.to_static() {
                    if let Err(_) = self.event_tx.send(event) {
                        *control_flow = ControlFlow::Exit;
                        
                    }
                }
            }
            _ => {}
        }
    }

    fn read_message(&mut self, message: GraphicsMessage) -> Render {
        // TODO: Refactor out the vec + enum once vulkano backend is removed
        for package_update in message.package_updates {
            match package_update {
                PackageUpdate::Package (package) => {
                    self.package = Some(package);
                }
                PackageUpdate::DeleteFighterFrame { fighter, action, frame_index } => {
                    if let &mut Some(ref mut package) = &mut self.package {
                        package.entities[fighter.as_ref()].actions[action.as_ref()].frames.remove(frame_index);
                    }
                }
                PackageUpdate::InsertFighterFrame { fighter, action, frame_index, frame } => {
                    if let &mut Some(ref mut package) = &mut self.package {
                        package.entities[fighter.as_ref()].actions[action.as_ref()].frames.insert(frame_index, frame);
                    }
                }
                PackageUpdate::DeleteStage { index, .. } => {
                    if let &mut Some(ref mut package) = &mut self.package {
                        package.stages.remove(index);
                    }
                }
                PackageUpdate::InsertStage { index, key, stage } => {
                    if let &mut Some(ref mut package) = &mut self.package {
                        package.stages.insert(index, key, stage);
                    }
                }
            }
        }
        message.render
    }

    fn window_resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }

        self.width = width;
        self.height = height;

        self.wsd = WindowSizeDependent::new(&self.device, &self.surface, width, height);
    }

    fn render(&mut self, render: Render) {
        // TODO: Fullscreen logic should handle the window manager setting fullscreen state.
        // *    Use this instead of self.prev_fullscreen
        // *    Send new fullscreen state back to the game logic thread
        // Waiting on Window::get_fullscreen() to be added to winit: https://github.com/tomaka/winit/issues/579

        if self.prev_fullscreen.is_none() {
            self.prev_fullscreen = Some(!render.fullscreen); // force set fullscreen state on first update
        }
        if render.fullscreen != self.prev_fullscreen.unwrap() { // Avoid needlessly recalling set_fullscreen(Some(..)) to avoid FPS drops on at least X11
            if render.fullscreen {
                let monitor = self.window.current_monitor();
                // TODO: Investigate exclusive fullscreen
                self.window.set_fullscreen(Some(Fullscreen::Borderless(monitor)));
            }
            else {
                self.window.set_fullscreen(None);
            }
            self.prev_fullscreen = Some(render.fullscreen);
        }

        // hide cursor during regular play in fullscreen
        let in_game_paused = if let RenderType::Game(game) = &render.render_type {
            if let GameState::Paused = &game.state {
                true
            } else {
                false
            }
        } else {
            false
        };
        self.window.set_cursor_visible(!render.fullscreen || in_game_paused);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: None
        });

        match &render.render_type {
            RenderType::Game (render) => {
                self.models.load_game(&self.device, &self.queue, render);
            }
            RenderType::Menu (render) => {
                let fighters = &self.package.as_ref().unwrap().fighters(); // TODO: avoid recreating multiple times every frame
                self.models.load_menu(&self.device, &self.queue, render, fighters);
            }
        }

        let frame = self.surface.get_current_texture().unwrap();

        let draws = match render.render_type {
            RenderType::Game(game) => self.game_render(game, &render.command_output),
            RenderType::Menu(menu) => self.menu_render(menu, &render.command_output)
        };

        let uniforms_bytes = {
            let uniforms_size = draws.iter().map(|x| x.ty.uniform_size_padded()).sum();
            let mut uniforms_bytes = vec!(0; uniforms_size);
            let mut uniforms_offset = 0;
            for draw in &draws {
                let size        = draw.ty.uniform_size();
                let size_padded = draw.ty.uniform_size_padded();

                uniforms_bytes[uniforms_offset..uniforms_offset+size].copy_from_slice(draw.ty.uniform_bytes());
                uniforms_offset += size_padded;
            }
            uniforms_bytes
        };

        if uniforms_bytes.len() > self.uniforms_buffer_len {
            self.uniforms_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &uniforms_bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
            });
            self.uniforms_buffer_len = uniforms_bytes.len();
        }
        else {
            self.queue.write_buffer(&self.uniforms_buffer, 0, &uniforms_bytes);
        }

        let view = &frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut bind_groups = vec!();
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &self.wsd.multisampled_framebuffer,
                    resolve_target: Some(view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.wsd.depth_stencil,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: true,
                    }),
                }),
                label: None,
            });

            let mut uniforms_offset = 0;
            for draw in &draws {
                let uniform_resource = wgpu::BindingResource::Buffer(BufferBinding{
                    buffer: &self.uniforms_buffer,
                    offset: uniforms_offset,
                    size: NonZeroU64::new(draw.ty.uniform_size() as u64),
                });
                let bind_group = match &draw.ty {
                    DrawType::Color { .. } => {
                        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            layout: &self.bind_group_layout_generic,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: uniform_resource,
                            }]
                        })
                    }
                    DrawType::Hitbox { .. } => {
                        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            layout: &self.bind_group_layout_generic,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: uniform_resource,
                            }]
                        })
                    }
                    DrawType::ModelAnimated { texture, .. } => self.create_bind_group_model3d(uniform_resource, texture),
                    DrawType::Fireball      { texture, .. } => self.create_bind_group_model3d(uniform_resource, texture),
                    DrawType::ModelStatic   { texture, .. } => self.create_bind_group_model3d(uniform_resource, texture),
                    DrawType::Lava          { texture, .. } => self.create_bind_group_model3d(uniform_resource, texture),
                };
                bind_groups.push(bind_group);
                uniforms_offset += draw.ty.uniform_size_padded() as u64;
            }

            for (i, draw) in draws.iter().enumerate() {
                let pipeline = match &draw.ty {
                    DrawType::Color         { debug: false, dimension3: false, .. } => &self.pipeline_color_2d,
                    DrawType::Color         { debug: false, dimension3: true,  .. } => &self.pipeline_color_3d,
                    DrawType::Color         { debug: true,                     .. } => &self.pipeline_debug,
                    DrawType::Hitbox        {                                  .. } => &self.pipeline_hitbox,
                    DrawType::ModelAnimated {                                  .. } => &self.pipeline_model3d_animated,
                    DrawType::ModelStatic   {                                  .. } => &self.pipeline_model3d_static,
                    DrawType::Lava          {                                  .. } => &self.pipeline_model3d_static_lava,
                    DrawType::Fireball      {                                  .. } => &self.pipeline_model3d_fireball,
                };
                rpass.set_pipeline(pipeline);
                rpass.set_bind_group(0, &bind_groups[i], &[]);
                rpass.set_index_buffer(draw.buffers.index.slice(..), wgpu::IndexFormat::Uint16);
                rpass.set_vertex_buffer(0, draw.buffers.vertex.slice(..));
                rpass.draw_indexed(0..draw.buffers.index_count as u32, 0, 0..1);
            }
        }
        self.glyph_brush.draw_queued(
            &self.device,
            &mut self.staging_belt.staging_belt,
            &mut encoder,
            view,
            self.width,
            self.height
        ).unwrap();
        self.staging_belt.finish();

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.staging_belt.recall();
    }

    fn create_bind_group_model3d(&self, uniform: wgpu::BindingResource, texture: &Rc<Texture>) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout_model3d,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform,
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture.create_view(&wgpu::TextureViewDescriptor::default())),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ]
        })
    }

    fn command_render(&mut self, lines: &[String]) {
        // TODO: Render white text, with black background
        for (i, line) in lines.iter().enumerate() {
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(line)
                    .with_color([1.0, 1.0, 0.0, 1.0])
                    .with_scale(20.0)
                    .with_font_id(self.hack_font_id)
                ),
                screen_position: (0.0, self.height as f32 - 25.0 - 20.0 * i as f32),
                .. Section::default()
            });
        }
    }

    fn game_timer_render(&mut self, timer: &Option<Duration>) {
        if let &Some(ref timer) = timer {
            let minutes = timer.as_secs() / 60;
            let seconds = timer.as_secs() % 60;
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(format!("{:02}:{:02}", minutes, seconds).as_ref())
                        .with_color([1.0, 1.0, 1.0, 1.0])
                        .with_scale(40.0)
                ),
                screen_position: ((self.width / 2) as f32 - 50.0, 4.0),
                .. Section::default()
            });
        }
    }

    fn game_hud_render(&mut self, objects: &[RenderObject]) {
        let mut entities = 0;
        for object in objects {
            if let RenderObject::Entity (entity) = object {
                if let RenderEntityType::Player (_) = &entity.render_type {
                    entities += 1;
                }
            }
        }
        let distance = (self.width / (entities + 1)) as f32;

        let mut location = -100.0;
        for object in objects {
            if let RenderObject::Entity (entity) = object {
                location += distance;
                if let RenderEntityType::Player (player) = &entity.render_type {
                    match PlayerAction::from_str(&entity.frames[0].action) {
                        Ok(PlayerAction::Eliminated) => { }
                        _ => {
                            let c = entity.fighter_color;
                            let color = [c[0], c[1], c[2], 1.0];

                            if let Some(stocks) = player.stocks {
                                let stocks_string = if stocks > 5 {
                                    format!("⬤ x {}", stocks)
                                } else {
                                    let mut stocks_string = String::new();
                                    for _ in 0..stocks {
                                        stocks_string.push('⬤');
                                    }
                                    stocks_string
                                };

                                self.glyph_brush.queue(Section {
                                    text: vec!(
                                        Text::new(stocks_string.as_ref())
                                            .with_color(color)
                                            .with_scale(22.0)
                                    ),
                                    screen_position: (location + 10.0, self.height as f32 - 130.0),
                                    .. Section::default()
                                });
                            }

                            self.glyph_brush.queue(Section {
                                text: vec!(
                                    Text::new(format!("{}%", player.damage).as_ref())
                                    .with_color(color)
                                    .with_scale(110.0)
                                ),
                                screen_position: (location, self.height as f32 - 117.0),
                                .. Section::default()
                            });
                        }
                    }
                }
            }
        }
    }

    fn fps_render(&mut self) {
        if self.frame_durations.len() == 60 {
            let total: Duration = self.frame_durations.iter().sum();
            let total = total.as_secs() as f64 + total.subsec_nanos() as f64 / 1_000_000_000.0;
            let average = total / 60.0;
            self.fps = format!("{:.0}", 1.0 / average);
            self.frame_durations.clear();
        }

        self.glyph_brush.queue(Section {
            text: vec!(
                Text::new(&self.fps)
                .with_color([1.0, 1.0, 1.0, 1.0])
                .with_scale(20.0)
            ),
            screen_position: (self.width as f32 - 70.0, 4.0),
            .. Section::default()
        });
    }

    fn bgm_change(&mut self, render: &RenderGame) {
        if let Some(bgm_metadata) = &render.bgm_metadata {
            self.bgm_metadata = Some((bgm_metadata.clone(), Instant::now()));
        }

        if let Some((bgm_metadata, start_time)) = self.bgm_metadata.clone() {
            if start_time.elapsed() > Duration::from_secs(10) {
                self.bgm_metadata = None;
            }

            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new("♪")
                    .with_color([1.0, 1.0, 1.0, 0.9])
                    .with_scale(150.0)
                ),
                screen_position: (80.0, 70.0),
                .. Section::default()
            });

            let title = format!("{}\n", bgm_metadata.title);
            let artist = bgm_metadata.artist.map(|x| format!("{}\n", x));
            let album = bgm_metadata.album.map(|x| format!("{}\n", x));

            let mut text = vec!(
                Text::new(&title)
                .with_color([1.0, 1.0, 1.0, 0.9])
                .with_scale(45.0)
            );

            if let Some(artist) = &artist {
                text.push(
                    Text::new(artist)
                    .with_color([1.0, 1.0, 1.0, 0.9])
                    .with_scale(20.0)
                );
                text.push(
                    Text::new("\n")
                    .with_color([1.0, 1.0, 1.0, 0.9])
                    .with_scale(5.0)
                );
            }

            if let Some(album) = &album {
                text.push(
                    Text::new(album)
                    .with_color([1.0, 1.0, 1.0, 0.9])
                    .with_scale(20.0)
                );
            }

            self.glyph_brush.queue(Section {
                text,
                screen_position: (160.0, 100.0),
                .. Section::default()
            });
        }
    }

    fn debug_lines_render(&mut self, lines: &[String]) {
        if lines.len() > 1 {
            for (i, line) in lines.iter().enumerate() {
                self.glyph_brush.queue(Section {
                    text: vec!(
                        Text::new(line)
                            .with_color([1.0, 1.0, 0.0, 1.0])
                            .with_scale(20.0)
                            .with_font_id(self.hack_font_id)
                    ),
                    screen_position: (0.0, 12.0 + 20.0 * i as f32),
                    .. Section::default()
                });
            }
        }
    }

    fn render_hitbox_buffers(
        &self,
        render:     &RenderGame,
        buffers:    Rc<Buffers>,
        entity:     &Matrix4<f32>,
        edge_color: [f32; 4],
        color:      [f32; 4]
    ) -> Draw {
        let camera = render.camera.transform();
        let transformation = camera * entity;
        let uniform = HitboxUniform {
            edge_color,
            color,
            transform: transformation.into(),
        };
        Draw {
            ty: DrawType::Hitbox { uniform },
            buffers
        }
    }

    fn render_model3d(
        &self,
        camera:          &Camera,
        model:           &Model3D,
        entity:          &Matrix4<f32>,
        animation_name:  &str,
        animation_frame: f32,
        animation_frame_no_restart: f32,
    ) -> Vec<Draw> {
        let camera = camera.transform();
        let mut draws = vec!();

        for mesh in &model.meshes {
            let transform = (camera * entity * mesh.transform).into();
            for primitive in &mesh.primitives {
                if let Some(texture) = primitive.texture.clone() {
                    let buffers = primitive.buffers.clone();

                    let draw = match primitive.vertex_type {
                        ModelVertexType::Animated => {
                            let mut joint_transforms = [Matrix4::identity().into(); 500];
                            for root_joint in &mesh.root_joints {
                                if let Some(animation) = model.animations.get(animation_name) {
                                    animation::generate_joint_transforms(animation, animation_frame, root_joint, Matrix4::identity(), &mut joint_transforms);
                                }
                            }

                            let uniform = AnimatedUniform {
                                transform,
                                joint_transforms,
                                frame_count: animation_frame_no_restart,
                            };
                            let ty = match primitive.shader_type {
                                ShaderType::Standard | ShaderType::Lava => DrawType::ModelAnimated { uniform, texture },
                                ShaderType::Fireball => DrawType::Fireball { uniform, texture },
                            };
                            Draw { ty, buffers }
                        }
                        ModelVertexType::Static => {
                            let ty = match primitive.shader_type {
                                ShaderType::Lava => {
                                    let uniform = TransformUniformCycle {
                                        transform,
                                        frame_count: animation_frame_no_restart,
                                    };
                                    DrawType::Lava { uniform, texture }
                                },
                                ShaderType::Standard | ShaderType::Fireball => {
                                    let uniform = TransformUniform { transform };
                                    DrawType::ModelStatic { uniform, texture }
                                }
                            };
                            Draw { ty, buffers }
                        }
                    };
                    draws.push(draw);
                } else {
                    error!("Models without textures are not rendered");
                }
            }
        }

        draws
    }

    fn render_color_buffers(
        &self,
        render:     &RenderGame,
        buffers:    Rc<Buffers>,
        entity:     &Matrix4<f32>,
        debug:      bool,
        dimension3: bool
    ) -> Draw {
        let camera = render.camera.transform();
        let transformation = camera * entity;
        let uniform = TransformUniform { transform: transformation.into() };

        Draw {
            ty: DrawType::Color { uniform, debug, dimension3 },
            buffers
        }
    }

    fn game_render(&mut self, render: RenderGame, command_output: &[String]) -> Vec<Draw> {
        let mut draws = vec!();
        let mut rng = StdRng::from_seed(render.seed);
        if command_output.is_empty() {
            self.game_hud_render(&render.entities);
            self.game_timer_render(&render.timer);
            self.debug_lines_render(&render.debug_lines);
            self.fps_render();
            self.bgm_change(&render);
        }
        else {
            self.command_render(command_output);
        }

        match render.state {
            GameState::Local  => { }
            GameState::Paused => {
                // TODO: blue vaporwavey background lines to indicate pause :D
                // also double as measuring/scale lines
                // configurable size via treeflection
                // but this might be desirable to have during normal gameplay to, hmmmm....
                // Just have a 5 second fade out time so it doesnt look clunky and can be used during frame advance
            }
            _ => { }
        }

        let stage_transformation = Matrix4::identity();
        if render.render_stage_mode.normal() {
            if let Some(stage) = self.models.get(&render.stage_model_name) {
                draws.extend(self.render_model3d(
                    &render.camera,
                    stage,
                    &stage_transformation,
                    "Main",
                    (render.current_frame % 300) as f32, // TODO: Somehow get the animation length from the gltf
                    render.current_frame as f32
                ));
            }
        }

        if render.render_stage_mode.debug() {
            if let Some(buffers) = Buffers::new_surfaces(&self.device, &render.surfaces) {
                draws.push(self.render_color_buffers(&render, buffers, &stage_transformation, false, false));
            }

            if let Some(buffers) = Buffers::new_surfaces_fill(&self.device, &render.surfaces) {
                draws.push(self.render_color_buffers(&render, buffers, &stage_transformation, false, false));
            }

            if let Some(buffers) = Buffers::new_selected_surfaces(&self.device, &render.surfaces, &render.selected_surfaces) {
                draws.push(self.render_color_buffers(&render, buffers, &stage_transformation, false, false));
            }
        }

        for entity in render.entities.iter() {
            match entity {
                RenderObject::Entity (entity) => {
                    fn entity_matrix(frame: &RenderEntityFrame) -> Matrix4<f32> {
                        let dir = Matrix4::from_nonuniform_scale(if frame.face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                        let rotate = Matrix4::from_angle_z(Rad(frame.frame_angle));
                        let position = Matrix4::from_translation(Vector3::new(frame.frame_bps.0, frame.frame_bps.1, 0.0));
                        position * rotate * dir
                    }

                    let transformation = entity_matrix(&entity.frames[0]);

                    // draw entity
                    let action = &entity.frames[0].action;
                    match PlayerAction::from_str(action) {
                        Ok(PlayerAction::Eliminated) => { }
                        _ => {
                            let fighter_model_name = &entity.frames[0].model_name;
                            if entity.debug.render.normal() && entity.visible {
                                let dir = Matrix4::from_angle_y(if entity.frames[0].face_right { Rad::turn_div_4() } else { -Rad::turn_div_4() });
                                let rotate: Matrix4<f32> = entity.frames[0].render_angle.into();
                                let position = Matrix4::from_translation(Vector3::new(
                                    entity.frames[0].render_bps.0,
                                    entity.frames[0].render_bps.1,
                                    entity.frames[0].render_bps.2,
                                ));
                                let transformation = position * rotate * dir;
                                if let Some(fighter) = self.models.get(fighter_model_name) {
                                    draws.extend(self.render_model3d(
                                        &render.camera,
                                        fighter,
                                        &transformation,
                                        action,
                                        entity.frames[0].frame as f32,
                                        entity.frames[0].frame_no_restart as f32
                                    ));
                                }
                            }
                        }
                    }

                    // draw entity ecb
                    if entity.debug.ecb {
                        if let Some(ecb) = &entity.frames[0].ecb {
                            // TODO: Set individual corner vertex colours to show which points of the ecb are selected
                            let buffers  = Buffers::new_ecb(&self.device, ecb);
                            let dir      = Matrix4::from_nonuniform_scale(if entity.frames[0].face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                            let position = Matrix4::from_translation(Vector3::new(entity.frames[0].frame_bps.0, entity.frames[0].frame_bps.1, 0.0));
                            let transformation = position * dir;

                            draws.push(self.render_color_buffers(&render, buffers, &transformation, false, false));
                        }
                    }

                    // draw entity debug overlay
                    if entity.debug.render.debug() {
                        if entity.debug.render.onion_skin() {
                            if let Some(frame) = entity.frames.get(2) {
                                if let Some(buffers) = Buffers::new_fighter_frame(&self.device, self.package.as_ref().unwrap(), &frame.entity_def_key, &frame.action, frame.frame) {
                                    let transformation = entity_matrix(frame);
                                    let onion_color = [0.4, 0.4, 0.4, 0.4];
                                    draws.push(self.render_hitbox_buffers(&render, buffers, &transformation, onion_color, onion_color));
                                }
                            }

                            if let Some(frame) = entity.frames.get(1) {
                                if let Some(buffers) = Buffers::new_fighter_frame(&self.device, self.package.as_ref().unwrap(), &frame.entity_def_key, &frame.action, frame.frame) {
                                    let transformation = entity_matrix(frame);
                                    let onion_color = [0.80, 0.80, 0.80, 0.9];
                                    draws.push(self.render_hitbox_buffers(&render, buffers, &transformation, onion_color, onion_color));
                                }
                            }
                        }

                        // draw entity
                        if let Some(buffers) = Buffers::new_fighter_frame(&self.device, self.package.as_ref().unwrap(), &entity.frames[0].entity_def_key, &entity.frames[0].action, entity.frames[0].frame) {
                            let color = [0.9, 0.9, 0.9, 1.0];
                            let edge_color = if entity.entity_selected {
                                [0.0, 1.0, 0.0, 1.0]
                            } else {
                                let c = entity.fighter_color;
                                [c[0], c[1], c[2], 1.0]
                            };
                            draws.push(self.render_hitbox_buffers(&render, buffers, &transformation, edge_color, color));
                        }
                        else {
                             // TODO: Give some indication that we are rendering a deleted or otherwise nonexistent frame
                        }
                    }

                    // draw selected colboxes
                    if !entity.selected_colboxes.is_empty() {
                        let color = [0.0, 1.0, 0.0, 1.0];
                        let buffers = Buffers::new_fighter_frame_colboxes(&self.device, self.package.as_ref().unwrap(), &entity.frames[0].entity_def_key, &entity.frames[0].action, entity.frames[0].frame, &entity.selected_colboxes);
                        draws.push(self.render_hitbox_buffers(&render, buffers, &transformation, color, color));
                    }

                    // draw hitbox debug arrows
                    // TODO: this should be usable for all entities
                    if entity.debug.hitbox_vectors {
                        // TODO: lets move these to the WgpuGraphics struct
                        let kbg_arrow = Buffers::new_arrow(&self.device, [1.0,  1.0,  1.0, 1.0]);
                        let bkb_arrow = Buffers::new_arrow(&self.device, [0.17, 0.17, 1.0, 1.0]);
                        for colbox in entity.frame_data.colboxes.iter() {
                            if let CollisionBoxRole::Hit(ref hitbox) = colbox.role {
                                let kb_squish = 0.5;
                                let squish_kbg = Matrix4::from_nonuniform_scale(0.6, hitbox.kbg * kb_squish, 1.0);
                                let squish_bkb = Matrix4::from_nonuniform_scale(0.3, (hitbox.bkb / 100.0) * kb_squish, 1.0); // divide by 100 so the arrows are comparable if the hit fighter is on 100%
                                let rotate = Matrix4::from_angle_z(Rad(hitbox.angle.to_radians() - f32::consts::PI / 2.0));
                                let x = entity.frames[0].frame_bps.0 + colbox.point.0;
                                let y = entity.frames[0].frame_bps.1 + colbox.point.1;
                                let position = Matrix4::from_translation(Vector3::new(x, y, 0.0));
                                let transformation_bkb = position * rotate * squish_bkb;
                                let transformation_kbg = position * rotate * squish_kbg;
                                draws.push(self.render_color_buffers(&render, kbg_arrow.clone(), &transformation_kbg, false, false));
                                draws.push(self.render_color_buffers(&render, bkb_arrow.clone(), &transformation_bkb, false, false));
                            }
                        }
                    }

                    // draw debug vector arrows
                    let num_arrows = entity.vector_arrows.len() as f32;
                    for (i, arrow) in entity.vector_arrows.iter().enumerate() {
                        let arrow_buffers = Buffers::new_arrow(&self.device, arrow.color);
                        let squish = Matrix4::from_nonuniform_scale((num_arrows - i as f32) / num_arrows, 1.0, 1.0); // consecutive arrows are drawn slightly thinner so we can see arrows behind
                        let rotate = Matrix4::from_angle_z(Rad(arrow.y.atan2(arrow.x) - f32::consts::PI / 2.0));
                        let position = Matrix4::from_translation(Vector3::new(entity.frames[0].frame_bps.0, entity.frames[0].frame_bps.1, 0.0));
                        let transformation = position * rotate * squish;
                        draws.push(self.render_color_buffers(&render, arrow_buffers, &transformation, false, false));
                    }

                    // draw particles
                    for particle in &entity.particles {
                        let c = particle.color;
                        match &particle.p_type {
                            &ParticleType::Spark { size, .. } => {
                                let rotate = Matrix4::from_angle_x(Rad(particle.angle))
                                    * Matrix4::from_angle_y(Rad(particle.angle))
                                    * Matrix4::from_angle_z(Rad(particle.angle));
                                let size = size * (1.0 - particle.counter_mult());
                                let size = Matrix4::from_nonuniform_scale(size, size, 1.0);
                                let position = Matrix4::from_translation(Vector3::new(particle.x, particle.y, particle.z));
                                let transformation = position * rotate * size;
                                let color = [c[0], c[1], c[2], 1.0];
                                let triangle_buffers = Buffers::new_triangle(&self.device, color);
                                draws.push(self.render_color_buffers(&render, triangle_buffers, &transformation, false, false));
                            }
                            &ParticleType::AirJump => {
                                let size = Matrix4::from_nonuniform_scale(3.0 + particle.counter_mult(), 1.15 + particle.counter_mult(), 1.0);
                                let position = Matrix4::from_translation(Vector3::new(particle.x, particle.y, particle.z));
                                let transformation = position * size;
                                let color = [c[0], c[1], c[2], (1.0 - particle.counter_mult()) * 0.7];
                                let jump_buffers = Buffers::new_circle(&self.device, color);
                                draws.push(self.render_color_buffers(&render, jump_buffers, &transformation, false, false));
                            }
                            &ParticleType::Hit { knockback, damage } => {
                                // needs to rendered last to ensure we dont have anything drawn on top of the inversion
                                let size = Matrix4::from_nonuniform_scale(0.2 * knockback, 0.08 * damage, 1.0);
                                let rotate = Matrix4::from_angle_z(Rad(particle.angle - f32::consts::PI / 2.0));
                                let position = Matrix4::from_translation(Vector3::new(particle.x, particle.y, particle.z));
                                let transformation = position * rotate * size;
                                let color = [0.5, 0.5, 0.5, 1.5];
                                let hit_buffers = Buffers::new_circle(&self.device, color);
                                draws.push(self.render_color_buffers(&render, hit_buffers, &transformation, false, false)); // TODO: Invert
                            }
                        }
                    }

                    // Draw spawn plat
                    if let RenderEntityType::Player (_) = entity.render_type {
                        match PlayerAction::from_str(&entity.frames[0].action) {
                            Ok(PlayerAction::ReSpawn) | Ok(PlayerAction::ReSpawnIdle) => {
                                // TODO: get width from player dimensions
                                let width = 15.0;
                                let height = width / 4.0;
                                let scale = Matrix4::from_nonuniform_scale(width, -height, 1.0); // negative y to point triangle downwards.
                                let frame_bps = &entity.frames[0].frame_bps;
                                let position = Matrix4::from_translation(Vector3::new(frame_bps.0, frame_bps.1, 0.0));
                                let transformation = position * scale;

                                let c = entity.fighter_color;
                                let color = [c[0], c[1], c[2], 1.0];
                                let triangle_buffers = Buffers::new_triangle(&self.device, color);

                                draws.push(self.render_color_buffers(&render, triangle_buffers, &transformation, false, false));
                            }
                            _ => { }
                        }
                    }
                }
                RenderObject::RectOutline (render_rect) => {
                    let transformation = Matrix4::identity();
                    let buffers = Buffers::rect_outline_buffers(&self.device, render_rect);
                    draws.push(self.render_color_buffers(&render, buffers, &transformation, false, false));
                }
                RenderObject::SpawnPoint (render_point) => {
                    let buffers = Buffers::new_spawn_point(&self.device, render_point.color);
                    let flip = Matrix4::from_nonuniform_scale(if render_point.face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                    let position = Matrix4::from_translation(Vector3::new(render_point.x, render_point.y, 0.0));
                    let transformation = position * flip;
                    draws.push(self.render_color_buffers(&render, buffers, &transformation, false, false));
                }
            }
        }

        // Some things need to be rendered after everything else as they are transparent
        for entity in render.entities.iter() {
            match entity {
                RenderObject::Entity (entity) => {
                    if let RenderEntityType::Player (player) = &entity.render_type {
                        // draw shield
                        if let Some(shield) = &player.shield {
                            let position = Matrix4::from_translation(Vector3::new(shield.pos.0, shield.pos.1, 0.0));
                            let color = if shield.distort > 0 {
                                let c = shield.color;
                                [c[0] * rng.gen_range(0.75..=1.25), c[1] * rng.gen_range(0.75..=1.25), c[2] * rng.gen_range(0.75..=1.25), c[3] * rng.gen_range(0.8..=1.2)]
                            } else {
                                shield.color
                            };
                            let buffers = Buffers::new_shield(&self.device, shield, color);
                            draws.push(self.render_color_buffers(&render, buffers, &position, false, true));
                        }
                    }
                }
                _ => { }
            }
        }

        draws
    }

    fn menu_render(&mut self, render: RenderMenu, command_output: &[String]) -> Vec<Draw> {
        self.fps_render();
        let mut draws = vec!();

        match render.state {
            RenderMenuState::GameSelect (selection) => {
                self.draw_game_selector(selection);
                self.command_render(command_output);
            }
            RenderMenuState::ReplaySelect (replay_names, selection) => {
                self.draw_replay_selector(&replay_names, selection);
                self.command_render(command_output);
            }
            RenderMenuState::CharacterSelect (selections, back_counter, back_counter_max) => {
                let mut plugged_in_selections: Vec<(&PlayerSelect, usize)> = vec!();
                for (i, selection) in selections.iter().enumerate() {
                    if selection.ui.is_visible() {
                        plugged_in_selections.push((selection, i));
                    }
                }

                draws.push(self.draw_back_counter(back_counter, back_counter_max));
                self.glyph_brush.queue(Section {
                    text: vec!(
                        Text::new("Select Fighters")
                        .with_color([1.0, 1.0, 1.0, 1.0])
                        .with_scale(50.0)
                    ),
                    screen_position: (100.0, 4.0),
                    .. Section::default()
                });

                match plugged_in_selections.len() {
                    0 => {
                        self.glyph_brush.queue(Section {
                            text: vec!(
                                Text::new("There are no controllers plugged in.")
                                    .with_color([1.0, 1.0, 1.0, 1.0])
                                    .with_scale(30.0)
                            ),
                            screen_position: (100.0, 100.0),
                            .. Section::default()
                        });
                    }
                    1 => {
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 0, -0.9, -0.8, 0.9, 0.9));

                    }
                    2 => {
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 0, -0.9, -0.8, 0.0, 0.9));
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 1,  0.0, -0.8, 0.9, 0.9));
                    }
                    3 => {
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 0, -0.9, -0.8, 0.0, 0.0));
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 1,  0.0, -0.8, 0.9, 0.0));
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 2, -0.9,  0.0, 0.0, 0.9));
                    }
                    4 => {
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 0, -0.9, -0.8, 0.0, 0.0));
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 1,  0.0, -0.8, 0.9, 0.0));
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 2, -0.9,  0.0, 0.0, 0.9));
                        draws.extend(self.draw_fighter_selector(&plugged_in_selections, 3,  0.0,  0.0, 0.9, 0.9));
                    }
                    _ => {
                        self.glyph_brush.queue(Section {
                            text: vec!(
                                Text::new("Currently only supports up to 4 controllers. Please unplug some.")
                                    .with_color([1.0, 1.0, 1.0, 1.0])
                                    .with_scale(30.0)
                            ),
                            screen_position: (100.0, 100.0),
                            .. Section::default()
                        });
                    }
                }
                self.command_render(command_output);
            }
            RenderMenuState::StageSelect (selection) => {
                draws.extend(self.draw_stage_selector(selection));
                self.command_render(command_output);
            }
            RenderMenuState::GameResults { results, replay_saved } => {
                let max = results.len() as f32;
                for (i, result) in results.iter().enumerate() {
                    let i = i as f32;
                    let start_x = i / max;
                    self.draw_player_result(result, start_x);
                }

                if replay_saved {
                    self.glyph_brush.queue(Section {
                        text: vec!(
                            Text::new("Replay saved!")
                                .with_color([1.0, 1.0, 1.0, 1.0])
                                .with_scale(30.0)
                        ),
                        screen_position: (30.0, self.height as f32 - 30.0),
                        .. Section::default()
                    });
                }
            }
            RenderMenuState::GenericText (ref text) => {
                self.glyph_brush.queue(Section {
                    text: vec!(
                        Text::new(text)
                        .with_color([1.0, 1.0, 0.0, 1.0])
                        .with_scale(30.0)
                    ),
                    screen_position: (100.0, 50.0),
                    .. Section::default()
                });
            }
        }

        draws
    }

    fn draw_game_selector(&mut self, selection: usize) {
        self.glyph_brush.queue(Section {
            text: vec!(
                Text::new("Select Game Mode")
                .with_color([1.0, 1.0, 1.0, 1.0])
                .with_scale(50.0)
            ),
            screen_position: (100.0, 4.0),
            .. Section::default()
        });

        let modes = vec!("Local", "Netplay", "Replays");
        for (mode_i, name) in modes.iter().enumerate() {
            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let x_offset = if mode_i == selection { 0.1 } else { 0.0 };
            let x = self.width as f32 * (0.1 + x_offset);
            let y = self.height as f32 * 0.1 + mode_i as f32 * 50.0;
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(name)
                        .with_color([1.0, 1.0, 1.0, 1.0])
                        .with_scale(size)
                ),
                screen_position: (x, y),
                .. Section::default()
            });
        }
    }

    fn draw_replay_selector(&mut self, replay_names: &[String], selection: usize) {
        self.glyph_brush.queue(Section {
            text: vec!(
                Text::new("Select Replay")
                    .with_color([1.0, 1.0, 1.0, 1.0])
                    .with_scale(50.0)
            ),
            screen_position: (100.0, 4.0),
            .. Section::default()
        });

        for (replay_i, name) in replay_names.iter().enumerate() {
            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let x_offset = if replay_i == selection { 0.1 } else { 0.0 };
            let x = self.width as f32 * (0.1 + x_offset);
            let y = self.height as f32 * 0.1 + replay_i as f32 * 50.0;
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(name.as_ref())
                        .with_color([1.0, 1.0, 1.0, 1.0])
                        .with_scale(size)
                ),
                screen_position: (x, y),
                .. Section::default()
            });
        }
    }

    // TODO: Rewrite text rendering to be part of scene instead of just plastered on top
    // TODO: Then this bar can be drawn on top of the package banner text
    fn draw_back_counter(&self, back_counter: usize, back_counter_max: usize) -> Draw {
        let transform = Matrix4::identity().into();
        let uniform = TransformUniform { transform };

        let rect = Rect {
            x1: -1.0,
            y1: -0.85,
            x2: back_counter as f32 / back_counter_max as f32 * 2.0 - 1.0,
            y2: -1.0,
        };
        let buffers = Buffers::rect_buffers(&self.device, rect, [1.0, 1.0, 1.0, 1.0]);

        Draw {
            ty: DrawType::Color {
                uniform,
                debug: true,
                dimension3: false,
            },
            buffers
        }
    }

    fn draw_fighter_selector(&mut self, selections: &[(&PlayerSelect, usize)], i: usize, start_x: f32, start_y: f32, end_x: f32, end_y: f32) -> Vec<Draw> {
        let mut draws = vec!();
        let fighters = &self.package.as_ref().unwrap().fighters();
        let (selection, controller_i) = selections[i];

        // render player name
        {
            let x = ((start_x+1.0) / 2.0) * self.width  as f32;
            let y = ((start_y+1.0) / 2.0) * self.height as f32;
            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let color = if let Some((check_i, _)) = selection.controller {
                // Use the team color of the controller currently manipulating this selection
                let mut team = 0;
                for val in selections {
                    let &(controller_selection, i) = val;
                    if check_i == i {
                        team = controller_selection.team;
                    }
                }
                graphics::get_team_color4(team)
            } else {
                [0.5, 0.5, 0.5, 1.0]
            };
            let name = match selection.ui {
                PlayerSelectUi::CpuAi        (_) => "CPU AI".to_string(),
                PlayerSelectUi::CpuFighter   (_) => "CPU Fighter".to_string(),
                PlayerSelectUi::HumanFighter (_) => format!("Port #{}", controller_i+1),
                PlayerSelectUi::HumanTeam    (_) => format!("Port #{} Team", controller_i+1),
                PlayerSelectUi::CpuTeam      (_) => "CPU Team".to_string(),
                PlayerSelectUi::HumanUnplugged   => unreachable!()
            };
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(name.as_ref())
                        .with_color(color)
                        .with_scale(size)
                ),
                screen_position: (x, y),
                .. Section::default()
            });
        }

        // render UI
        let mut options = vec!();
        match selection.ui {
            PlayerSelectUi::HumanFighter (_) => {
                options.extend(fighters.iter().map(|x| x.1.name.clone()));
                options.push(String::from("Change Team"));
                options.push(String::from("Add CPU"));
            }
            PlayerSelectUi::CpuFighter (_) => {
                options.extend(fighters.iter().map(|x| x.1.name.clone()));
                options.push(String::from("Change Team"));
                options.push(String::from("Change AI"));
                options.push(String::from("Remove CPU"));
            }
            PlayerSelectUi::HumanTeam (_) => {
                options.extend(graphics::get_colors().iter().map(|x| x.name.clone()));
                options.push(String::from("Return"));
            }
            PlayerSelectUi::CpuTeam (_) => {
                options.extend(graphics::get_colors().iter().map(|x| x.name.clone()));
                options.push(String::from("Return"));
            }
            PlayerSelectUi::CpuAi (_) => {
                options.push(String::from("Return"));
            }
            PlayerSelectUi::HumanUnplugged => unreachable!()
        }

        for (option_i, option) in options.iter().enumerate() {
            let x_offset = if option_i == selection.ui.ticker_unwrap().cursor { 0.1 } else { 0.0 };
            let x = ((start_x+1.0 + x_offset) / 2.0) * self.width  as f32;
            let y = ((start_y+1.0           ) / 2.0) * self.height as f32 + (option_i+1) as f32 * 40.0;

            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let mut color = [1.0, 1.0, 1.0, 1.0];
            match selection.ui {
                PlayerSelectUi::HumanFighter (_) |
                PlayerSelectUi::CpuFighter (_) => {
                    if let Some(selected_option_i) = selection.fighter {
                        if selected_option_i == option_i {
                            color = graphics::get_team_color4(selection.team);
                        }
                    }
                }
                PlayerSelectUi::HumanTeam (_) |
                PlayerSelectUi::CpuTeam (_) => {
                    if option_i < graphics::get_colors().len() {
                        color = graphics::get_team_color4(option_i);
                    }
                }
                _ => { }
            }
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(option.as_ref())
                        .with_color(color)
                        .with_scale(size)
                ),
                screen_position: (x, y),
                .. Section::default()
            });
        }

        // render fighter
        if let Some(selection_i) = selection.fighter {
            let fighter = fighters[selection_i].1;

            let camera_dimension = 40.0;
            let fighter_x_base  = start_x + (end_x - start_x) / 2.0;
            let fighter_y_base = end_y * -1.0 + 0.05;

            let fighter_x_ar = if self.aspect_ratio() > 1.0 {
                1.0
            } else {
                self.aspect_ratio()
            };
            let fighter_y_ar = if self.aspect_ratio() > 1.0 {
                1.0 / self.aspect_ratio()
            } else {
                1.0
            };

            let fighter_x = fighter_x_base * camera_dimension * fighter_x_ar;
            let fighter_y = fighter_y_base * camera_dimension * fighter_y_ar;
            let face_right = start_x < 0.0;

            //let zoom = fighter.css_scale; // TODO
            let dir      = Matrix4::from_angle_y(if face_right { Rad::turn_div_4() } else { -Rad::turn_div_4() });
            let position = Matrix4::from_translation(Vector3::new(fighter_x, fighter_y, 0.0));
            let transformation = position * dir;
            let camera = Camera::new_for_menu(self.aspect_ratio(), self.width as f32, self.height as f32, camera_dimension);

            if let Some(model) = self.models.get(&fighter.name) {
                // TODO
                //let action: &str = PlayerAction::from_u64(fighter.css_action)
                //    .map(|x| x.into())
                //    .unwrap_or("Idle");
                let action = "Idle";
                let frame = selection.animation_frame as f32;
                draws.extend(self.render_model3d(&camera, model, &transformation, action, frame, frame));
            }
        }

        draws
    }

    fn draw_stage_selector(&mut self, selection: usize) -> Vec<Draw> {
        let mut draws = vec!();
        self.glyph_brush.queue(Section {
            text: vec!(
                Text::new("Select Stage")
                    .with_color([1.0, 1.0, 1.0, 1.0])
                    .with_scale(50.0)
            ),
            screen_position: (100.0, 4.0),
            .. Section::default()
        });
        let stages = &self.package.as_ref().unwrap().stages;
        for (stage_i, stage) in stages.key_value_iter().enumerate() {
            let (stage_key, stage) = stage;
            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let x_offset = if stage_i == selection { 0.05 } else { 0.0 };
            let x = self.width as f32 * (0.1 + x_offset);
            let y = self.height as f32 * 0.1 + stage_i as f32 * 50.0;
            self.glyph_brush.queue(Section {
                text: vec!(
                    Text::new(stage.name.as_ref())
                        .with_color([1.0, 1.0, 1.0, 1.0])
                        .with_scale(size)
                ),
                screen_position: (x, y),
                .. Section::default()
            });

            if stage_i == selection {
                let zoom_divider = 100.0;
                let zoom = 1.0 / zoom_divider;
                let y = -0.2 * zoom_divider;

                let camera   = Matrix4::from_nonuniform_scale(zoom, zoom * self.aspect_ratio(), 1.0);
                let position = Matrix4::from_translation(Vector3::new(1.0, y, 0.0));
                let transformation = camera * position;
                let uniform = TransformUniform { transform: transformation.into() };

                let stage = &self.package.as_ref().unwrap().stages[stage_key.as_str()];

                if let Some(buffers) = Buffers::new_surfaces(&self.device, &stage.surfaces) {
                    draws.push(Draw {
                        ty: DrawType::Color {
                            uniform,
                            debug: true,
                            dimension3: false,
                        },
                        buffers
                    });
                }

                if let Some(buffers) = Buffers::new_surfaces_fill(&self.device, &stage.surfaces) {
                    draws.push(Draw {
                        ty: DrawType::Color {
                            uniform,
                            debug: true,
                            dimension3: false,
                        },
                        buffers
                    });
                }
            }
        }

        draws
    }

    fn draw_player_result(&mut self, result: &PlayerResult, start_x: f32) {
        let fighter_name = self.package.as_ref().unwrap().entities[result.fighter.as_ref()].name.as_str();
        let color = graphics::get_team_color4(result.team);
        let x = (start_x + 0.05) * self.width as f32;
        let y = 30.0;
        self.glyph_brush.queue(Section {
            text: vec!(
                Text::new((result.place + 1).to_string().as_ref())
                    .with_color(color)
                    .with_scale(100.0),
                Text::new(format!("

{}
Kills: {}
Deaths: {}
L-Cancel Success: {}%",
                        fighter_name,
                        result.kills.len(),
                        result.deaths.len(),
                        result.lcancel_percent
                ).as_str())
                    .with_color(color)
                    .with_scale(30.0)
            ),
            screen_position: (x, y),
            .. Section::default()
        });
    }

    fn aspect_ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

struct WindowSizeDependent {
    multisampled_framebuffer: TextureView,
    depth_stencil:            TextureView,
}

impl WindowSizeDependent {
    /// This method is called once during initialization, then again whenever the window is resized
    fn new(device: &Device, surface: &Surface, width: u32, height: u32) -> WindowSizeDependent {
        surface.configure(
            device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8Unorm,
                present_mode: wgpu::PresentMode::Mailbox,
                width,
                height,
            },
        );

        let multisampled_frame_descriptor = &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        };
        let multisampled_framebuffer = device.create_texture(multisampled_frame_descriptor).create_view(&wgpu::TextureViewDescriptor::default());

        let depth_stencil_descriptor = &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        };
        let depth_stencil = device.create_texture(depth_stencil_descriptor).create_view(&wgpu::TextureViewDescriptor::default());

        WindowSizeDependent {
            multisampled_framebuffer,
            depth_stencil,
        }
    }
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct HitboxUniform {
    edge_color: [f32; 4],
    color:      [f32; 4],
    transform:  [[f32; 4]; 4],
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct TransformUniform {
    transform: [[f32; 4]; 4],
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct TransformUniformCycle {
    transform:   [[f32; 4]; 4],
    frame_count: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct AnimatedUniform {
    transform: [[f32; 4]; 4],
    joint_transforms: JointTransforms,
    frame_count: f32,
}
type JointTransforms = [[[f32; 4]; 4]; 500];

unsafe impl Pod for AnimatedUniform {}
unsafe impl Zeroable for AnimatedUniform {}

struct Draw {
    ty:      DrawType,
    buffers: Rc<Buffers>,
}

enum DrawType {
    Color         { uniform: TransformUniform, debug: bool, dimension3: bool },
    Hitbox        { uniform: HitboxUniform },
    ModelAnimated { uniform: AnimatedUniform,  texture: Rc<Texture> },
    Fireball      { uniform: AnimatedUniform,  texture: Rc<Texture> },
    ModelStatic   { uniform: TransformUniform, texture: Rc<Texture> },
    Lava          { uniform: TransformUniformCycle, texture: Rc<Texture> },
}

impl DrawType {
    fn uniform_bytes(&self) -> &[u8] {
        match &self {
            DrawType::Color         { uniform, .. } => bytemuck::bytes_of(uniform),
            DrawType::Hitbox        { uniform, .. } => bytemuck::bytes_of(uniform),
            DrawType::ModelStatic   { uniform, .. } => bytemuck::bytes_of(uniform),
            DrawType::ModelAnimated { uniform, .. } => bytemuck::bytes_of(uniform),
            DrawType::Fireball      { uniform, .. } => bytemuck::bytes_of(uniform),
            DrawType::Lava          { uniform, .. } => bytemuck::bytes_of(uniform),
        }
    }

    fn uniform_size(&self) -> usize {
        match &self {
            DrawType::Color         { .. } => mem::size_of::<TransformUniform>(),
            DrawType::Hitbox        { .. } => mem::size_of::<HitboxUniform>(),
            DrawType::ModelAnimated { .. } => mem::size_of::<AnimatedUniform>(),
            DrawType::Fireball      { .. } => mem::size_of::<AnimatedUniform>(),
            DrawType::ModelStatic   { .. } => mem::size_of::<TransformUniform>(),
            DrawType::Lava          { .. } => mem::size_of::<TransformUniformCycle>(),
        }
    }

    fn uniform_size_padded(&self) -> usize {
        let unpadded_size = self.uniform_size();
        let align = 256;
        let padding = (align - unpadded_size % align) % align;
        unpadded_size + padding
    }
}

struct StagingBelt {
    pub staging_belt:  wgpu::util::StagingBelt,
        local_pool:    futures::executor::LocalPool,
        local_spawner: futures::executor::LocalSpawner,
}

impl StagingBelt {
    pub fn new() -> Self {
        let staging_belt = wgpu::util::StagingBelt::new(1024);
        let local_pool = futures::executor::LocalPool::new();
        let local_spawner = local_pool.spawner();
        StagingBelt { staging_belt, local_pool, local_spawner }
    }

    pub fn finish(&mut self) {
        self.staging_belt.finish();
    }

    /// Recall unused staging buffers
    pub fn recall(&mut self) {
        use futures::task::SpawnExt;

        self.local_spawner
            .spawn(self.staging_belt.recall())
            .expect("Recall staging belt");

        self.local_pool.run_until_stalled();
    }
}

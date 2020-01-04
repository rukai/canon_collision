mod buffers;
mod model3d;
mod animation;

use buffers::{ColorVertex, ColorBuffers, Vertex, Buffers};
use model3d::{Models, Model3D, ModelVertexType, ModelVertexAnimated, ModelVertexStatic, Joint};
use crate::game::{GameState, RenderEntity, RenderGame};
use crate::graphics::{self, GraphicsMessage, Render, RenderType};
use crate::menu::{RenderMenu, RenderMenuState, PlayerSelect, PlayerSelectUi};
use crate::particle::ParticleType;
use crate::player::{RenderPlayer, RenderPlayerFrame, DebugPlayer};
use crate::results::PlayerResult;
use canon_collision_lib::fighter::{Action, ECB, CollisionBoxRole, ActionFrame};
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::package::{Package, PackageUpdate};

use std::collections::HashSet;
use std::sync::mpsc::{Sender, Receiver};
use std::time::{Duration, Instant};
use std::{mem, f32};

use cgmath::Rad;
use cgmath::prelude::*;
use cgmath::{Matrix4, Vector3};
use num_traits::{FromPrimitive, ToPrimitive};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use wgpu::{Device, Queue, Surface, SwapChain, BindGroup, BindGroupLayout, RenderPipeline, RenderPass, TextureView, Sampler};
use wgpu_glyph::{Section, GlyphBrush, GlyphBrushBuilder, FontId, Scale as GlyphScale};

use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;
use winit::event::{Event, WindowEvent};
use winit::window::Fullscreen;

pub struct WgpuGraphics {
    package:                   Option<Package>,
    models:                    Models,
    glyph_brush:               GlyphBrush<'static, ()>,
    hack_font_id:              FontId,
    window:                    Window,
    os_input_tx:               Sender<Event<()>>,
    render_rx:                 Receiver<GraphicsMessage>,
    device:                    Device,
    queue:                     Queue,
    surface:                   Surface,
    wsd:                       WindowSizeDependent,
    pipeline_color:            RenderPipeline,
    pipeline_hitbox:           RenderPipeline,
    pipeline_debug:            RenderPipeline,
    pipeline_model3d_static:   RenderPipeline,
    pipeline_model3d_animated: RenderPipeline,
    bind_group_layout_generic: BindGroupLayout,
    bind_group_layout_model3d: BindGroupLayout,
    sampler:                   Sampler,
    prev_fullscreen:           Option<bool>,
    frame_durations:           Vec<Duration>,
    fps:                       String,
    width:                     u32,
    height:                    u32,
}

const SAMPLE_COUNT: u32 = 4;

impl WgpuGraphics {
    pub fn new(event_loop: &EventLoop<()>, os_input_tx: Sender<Event<()>>, render_rx: Receiver<GraphicsMessage>) -> WgpuGraphics {
        let window = Window::new(&event_loop).unwrap();
        window.set_title("Canon Collision");

        let size = window
            .inner_size()
            .to_physical(window.hidpi_factor());

        let surface = wgpu::Surface::create(&window);

        let adapter = wgpu::Adapter::request(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            backends: wgpu::BackendBit::PRIMARY,
        }).unwrap();

        let (mut device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            limits: wgpu::Limits::default(),
            extensions: wgpu::Extensions {
                anisotropic_filtering: false,
            },
        });

        let color_vs = vk_shader_macros::include_glsl!("src/shaders/color-vertex.glsl", kind: vert);
        let color_vs_module = device.create_shader_module(color_vs);

        let color_fs = vk_shader_macros::include_glsl!("src/shaders/color-fragment.glsl", kind: frag);
        let color_fs_module = device.create_shader_module(color_fs);

        let bind_group_layout_generic = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                bindings: &[
                    wgpu::BindGroupLayoutBinding {
                        binding: 0,
                        visibility: wgpu::ShaderStage::all(),
                        ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                    },
                ]
            }
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout_generic],
        });

        let rasterization_state = Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: wgpu::CullMode::None,
            depth_bias: 0,
            depth_bias_slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        });

        let color_states = [wgpu::ColorStateDescriptor {
            format: wgpu::TextureFormat::Bgra8Unorm,
            color_blend: wgpu::BlendDescriptor {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha_blend: wgpu::BlendDescriptor {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            write_mask: wgpu::ColorWrite::ALL,
        }];
        let depth_stencil_state = Some(wgpu::DepthStencilStateDescriptor {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_read_mask: 0,
            stencil_write_mask: 0,
        });

        let pipeline_color = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &color_vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &color_fs_module,
                entry_point: "main",
            }),
            rasterization_state: rasterization_state.clone(),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &color_states,
            depth_stencil_state: depth_stencil_state.clone(),
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<ColorVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 4 * 4,
                        shader_location: 1,
                    },
                ],
            }],
            sample_count: SAMPLE_COUNT,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let pipeline_debug = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &color_vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &color_fs_module,
                entry_point: "main",
            }),
            rasterization_state: rasterization_state.clone(),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &color_states,
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_read_mask: 0,
                stencil_write_mask: 0,
            }),
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<ColorVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 4 * 4,
                        shader_location: 1,
                    },
                ],
            }],
            sample_count: SAMPLE_COUNT,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let hitbox_vs = vk_shader_macros::include_glsl!("src/shaders/hitbox-vertex.glsl", kind: vert);
        let hitbox_vs_module = device.create_shader_module(hitbox_vs);

        let hitbox_fs = vk_shader_macros::include_glsl!("src/shaders/hitbox-fragment.glsl", kind: frag);
        let hitbox_fs_module = device.create_shader_module(hitbox_fs);

        let pipeline_hitbox = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &hitbox_vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &hitbox_fs_module,
                entry_point: "main",
            }),
            rasterization_state: rasterization_state.clone(),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &color_states,
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_read_mask: 0,
                stencil_write_mask: 0,
            }),
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float,
                        offset: 8,
                        shader_location: 1,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Uint,
                        offset: 12,
                        shader_location: 2,
                    },
                ],
            }],
            sample_count: SAMPLE_COUNT,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let model3d_fs = vk_shader_macros::include_glsl!("src/shaders/model3d-fragment.glsl", kind: frag);
        let model3d_fs_module = device.create_shader_module(model3d_fs);

        let model3d_static_vs = vk_shader_macros::include_glsl!("src/shaders/model3d-static-vertex.glsl", kind: vert);
        let model3d_static_vs_module = device.create_shader_module(model3d_static_vs);

        let model3d_animated_vs = vk_shader_macros::include_glsl!("src/shaders/model3d-animated-vertex.glsl", kind: vert);
        let model3d_animated_vs_module = device.create_shader_module(model3d_animated_vs);

        let bind_group_layout_model3d = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                bindings: &[
                    wgpu::BindGroupLayoutBinding {
                        binding: 0,
                        visibility: wgpu::ShaderStage::all(),
                        ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                    },
                    wgpu::BindGroupLayoutBinding {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::SampledTexture {
                            multisampled: false,
                            dimension: wgpu::TextureViewDimension::D2,
                        },
                    },
                    wgpu::BindGroupLayoutBinding {
                        binding: 2,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler,
                    },
                ]
            }
        );
        let pipeline_model3d_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout_model3d],
        });

        let pipeline_model3d_static = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_model3d_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &model3d_static_vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &model3d_fs_module,
                entry_point: "main",
            }),
            rasterization_state: rasterization_state.clone(),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &color_states,
            depth_stencil_state: depth_stencil_state.clone(),
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<ModelVertexStatic>() as wgpu::BufferAddress,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    // position
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 0,
                        shader_location: 0,
                    },
                    // uv
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 4 * 4,
                        shader_location: 1,
                    },
                ],
            }],
            sample_count: SAMPLE_COUNT,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let pipeline_model3d_animated = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_model3d_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &model3d_animated_vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &model3d_fs_module,
                entry_point: "main",
            }),
            rasterization_state: rasterization_state.clone(),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &color_states,
            depth_stencil_state: depth_stencil_state.clone(),
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<ModelVertexAnimated>() as wgpu::BufferAddress,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    // position
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 0,
                        shader_location: 0,
                    },
                    // uv
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 4 * 4,
                        shader_location: 1,
                    },
                    // joints
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Uint4,
                        offset: 4 * 4 + 4 * 2,
                        shader_location: 2,
                    },
                    // weights
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float4,
                        offset: 4 * 4 + 4 * 2 + 4 * 4,
                        shader_location: 3,
                    },
                ],
            }],
            sample_count: SAMPLE_COUNT,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            compare_function: wgpu::CompareFunction::Always,
        });

        let dejavu: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
        let hack: &[u8] = include_bytes!("../fonts/Hack-Regular.ttf");

        let mut glyph_brush_builder = GlyphBrushBuilder::using_font_bytes(dejavu);
        let hack_font_id = glyph_brush_builder.add_font_bytes(hack);
        let glyph_brush = glyph_brush_builder
            .initial_cache_size((512, 512))
            .build(&mut device, wgpu::TextureFormat::Bgra8Unorm);

        let width = size.width.round() as u32;
        let height = size.height.round() as u32;
        let wsd = WindowSizeDependent::new(&device, &surface, width, height);

        let models = Models::new();

        WgpuGraphics {
            package: None,
            models,
            glyph_brush,
            hack_font_id,
            window,
            os_input_tx,
            render_rx,
            surface,
            device,
            queue,
            wsd,
            pipeline_color,
            pipeline_hitbox,
            pipeline_debug,
            pipeline_model3d_static,
            pipeline_model3d_animated,
            bind_group_layout_generic,
            bind_group_layout_model3d,
            sampler,
            prev_fullscreen: None,
            frame_durations: vec!(),
            fps: "".into(),
            width,
            height,
        }
    }

    pub fn update(&mut self, event: Event<()>, control_flow: &mut ControlFlow) {
        match event {
            Event::MainEventsCleared => {
                let frame_start = Instant::now();

                if self.force_send_window_events() {
                    *control_flow = ControlFlow::Exit;
                }

                // get the most recent render
                let mut render = if let Ok(message) = self.render_rx.recv() {
                    self.read_message(message)
                } else {
                    *control_flow = ControlFlow::Exit;
                    return;
                };
                while let Ok(message) = self.render_rx.try_recv() {
                    render = self.read_message(message);
                }

                let resolution: (u32, u32) = self.window.inner_size()
                    .to_physical(self.window.hidpi_factor())
                    .into();
                self.window_resize(resolution.0, resolution.1);

                self.render(render);
                self.frame_durations.push(frame_start.elapsed());
            }
            Event::NewEvents (_) => { }
            _ => {
                self.os_input_tx.send(event).ok();
            }
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
                        package.fighters[fighter.as_ref()].actions[action].frames.remove(frame_index);
                    }
                }
                PackageUpdate::InsertFighterFrame { fighter, action, frame_index, frame } => {
                    if let &mut Some(ref mut package) = &mut self.package {
                        package.fighters[fighter.as_ref()].actions[action].frames.insert(frame_index, frame);
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

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });

        if let RenderType::Game(render) = &render.render_type {
            self.models.load(&self.device, &mut encoder, render);
        }

        let mut swap_chain = self.wsd.swap_chain.take().unwrap();
        {
            let frame = swap_chain.get_next_texture();

            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &self.wsd.multisampled_framebuffer,
                        resolve_target: Some(&frame.view),
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::BLACK,
                    }],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                        attachment: &self.wsd.depth_stencil,
                        depth_load_op: wgpu::LoadOp::Clear,
                        depth_store_op: wgpu::StoreOp::Store,
                        stencil_load_op: wgpu::LoadOp::Clear,
                        stencil_store_op: wgpu::StoreOp::Store,
                        clear_depth: 1.0,
                        clear_stencil: 0,
                    }),
                });

                match render.render_type {
                    RenderType::Game(game) => self.game_render(game, &mut rpass, &render.command_output),
                    RenderType::Menu(menu) => self.menu_render(menu, &mut rpass, &render.command_output)
                }
            }

            self.glyph_brush.draw_queued(&mut self.device, &mut encoder, &frame.view, self.width, self.height).unwrap();

            self.queue.submit(&[encoder.finish()]);
        }
        self.wsd.swap_chain = Some(swap_chain);
    }

    fn command_render(&mut self, lines: &[String]) {
        // TODO: Render white text, with black background
        for (i, line) in lines.iter().enumerate() {
            self.glyph_brush.queue(Section {
                text: line,
                color: [1.0, 1.0, 0.0, 1.0],
                screen_position: (0.0, self.height as f32 - 25.0 - 20.0 * i as f32),
                scale: GlyphScale::uniform(20.0),
                font_id: self.hack_font_id,
                .. Section::default()
            });
        }
    }

    fn game_timer_render(&mut self, timer: &Option<Duration>) {
        if let &Some(ref timer) = timer {
            let minutes = timer.as_secs() / 60;
            let seconds = timer.as_secs() % 60;
            self.glyph_brush.queue(Section {
                text: format!("{:02}:{:02}", minutes, seconds).as_ref(),
                color: [1.0, 1.0, 1.0, 1.0],
                screen_position: ((self.width / 2) as f32 - 50.0, 4.0),
                scale: GlyphScale::uniform(40.0),
                .. Section::default()
            });
        }
    }

    fn game_hud_render(&mut self, entities: &[RenderEntity]) {
        let mut players = 0;
        for entity in entities {
            if let &RenderEntity::Player(_) = entity {
                players += 1;
            }
        }
        let distance = (self.width / (players + 1)) as f32;

        let mut location = -100.0;
        for entity in entities {
            if let &RenderEntity::Player(ref player) = entity {
                location += distance;
                match Action::from_u64(player.frames[0].action as u64) {
                    Some(Action::Eliminated) => { }
                    _ => {
                        let c = player.fighter_color.clone();
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
                                text: stocks_string.as_ref(),
                                color,
                                screen_position: (location + 10.0, self.height as f32 - 130.0),
                                scale: GlyphScale::uniform(22.0),
                                .. Section::default()
                            });
                        }

                        self.glyph_brush.queue(Section {
                            text: format!("{}%", player.damage).as_ref(),
                            color,
                            screen_position: (location, self.height as f32 - 117.0),
                            scale: GlyphScale::uniform(110.0),
                            .. Section::default()
                        });
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
            text: &self.fps,
            color: [1.0, 1.0, 1.0, 1.0],
            screen_position: (self.width as f32 - 30.0, 4.0),
            scale: GlyphScale::uniform(20.0),
            .. Section::default()
        });
    }

    fn debug_lines_render(&mut self, lines: &[String]) {
        if lines.len() > 1 {
            for (i, line) in lines.iter().enumerate() {
                self.glyph_brush.queue(Section {
                    text: line,
                    color: [1.0, 1.0, 0.0, 1.0],
                    screen_position: (0.0, 12.0 + 20.0 * i as f32),
                    scale: GlyphScale::uniform(20.0),
                    font_id: self.hack_font_id,
                    .. Section::default()
                });
            }
        }
    }

    fn render_hitbox_buffers(
        &self,
        rpass:      &mut RenderPass,
        render:     &RenderGame,
        buffers:    Buffers,
        entity:     &Matrix4<f32>,
        edge_color: [f32; 4],
        color:      [f32; 4]
    ) {
        let camera = render.camera.transform();
        let transformation = camera * entity;
        let uniform = Uniform {
            edge_color,
            color,
            transform: transformation.into(),
        };

        let uniform_buffer = self.device.create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM)
            .fill_from_slice(&[uniform]);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.bind_group_layout_generic,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &uniform_buffer,
                        range: 0..mem::size_of::<Uniform>() as wgpu::BufferAddress,
                    }
                },
            ]
        });

        rpass.set_pipeline(&self.pipeline_hitbox);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.set_index_buffer(&buffers.index, 0);
        rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
        rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
    }

    fn flatten_joint_transforms(joint: &Joint, buffer: &mut [[[f32; 4]; 4]; 500]) {
        buffer[joint.index] = joint.transform.into();

        for child in &joint.children {
            WgpuGraphics::flatten_joint_transforms(child, buffer);
        }
    }

    fn render_model3d(
        &self,
        rpass:           &mut RenderPass,
        render:          &RenderGame,
        model:           &Model3D,
        entity:          &Matrix4<f32>,
        animation_name:  &str,
        animation_frame: f32,
    ) {
        let camera = render.camera.transform();

        for mesh in &model.meshes {
            let transformation = camera * entity * mesh.transform;

            let transform_uniform = TransformUniform { transform: transformation.into() };
            let transform_uniform = self.device.create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM)
                .fill_from_slice(&[transform_uniform]);

            let mut joint_transforms = [Matrix4::identity().into(); 500];
            if let Some(mut root_joint) = mesh.root_joint.clone() {
                if let Some(animation) = model.animations.get(animation_name) {
                    animation::set_animated_joints(animation, animation_frame, &mut root_joint, Matrix4::identity());
                }
                WgpuGraphics::flatten_joint_transforms(&root_joint, &mut joint_transforms);
            }
            let animated_uniform = {
                let transform = (camera * entity).into();
                let uniform = AnimatedUniform { transform, joint_transforms };

                self.device.create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM).fill_from_slice(&[uniform])
            };

            for primitive in &mesh.primitives {
                if let Some(texture) = primitive.texture.and_then(|x| model.textures.get(x)) {
                    match primitive.vertex_type {
                        ModelVertexType::Animated => rpass.set_pipeline(&self.pipeline_model3d_animated),
                        ModelVertexType::Static   => rpass.set_pipeline(&self.pipeline_model3d_static),
                    }

                    let bind_group = match primitive.vertex_type {
                        ModelVertexType::Animated =>
                            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                layout: &self.bind_group_layout_model3d,
                                bindings: &[
                                    wgpu::Binding {
                                        binding: 0,
                                        resource: wgpu::BindingResource::Buffer {
                                            buffer: &animated_uniform,
                                            range: 0..mem::size_of::<AnimatedUniform>() as u64,
                                        }
                                    },
                                    wgpu::Binding {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(&texture.create_default_view()),
                                    },
                                    wgpu::Binding {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                                    },
                                ]
                            }),
                        ModelVertexType::Static =>
                            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                layout: &self.bind_group_layout_model3d,
                                bindings: &[
                                    wgpu::Binding {
                                        binding: 0,
                                        resource: wgpu::BindingResource::Buffer {
                                            buffer: &transform_uniform,
                                            range: 0..mem::size_of::<TransformUniform>() as u64,
                                        }
                                    },
                                    wgpu::Binding {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(&texture.create_default_view()),
                                    },
                                    wgpu::Binding {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                                    },
                                ]
                            }),
                    };

                    rpass.set_bind_group(0, &bind_group, &[]);
                    rpass.set_index_buffer(&primitive.index, 0);
                    rpass.set_vertex_buffers(0, &[(&primitive.vertex, 0)]);
                    rpass.draw_indexed(0 .. primitive.index_count, 0, 0 .. 1);
                } else {
                    error!("Models without textures are not rendered");
                }
            }
        }
    }

    fn render_color_buffers(
        &self,
        rpass:   &mut RenderPass,
        render:  &RenderGame,
        buffers: ColorBuffers,
        entity:  &Matrix4<f32>,
    ) {
        let camera = render.camera.transform();
        let transformation = camera * entity;
        let uniform = TransformUniform { transform: transformation.into() };

        let uniform_buffer = self.device.create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM)
            .fill_from_slice(&[uniform]);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.bind_group_layout_generic,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &uniform_buffer,
                    range: 0..mem::size_of::<TransformUniform>() as u64,
                }
            }]
        });

        rpass.set_pipeline(&self.pipeline_color);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.set_index_buffer(&buffers.index, 0);
        rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
        rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
    }

    fn render_debug_buffers(
        &self,
        rpass:   &mut RenderPass,
        render:  &RenderGame,
        buffers: ColorBuffers,
        entity:  &Matrix4<f32>,
    ) {
        let camera = render.camera.transform();
        let transformation = camera * entity;
        let uniform = TransformUniform { transform: transformation.into() };

        let uniform_buffer = self.device.create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM)
            .fill_from_slice(&[uniform]);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.bind_group_layout_generic,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &uniform_buffer,
                    range: 0..mem::size_of::<TransformUniform>() as u64,
                }
            }]
        });

        rpass.set_pipeline(&self.pipeline_debug);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.set_index_buffer(&buffers.index, 0);
        rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
        rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
    }

    fn new_bind_group<T>(&self, uniform: T) -> BindGroup where T: 'static + Copy {
        let uniform_buffer = self.device.create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM)
            .fill_from_slice(&[uniform]);

        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.bind_group_layout_generic,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &uniform_buffer,
                    range: 0..mem::size_of::<T>() as wgpu::BufferAddress,
                }
            }]
        })
    }

    fn game_render(&mut self, render: RenderGame, rpass: &mut RenderPass, command_output: &[String]) {
        let mut rng = StdRng::from_seed(render.seed);
        if command_output.len() == 0 {
            self.game_hud_render(&render.entities);
            self.game_timer_render(&render.timer);
            self.debug_lines_render(&render.debug_lines);
            self.fps_render();
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
                self.render_model3d(rpass, &render, &stage, &stage_transformation, "NONE", 0.0);
            }
        }

        if render.render_stage_mode.debug() {
            if let Some(buffers) = ColorBuffers::new_surfaces(&self.device, &render.surfaces) {
                self.render_debug_buffers(rpass, &render, buffers, &stage_transformation);
            }

            if let Some(buffers) = ColorBuffers::new_surfaces_fill(&self.device, &render.surfaces) {
                self.render_debug_buffers(rpass, &render, buffers, &stage_transformation);
            }

            if let Some(buffers) = ColorBuffers::new_selected_surfaces(&self.device, &render.surfaces, &render.selected_surfaces) {
                self.render_debug_buffers(rpass, &render, buffers, &stage_transformation);
            }
        }

        for entity in render.entities.iter() {
            match entity {
                &RenderEntity::Player(ref player) => {
                    fn player_matrix(frame: &RenderPlayerFrame) -> Matrix4<f32> {
                        let dir      = Matrix4::from_nonuniform_scale(if frame.face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                        let rotate   = Matrix4::from_angle_z(Rad(frame.angle));
                        let position = Matrix4::from_translation(Vector3::new(frame.bps.0, frame.bps.1, 0.0));
                        position * rotate * dir
                    }

                    let transformation = player_matrix(&player.frames[0]);

                    // draw fighter
                    let action_index = player.frames[0].action;
                    match Action::from_u64(action_index as u64) {
                        Some(Action::Eliminated) => { }
                        _ => {
                            let fighter_model_name = player.frames[0].model_name.replace(" ", "");
                            if player.debug.fighter.normal() {
                                let dir      = Matrix4::from_angle_y(if player.frames[0].face_right { Rad::turn_div_4() } else { -Rad::turn_div_4() });
                                let rotate   = Matrix4::from_angle_z(Rad(player.frames[0].angle));
                                let position = Matrix4::from_translation(Vector3::new(player.frames[0].bps.0, player.frames[0].bps.1, 0.0));
                                let transformation = position * rotate * dir;
                                if let Some(fighter) = self.models.get(&fighter_model_name) {
                                    let action = Action::action_index_to_string(action_index);
                                    self.render_model3d(rpass, &render, &fighter, &transformation, &action, player.frames[0].frame as f32);
                                }
                            }
                        }
                    }

                    // draw player ecb
                    if player.debug.ecb {
                        // TODO: Set individual corner vertex colours to show which points of the ecb are selected
                        let buffers = ColorBuffers::new_ecb(&self.device, &player);
                        let dir      = Matrix4::from_nonuniform_scale(if player.frames[0].face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                        let position = Matrix4::from_translation(Vector3::new(player.frames[0].bps.0, player.frames[0].bps.1, 0.0));
                        let transformation = position * dir;

                        self.render_debug_buffers(rpass, &render, buffers, &transformation);
                    }

                    // draw fighter debug overlay
                    if player.debug.fighter.debug() {
                        if player.debug.fighter.onion_skin() {
                            if let Some(frame) = player.frames.get(2) {
                                if let Some(buffers) = Buffers::new_fighter_frame(&self.device, &self.package.as_ref().unwrap(), &frame.fighter, frame.action, frame.frame) {
                                    let transformation = player_matrix(frame);
                                    let onion_color = [0.4, 0.4, 0.4, 0.4];
                                    self.render_hitbox_buffers(rpass, &render, buffers.clone(), &transformation, onion_color, onion_color);
                                }
                            }

                            if let Some(frame) = player.frames.get(1) {
                                if let Some(buffers) = Buffers::new_fighter_frame(&self.device, &self.package.as_ref().unwrap(), &frame.fighter, frame.action, frame.frame) {
                                    let transformation = player_matrix(frame);
                                    let onion_color = [0.80, 0.80, 0.80, 0.9];
                                    self.render_hitbox_buffers(rpass, &render, buffers.clone(), &transformation, onion_color, onion_color);
                                }
                            }
                        }

                        // draw fighter
                        if let Some(buffers) = Buffers::new_fighter_frame(&self.device, &self.package.as_ref().unwrap(), &player.frames[0].fighter, player.frames[0].action, player.frames[0].frame) {
                            let color = [0.9, 0.9, 0.9, 1.0];
                            let edge_color = if player.fighter_selected {
                                [0.0, 1.0, 0.0, 1.0]
                            } else {
                                let c = player.fighter_color.clone();
                                [c[0], c[1], c[2], 1.0]
                            };
                            self.render_hitbox_buffers(rpass, &render, buffers.clone(), &transformation, edge_color, color);
                        }
                        else {
                             // TODO: Give some indication that we are rendering a deleted or otherwise nonexistent frame
                        }
                    }

                    // draw selected colboxes
                    if player.selected_colboxes.len() > 0 {
                        let color = [0.0, 1.0, 0.0, 1.0];
                        let buffers = Buffers::new_fighter_frame_colboxes(&self.device, &self.package.as_ref().unwrap(), &player.frames[0].fighter, player.frames[0].action, player.frames[0].frame, &player.selected_colboxes);
                        self.render_hitbox_buffers(rpass, &render, buffers, &transformation, color, color);
                    }

                    // draw hitbox debug arrows
                    if player.debug.hitbox_vectors {
                        let kbg_arrow = ColorBuffers::new_arrow(&self.device, [1.0,  1.0,  1.0, 1.0]);
                        let bkb_arrow = ColorBuffers::new_arrow(&self.device, [0.17, 0.17, 1.0, 1.0]);
                        for colbox in player.frame_data.colboxes.iter() {
                            if let CollisionBoxRole::Hit(ref hitbox) = colbox.role {
                                let kb_squish = 0.5;
                                let squish_kbg = Matrix4::from_nonuniform_scale(0.6, hitbox.kbg * kb_squish, 1.0);
                                let squish_bkb = Matrix4::from_nonuniform_scale(0.3, (hitbox.bkb / 100.0) * kb_squish, 1.0); // divide by 100 so the arrows are comparable if the hit fighter is on 100%
                                let rotate = Matrix4::from_angle_z(Rad(hitbox.angle.to_radians() - f32::consts::PI / 2.0));
                                let x = player.frames[0].bps.0 + colbox.point.0;
                                let y = player.frames[0].bps.1 + colbox.point.1;
                                let position = Matrix4::from_translation(Vector3::new(x, y, 0.0));
                                let transformation_bkb = position * rotate * squish_bkb;
                                let transformation_kbg = position * rotate * squish_kbg;
                                self.render_debug_buffers(rpass, &render, kbg_arrow.clone(), &transformation_kbg);
                                self.render_debug_buffers(rpass, &render, bkb_arrow.clone(), &transformation_bkb);
                            }
                        }
                    }

                    // draw debug vector arrows
                    let num_arrows = player.vector_arrows.len() as f32;
                    for (i, arrow) in player.vector_arrows.iter().enumerate() {
                        let arrow_buffers = ColorBuffers::new_arrow(&self.device, arrow.color.clone());
                        let squish = Matrix4::from_nonuniform_scale((num_arrows - i as f32) / num_arrows, 1.0, 1.0); // consecutive arrows are drawn slightly thinner so we can see arrows behind
                        let rotate = Matrix4::from_angle_z(Rad(arrow.y.atan2(arrow.x) - f32::consts::PI / 2.0));
                        let position = Matrix4::from_translation(Vector3::new(player.frames[0].bps.0, player.frames[0].bps.1, 0.0));
                        let transformation = position * rotate * squish;
                        self.render_debug_buffers(rpass, &render, arrow_buffers, &transformation);
                    }

                    // draw particles
                    for particle in &player.particles {
                        let c = particle.color.clone();
                        match &particle.p_type {
                            &ParticleType::Spark { size, .. } => {
                                let rotate = Matrix4::from_angle_z(Rad(particle.angle));
                                let size = size * (1.0 - particle.counter_mult());
                                let size = Matrix4::from_nonuniform_scale(size, size, 1.0);
                                let position = Matrix4::from_translation(Vector3::new(particle.x, particle.y, 0.0));
                                let transformation = position * rotate * size;
                                let color = [c[0], c[1], c[2], 1.0];
                                if c[0] == 1.0 && c[1] == 1.0 && c[2] == 1.0 {
                                    // TODO: wireframe
                                } else {
                                    // TODO: not wireframe
                                };
                                let triangle_buffers = ColorBuffers::new_triangle(&self.device, color);
                                self.render_debug_buffers(rpass, &render, triangle_buffers, &transformation)
                            }
                            &ParticleType::AirJump => {
                                let size = Matrix4::from_nonuniform_scale(3.0 + particle.counter_mult(), 1.15 + particle.counter_mult(), 1.0);
                                let position = Matrix4::from_translation(Vector3::new(particle.x, particle.y, 0.0));
                                let transformation = position * size;
                                let color = [c[0], c[1], c[2], (1.0 - particle.counter_mult()) * 0.7];
                                let jump_buffers = ColorBuffers::new_circle(&self.device, color);
                                self.render_debug_buffers(rpass, &render, jump_buffers, &transformation)
                            }
                            &ParticleType::Hit { knockback, damage } => {
                                // needs to rendered last to ensure we dont have anything drawn on top of the inversion
                                let size = Matrix4::from_nonuniform_scale(0.2 * knockback, 0.08 * damage, 1.0);
                                let rotate = Matrix4::from_angle_z(Rad(particle.angle - f32::consts::PI / 2.0));
                                let position = Matrix4::from_translation(Vector3::new(particle.x, particle.y, 0.0));
                                let transformation = position * rotate * size;
                                let color = [0.5, 0.5, 0.5, 1.5];
                                let hit_buffers = ColorBuffers::new_circle(&self.device, color);
                                self.render_debug_buffers(rpass, &render, hit_buffers, &transformation) // TODO: Invert
                            }
                        }
                    }

                    // Draw spawn plat
                    match Action::from_u64(player.frames[0].action as u64) {
                        Some(Action::ReSpawn) | Some(Action::ReSpawnIdle) => {
                            // TODO: get width from player dimensions
                            let width = 15.0;
                            let height = width / 4.0;
                            let scale = Matrix4::from_nonuniform_scale(width, -height, 1.0); // negative y to point triangle downwards.
                            let rotate = Matrix4::from_angle_z(Rad(player.frames[0].angle));
                            let bps = &player.frames[0].bps;
                            let position = Matrix4::from_translation(Vector3::new(bps.0, bps.1, 0.0));
                            let transformation = position * rotate * scale;

                            let c = player.fighter_color.clone();
                            let color = [c[0], c[1], c[2], 1.0];
                            let triangle_buffers = ColorBuffers::new_triangle(&self.device, color);

                            self.render_debug_buffers(rpass, &render, triangle_buffers, &transformation)
                        }
                        _ => { }
                    }
                }
                &RenderEntity::RectOutline (ref render_rect) => {
                    let transformation = Matrix4::identity();
                    let buffers = ColorBuffers::rect_outline_buffers(&self.device, &render_rect);
                    self.render_debug_buffers(rpass, &render, buffers, &transformation);
                }
                &RenderEntity::SpawnPoint (ref render_point) => {
                    let buffers = ColorBuffers::new_spawn_point(&self.device, render_point.color);
                    let flip = Matrix4::from_nonuniform_scale(if render_point.face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                    let position = Matrix4::from_translation(Vector3::new(render_point.x, render_point.y, 0.0));
                    let transformation = position * flip;
                    self.render_debug_buffers(rpass, &render, buffers, &transformation)
                }
            }
        }

        // Some things need to be rendered after everything else as they are transparent
        for entity in render.entities.iter() {
            match entity {
                &RenderEntity::Player(ref player) => {
                    // draw shield
                    if let &Some(ref shield) = &player.shield {
                        let position = Matrix4::from_translation(Vector3::new(shield.pos.0, shield.pos.1, 0.0));
                        let color = if shield.distort > 0 {
                            let c = shield.color;
                            [c[0] * rng.gen_range(0.75, 1.25), c[1] * rng.gen_range(0.75, 1.25), c[2] * rng.gen_range(0.75, 1.25), c[3] * rng.gen_range(0.8, 1.2)]
                        } else {
                            shield.color
                        };
                        let buffers = ColorBuffers::new_shield(&self.device, shield, color);
                        self.render_color_buffers(rpass, &render, buffers, &position);
                    }
                }
                _ => { }
            }
        }
    }

    fn menu_render(&mut self, render: RenderMenu, rpass: &mut RenderPass, command_output: &[String]) {
        self.fps_render();
        let mut entities: Vec<MenuEntityAndBindGroup> = vec!();

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

                self.draw_back_counter(&mut entities, back_counter, back_counter_max);
                self.glyph_brush.queue(Section {
                    text: "Select Fighters",
                    color: [1.0, 1.0, 1.0, 1.0],
                    screen_position: (100.0, 4.0),
                    scale: GlyphScale::uniform(50.0),
                    .. Section::default()
                });

                match plugged_in_selections.len() {
                    0 => {
                        self.glyph_brush.queue(Section {
                            text: "There are no controllers plugged in.",
                            color: [1.0, 1.0, 1.0, 1.0],
                            screen_position: (100.0, 100.0),
                            scale: GlyphScale::uniform(30.0),
                            .. Section::default()
                        });
                    }
                    1 => {
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 0, -0.9, -0.8, 0.9, 0.9);

                    }
                    2 => {
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 0, -0.9, -0.8, 0.0, 0.9);
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 1,  0.0, -0.8, 0.9, 0.9);
                    }
                    3 => {
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 0, -0.9, -0.8, 0.0, 0.0);
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 1,  0.0, -0.8, 0.9, 0.0);
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 2, -0.9,  0.0, 0.0, 0.9);
                    }
                    4 => {
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 0, -0.9, -0.8, 0.0, 0.0);
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 1,  0.0, -0.8, 0.9, 0.0);
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 2, -0.9,  0.0, 0.0, 0.9);
                        self.draw_fighter_selector(&mut entities, &plugged_in_selections, 3,  0.0,  0.0, 0.9, 0.9);
                    }
                    _ => {
                        self.glyph_brush.queue(Section {
                            text: "Currently only supports up to 4 controllers. Please unplug some.",
                            color: [1.0, 1.0, 1.0, 1.0],
                            screen_position: (100.0, 100.0),
                            scale: GlyphScale::uniform(30.0),
                            .. Section::default()
                        });
                    }
                }
                self.command_render(command_output);
            }
            RenderMenuState::StageSelect (selection) => {
                self.draw_stage_selector(&mut entities, selection);
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
                        text: "Replay saved!",
                        color: [1.0, 1.0, 1.0, 1.0],
                        screen_position: (30.0, self.height as f32 - 30.0),
                        scale: GlyphScale::uniform(30.0),
                        .. Section::default()
                    });
                }
            }
            RenderMenuState::GenericText (ref text) => {
                self.glyph_brush.queue(Section {
                    text,
                    color: [1.0, 1.0, 0.0, 1.0],
                    screen_position: (100.0, 50.0),
                    scale: GlyphScale::uniform(30.0),
                    .. Section::default()
                });
            }
        }

        for entity in entities {
            match entity.entity {
                MenuEntity::Fighter { ref fighter, action, frame } => {
                    if let Some(buffers) = Buffers::new_fighter_frame(&self.device, &self.package.as_ref().unwrap(), fighter, action, frame) {
                        rpass.set_pipeline(&self.pipeline_hitbox);
                        rpass.set_bind_group(0, &entity.bind_group, &[]);
                        rpass.set_index_buffer(&buffers.index, 0);
                        rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
                        rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
                    }
                }
                MenuEntity::Stage (ref stage) => {
                    let stage: &str = stage.as_ref();
                    let stage = &self.package.as_ref().unwrap().stages[stage];
                    if let Some(buffers) = ColorBuffers::new_surfaces(&self.device, &stage.surfaces) {
                        rpass.set_pipeline(&self.pipeline_debug);
                        rpass.set_bind_group(0, &entity.bind_group, &[]);
                        rpass.set_index_buffer(&buffers.index, 0);
                        rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
                        rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
                    }

                }
                MenuEntity::StageFill (ref stage) => {
                    let stage: &str = stage.as_ref();
                    let stage = &self.package.as_ref().unwrap().stages[stage];
                    if let Some(buffers) = ColorBuffers::new_surfaces_fill(&self.device, &stage.surfaces) {
                        rpass.set_pipeline(&self.pipeline_debug);
                        rpass.set_bind_group(0, &entity.bind_group, &[]);
                        rpass.set_index_buffer(&buffers.index, 0);
                        rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
                        rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
                    }
                }
                MenuEntity::Rect (ref rect) => {
                    let buffers = ColorBuffers::rect_buffers(&self.device, rect.clone(), [1.0, 1.0, 1.0, 1.0]);
                    rpass.set_pipeline(&self.pipeline_debug);
                    rpass.set_bind_group(0, &entity.bind_group, &[]);
                    rpass.set_index_buffer(&buffers.index, 0);
                    rpass.set_vertex_buffers(0, &[(&buffers.vertex, 0)]);
                    rpass.draw_indexed(0 .. buffers.index_count, 0, 0 .. 1);
                }
            }
        }
    }

    fn draw_game_selector(&mut self, selection: usize) {
        self.glyph_brush.queue(Section {
            text: "Select Game Mode",
            color: [1.0, 1.0, 1.0, 1.0],
            screen_position: (100.0, 4.0),
            scale: GlyphScale::uniform(50.0),
            .. Section::default()
        });

        let modes = vec!("Local", "Netplay", "Replays");
        for (mode_i, name) in modes.iter().enumerate() {
            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let x_offset = if mode_i == selection { 0.1 } else { 0.0 };
            let x = self.width as f32 * (0.1 + x_offset);
            let y = self.height as f32 * 0.1 + mode_i as f32 * 50.0;
            self.glyph_brush.queue(Section {
                text: name.as_ref(),
                color: [1.0, 1.0, 1.0, 1.0],
                screen_position: (x, y),
                scale: GlyphScale::uniform(size),
                .. Section::default()
            });
        }
    }

    fn draw_replay_selector(&mut self, replay_names: &[String], selection: usize) {
        self.glyph_brush.queue(Section {
            text: "Select Replay",
            color: [1.0, 1.0, 1.0, 1.0],
            screen_position: (100.0, 4.0),
            scale: GlyphScale::uniform(50.0),
            .. Section::default()
        });

        for (replay_i, name) in replay_names.iter().enumerate() {
            let size = 26.0; // TODO: determine from width/height of screen and start/end pos
            let x_offset = if replay_i == selection { 0.1 } else { 0.0 };
            let x = self.width as f32 * (0.1 + x_offset);
            let y = self.height as f32 * 0.1 + replay_i as f32 * 50.0;
            self.glyph_brush.queue(Section {
                text: name.as_ref(),
                color: [1.0, 1.0, 1.0, 1.0],
                screen_position: (x, y),
                scale: GlyphScale::uniform(size),
                .. Section::default()
            });
        }
    }

    // TODO: Rewrite text rendering to be part of scene instead of just plastered on top
    // TODO: Then this bar can be drawn on top of the package banner text
    fn draw_back_counter(&mut self, entities: &mut Vec<MenuEntityAndBindGroup>, back_counter: usize, back_counter_max: usize) {
        let uniform = Uniform {
            edge_color:     [1.0, 1.0, 1.0, 1.0],
            color:          [1.0, 1.0, 1.0, 1.0],
            transform: Matrix4::identity().into(),
        };
        let bind_group = self.new_bind_group(uniform);

        let entity = MenuEntity::Rect (Rect {
            x1: -1.0,
            y1: -0.85,
            x2: back_counter as f32 / back_counter_max as f32 * 2.0 - 1.0,
            y2: -1.0,
        });

        entities.push(MenuEntityAndBindGroup { bind_group, entity });
    }

    fn draw_fighter_selector(&mut self, entities: &mut Vec<MenuEntityAndBindGroup>, selections: &[(&PlayerSelect, usize)], i: usize, start_x: f32, start_y: f32, end_x: f32, end_y: f32) {
        let fighters = &self.package.as_ref().unwrap().fighters;
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
                PlayerSelectUi::CpuAi        (_) => format!("CPU AI"),
                PlayerSelectUi::CpuFighter   (_) => format!("CPU Fighter"),
                PlayerSelectUi::HumanFighter (_) => format!("Port #{}", controller_i+1),
                PlayerSelectUi::HumanTeam    (_) => format!("Port #{} Team", controller_i+1),
                PlayerSelectUi::CpuTeam      (_) => format!("CPU Team"),
                PlayerSelectUi::HumanUnplugged   => unreachable!()
            };
            self.glyph_brush.queue(Section {
                text: name.as_ref(),
                color: color,
                screen_position: (x, y),
                scale: GlyphScale::uniform(size),
                .. Section::default()
            });
        }

        // render UI
        let mut options = vec!();
        match selection.ui {
            PlayerSelectUi::HumanFighter (_) => {
                options.extend(fighters.iter().map(|x| x.name.clone()));
                options.push(String::from("Change Team"));
                options.push(String::from("Add CPU"));
            }
            PlayerSelectUi::CpuFighter (_) => {
                options.extend(fighters.iter().map(|x| x.name.clone()));
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
                text: option.as_ref(),
                color,
                screen_position: (x, y),
                scale: GlyphScale::uniform(size),
                .. Section::default()
            });
        }

        // render fighter
        for (fighter_i, (fighter_key, _)) in fighters.key_value_iter().enumerate() {
            if let Some(selection_i) = selection.fighter {
                if fighter_i == selection_i {
                    let fighter = fighters.key_to_value(fighter_key).unwrap();

                    // Determine action, handling the user setting it to an invalid value
                    let css_action = fighter.css_action as usize;
                    let action = if css_action < fighter.actions.len() {
                        css_action
                    } else {
                        Action::Idle.to_u64().unwrap() as usize
                    };

                    let frames = vec!(RenderPlayerFrame {
                        fighter:    fighter_key.clone(),
                        model_name: "TODO".into(),
                        bps:        (0.0, 0.0),
                        ecb:        ECB::default(),
                        face_right: start_x < 0.0,
                        frame:      selection.animation_frame,
                        angle:      0.0,
                        action,
                    });

                    // draw fighter
                    let player = RenderPlayer {
                        team:              selection.team,
                        debug:             DebugPlayer::default(),
                        damage:            0.0,
                        stocks:            None,
                        frame_data:        ActionFrame::default(),
                        fighter_color:     graphics::get_team_color3(selection.team),
                        fighter_selected:  false,
                        player_selected:   false,
                        selected_colboxes: HashSet::new(),
                        shield:            None,
                        vector_arrows:     vec!(),
                        particles:         vec!(),
                        frames,
                    };

                    // fudge player data TODO: One day I would like to have the menu selection fighters (mostly) playable
                    let zoom = fighter.css_scale / 40.0;
                    let fighter_x = start_x + (end_x - start_x) / 2.0;
                    let fighter_y = end_y * -1.0 + 0.05;

                    let camera   = Matrix4::from_nonuniform_scale(zoom, zoom * self.aspect_ratio(), 1.0);
                    let position = Matrix4::from_translation(Vector3::new(fighter_x, fighter_y, 0.0));
                    let dir      = Matrix4::from_nonuniform_scale(if player.frames[0].face_right { 1.0 } else { -1.0 }, 1.0, 1.0);
                    let transformation = position * (camera * dir);
                    let uniform = Uniform {
                        edge_color:     graphics::get_team_color4(selection.team),
                        color:          [0.9, 0.9, 0.9, 1.0],
                        transform: transformation.into(),
                    };
                    let bind_group = self.new_bind_group(uniform);

                    let entity = MenuEntity::Fighter {
                        fighter: player.frames[0].fighter.clone(),
                        action:  player.frames[0].action,
                        frame:   player.frames[0].frame
                    };

                    entities.push(MenuEntityAndBindGroup { bind_group, entity });
                }
            }
        }
    }

    fn draw_stage_selector(&mut self, entities: &mut Vec<MenuEntityAndBindGroup>, selection: usize) {
        self.glyph_brush.queue(Section {
            text: "Select Stage",
            color: [1.0, 1.0, 1.0, 1.0],
            screen_position: (100.0, 4.0),
            scale: GlyphScale::uniform(50.0),
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
                text: stage.name.as_ref(),
                color: [1.0, 1.0, 1.0, 1.0],
                screen_position: (x, y),
                scale: GlyphScale::uniform(size),
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

                let bind_group = self.new_bind_group(uniform);
                let entity = MenuEntity::Stage(stage_key.clone());
                entities.push(MenuEntityAndBindGroup { bind_group, entity });

                let bind_group = self.new_bind_group(uniform);
                let entity = MenuEntity::StageFill(stage_key.clone());
                entities.push(MenuEntityAndBindGroup { bind_group, entity });
            }
        }
    }

    fn draw_player_result(&mut self, result: &PlayerResult, start_x: f32) {
        let fighter_name = self.package.as_ref().unwrap().fighters[result.fighter.as_ref()].name.as_ref();
        let color = graphics::get_team_color4(result.team);
        let x = (start_x + 0.05) * self.width as f32;
        let mut y = 30.0;
        self.glyph_brush.queue(Section {
            text: (result.place + 1).to_string().as_ref(),
            color,
            screen_position: (x, y),
            scale: GlyphScale::uniform(100.0),
            .. Section::default()
        });
        y += 120.0;
        self.glyph_brush.queue(Section {
            text: fighter_name,
            color,
            screen_position: (x, y),
            scale: GlyphScale::uniform(30.0),
            .. Section::default()
        });
        y += 30.0;
        self.glyph_brush.queue(Section {
            text: format!("Kills: {}", result.kills.len()).as_str(),
            color,
            screen_position: (x, y),
            scale: GlyphScale::uniform(30.0),
            .. Section::default()
        });
        y += 30.0;
        self.glyph_brush.queue(Section {
            text: format!("Deaths: {}", result.deaths.len()).as_str(),
            color,
            screen_position: (x, y),
            scale: GlyphScale::uniform(30.0),
            .. Section::default()
        });
        y += 30.0;
        self.glyph_brush.queue(Section {
            text: format!("L-Cancel Success: {}%", result.lcancel_percent).as_str(),
            color,
            screen_position: (x, y),
            scale: GlyphScale::uniform(30.0),
            .. Section::default()
        });
    }

    fn aspect_ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// returns true iff fails
    fn force_send_window_events(&mut self) -> bool {
        // We need to force send the resolution and dpi every frame because WinitInputHelper may receive the normal events while it isn't listening for them.
        let event = Event::WindowEvent {
            window_id: self.window.id(),
            event: WindowEvent::Resized(self.window.inner_size())
        };

        if let Err(_) = self.os_input_tx.send(event) {
            return true;
        }

        // force send the current dpi
        let event = Event::WindowEvent {
            window_id: self.window.id(),
            event: WindowEvent::HiDpiFactorChanged(self.window.hidpi_factor())
        };
        if let Err(_) = self.os_input_tx.send(event) {
            return true;
        }

        false
    }
}

struct WindowSizeDependent {
    swap_chain:               Option<SwapChain>,
    multisampled_framebuffer: TextureView,
    depth_stencil:            TextureView,
}

impl WindowSizeDependent {
    /// This method is called once during initialization, then again whenever the window is resized
    fn new(device: &Device, surface: &Surface, width: u32, height: u32) -> WindowSizeDependent {
        let swap_chain = Some(device.create_swap_chain(
            &surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8Unorm,
                present_mode: wgpu::PresentMode::Vsync,
                width,
                height,
            },
        ));

        let multisampled_frame_descriptor = &wgpu::TextureDescriptor {
            size: wgpu::Extent3d { width, height, depth: 1 },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        };
        let multisampled_framebuffer = device.create_texture(multisampled_frame_descriptor).create_default_view();

        let depth_stencil_descriptor = &wgpu::TextureDescriptor {
            size: wgpu::Extent3d { width, height, depth: 1 },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        };
        let depth_stencil = device.create_texture(depth_stencil_descriptor).create_default_view();

        WindowSizeDependent {
            swap_chain,
            multisampled_framebuffer,
            depth_stencil,
        }
    }
}

enum MenuEntity {
    Fighter   { fighter: String, action: usize, frame: usize },
    Stage     (String),
    StageFill (String),
    Rect      (Rect),
}

struct MenuEntityAndBindGroup {
    entity:     MenuEntity,
    bind_group: BindGroup,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct Uniform {
    edge_color: [f32; 4],
    color:      [f32; 4],
    transform:  [[f32; 4]; 4],
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct TransformUniform {
    transform: [[f32; 4]; 4],
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct AnimatedUniform {
    transform: [[f32; 4]; 4],
    joint_transforms: [[[f32; 4]; 4]; 500],
}

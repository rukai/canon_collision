#![windows_subsystem = "windows"]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate treeflection_derive;

pub(crate) mod ai;
pub(crate) mod app;
pub(crate) mod assets;
pub(crate) mod camera;
pub(crate) mod cli;
pub(crate) mod collision;
pub(crate) mod game;
pub(crate) mod graphics;
pub(crate) mod input;
pub(crate) mod menu;
pub(crate) mod particle;
pub(crate) mod player;
pub(crate) mod replays;
pub(crate) mod results;

#[cfg(feature = "wgpu_renderer")]
pub(crate) mod wgpu;

use canon_collision_lib::logger;
#[cfg(feature = "wgpu_renderer")]
use crate::wgpu::WgpuGraphics;
use crate::cli::GraphicsBackendChoice;

use winit::event_loop::EventLoop;

fn main() {
    canon_collision_lib::setup_panic_handler!();
    logger::init();

    let cli_results = cli::cli();
    let graphics_backend = cli_results.graphics_backend.clone();
    let (os_input_tx, render_rx) = app::run_in_thread(cli_results);

    match graphics_backend {
        #[cfg(feature = "wgpu_renderer")]
        GraphicsBackendChoice::Wgpu => {
            let event_loop = EventLoop::new();
            let mut graphics = WgpuGraphics::new(&event_loop, os_input_tx, render_rx);
            event_loop.run(move |event, _, control_flow| {
                graphics.update(event, control_flow);
            });
        }
        GraphicsBackendChoice::Headless => {
            loop { }
        }
    }
}

pub enum ActualBackend {
    #[cfg(feature = "wgpu_renderer")]
    Wgpu,
    Headless,
}

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

use app::run;
use canon_collision_lib::logger;

fn main() {
    canon_collision_lib::setup_panic_handler!();
    logger::init();
    run();
}

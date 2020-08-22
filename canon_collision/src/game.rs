use crate::camera::Camera;
use crate::collision::collision_box;
use crate::collision::item_grab;
use crate::entity::player::{Player};
use crate::entity::{Entity, EntityType, StepContext, RenderEntity, DebugEntity, Entities, DebugEntities, EntityKey};
use crate::graphics::{GraphicsMessage, Render, RenderType};
use crate::menu::ResumeMenu;
use crate::replays::Replay;
use crate::replays;
use crate::results::{GameResults, RawPlayerResult, PlayerResult};
use crate::rules::{Rules, Goal};

use canon_collision_lib::command_line::CommandLine;
use canon_collision_lib::config::Config;
use canon_collision_lib::entity_def::{ActionFrame, CollisionBox, Action};
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::input::Input;
use canon_collision_lib::input::state::{PlayerInput, ControllerInput};
use canon_collision_lib::network::Netplay;
use canon_collision_lib::package::Package;
use canon_collision_lib::stage::{Stage, DebugStage, SpawnPoint, Surface, Floor, RenderStageMode};

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use byteorder::{LittleEndian, WriteBytesExt};
use chrono::Local;
use num_traits::{FromPrimitive, ToPrimitive};
use rand_chacha::ChaChaRng;
use rand_chacha::rand_core::SeedableRng;
use treeflection::{Node, NodeRunner, NodeToken};
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;

#[NodeActions(
    NodeAction(function="save_replay", return_string),
    NodeAction(function="reset_deadzones", return_string),
    NodeAction(function="copy_stage_to_package", return_string),
    NodeAction(function="copy_package_to_stage", return_string),
)]
#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct Game {
    pub package:                Package,
    pub init_seed:              u64,
    pub state:                  GameState,
        entity_history:         Vec<Entities>,
    pub stage_history:          Vec<Stage>,
    pub current_frame:          usize,
    pub saved_frame:            usize,
    pub deleted_history_frames: usize,
    pub max_history_frames:     Option<usize>,
    pub stage:                  Stage,
        entities:               Entities,
    pub debug_stage:            DebugStage,
        debug_entities:         DebugEntities,
    pub selected_controllers:   Vec<usize>,
    pub selected_ais:           Vec<usize>,
    pub selected_stage:         String,
    pub rules:                  Rules,
        edit:                   Edit,
    pub debug_output_this_step: bool,
    pub debug_lines:            Vec<String>,
    pub selector:               Selector,
        copied_frame:           Option<ActionFrame>,
    pub camera:                 Camera,
    pub tas:                    Vec<ControllerInput>,
    save_replay:                bool,
    reset_deadzones:            bool,
    prev_mouse_point:           Option<(f32, f32)>,
}

/// Frame 0 refers to the initial state of the game.
/// Any changes occur in the proceeding frames i.e. frames 1, 2, 3 ...

/// All previous frame state is used to calculate the next frame, then the current_frame is incremented.

impl Game {
    pub fn new(package: Package, setup: GameSetup) -> Game {
        let stage = if let Some(stage) = setup.hot_reload_stage {
            stage
        } else {
            package.stages[setup.stage.as_ref()].clone()
        };

        let debug_stage = if let Some(debug_stage) = setup.debug_stage {
            debug_stage
        } else if setup.debug {
            DebugStage::all()
        } else {
            DebugStage::default()
        };

        // generate players
        let mut entities: Entities = Default::default();
        {
            for (i, player) in setup.players.iter().enumerate() {
                // Stage can have less spawn points then players
                let fighter = player.fighter.clone();
                let team = player.team;
                entities.insert(Entity {
                    ty: EntityType::Player(Player::new(fighter, team, i, &stage, &package, &setup.rules))
                });
            }
        }

        let mut debug_entities = if let Some(value) = setup.debug_entities {
            value
        } else {
            Default::default()
        };

        if setup.debug {
            for key in entities.keys() {
                debug_entities.insert(key, DebugEntity::all());
            }
        }

        if let Some(overwrite) = setup.hot_reload_entities {
            entities = overwrite;
        }

        Game {
            init_seed:              setup.init_seed,
            state:                  setup.state,
            entity_history:         setup.entity_history,
            stage_history:          setup.stage_history,
            current_frame:          setup.current_frame,
            saved_frame:            0,
            max_history_frames:     setup.max_history_frames,
            deleted_history_frames: setup.deleted_history_frames,
            selected_controllers:   setup.controllers,
            selected_ais:           setup.ais,
            selected_stage:         setup.stage,
            rules:                  setup.rules,
            edit:                   setup.edit,
            debug_output_this_step: false,
            debug_lines:            vec!(),
            selector:               Default::default(),
            copied_frame:           None,
            camera:                 setup.camera,
            tas:                    vec!(),
            save_replay:            false,
            reset_deadzones:        false,
            prev_mouse_point:       None,
            package,
            stage,
            entities,
            debug_stage,
            debug_entities,
        }
    }

    pub fn step(&mut self, config: &mut Config, input: &mut Input, os_input: &WinitInputHelper, os_input_blocked: bool, netplay: &Netplay) -> GameState {
        if os_input.held_alt() && os_input.key_pressed(VirtualKeyCode::Return) {
            config.fullscreen = !config.fullscreen;
            config.save();
        }

        if self.save_replay {
            replays::save_replay(&Replay::new(self, input));
            self.save_replay = false;
        }

        {
            let state = self.state.clone();
            match state {
                GameState::Local                     => self.step_local(input, netplay),
                GameState::Netplay                   => self.step_netplay(input, netplay),
                GameState::ReplayForwardsFromHistory => self.step_replay_forwards_from_history(input),
                GameState::ReplayForwardsFromInput   => self.step_replay_forwards_from_input(input, netplay),
                GameState::ReplayBackwards           => self.step_replay_backwards(input),
                GameState::StepThenPause             => { self.step_local(input, netplay); self.state = GameState::Paused; }
                GameState::StepForwardThenPause      => { self.step_replay_forwards_from_history(input); self.state = GameState::Paused; }
                GameState::StepBackwardThenPause     => { self.step_replay_backwards(input); self.state = GameState::Paused; }
                GameState::Paused                    => self.step_pause(input),
                GameState::Quit (_)                  => unreachable!(),
            }

            if !os_input_blocked {
                match state {
                    GameState::Local                     => self.step_local_os_input(os_input),
                    GameState::ReplayForwardsFromHistory => self.step_replay_forwards_os_input(os_input),
                    GameState::ReplayForwardsFromInput   => self.step_replay_forwards_os_input(os_input),
                    GameState::ReplayBackwards           => self.step_replay_backwards_os_input(os_input),
                    GameState::Paused                    => self.step_pause_os_input(input, os_input, netplay),
                    GameState::Quit (_)                  => unreachable!(),

                    GameState::Netplay              | GameState::StepThenPause |
                    GameState::StepForwardThenPause | GameState::StepBackwardThenPause => { }
                }
                self.camera.update_os_input(os_input);
                self.prev_mouse_point = os_input.mouse();
            }
            self.camera.update(os_input, &self.entities, &self.package.entities, &self.stage);

            self.generate_debug(input, netplay);
        }

        self.set_context();

        debug!("current_frame: {}", self.current_frame);
        self.state.clone()
    }

    fn game_mouse(&self, os_input: &WinitInputHelper) -> Option<(f32, f32)> {
        os_input.mouse().and_then(|point| self.camera.mouse_to_game(point))
    }

    fn game_mouse_diff(&self, os_input: &WinitInputHelper) -> (f32, f32) {
        if let (Some(cur), Some(prev)) = (os_input.mouse(), self.prev_mouse_point) {
            if let (Some(cur), Some(prev)) = (self.camera.mouse_to_game(cur), self.camera.mouse_to_game(prev)) {
                return (cur.0 - prev.0, cur.1 - prev.1)
            }
        }

        (0.0, 0.0)
    }

    pub fn save_replay(&mut self) -> String {
        self.save_replay = true;
        // TODO: We are actually lying here, we cant complete the save until the Game::step where we have access to the input data.
        String::from("Save replay completed")
    }

    pub fn reset_deadzones(&mut self) -> String {
        self.reset_deadzones = true;
        String::from("Deadzones reset")
    }

    pub fn copy_stage_to_package(&mut self) -> String {
        self.package.stages[self.selected_stage.as_ref()] = self.stage.clone();
        String::from("Current stage state copied to package")
    }

    pub fn copy_package_to_stage(&mut self) -> String {
        self.stage = self.package.stages[self.selected_stage.as_ref()].clone();
        String::from("Package copied to current stage state")
    }

    pub fn check_reset_deadzones(&mut self) -> bool {
        let value = self.reset_deadzones;
        self.reset_deadzones = false;
        value
    }

    fn set_context(&mut self) {
        match self.edit {
            Edit::Entity (entity_i) => {
                if let Some(entity) = self.entities.get(entity_i) {
                    let entity_def_key  = entity.entity_def_key().as_ref();
                    let entity_action   = entity.action() as usize;
                    let entity_frame    = entity.frame() as usize;
                    let entity_colboxes = self.selector.colboxes_vec();

                    let entity_defs = &mut self.package.entities;
                    if let Some(fighter_index) = entity_defs.key_to_index(entity_def_key) {
                        entity_defs.set_context(fighter_index);
                    }
                    else {
                        return;
                    }

                    let actions = &mut entity_defs[entity_def_key].actions;
                    if entity_action >= actions.len() {
                        return;
                    }
                    actions.set_context(entity_action);

                    let frames = &mut actions[entity_action].frames;
                    if entity_frame >= frames.len() {
                        return;
                    }
                    frames.set_context(entity_frame);

                    let colboxes = &mut frames[entity_frame].colboxes;
                    colboxes.set_context_vec(entity_colboxes);
                }
            }
            Edit::Stage => {
                self.stage.surfaces.set_context_vec(self.selector.surfaces_vec());
                self.stage.spawn_points.set_context_vec(self.selector.spawn_points.iter().cloned().collect());
                self.stage.respawn_points.set_context_vec(self.selector.respawn_points.iter().cloned().collect());
            }
        }
    }

    fn step_local(&mut self, input: &mut Input, netplay: &Netplay) {
        self.entity_history.push(self.entities.clone());
        self.stage_history.push(self.stage.clone());
        self.current_frame += 1;

        // erase any future history
        for _ in self.current_history_index()..self.entity_history.len() {
            self.entity_history.pop();
        }
        for _ in self.current_history_index()..self.stage_history.len() {
            self.stage_history.pop();
        }

        // run game loop
        input.game_update(self.current_frame);
        let player_inputs = &input.players(self.current_frame, netplay);
        self.step_game(input, player_inputs);

        if let Some(max_history_frames) = self.max_history_frames {
            let extra_frames = self.entity_history.len().saturating_sub(max_history_frames);
            self.deleted_history_frames += extra_frames;
            if extra_frames > 0 {
                self.entity_history.drain(0..extra_frames);
                self.stage_history.drain(0..extra_frames);
            }
        }

        // pause game
        if input.start_pressed() {
            self.state = GameState::Paused;
        }
    }

    fn step_local_os_input(&mut self, os_input: &WinitInputHelper) {
        if os_input.key_pressed(VirtualKeyCode::Space) || os_input.key_pressed(VirtualKeyCode::Return) {
            self.state = GameState::Paused;
        }
    }

    fn step_netplay(&mut self, input: &mut Input, netplay: &Netplay) {
        if !netplay.skip_frame() {
            self.current_frame += 1;

            let start = self.current_frame - netplay.frames_to_step();
            let end = self.current_frame;

            self.entity_history.truncate(start);
            self.stage_history.truncate(start);
            if start != 0 {
                self.entities = self.entity_history.get(start-1).unwrap().clone();
                self.stage   = self.stage_history.get(start-1).unwrap().clone();
            }

            input.netplay_update();

            for frame in start..end {
                let player_inputs = &input.players(frame, netplay);
                self.step_game(input, player_inputs);

                self.entity_history.push(self.entities.clone());
                self.stage_history.push(self.stage.clone());
            }
        }
    }

    fn step_pause(&mut self, input: &mut Input) {
        if input.game_quit_held() {
            self.state = GameState::Quit (ResumeMenu::Unchanged);
        }
        else if input.start_pressed() {
            self.state = GameState::Local;
        }
    }

    fn step_pause_os_input(&mut self, input: &mut Input, os_input: &WinitInputHelper, netplay: &Netplay) {
        // game flow control
        if os_input.key_pressed(VirtualKeyCode::J) {
            self.step_replay_backwards(input);
        }
        else if os_input.held_shift() && os_input.key_pressed(VirtualKeyCode::K) {
            self.step_replay_forwards_from_input(input, netplay);
        }
        else if os_input.key_pressed(VirtualKeyCode::K) {
            self.step_replay_forwards_from_history(input);
        }
        else if os_input.key_pressed(VirtualKeyCode::H) {
            self.state = GameState::ReplayBackwards;
        }
        else if os_input.held_shift() && os_input.key_pressed(VirtualKeyCode::L) {
            self.state = GameState::ReplayForwardsFromInput;
        }
        else if os_input.key_pressed(VirtualKeyCode::L) {
            self.state = GameState::ReplayForwardsFromHistory;
        }
        else if os_input.key_pressed(VirtualKeyCode::Space) {
            self.step_local(input, netplay);
        }
        else if os_input.key_pressed(VirtualKeyCode::U) {
            self.saved_frame = self.current_frame;
        }
        else if os_input.key_pressed(VirtualKeyCode::I) {
            self.jump_frame(self.saved_frame);
        }
        else if os_input.key_pressed(VirtualKeyCode::Return) {
            self.state = GameState::Local;
        }

        if self.camera.dev_mode() {
            self.step_editor(input, os_input, netplay);
        }
    }

    fn step_editor(&mut self, input: &mut Input, os_input: &WinitInputHelper, netplay: &Netplay) {
        // set current edit state
        if os_input.key_pressed(VirtualKeyCode::Key0) {
            self.edit = Edit::Stage;
        }
        else if os_input.key_pressed(VirtualKeyCode::Key1) {
            if let Some(i) = self.entities.keys().skip(0).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key2) {
            if let Some(i) = self.entities.keys().skip(1).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key3) {
            if let Some(i) = self.entities.keys().skip(2).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key4) {
            if let Some(i) = self.entities.keys().skip(3).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key5) {
            if let Some(i) = self.entities.keys().skip(4).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key6) {
            if let Some(i) = self.entities.keys().skip(5).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key7) {
            if let Some(i) = self.entities.keys().skip(6).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key8) {
            if let Some(i) = self.entities.keys().skip(7).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }
        else if os_input.key_pressed(VirtualKeyCode::Key9) {
            if let Some(i) = self.entities.keys().skip(8).next() {
                self.edit = Edit::Entity (i);
            }
            self.update_frame();
        }

        match self.edit {
            Edit::Entity (entity_i) => {
                if self.entities.contains_key(entity_i) {
                    if !self.debug_entities.contains_key(entity_i) {
                        self.debug_entities.insert(entity_i, Default::default());
                    }

                    let entity_def_key = self.entities[entity_i].entity_def_key().to_string();
                    let fighter = entity_def_key.as_ref();
                    let action = self.entities[entity_i].action() as usize;
                    let action_enum = Action::from_u64(self.entities[entity_i].action());
                    let frame  = self.entities[entity_i].frame() as usize;
                    self.debug_entities[entity_i].step(os_input);

                    // by adding the same amount of frames that are skipped in the entity logic,
                    // the user continues to see the same frames as they step through the action
                    let repeat_frames = if let EntityType::Player (player) = &self.entities[entity_i].ty {
                        if action_enum.as_ref().map_or(false, |x| x.is_land()) {
                            player.land_frame_skip + 1
                        } else {
                            1
                        }
                    } else {
                        1
                    };


                    // move collisionboxes
                    if self.selector.moving {
                        // undo the operations used to render the entity
                        let (raw_d_x, raw_d_y) = self.game_mouse_diff(os_input);
                        let angle = -self.entities[entity_i].angle(&self.package.entities[fighter], &self.stage.surfaces); // rotate by the inverse of the angle
                        let d_x = raw_d_x * angle.cos() - raw_d_y * angle.sin();
                        let d_y = raw_d_x * angle.sin() + raw_d_y * angle.cos();
                        let distance = (self.entities[entity_i].relative_f(d_x), d_y); // *= -1 is its own inverse
                        self.package.move_fighter_colboxes(fighter, action, frame, &self.selector.colboxes, distance);

                        // end move
                        if os_input.mouse_pressed(0) {
                            self.update_frame();
                        }
                    }
                    else {
                        // copy frame
                        if os_input.key_pressed(VirtualKeyCode::V) {
                            let frame = self.package.entities[fighter].actions[action].frames[frame].clone();
                            self.copied_frame = Some(frame);
                        }
                        // paste over current frame
                        if os_input.key_pressed(VirtualKeyCode::B) {
                            let action_frame = self.copied_frame.clone();
                            if let Some(action_frame) = action_frame {
                                self.package.insert_fighter_frame(fighter, action, frame, action_frame);
                                self.package.delete_fighter_frame(fighter, action, frame+1);
                            }
                        }

                        // new frame
                        if os_input.key_pressed(VirtualKeyCode::M) {
                            for i in 0..repeat_frames {
                                self.package.new_fighter_frame(fighter, action, frame + i as usize);
                            }
                            // We want to step just the entities current frame to simplify the animation work flow
                            // However we need to do a proper full step so that the history doesn't get mucked up.
                            self.step_local(input, netplay);
                        }
                        // delete frame
                        if os_input.key_pressed(VirtualKeyCode::N) {
                            let i = 0; //for i in 0..repeat_frames { // TODO: Panic
                                if self.package.delete_fighter_frame(fighter, action, frame - i as usize) {
                                    // Correct any entities that are now on a nonexistent frame due to the frame deletion.
                                    // This is purely to stay on the same action for usability.
                                    // The entity itself must handle being on a frame that has been deleted in order for replays to work.
                                    for any_entity in &mut self.entities.values_mut() {
                                        if any_entity.entity_def_key() == fighter && any_entity.action() as usize == action
                                            && any_entity.frame() as usize == self.package.entities[fighter].actions[action].frames.len()
                                        {
                                            any_entity.set_frame(any_entity.frame() - 1);
                                        }
                                    }
                                    self.update_frame();
                                }
                            //}
                        }

                        // start move collisionbox
                        if os_input.key_pressed(VirtualKeyCode::A) {
                            if self.selector.colboxes.len() > 0 {
                                self.selector.moving = true;
                            }
                        }
                        // enter pivot mode
                        if os_input.key_pressed(VirtualKeyCode::S) {
                            // TODO
                        }
                        // delete collisionbox
                        if os_input.key_pressed(VirtualKeyCode::D) {
                            self.package.delete_fighter_colboxes(fighter, action, frame, &self.selector.colboxes);
                            self.update_frame();
                        }
                        // add collisionbox
                        if os_input.key_pressed(VirtualKeyCode::F) {
                            if let Some((m_x, m_y)) = self.game_mouse(os_input) {
                                let selected = {
                                    let entity = &self.entities[entity_i];
                                    let (p_x, p_y) = entity.public_bps_xy(&self.entities, &self.package.entities, &self.stage.surfaces);

                                    let point = (entity.relative_f(m_x - p_x), m_y - p_y);
                                    let new_colbox = CollisionBox::new(point);

                                    self.package.append_fighter_colbox(fighter, action, frame, new_colbox)
                                };
                                self.update_frame();
                                self.selector.colboxes.insert(selected);
                            }
                        }
                        // resize collisionbox
                        if os_input.key_pressed(VirtualKeyCode::LBracket) {
                            self.package.resize_fighter_colboxes(fighter, action, frame, &self.selector.colboxes, -0.1);
                        }
                        if os_input.key_pressed(VirtualKeyCode::RBracket) {
                            self.package.resize_fighter_colboxes(fighter, action, frame, &self.selector.colboxes, 0.1);
                        }
                        if os_input.key_pressed(VirtualKeyCode::Comma) {
                            if os_input.held_shift() {
                                self.package.fighter_colboxes_order_set_first(fighter, action, frame, &self.selector.colboxes)
                            }
                            else {
                                self.package.fighter_colboxes_order_decrease(fighter, action, frame, &self.selector.colboxes)
                            }
                        }
                        if os_input.key_pressed(VirtualKeyCode::Period) {
                            if os_input.held_shift() {
                                self.package.fighter_colboxes_order_set_last(fighter, action, frame, &self.selector.colboxes)
                            }
                            else {
                                self.package.fighter_colboxes_order_increase(fighter, action, frame, &self.selector.colboxes)
                            }
                        }
                        // set hitbox angle
                        if os_input.key_pressed(VirtualKeyCode::Q) {
                            if let Some((m_x, m_y)) = self.game_mouse(os_input) {
                                let entity = &self.entities[entity_i];
                                let (p_x, p_y) = entity.public_bps_xy(&self.entities, &self.package.entities, &self.stage.surfaces);

                                let x = entity.relative_f(m_x - p_x);
                                let y = m_y - p_y;
                                self.package.point_hitbox_angles_to(fighter, action, frame, &self.selector.colboxes, x, y);
                            }
                        }

                        // handle single selection
                        if let Some((m_x, m_y)) = self.selector.step_single_selection(os_input, &self.camera) {
                            let (entity_x, entity_y) = self.entities[entity_i].public_bps_xy(&self.entities, &self.package.entities, &self.stage.surfaces);
                            let frame = self.entities[entity_i].relative_frame(&self.package.entities[fighter], &self.stage.surfaces);

                            for (i, colbox) in frame.colboxes.iter().enumerate() {
                                let hit_x = colbox.point.0 + entity_x;
                                let hit_y = colbox.point.1 + entity_y;

                                let distance = ((m_x - hit_x).powi(2) + (m_y - hit_y).powi(2)).sqrt();
                                if distance < colbox.radius {
                                    if os_input.held_alt() {
                                        self.selector.colboxes.remove(&i);
                                    } else {
                                        self.selector.colboxes.insert(i);
                                    }
                                }
                            }

                            // Select topmost colbox
                            // TODO: Broken by the addition of ActionFrame.render_order, fix by taking it into account
                            if os_input.held_control() {
                                let mut selector_vec = self.selector.colboxes_vec();
                                selector_vec.sort();
                                selector_vec.reverse();
                                selector_vec.truncate(1);
                                self.selector.colboxes = selector_vec.into_iter().collect();
                            }
                        }

                        // handle multiple selection
                        if let Some(rect) = self.selector.step_multiple_selection(os_input, &self.camera) {
                            let (entity_x, entity_y) = self.entities[entity_i].public_bps_xy(&self.entities, &self.package.entities, &self.stage.surfaces);
                            let frame = self.entities[entity_i].relative_frame(&self.package.entities[fighter], &self.stage.surfaces);

                            for (i, colbox) in frame.colboxes.iter().enumerate() {
                                let hit_x = colbox.point.0 + entity_x;
                                let hit_y = colbox.point.1 + entity_y;

                                if rect.contains_point(hit_x, hit_y) {
                                    if os_input.held_alt() {
                                        self.selector.colboxes.remove(&i);
                                    } else {
                                        self.selector.colboxes.insert(i);
                                    }
                                }
                            }
                            self.selector.point = None;
                        }
                    }
                }
            }
            Edit::Stage => {
                self.debug_stage.step(os_input);
                if self.selector.moving {
                    let (d_x, d_y) = self.game_mouse_diff(os_input);
                    for (i, spawn) in self.stage.spawn_points.iter_mut().enumerate() {
                        if self.selector.spawn_points.contains(&i) {
                            spawn.x += d_x;
                            spawn.y += d_y;
                        }
                    }

                    for (i, respawn) in self.stage.respawn_points.iter_mut().enumerate() {
                        if self.selector.respawn_points.contains(&i) {
                            respawn.x += d_x;
                            respawn.y += d_y;
                        }
                    }

                    for (i, surface) in self.stage.surfaces.iter_mut().enumerate() {
                        if self.selector.surfaces.contains(&SurfaceSelection::P1(i)) {
                            surface.x1 += d_x;
                            surface.y1 += d_y;
                        }
                        if self.selector.surfaces.contains(&SurfaceSelection::P2(i)) {
                            surface.x2 += d_x;
                            surface.y2 += d_y;
                        }
                    }

                    // end move
                    if os_input.mouse_pressed(0) {
                        self.update_frame();
                    }
                }
                else {
                    // start move elements
                    if os_input.key_pressed(VirtualKeyCode::A) {
                        if self.selector.surfaces.len() + self.selector.spawn_points.len() + self.selector.respawn_points.len() > 0 {
                            self.selector.moving = true;
                        }
                    }
                    // delete elements
                    if os_input.key_pressed(VirtualKeyCode::D) {
                        // the indexes are sorted in reverse order to preserve index order while deleting.
                        let mut spawns_to_delete: Vec<usize> = self.selector.spawn_points.iter().cloned().collect();
                        spawns_to_delete.sort();
                        spawns_to_delete.reverse();
                        for spawn_i in spawns_to_delete {
                            self.stage.spawn_points.remove(spawn_i);
                        }

                        let mut respawns_to_delete: Vec<usize> = self.selector.respawn_points.iter().cloned().collect();
                        respawns_to_delete.sort();
                        respawns_to_delete.reverse();
                        for respawn_i in respawns_to_delete {
                            self.stage.respawn_points.remove(respawn_i);
                        }

                        let mut surfaces_to_delete = self.selector.surfaces_vec();
                        surfaces_to_delete.sort();
                        surfaces_to_delete.reverse();
                        let entities = self.entities.clone();
                        for surface_i in surfaces_to_delete {
                            for (_, entity) in self.entities.iter_mut() {
                                entity.platform_deleted(&entities, &self.package.entities, &self.stage.surfaces, surface_i);
                            }
                            self.stage.surfaces.remove(surface_i);
                        }

                        self.update_frame();
                    }
                    // add decorative surface
                    if os_input.key_pressed(VirtualKeyCode::Q) {
                        self.add_surface(Surface::default(), os_input);
                    }
                    // add ceiling surface
                    if os_input.key_pressed(VirtualKeyCode::W) {
                        let surface = Surface { ceiling: true, .. Surface::default() };
                        self.add_surface(surface, os_input);
                    }
                    // add wall surface
                    if os_input.key_pressed(VirtualKeyCode::E) {
                        let surface = Surface { wall: true, .. Surface::default() };
                        self.add_surface(surface, os_input);
                    }
                    // add stage surface
                    if os_input.key_pressed(VirtualKeyCode::R) {
                        let surface = Surface { floor: Some(Floor { traction: 1.0, pass_through: false }), .. Surface::default() };
                        self.add_surface(surface, os_input);
                    }
                    // add platform surface
                    if os_input.key_pressed(VirtualKeyCode::F) {
                        let surface = Surface { floor: Some(Floor { traction: 1.0, pass_through: true }), .. Surface::default() };
                        self.add_surface(surface, os_input);
                    }
                    // add spawn point
                    if os_input.key_pressed(VirtualKeyCode::Z) {
                        if let Some((m_x, m_y)) = self.game_mouse(os_input) {
                            self.stage.spawn_points.push(SpawnPoint::new(m_x, m_y));
                            self.update_frame();
                        }
                    }
                    // add respawn point
                    if os_input.key_pressed(VirtualKeyCode::X) {
                        if let Some((m_x, m_y)) = self.game_mouse(os_input) {
                            self.stage.respawn_points.push(SpawnPoint::new(m_x, m_y));
                            self.update_frame();
                        }
                    }
                    if os_input.key_pressed(VirtualKeyCode::S) {
                        let mut join = false;
                        let mut points: Vec<(f32, f32)> = vec!();
                        for selection in self.selector.surfaces.iter() {
                            match selection {
                                &SurfaceSelection::P1 (i) => {
                                    let surface = &self.stage.surfaces[i];
                                    if let Some((prev_x, prev_y)) = points.last().cloned() {
                                        if surface.x1 != prev_x || surface.y1 != prev_y {
                                            join = true;
                                        }
                                    }
                                    points.push((surface.x1, surface.y1));
                                }
                                &SurfaceSelection::P2 (i) => {
                                    let surface = &self.stage.surfaces[i];
                                    if let Some((prev_x, prev_y)) = points.last().cloned() {
                                        if surface.x2 != prev_x || surface.y2 != prev_y {
                                            join = true;
                                        }
                                    }
                                    points.push((surface.x2, surface.y2));
                                }
                            }
                        }

                        let mut average_x = 0.0;
                        let mut average_y = 0.0;
                        for (x, y) in points.iter().cloned() {
                            average_x += x;
                            average_y += y;
                        }
                        average_x /= points.len() as f32;
                        average_y /= points.len() as f32;

                        if join {
                            for selection in self.selector.surfaces.iter() {
                                match selection {
                                    &SurfaceSelection::P1 (i) => {
                                        let surface = &mut self.stage.surfaces[i];
                                        surface.x1 = average_x;
                                        surface.y1 = average_y;
                                    }
                                    &SurfaceSelection::P2 (i) => {
                                        let surface = &mut self.stage.surfaces[i];
                                        surface.x2 = average_x;
                                        surface.y2 = average_y;
                                    }
                                }
                            }
                        } else { // split
                            for selection in self.selector.surfaces.iter() {
                                match selection {
                                    &SurfaceSelection::P1 (i) => {
                                        let surface = &mut self.stage.surfaces[i];
                                        surface.x1 = average_x + (surface.x2 - average_x) / 5.0;
                                        surface.y1 = average_y + (surface.y2 - average_y) / 5.0;
                                    }
                                    &SurfaceSelection::P2 (i) => {
                                        let surface = &mut self.stage.surfaces[i];
                                        surface.x2 = average_x + (surface.x1 - average_x) / 5.0;
                                        surface.y2 = average_y + (surface.y1 - average_y) / 5.0;
                                    }
                                }
                            }
                        }
                    }
                }

                // handle single selection
                if let Some((m_x, m_y)) = self.selector.step_single_selection(os_input, &self.camera) {
                    if self.debug_stage.spawn_points {
                        for (i, point) in self.stage.spawn_points.iter().enumerate() {
                            let distance = ((m_x - point.x).powi(2) + (m_y - point.y).powi(2)).sqrt();
                            if distance < 4.0 {
                                if os_input.held_alt() {
                                    self.selector.spawn_points.remove(&i);
                                } else {
                                    self.selector.spawn_points.insert(i);
                                }
                            }
                        }
                    }
                    if self.debug_stage.respawn_points {
                        for (i, point) in self.stage.respawn_points.iter().enumerate() {
                            let distance = ((m_x - point.x).powi(2) + (m_y - point.y).powi(2)).sqrt();
                            if distance < 4.0 {
                                if os_input.held_alt() {
                                    self.selector.respawn_points.remove(&i);
                                } else {
                                    self.selector.respawn_points.insert(i);
                                }
                            }
                        }
                    }
                    for (i, surface) in self.stage.surfaces.iter().enumerate() {
                        let distance1 = ((m_x - surface.x1).powi(2) + (m_y - surface.y1).powi(2)).sqrt();
                        if distance1 < 3.0 { // TODO: check entire half of surface, not just the edge
                            if os_input.held_alt() {
                                self.selector.surfaces.remove(&SurfaceSelection::P1(i));
                            } else {
                                self.selector.surfaces.insert(SurfaceSelection::P1(i));
                            }
                        }
                        let distance2 = ((m_x - surface.x2).powi(2) + (m_y - surface.y2).powi(2)).sqrt();
                        if distance2 < 3.0 {
                            if os_input.held_alt() {
                                self.selector.surfaces.remove(&SurfaceSelection::P2(i));
                            } else {
                                self.selector.surfaces.insert(SurfaceSelection::P2(i));
                            }
                        }
                    }
                }

                // handle multiple selection
                if let Some(rect) = self.selector.step_multiple_selection(os_input, &self.camera) {
                    if self.debug_stage.spawn_points {
                        for (i, point) in self.stage.spawn_points.iter().enumerate() {
                            if rect.contains_point(point.x, point.y) { // TODO: check entire half of surface, not just the edge
                                if os_input.held_alt() {
                                    self.selector.spawn_points.remove(&i);
                                } else {
                                    self.selector.spawn_points.insert(i);
                                }
                            }
                        }
                    }
                    if self.debug_stage.respawn_points {
                        for (i, point) in self.stage.respawn_points.iter().enumerate() {
                            if rect.contains_point(point.x, point.y) {
                                if os_input.held_alt() {
                                    self.selector.respawn_points.remove(&i);
                                } else {
                                    self.selector.respawn_points.insert(i);
                                }
                            }
                        }
                    }
                    for (i, surface) in self.stage.surfaces.iter().enumerate() {
                        if rect.contains_point(surface.x1, surface.y1) {
                            if os_input.held_alt() {
                                self.selector.surfaces.remove(&SurfaceSelection::P1(i));
                            } else {
                                self.selector.surfaces.insert(SurfaceSelection::P1(i));
                            }
                        }
                        if rect.contains_point(surface.x2, surface.y2) {
                            if os_input.held_alt() {
                                self.selector.surfaces.remove(&SurfaceSelection::P2(i));
                            } else {
                                self.selector.surfaces.insert(SurfaceSelection::P2(i));
                            }
                        }
                    }
                    self.selector.point = None;
                }
            }
        }
        self.selector.mouse = self.game_mouse(os_input); // hack to access mouse during render call, dont use this otherwise
    }

    fn add_surface(&mut self, surface: Surface, os_input: &WinitInputHelper) {
        if let Some((m_x, m_y)) = self.game_mouse(os_input) {
            if self.selector.surfaces.len() == 1 {
                // create new surface, p1 is selected surface, p2 is current mouse
                let (x1, y1) = match self.selector.surfaces.iter().next().unwrap() {
                    &SurfaceSelection::P1 (i) => (self.stage.surfaces[i].x1, self.stage.surfaces[i].y1),
                    &SurfaceSelection::P2 (i) => (self.stage.surfaces[i].x2, self.stage.surfaces[i].y2)
                };

                self.selector.clear();
                self.selector.surfaces.insert(SurfaceSelection::P2(self.stage.surfaces.len()));
                self.stage.surfaces.push(Surface { x1, y1, x2: m_x, y2: m_y, .. surface });
            }
            else if self.selector.surfaces.len() == 0 {
                // create new surface, p1 is current mouse, p2 is moving
                self.selector.clear();
                self.selector.surfaces.insert(SurfaceSelection::P2(self.stage.surfaces.len()));
                self.selector.moving = true;
                self.stage.surfaces.push(Surface { x1: m_x, y1: m_y, x2: m_x, y2: m_y, .. surface } );
            }
        }
    }

    /// next frame is advanced by using the input history on the current frame
    fn step_replay_forwards_from_input(&mut self, input: &mut Input, netplay: &Netplay) {
        if self.current_frame <= input.last_frame() {
            self.current_frame += 1;
            let player_inputs = &input.players(self.current_frame, netplay);
            self.step_game(input, player_inputs);

            self.update_frame();
        }
        else {
            self.state = GameState::Paused;
        }

        if input.start_pressed() {
            self.state = GameState::Paused;
        }
    }

    /// next frame is advanced by taking the next frame in history
    fn step_replay_forwards_from_history(&mut self, input: &mut Input) {
        if self.current_history_index() < self.entity_history.len() {
            self.jump_frame(self.current_frame + 1);
        }
        else {
            self.state = GameState::Paused;
        }

        if input.start_pressed() {
            self.state = GameState::Paused;
        }
    }

    fn step_replay_forwards_os_input(&mut self, os_input: &WinitInputHelper) {
        if os_input.key_pressed(VirtualKeyCode::H) {
            self.state = GameState::ReplayBackwards;
        }
        else if os_input.held_shift() && os_input.key_pressed(VirtualKeyCode::L) {
            self.state = GameState::ReplayForwardsFromInput;
        }
        else if os_input.key_pressed(VirtualKeyCode::L) {
            self.state = GameState::ReplayForwardsFromHistory;
        }

        if os_input.key_pressed(VirtualKeyCode::Space) || os_input.key_pressed(VirtualKeyCode::Return) {
            self.state = GameState::Paused;
        }
    }

    /// Immediately jumps to the previous frame in history
    fn step_replay_backwards(&mut self, input: &mut Input) {
        if self.current_frame > 0 {
            self.jump_frame(self.current_frame - 1);
        }
        else {
            self.state = GameState::Paused;
        }

        if input.start_pressed() {
            self.state = GameState::Paused;
            self.update_frame();
        }
    }

    fn step_replay_backwards_os_input(&mut self, os_input: &WinitInputHelper) {
        if os_input.held_shift() && os_input.key_pressed(VirtualKeyCode::L) {
            self.state = GameState::ReplayForwardsFromInput;
        }
        else if os_input.key_pressed(VirtualKeyCode::L) {
            self.state = GameState::ReplayForwardsFromHistory;
        }
        else if os_input.key_pressed(VirtualKeyCode::Space) || os_input.key_pressed(VirtualKeyCode::Return) {
            self.state = GameState::Paused;
            self.update_frame();
        }
    }

    /// Jump to the saved frame in history
    fn jump_frame(&mut self, to_frame: usize) {
        let history_index = to_frame - self.deleted_history_frames;
        if history_index < self.entity_history.len() {
            self.entities = self.entity_history.get(history_index).unwrap().clone();
            self.stage   = self.stage_history .get(history_index).unwrap().clone();

            self.current_frame = to_frame;
            self.update_frame();
        }
    }

    fn get_seed(&self) -> [u8; 32] {
        let mut seed = [0; 32];
        (&mut seed[0..8]).write_u64::<LittleEndian>(self.init_seed).unwrap();
        (&mut seed[8..16]).write_u64::<LittleEndian>(self.current_frame as u64).unwrap();
        seed
    }

    fn step_game(&mut self, input: &Input, player_inputs: &[PlayerInput]) {
        let default_input = PlayerInput::empty();
        {
            let mut rng = ChaChaRng::from_seed(self.get_seed());
            let mut new_entities = vec!();
            let mut messages = vec!();

            // To synchronize entity stepping, we step through entity logic in stages (item grab logic, action logic, physics logic, collision logic)
            // Modified entities are copied from the previous stage so that every entity perceives themselves as being stepped first, within that stage.


            // step each entity action
            let mut action_entities = self.entities.clone();
            let keys: Vec<_> = action_entities.keys().collect();
            for key in keys {
                let delete_self = {
                    let entity = &mut action_entities[key];
                    let input_i = entity.player_id().and_then(|x| self.selected_controllers.get(x));
                    let input = input_i.and_then(|x| player_inputs.get(*x)).unwrap_or(&default_input);
                    let mut context = StepContext {
                        entities:     &self.entities,
                        entity_defs:  &self.package.entities,
                        entity_def:   &self.package.entities[entity.entity_def_key()],
                        stage:        &self.stage,
                        surfaces:     &self.stage.surfaces,
                        rng:          &mut rng,
                        new_entities: &mut new_entities,
                        messages:     &mut messages,
                        delete_self:  false,
                        input,
                    };
                    entity.action_hitlag_step(&mut context);
                    context.delete_self
                };
                if delete_self {
                    action_entities.remove(key);
                }
            }

            // step each player item grab
            // No need to clone entity slotmap, all the real logic lives in collision_check which operates on all entities at once.
            let item_grab_results = item_grab::collision_check(&action_entities, &self.package.entities, &self.stage.surfaces);
            for (current_key, hit_key) in item_grab_results {
                let hit_id = action_entities.get_mut(hit_key).and_then(|x| x.player_id());
                if let Some(current_entity) = action_entities.get_mut(current_key) {
                    current_entity.item_grab(hit_key, hit_id);
                }
            }

            // step each entity physics
            let mut physics_entities = action_entities.clone();
            let keys: Vec<_> = physics_entities.keys().collect();
            for key in keys {
                let delete_self = {
                    let entity = &mut physics_entities[key];
                    let input_i = entity.player_id().and_then(|x| self.selected_controllers.get(x));
                    let input = input_i.and_then(|x| player_inputs.get(*x)).unwrap_or(&default_input);
                    let mut context = StepContext {
                        entities:     &action_entities,
                        entity_defs:  &self.package.entities,
                        entity_def:   &self.package.entities[entity.entity_def_key()],
                        stage:        &self.stage,
                        surfaces:     &self.stage.surfaces,
                        rng:          &mut rng,
                        new_entities: &mut new_entities,
                        messages:     &mut messages,
                        delete_self:  false,
                        input,
                    };
                    entity.physics_step(&mut context, self.current_frame, self.rules.goal.clone());
                    context.delete_self
                };
                if delete_self {
                    physics_entities.remove(key);
                }
            }

            // TODO: resolve invalid states resulting from physics_step that occured because are
            // entities only see other entities from the previous frame.
            // e.g. Two players both grabbing the same ledge, we should randomly pick a player that misses the ledge.
            //
            // Alternatively we could randomize the physics_step order and use the current state of other entities
            // This might be needed actually, I dont undoing a ledge grab will end up nice and/or possible

            // check for hits and run hit logic
            let mut collision_entities = physics_entities.clone();
            let collision_results = collision_box::collision_check(&physics_entities, &self.package.entities, &self.stage.surfaces);
            let keys: Vec<_> = collision_entities.keys().collect();
            for key in keys {
                let delete_self = {
                    let entity = &mut collision_entities[key];
                    let input_i = entity.player_id().and_then(|x| self.selected_controllers.get(x));
                    let input = input_i.and_then(|x| player_inputs.get(*x)).unwrap_or(&default_input);
                    let mut context = StepContext {
                        entities:     &physics_entities,
                        entity_defs:  &self.package.entities,
                        entity_def:   &self.package.entities[entity.entity_def_key()],
                        stage:        &self.stage,
                        surfaces:     &self.stage.surfaces,
                        rng:          &mut rng,
                        new_entities: &mut new_entities,
                        messages:     &mut messages,
                        delete_self:  false,
                        input,
                    };
                    entity.step_collision(&mut context, &collision_results[key]);
                    context.delete_self
                };
                if delete_self {
                    collision_entities.remove(key);
                }
            }

            for message in messages {
                let entity = &mut collision_entities[message.recipient];
                let input_i = entity.player_id().and_then(|x| self.selected_controllers.get(x));
                let input = input_i.and_then(|x| player_inputs.get(*x)).unwrap_or(&default_input);
                let context = StepContext {
                    entities:     &physics_entities,
                    entity_defs:  &self.package.entities,
                    entity_def:   &self.package.entities[entity.entity_def_key()],
                    stage:        &self.stage,
                    surfaces:     &self.stage.surfaces,
                    rng:          &mut rng,
                    new_entities: &mut new_entities,
                    messages:     &mut vec!(),
                    delete_self:  false,
                    input,
                };
                entity.process_message(message, &context);
            }

            for entity in new_entities {
                collision_entities.insert(entity);
            }

            self.entities = collision_entities;
        }

        if self.time_out() ||
           (self.entities.len() == 1 && self.players_iter().filter(|x| x.action != Action::Eliminated.to_u64().unwrap()).count() == 0) ||
           (self.entities.len() >  1 && self.players_iter().filter(|x| x.action != Action::Eliminated.to_u64().unwrap()).count() == 1)
        {
            self.state = self.generate_game_results(input);
        }

        self.update_frame();
    }

    pub fn time_out(&self) -> bool {
        if let Some(time_limit_frames) = self.rules.time_limit_frames() {
            self.current_frame as u64 > time_limit_frames
        } else {
            false
        }
    }

    fn players_iter(&self) -> impl Iterator<Item=&Player> {
        self.entities.values().filter_map(|x| match &x.ty {
            EntityType::Player(player) => Some(player),
            _ => None,
        })
    }

    pub fn generate_game_results(&self, input: &Input) -> GameState {
        let raw_player_results: Vec<RawPlayerResult> = self.players_iter().map(|x| x.result()).collect();
        // TODO: Players on the same team score to the same pool and share their place.
        let places: Vec<usize> = match self.rules.goal {
            Goal::LastManStanding => {
                // most stocks remaining wins
                // tie-breaker:
                //  * if both eliminated: who lost their last stock last wins
                //  * if both alive:      lowest percentage wins
                let mut raw_player_results_i: Vec<(usize, &RawPlayerResult)> = raw_player_results.iter().enumerate().collect();
                raw_player_results_i.sort_by(
                    |a_set, b_set| {
                        let a = a_set.1;
                        let b = b_set.1;
                        let a_deaths = a.deaths.len();
                        let b_deaths = b.deaths.len();
                        a_deaths.cmp(&b_deaths).then(
                            if a_deaths == 0 {
                                if let Some(death_a) = a.deaths.last() {
                                    if let Some(death_b) = b.deaths.last() {
                                        death_a.frame.cmp(&death_b.frame)
                                    }
                                    else {
                                        Ordering::Equal
                                    }
                                }
                                else {
                                    Ordering::Equal
                                }
                            }
                            else {
                                a.final_damage.unwrap().partial_cmp(&b.final_damage.unwrap()).unwrap_or(Ordering::Equal)
                            }
                        )
                    }
                );
                raw_player_results_i.iter().map(|x| x.0).collect()
            }
            Goal::KillDeathScore => {
                // highest kills wins
                // tie breaker: least deaths wins
                let mut raw_player_results_i: Vec<(usize, &RawPlayerResult)> = raw_player_results.iter().enumerate().collect();
                raw_player_results_i.sort_by(
                    |a_set, b_set| {
                        // Repopulating kill lists every frame shouldnt be too bad
                        let a_kills: Vec<usize> = vec!(); // TODO: populate
                        let b_kills: Vec<usize> = vec!(); // TODO: populate
                        let a = a_set.1;
                        let b = b_set.1;
                        let a_kills = a_kills.len();
                        let b_kills = b_kills.len();
                        let a_deaths = a.deaths.len();
                        let b_deaths = b.deaths.len();
                        b_kills.cmp(&a_kills).then(a_deaths.cmp(&b_deaths))
                    }
                );
                raw_player_results_i.iter().map(|x| x.0).collect()
            }
        };

        let mut player_results: Vec<PlayerResult> = vec!();
        for (i, raw_player_result) in raw_player_results.iter().enumerate() {
            let lcancel_percent = if raw_player_result.lcancel_attempts == 0 {
                100.0
            }
            else {
                raw_player_result.lcancel_success as f32 / raw_player_result.lcancel_attempts as f32
            };
            player_results.push(PlayerResult {
                fighter:         raw_player_result.ended_as_fighter.clone().unwrap(),
                team:            raw_player_result.team,
                controller:      self.selected_controllers[i],
                place:           places[i],
                kills:           vec!(), // TODO
                deaths:          raw_player_result.deaths.clone(),
                lcancel_percent: lcancel_percent,
            });
        }
        player_results.sort_by_key(|x| x.place);

        let replay = Replay::new(self, input);

        GameState::Quit (
            ResumeMenu::Results (
                GameResults {
                    player_results,
                    replay,
                }
            )
        )
    }

    fn generate_debug(&mut self, input: &Input, netplay: &Netplay) {
        let frame = self.current_frame;
        let player_inputs = &input.players_no_log(frame, netplay);

        self.debug_lines = self.camera.debug_print();
        self.debug_lines.push(format!("Frame: {}    state: {}", frame, self.state));
        for (i, debug_entity) in self.debug_entities.iter() {
            if let Some(entity) = self.entities.get(i) {
                let input_i = entity.player_id().and_then(|x| self.selected_controllers.get(x));
                let input = input_i.and_then(|x| player_inputs.get(*x));
                self.debug_lines.extend(entity.debug_print(&self.package.entities, input, debug_entity, i));
            }
        }

        if self.debug_output_this_step {
            self.debug_output_this_step = false;
            for i in 1..self.debug_lines.len() {
                debug!("{}", self.debug_lines[i]);
            }
        }
    }

    /// Call this whenever an entity's frame is changed, this can be from:
    /// *   the fighter's frame data is changed
    /// *   the entity now refers to a different frame.
    fn update_frame(&mut self) {
        self.selector = Default::default();
        self.debug_output_this_step = true;
    }

    #[allow(unused)] // Needed for headless build
    pub fn render(&self) -> RenderGame {
        let mut render_entities = vec!();

        let entity_defs = &self.package.entities;
        let surfaces = &self.stage.surfaces;
        for (i, entity) in self.entities.iter() {
            let mut selected_colboxes = HashSet::new();
            let mut entity_selected = false;
            if let GameState::Paused = self.state {
                match self.edit {
                    Edit::Entity (entity_i) => {
                        if i == entity_i {
                            selected_colboxes = self.selector.colboxes.clone();
                            entity_selected = true;
                        }
                    }
                    _ => { }
                }
            }

            let debug = self.debug_entities.get(i).cloned().unwrap_or_default();
            if debug.cam_area {
                if let Some(cam_area) = entity.cam_area(&self.stage.camera, &self.entities, &self.package.entities, &self.stage.surfaces) {
                    render_entities.push(RenderObject::rect_outline(cam_area, 0.0, 0.0, 1.0));
                }
            }
            if debug.item_grab_area {
                if let Some(item_grab_box) = entity.item_grab_box(&self.entities, &self.package.entities, &self.stage.surfaces) {
                    render_entities.push(RenderObject::rect_outline(item_grab_box, 0.0, 1.0, 0.0));
                }
            }

            let player_render = entity.render(selected_colboxes, entity_selected, debug, i, &self.entity_history[0..self.current_history_index()], &self.entities, entity_defs, surfaces);
            render_entities.push(RenderObject::Entity(player_render));
        }

        // render stage debug entities
        if self.debug_stage.blast {
            render_entities.push(RenderObject::rect_outline(self.stage.blast.clone(),  1.0, 0.0, 0.0));
        }
        if self.debug_stage.camera {
            render_entities.push(RenderObject::rect_outline(self.stage.camera.clone(), 0.0, 0.0, 1.0));
        }
        if self.debug_stage.spawn_points {
            for (i, point) in self.stage.spawn_points.iter().enumerate() {
                if self.selector.spawn_points.contains(&i) {
                    render_entities.push(RenderObject::spawn_point(point.clone(), 0.0, 1.0, 0.0));
                } else {
                    render_entities.push(RenderObject::spawn_point(point.clone(), 1.0, 0.0, 1.0));
                }
            }
        }
        if self.debug_stage.respawn_points {
            for (i, point) in self.stage.respawn_points.iter().enumerate() {
                if self.selector.respawn_points.contains(&i) {
                    render_entities.push(RenderObject::spawn_point(point.clone(), 0.0, 1.0, 0.0));
                } else {
                    render_entities.push(RenderObject::spawn_point(point.clone(), 1.0, 1.0, 0.0));
                }
            }
        }

        // render selector box
        if let Some(point) = self.selector.point {
            if let Some(mouse) = self.selector.mouse {
                let render_box = Rect::from_tuples(point, mouse);
                render_entities.push(RenderObject::rect_outline(render_box, 0.0, 1.0, 0.0));
            }
        }

        let timer = if let Some(time_limit_frames) = self.rules.time_limit_frames() {
            let frames_remaining = time_limit_frames.saturating_sub(self.current_frame as u64);
            let frame_duration = Duration::new(1, 0) / 60;
            Some(frame_duration * frames_remaining as u32)
        } else {
            None
        };

        RenderGame {
            seed:              self.get_seed(),
            current_frame:     self.current_frame,
            surfaces:          self.stage.surfaces.to_vec(),
            selected_surfaces: self.selector.surfaces.clone(),
            render_stage_mode: self.debug_stage.render_stage_mode.clone(),
            stage_model_name:  self.stage.name.clone(),
            entities:          render_entities,
            state:             self.state.clone(),
            camera:            self.camera.clone(),
            debug_lines:       self.debug_lines.clone(),
            timer:             timer,
        }
    }

    #[allow(unused)] // Needed for headless build
    pub fn graphics_message(&mut self, config: &Config, command_line: &CommandLine) -> GraphicsMessage {
        let render = Render {
            command_output: command_line.output(),
            render_type:    RenderType::Game(self.render()),
            fullscreen:     config.fullscreen,
        };

        GraphicsMessage {
            package_updates: self.package.updates(),
            render:          render,
        }
    }

    pub fn current_history_index(&self) -> usize {
        self.current_frame - self.deleted_history_frames
    }

    pub fn reclaim(self) -> Package {
        self.package
    }

    /// TODO:
    /// hacky...
    /// lets add the ability to skip public fields to treefleciton instead
    pub fn entity_history(&self) -> Vec<Entities> {
        self.entity_history.clone()
    }
    pub fn edit(&self) -> Edit {
        self.edit.clone()
    }
    pub fn debug_entities(&self) -> DebugEntities {
        self.debug_entities.clone()
    }
    pub fn entities(&self) -> Entities {
        self.entities.clone()
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum GameState {
    Local,
    ReplayForwardsFromHistory,
    ReplayForwardsFromInput,
    ReplayBackwards,
    Netplay,
    Paused, // Only Local, ReplayForwardsFromHistory, ReplayForwardsFromInput and ReplayBackwards can be paused
    Quit (ResumeMenu), // Both Local and Netplay end at Quit

    // Used for TAS, in game these are run during pause state
    StepThenPause,
    StepForwardThenPause,
    StepBackwardThenPause,
}

impl fmt::Display for GameState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &GameState::Local                     => write!(f, "Local"),
            &GameState::ReplayForwardsFromHistory => write!(f, "ReplayForwardsFromHistory"),
            &GameState::ReplayForwardsFromInput   => write!(f, "ReplayForwardsFromInput"),
            &GameState::ReplayBackwards           => write!(f, "ReplayBackwards"),
            &GameState::Netplay                   => write!(f, "Netplay"),
            &GameState::Paused                    => write!(f, "Paused"),
            &GameState::Quit (_)                  => write!(f, "Quit"),
            &GameState::StepThenPause             => write!(f, "StepThenPause"),
            &GameState::StepForwardThenPause      => write!(f, "StepForwardThenPause"),
            &GameState::StepBackwardThenPause     => write!(f, "StepBackwardThenPause)"),
        }
    }
}

impl Default for GameState {
    fn default() -> GameState {
        GameState::Paused
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Edit {
    Entity (EntityKey),
    Stage
}

impl Default for Edit {
    fn default() -> Edit {
        Edit::Stage
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Node)]
pub struct Selector {
    colboxes:       HashSet<usize>,
    surfaces:       HashSet<SurfaceSelection>,
    spawn_points:   HashSet<usize>,
    respawn_points: HashSet<usize>,
    moving:         bool,
    point:          Option<(f32, f32)>, // selector starting point
    mouse:          Option<(f32, f32)>, // used to know mouse point during render
}

impl Selector {
    fn colboxes_vec(&self) -> Vec<usize> {
        self.colboxes.iter().cloned().collect()
    }

    fn surfaces_vec(&self) -> Vec<usize> {
        let mut result = vec!();
        let mut prev_i: Option<usize> = None;
        let mut surfaces: Vec<usize> = self.surfaces.iter().map(|x| x.index()).collect();
        surfaces.sort();

        for surface_i in surfaces {
            if let Some(prev_i) = prev_i {
                if prev_i != surface_i {
                    result.push(surface_i)
                }
            }
            else {
                result.push(surface_i)
            }
            prev_i = Some(surface_i);
        }
        result
    }

    fn start(&mut self, mouse: (f32, f32)) {
        self.point  = Some(mouse);
        self.moving = false;
        self.mouse  = None;
    }

    fn clear(&mut self) {
        self.colboxes.clear();
        self.surfaces.clear();
        self.spawn_points.clear();
        self.respawn_points.clear();
    }

    /// Returns a selection rect iff a multiple selection is finished.
    fn step_multiple_selection(&mut self, os_input: &WinitInputHelper, camera: &Camera) -> Option<Rect> {
        let game_mouse = os_input.mouse().and_then(|point| camera.mouse_to_game(point));
        // start selection
        if os_input.mouse_pressed(1) {
            if let Some(mouse) = game_mouse {
                self.start(mouse);
            }
        }

        // finish selection
        if let (Some(p1), Some(p2)) = (self.point, game_mouse) {
            if os_input.mouse_released(1) {
                if !(os_input.held_shift() || os_input.held_alt()) {
                    self.clear();
                }
                return Some(Rect::from_tuples(p1, p2))
            }
        }
        None
    }

    /// Returns a selection point iff a single selection is made.
    fn step_single_selection(&mut self, os_input: &WinitInputHelper, camera: &Camera) -> Option<(f32, f32)> {
        if os_input.mouse_pressed(0) {
            if let point @ Some(_) = os_input.mouse().and_then(|point| camera.mouse_to_game(point)) {
                if !(os_input.held_shift() || os_input.held_alt()) {
                    self.clear();
                }
                return point;
            }
        }
        None
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Node)]
pub enum SurfaceSelection {
    P1 (usize),
    P2 (usize)
}

impl SurfaceSelection {
    fn index(&self) -> usize {
        match self {
            &SurfaceSelection::P1 (index) |
            &SurfaceSelection::P2 (index) => index
        }
    }
}

impl Default for SurfaceSelection {
    fn default() -> SurfaceSelection {
        SurfaceSelection::P1 (0)
    }
}

pub struct RenderGame {
    pub seed:              [u8; 32],
    pub current_frame:     usize,
    pub surfaces:          Vec<Surface>,
    pub selected_surfaces: HashSet<SurfaceSelection>,
    pub render_stage_mode: RenderStageMode,
    pub stage_model_name:  String,
    pub entities:          Vec<RenderObject>,
    pub state:             GameState,
    pub camera:            Camera,
    pub debug_lines:       Vec<String>,
    pub timer:             Option<Duration>,
}

pub enum RenderObject {
    Entity      (RenderEntity),
    RectOutline (RenderRect),
    SpawnPoint  (RenderSpawnPoint),
}

impl RenderObject {
    pub fn rect_outline(rect: Rect, r: f32, g: f32, b: f32) -> RenderObject {
        RenderObject::RectOutline (
            RenderRect {
                rect,
                color: [r, g, b, 1.0]
            }
        )
    }

    pub fn spawn_point(point: SpawnPoint, r: f32, g: f32, b: f32) -> RenderObject {
        RenderObject::SpawnPoint (
            RenderSpawnPoint {
                x: point.x,
                y: point.y,
                face_right: point.face_right,
                color: [r, g, b, 1.0]
            }
        )
    }
}

pub struct RenderRect {
    pub rect:  Rect,
    pub color: [f32; 4]
}

pub struct RenderSpawnPoint {
    pub x: f32,
    pub y: f32,
    pub face_right: bool,
    pub color: [f32; 4]
}

#[derive(Clone)]
pub struct GameSetup {
    pub init_seed:              u64,
    pub input_history:          Vec<Vec<ControllerInput>>,
    pub entity_history:         Vec<Entities>,
    pub stage_history:          Vec<Stage>,
    pub controllers:            Vec<usize>,
    pub players:                Vec<PlayerSetup>,
    pub ais:                    Vec<usize>,
    pub stage:                  String,
    pub state:                  GameState,
    pub rules:                  Rules,
    pub debug:                  bool,
    pub max_history_frames:     Option<usize>,
    pub deleted_history_frames: usize,
    pub current_frame:          usize,
    pub camera:                 Camera,
    pub debug_stage:            Option<DebugStage>,
    pub debug_entities:         Option<DebugEntities>,
    // TODO: lets not have hot_reload specific fields here
    //       or maybe we should even rewrite to have a single Option<HotReload> field
    pub hot_reload_entities:    Option<Entities>,
    pub hot_reload_stage:       Option<Stage>,
    pub edit:                   Edit,
}

impl GameSetup {
    pub fn gen_seed() -> u64 {
        Local::now().timestamp() as u64
    }
}

#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct PlayerSetup {
    pub fighter: String,
    pub team:    usize,
}

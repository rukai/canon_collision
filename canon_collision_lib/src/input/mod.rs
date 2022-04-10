mod filter;
pub mod gcadapter;
pub mod generic;
pub mod maps;
pub mod state;

use gcadapter::GCAdapter;
use generic::GenericController;
use maps::ControllerMaps;
use state::{Button, ControllerInput, Deadzone, PlayerInput, Stick, Trigger};

use gilrs_core::{Event, Gilrs};
use rusb::Context;

use crate::network::{Netplay, NetplayState};

enum InputSource {
    GCAdapter(GCAdapter),
    GenericController(GenericController),
}

pub struct Input {
    // game past and (potentially) future inputs, frame 0 has index 2
    // structure: frames Vec<controllers Vec<ControllerInput>>
    game_inputs: Vec<Vec<ControllerInput>>,
    current_inputs: Vec<ControllerInput>, // inputs for this frame
    prev_start: bool,
    input_sources: Vec<InputSource>,
    _rusb_context: Context,
    gilrs: Gilrs,
    controller_maps: ControllerMaps,
    pub events: Vec<Event>,
}

// In/Out is from perspective of computer
// Out means: computer->adapter
// In means:  adapter->computer

impl Input {
    pub fn new() -> Input {
        let mut _rusb_context = Context::new().unwrap();
        let gilrs = Gilrs::new().unwrap();
        let controller_maps = ControllerMaps::load();
        let input_sources = GCAdapter::get_adapters(&mut _rusb_context)
            .into_iter()
            .map(InputSource::GCAdapter)
            .collect();

        Input {
            game_inputs: vec![],
            current_inputs: vec![],
            events: vec![],
            prev_start: false,
            input_sources,
            _rusb_context,
            gilrs,
            controller_maps,
        }
    }

    /// Call this once every frame
    pub fn step(
        &mut self,
        tas_inputs: &[ControllerInput],
        ai_inputs: &[ControllerInput],
        netplay: &mut Netplay,
        reset_deadzones: bool,
    ) {
        // clear deadzones so they will be set at next read
        // TODO: uh why is this even like this, surely just implement deadzone reset on controller plugin (maybe it was for netplay???)
        if reset_deadzones {
            for source in &mut self.input_sources {
                match source {
                    &mut InputSource::GCAdapter(_) => {} // TODO: send message to thread or delete all manual reset_deadzone logic adapter.deadzones = Deadzone::empty4()
                    &mut InputSource::GenericController(ref mut controller) => {
                        controller.deadzone = Deadzone::empty()
                    }
                }
            }
        }

        self.events.clear();
        while let Some(ev) = self.gilrs.next_event() {
            self.events.push(ev);
        }
        self.events.sort_by_key(|x| x.time);

        let mut generic_controllers = vec![];
        for input_source in &self.input_sources {
            if let InputSource::GenericController(controller) = input_source {
                generic_controllers.push(controller);
            }
        }
        for controller in GenericController::get_controllers(&mut self.gilrs, &generic_controllers)
        {
            self.input_sources
                .push(InputSource::GenericController(controller));
        }

        // read input from controllers
        let mut inputs: Vec<ControllerInput> = Vec::new();
        for source in &mut self.input_sources {
            match source {
                InputSource::GCAdapter(ref mut adapter) => {
                    inputs.extend_from_slice(adapter.get_inputs());
                }
                InputSource::GenericController(ref mut controller) => {
                    let events = self
                        .events
                        .iter()
                        .filter(|x| x.id == controller.index)
                        .map(|x| &x.event)
                        .cloned()
                        .collect();
                    let gamepad = &self.gilrs.gamepad(controller.index).unwrap(); // Old gamepads stick around forever so its fine to unwrap.
                    let maps = &self.controller_maps.maps;
                    inputs.push(controller.read(maps, events, gamepad));
                }
            }
        }

        if netplay.skip_frame() {
            // TODO: combine the skipped frames input with the next frame:
            // * average float values
            // * detect dropped presses and include the press
        } else {
            netplay.send_controller_inputs(inputs.clone());
        }

        // append AI inputs
        inputs.extend_from_slice(ai_inputs);

        if let NetplayState::Offline = netplay.state() {
            // replace tas inputs
            for i in 0..tas_inputs.len().min(inputs.len()) {
                inputs[i] = tas_inputs[i];
            }
        }

        self.prev_start = self.current_inputs.iter().any(|x| x.start);
        self.current_inputs = inputs;

        debug!("step");
    }

    /// Reset the game input history
    pub fn reset_history(&mut self) {
        self.game_inputs.clear();
        self.prev_start = false;
    }

    /// Set the game input history
    pub fn set_history(&mut self, history: Vec<Vec<ControllerInput>>) {
        self.game_inputs = history;
    }

    /// Get the game input history
    pub fn get_history(&self) -> Vec<Vec<ControllerInput>> {
        self.game_inputs.clone()
    }

    /// Call this once from the game update logic only
    /// Throws out all future history that may exist
    pub fn game_update(&mut self, frame: usize) {
        for _ in frame..=self.game_inputs.len() {
            self.game_inputs.pop();
        }

        self.game_inputs.push(self.current_inputs.clone());
    }

    /// Call this once from netplay game/menu update logic only (instead of game_update)
    pub fn netplay_update(&mut self) {
        self.game_inputs.push(self.current_inputs.clone());
    }

    /// Return game inputs at specified index into history
    pub fn players_no_log(&self, frame: usize, netplay: &Netplay) -> Vec<PlayerInput> {
        let mut result_inputs: Vec<PlayerInput> = vec![];

        let local_index = netplay.local_index();
        let mut peer_offset = 0;
        let peers_inputs = &netplay.confirmed_inputs;
        for i in 0..netplay.number_of_peers() {
            if i == local_index {
                peer_offset = 1;

                for i in 0..self.current_inputs.len() {
                    let inputs = self.get_8frames_of_input(&self.game_inputs, i, frame as i64);
                    result_inputs.push(Input::controller_inputs_to_player_input(inputs));
                }
            } else {
                let peer_inputs = &peers_inputs[i - peer_offset];
                let num_controllers = peer_inputs.last().map_or(0, |x| x.len());
                for i in 0..num_controllers {
                    let inputs =
                        self.get_8frames_of_input(&peer_inputs[..], i, netplay.frame() as i64);
                    result_inputs.push(Input::controller_inputs_to_player_input(inputs));
                }
            }
        }

        result_inputs
    }

    /// Return game inputs at specified index into history
    pub fn players(&self, frame: usize, netplay: &Netplay) -> Vec<PlayerInput> {
        let result_inputs = self.players_no_log(frame, netplay);

        debug!("players()");
        for (i, input) in result_inputs.iter().enumerate() {
            debug!(
                "    #{} a: {} b: {} input.stick_x: {} input.stick_y: {}",
                i, input.a.value, input.b.value, input.stick_x.value, input.stick_y.value
            );
        }

        result_inputs
    }

    #[rustfmt::skip]
    fn controller_inputs_to_player_input(inputs: Vec<ControllerInput>) -> PlayerInput {
        if inputs[0].plugged_in {
            PlayerInput {
                plugged_in: true,

                up:    Button { value: inputs[0].up,    press: inputs[0].up    && !inputs[1].up },
                down:  Button { value: inputs[0].down,  press: inputs[0].down  && !inputs[1].down },
                right: Button { value: inputs[0].right, press: inputs[0].right && !inputs[1].right },
                left:  Button { value: inputs[0].left,  press: inputs[0].left  && !inputs[1].left },
                y:     Button { value: inputs[0].y,     press: inputs[0].y     && !inputs[1].y },
                x:     Button { value: inputs[0].x,     press: inputs[0].x     && !inputs[1].x },
                b:     Button { value: inputs[0].b,     press: inputs[0].b     && !inputs[1].b },
                a:     Button { value: inputs[0].a,     press: inputs[0].a     && !inputs[1].a },
                l:     Button { value: inputs[0].l,     press: inputs[0].l     && !inputs[1].l },
                r:     Button { value: inputs[0].r,     press: inputs[0].r     && !inputs[1].r },
                z:     Button { value: inputs[0].z,     press: inputs[0].z     && !inputs[1].z },
                start: Button { value: inputs[0].start, press: inputs[0].start && !inputs[1].start },

                stick_x:   Stick { value: inputs[0].stick_x,   diff: inputs[0].stick_x   - inputs[1].stick_x },
                stick_y:   Stick { value: inputs[0].stick_y,   diff: inputs[0].stick_y   - inputs[1].stick_y },
                c_stick_x: Stick { value: inputs[0].c_stick_x, diff: inputs[0].c_stick_x - inputs[1].c_stick_x },
                c_stick_y: Stick { value: inputs[0].c_stick_y, diff: inputs[0].c_stick_y - inputs[1].c_stick_y },

                l_trigger:  Trigger { value: inputs[0].l_trigger, diff: inputs[0].l_trigger - inputs[1].l_trigger },
                r_trigger:  Trigger { value: inputs[0].r_trigger, diff: inputs[0].r_trigger - inputs[1].r_trigger },
                history: inputs,
            }
        }
        else {
            PlayerInput::empty()
        }
    }

    /// converts frames Vec<controllers Vec<ControllerInput>> into frames Vec<ControllerInput> for the specified controller_i
    /// Output must be 8 frames long, any missing frames due to either netplay lag or the game just starting are filled in
    fn get_8frames_of_input(
        &self,
        game_inputs: &[Vec<ControllerInput>],
        controller_i: usize,
        frame: i64,
    ) -> Vec<ControllerInput> {
        let mut result: Vec<ControllerInput> = vec![];
        let empty_vec = vec![];

        for frame_i in (frame - 8..frame).rev() {
            result.push(if frame_i < 0 {
                ControllerInput::empty()
            } else {
                let controllers = match game_inputs.get(frame_i as usize) {
                    Some(controllers) => controllers,
                    None => game_inputs.last().unwrap_or(&empty_vec),
                };
                match controllers.get(controller_i) {
                    Some(value) => *value,
                    None => ControllerInput::empty(),
                }
            });
        }

        assert!(
            result.len() == 8,
            "get_8frames_of_input needs to return a vector of size 8 but it was {}",
            result.len()
        );
        result
    }

    /// Returns the index to the last frame in history
    pub fn last_frame(&self) -> usize {
        self.game_inputs.len() - 1
    }

    /// The player input history system cannot be used when the game is paused (or it would create bogus entries into the history)
    /// Instead we need to create custom functions for handling input when paused.

    /// Check for start button press
    pub fn start_pressed(&mut self) -> bool {
        !self.prev_start && self.current_inputs.iter().any(|x| x.start)
    }

    /// button combination for quiting the game
    pub fn game_quit_held(&mut self) -> bool {
        self.current_inputs
            .iter()
            .any(|x| x.a && x.l && x.r && x.start)
            && self.start_pressed()
    }
}

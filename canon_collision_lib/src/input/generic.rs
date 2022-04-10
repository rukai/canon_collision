use std::f32;

use gilrs_core::{EvCode, EventType, Gamepad, Gilrs};
use uuid::Uuid;

use super::filter;
use super::maps::{AnalogDest, AnalogFilter, ControllerMap, DigitalFilter};
use super::state::{ControllerInput, Deadzone};

pub(crate) struct GenericController {
    pub index: usize,
    pub state: ControllerInput,
    pub deadzone: Deadzone,
}

impl GenericController {
    pub fn get_controllers(
        gilrs: &mut Gilrs,
        existing_controllers: &[&GenericController],
    ) -> Vec<GenericController> {
        let mut controllers = vec![];
        // find new generic controllers
        for index in 0..gilrs.last_gamepad_hint() {
            let gamepad = gilrs.gamepad(index).unwrap();
            if gamepad.is_connected() {
                let exists = existing_controllers.iter().any(|x| x.index == index);

                // Force users to use native GC->Wii U input
                if !exists
                    && gamepad.name() != "mayflash limited MAYFLASH GameCube Controller Adapter"
                {
                    let state = ControllerInput {
                        plugged_in: true,
                        ..ControllerInput::default()
                    };
                    controllers.push(GenericController {
                        index,
                        state,
                        deadzone: Deadzone::empty(),
                    });
                }
            }
        }
        controllers
    }

    /// Add a single controller to inputs, reading from the passed gamepad
    pub fn read(
        &mut self,
        controller_maps: &[ControllerMap],
        events: Vec<EventType>,
        gamepad: &Gamepad,
    ) -> ControllerInput {
        let mut controller_map_use = None;
        for controller_map in controller_maps {
            if controller_map.name == gamepad.name()
                && controller_map.uuid == Uuid::from_bytes(gamepad.uuid())
            {
                controller_map_use = Some(controller_map);
            }
        }

        if let Some(controller_map) = controller_map_use {
            // update internal state
            for event in events {
                match event {
                    // TODO: better handle multiple sources pointing to the same destination
                    // maybe keep a unique ControllerInput state for each source input
                    EventType::ButtonPressed(code) => {
                        for map in &controller_map.analog_maps {
                            if let AnalogFilter::FromDigital { value } = map.filter {
                                if map.source == code_to_usize(&code) {
                                    self.state.set_analog_dest(map.dest.clone(), value);
                                }
                            }
                        }

                        for map in &controller_map.digital_maps {
                            if let DigitalFilter::FromDigital = map.filter {
                                if map.source == code_to_usize(&code) {
                                    self.state.set_digital_dest(map.dest.clone(), true);
                                }
                            };
                        }
                    }
                    EventType::ButtonReleased(code) => {
                        for map in &controller_map.analog_maps {
                            if let AnalogFilter::FromDigital { .. } = map.filter {
                                if map.source == code_to_usize(&code) {
                                    self.state.set_analog_dest(map.dest.clone(), 0.0);
                                }
                            }
                        }

                        for map in &controller_map.digital_maps {
                            if let DigitalFilter::FromDigital = map.filter {
                                if map.source == code_to_usize(&code) {
                                    self.state.set_digital_dest(map.dest.clone(), false);
                                }
                            };
                        }
                    }
                    EventType::AxisValueChanged(value, code) => {
                        for map in &controller_map.analog_maps {
                            if let AnalogFilter::FromAnalog { min, max, flip } = map.filter {
                                // Implemented as per https://stackoverflow.com/questions/345187/math-mapping-numbers
                                let mut new_value =
                                    ((value - min) as f32) / ((max - min) as f32) * 2.0 - 1.0;

                                new_value *= if flip { -1.0 } else { 1.0 };

                                match &map.dest {
                                    &AnalogDest::LTrigger | &AnalogDest::RTrigger => {
                                        new_value = (new_value + 1.0) / 2.0;
                                    }
                                    _ => {}
                                }

                                if map.source == code_to_usize(&code) {
                                    self.state.set_analog_dest(map.dest.clone(), new_value);
                                }
                            };
                        }

                        for map in &controller_map.digital_maps {
                            if let DigitalFilter::FromAnalog { min, max } = map.filter {
                                let value = value >= min && value <= max;

                                if map.source == code_to_usize(&code) {
                                    self.state.set_digital_dest(map.dest.clone(), value);
                                }
                            };
                        }
                    }
                    EventType::Connected => {
                        self.state.plugged_in = true;
                    }
                    EventType::Disconnected => {
                        self.state.plugged_in = false;
                    }
                }
            }

            // convert state floats to bytes
            let raw_stick_x = GenericController::generic_to_byte(self.state.stick_x);
            let raw_stick_y = GenericController::generic_to_byte(self.state.stick_y);
            let raw_c_stick_x = GenericController::generic_to_byte(self.state.c_stick_x);
            let raw_c_stick_y = GenericController::generic_to_byte(self.state.c_stick_y);

            let raw_l_trigger = GenericController::generic_to_byte(self.state.l_trigger);
            let raw_r_trigger = GenericController::generic_to_byte(self.state.r_trigger);

            // update deadzones
            if self.state.plugged_in && !self.deadzone.plugged_in {
                // Only reset deadzone if controller was just plugged in
                self.deadzone = Deadzone {
                    plugged_in: true,
                    stick_x: raw_stick_x,
                    stick_y: raw_stick_y,
                    c_stick_x: raw_c_stick_x,
                    c_stick_y: raw_c_stick_y,
                    l_trigger: raw_l_trigger,
                    r_trigger: raw_r_trigger,
                };
            }
            if !self.state.plugged_in {
                self.deadzone = Deadzone::empty();
            }

            // convert bytes to result floats
            let (stick_x, stick_y) = filter::stick_filter(
                filter::stick_deadzone(raw_stick_x, self.deadzone.stick_x),
                filter::stick_deadzone(raw_stick_y, self.deadzone.stick_y),
            );
            let (c_stick_x, c_stick_y) = filter::stick_filter(
                filter::stick_deadzone(raw_c_stick_x, self.deadzone.c_stick_x),
                filter::stick_deadzone(raw_c_stick_y, self.deadzone.c_stick_y),
            );

            let l_trigger =
                filter::trigger_filter(raw_l_trigger.saturating_sub(self.deadzone.l_trigger));
            let r_trigger =
                filter::trigger_filter(raw_r_trigger.saturating_sub(self.deadzone.r_trigger));

            ControllerInput {
                stick_x,
                stick_y,
                c_stick_x,
                c_stick_y,
                l_trigger,
                r_trigger,
                ..self.state.clone()
            }
        } else {
            ControllerInput::default()
        }
    }

    fn generic_to_byte(value: f32) -> u8 {
        (value.min(1.0).max(-1.0) * 127.0 + 127.0) as u8
    }
}

// gilrs returns the code as a u32 in the following formats
// Linux:
// *   16 bytes - kind
// *   16 bytes - code
// Windows:
// *   24 bytes - padding
// *   8 bytes  - code

// On linux we only need the code so we strip out the kind, so the numbers are nicer to work with (when creating maps)
pub fn code_to_usize(code: &EvCode) -> usize {
    (code.into_u32() & 0xFFFF) as usize
}

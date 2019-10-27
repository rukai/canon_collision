use std::ops::Index;
use treeflection::{Node, NodeRunner, NodeToken};

use super::maps::{AnalogDest, DigitalDest};

impl ControllerInput {
    pub fn empty() -> ControllerInput {
        ControllerInput {
            plugged_in: false,

            up:    false,
            down:  false,
            right: false,
            left:  false,
            y:     false,
            x:     false,
            b:     false,
            a:     false,
            l:     false,
            r:     false,
            z:     false,
            start: false,

            stick_x:   0.0,
            stick_y:   0.0,
            c_stick_x: 0.0,
            c_stick_y: 0.0,
            l_trigger: 0.0,
            r_trigger: 0.0,
        }
    }

    pub fn stick_angle(&self) -> Option<f32> {
        if self.stick_x == 0.0 && self.stick_y == 0.0 {
            None
        } else {
            Some(self.stick_y.atan2(self.stick_x))
        }
    }

    #[allow(dead_code)]
    pub fn c_stick_angle(&self) -> Option<f32> {
        if self.stick_x == 0.0 && self.stick_y == 0.0 {
            None
        } else {
            Some(self.c_stick_y.atan2(self.c_stick_x))
        }
    }
}

/// Internal input storage
#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct ControllerInput {
    pub plugged_in: bool,

    pub a:     bool,
    pub b:     bool,
    pub x:     bool,
    pub y:     bool,
    pub left:  bool,
    pub right: bool,
    pub down:  bool,
    pub up:    bool,
    pub start: bool,
    pub z:     bool,
    pub r:     bool,
    pub l:     bool,

    pub stick_x:   f32,
    pub stick_y:   f32,
    pub c_stick_x: f32,
    pub c_stick_y: f32,
    pub r_trigger: f32,
    pub l_trigger: f32,
}

impl ControllerInput {
    pub(crate) fn set_analog_dest(&mut self, analog_dest: AnalogDest, value: f32) {
        match analog_dest {
            AnalogDest::StickX   => { self.stick_x = value }
            AnalogDest::StickY   => { self.stick_y = value }
            AnalogDest::CStickX  => { self.c_stick_x = value }
            AnalogDest::CStickY  => { self.c_stick_y = value }
            AnalogDest::RTrigger => { self.l_trigger = value }
            AnalogDest::LTrigger => { self.r_trigger = value }
        }
    }

    pub(crate) fn set_digital_dest(&mut self, analog_dest: DigitalDest, value: bool) {
        match analog_dest {
            DigitalDest::A     => { self.a = value }
            DigitalDest::B     => { self.b = value }
            DigitalDest::X     => { self.x = value }
            DigitalDest::Y     => { self.y = value }
            DigitalDest::Left  => { self.left = value }
            DigitalDest::Right => { self.right = value }
            DigitalDest::Down  => { self.down = value }
            DigitalDest::Up    => { self.up = value }
            DigitalDest::Start => { self.start = value }
            DigitalDest::Z     => { self.z = value }
            DigitalDest::R     => { self.r = value }
            DigitalDest::L     => { self.l = value }
        }
    }
}

/// External data access
pub struct PlayerInput {
    pub plugged_in: bool,

    pub a:     Button,
    pub b:     Button,
    pub x:     Button,
    pub y:     Button,
    pub left:  Button,
    pub right: Button,
    pub down:  Button,
    pub up:    Button,
    pub start: Button,
    pub z:     Button,
    pub r:     Button,
    pub l:     Button,

    pub stick_x:   Stick,
    pub stick_y:   Stick,
    pub c_stick_x: Stick,
    pub c_stick_y: Stick,
    pub r_trigger:  Trigger,
    pub l_trigger:  Trigger,
    pub history: Vec<ControllerInput>, // guaranteed to contain 8 elements
}

impl PlayerInput {
    pub fn empty() -> PlayerInput {
        PlayerInput {
            plugged_in: false,

            up:    Button { value: false, press: false },
            down:  Button { value: false, press: false },
            right: Button { value: false, press: false },
            left:  Button { value: false, press: false },
            y:     Button { value: false, press: false },
            x:     Button { value: false, press: false },
            b:     Button { value: false, press: false },
            a:     Button { value: false, press: false },
            l:     Button { value: false, press: false },
            r:     Button { value: false, press: false },
            z:     Button { value: false, press: false },
            start: Button { value: false, press: false },

            stick_x:   Stick { value: 0.0, diff: 0.0 },
            stick_y:   Stick { value: 0.0, diff: 0.0 },
            c_stick_x: Stick { value: 0.0, diff: 0.0 },
            c_stick_y: Stick { value: 0.0, diff: 0.0 },

            l_trigger:  Trigger { value: 0.0, diff: 0.0 },
            r_trigger:  Trigger { value: 0.0, diff: 0.0 },
            history: vec!(ControllerInput::empty(); 8),
        }
    }
}


impl Index<usize> for PlayerInput {
    type Output = ControllerInput;

    fn index(&self, index: usize) -> &ControllerInput {
        &self.history[index]
    }
}

// TODO: now that we have history we could remove the value from these, turning them into primitive values

pub struct Button {
    pub value: bool, // on
    pub press: bool, // off->on this frame
}

pub struct Stick {
    pub value: f32, // current.value
    pub diff:  f32, // current.value - previous.value
}

pub struct Trigger {
    pub value: f32, // current.value
    pub diff:  f32, // current.value - previous.value
}

/// Stores the first value returned from an input source
pub struct Deadzone {
    pub plugged_in: bool,
    pub stick_x:    u8,
    pub stick_y:    u8,
    pub c_stick_x:  u8,
    pub c_stick_y:  u8,
    pub l_trigger:  u8,
    pub r_trigger:  u8,
}

impl Deadzone {
    pub fn empty() -> Self {
        Deadzone {
            plugged_in: false,
            stick_x:    0,
            stick_y:    0,
            c_stick_x:  0,
            c_stick_y:  0,
            l_trigger:  0,
            r_trigger:  0,
        }
    }

    pub fn empty4() -> [Self; 4] {
        [
            Deadzone::empty(),
            Deadzone::empty(),
            Deadzone::empty(),
            Deadzone::empty()
        ]
    }
}

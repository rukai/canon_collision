use crate::entity::EntityKey;

use canon_collision_lib::entity_def::{EntityDef, ActionFrame};

use num_traits::{FromPrimitive, ToPrimitive};
use rand_chacha::ChaChaRng;
use rand::Rng;

#[derive(Clone, Serialize, Deserialize)]
pub struct ActionState {
    pub entity_def_key:    String,
    pub action:            u64,
    pub frame:             i64, // TODO: u64
    pub frame_no_restart:  i64,
    pub hitlist:           Vec<EntityKey>,
    pub hitlag:            Hitlag,
}

impl ActionState {
    pub fn new<T: ToPrimitive>(entity_def_key: String, action: T) -> ActionState {
        ActionState {
            entity_def_key,
            action:           action.to_u64().unwrap(),
            frame:            0,
            frame_no_restart: 0,
            hitlist:          vec!(),
            hitlag:           Hitlag::None,
        }
    }
    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        if entity_def.actions.len() > self.action as usize {
            let frames = &entity_def.actions[self.action as usize].frames;
            if frames.len() > self.frame as usize {
                return Some(&frames[self.frame as usize]);
            }
        }
        None
    }

    pub fn debug_string<T: FromPrimitive + std::fmt::Debug>(&self, entity_def: &EntityDef, index: EntityKey) -> String {
        let action = T::from_u64(self.action).unwrap();
        let last_action_frame = entity_def.actions[self.action as usize].frames.len() as u64 - 1;
        let iasa = entity_def.actions[self.action as usize].iasa;

        format!("Entity: {:?}  hitlag: {:?}  action: {:?}  frame: {}/{}  frame no restart: {}  IASA: {}",
            index, self.hitlag, action, self.frame, last_action_frame, self.frame_no_restart, iasa)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Hitlag {
    Attack { counter: u64 },
    Launch { counter: u64, wobble_x: f32 },
    None
}

impl Hitlag {
    pub fn step(&mut self, rng: &mut ChaChaRng) {
        match self {
            &mut Hitlag::Attack { ref mut counter} => {
                *counter -= 1;
                if *counter == 0 {
                    *self = Hitlag::None;
                }
            }
            &mut Hitlag::Launch { ref mut counter, ref mut wobble_x } => {
                *wobble_x = (rng.gen::<f32>() - 0.5) * 3.0;
                *counter -= 1;
                if *counter == 0 {
                    *self = Hitlag::None;
                }
            }
            &mut Hitlag::None => { }
        }
    }
}


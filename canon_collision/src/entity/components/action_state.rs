use crate::entity::EntityKey;

use canon_collision_lib::entity_def::{EntityDef, ActionFrame};

use rand::Rng;
use rand_chacha::ChaChaRng;
use treeflection::KeyedContextVec;

use std::str::FromStr;

#[derive(Clone, Serialize, Deserialize)]
pub struct ActionState {
    // If I need to, I could make lookups for entity_def_key and action more effecient by storing as a usize, by calling actions.key_to_index
    pub entity_def_key:   String,
    pub action:           String,
    pub frame:            i64, // TODO: u64
    pub frame_no_restart: i64,
    pub hitlist:          Vec<EntityKey>,
    pub hitlag:           Hitlag,
}

impl ActionState {
    pub fn new<T: Into<&'static str>>(entity_def_key: String, action: T) -> ActionState {
        ActionState {
            entity_def_key,
            action:           action.into().to_string(),
            frame:            0,
            frame_no_restart: 0,
            hitlist:          vec!(),
            hitlag:           Hitlag::None,
        }
    }

    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        if entity_def.actions.contains_key(&self.action) {
            let frames = &entity_def.actions[self.action.as_ref()].frames;
            if frames.len() > self.frame as usize {
                return Some(&frames[self.frame as usize]);
            }
        }
        None
    }

    pub fn interruptible(&self, entity_def: &EntityDef) -> bool {
        self.frame >= entity_def.actions[self.action.as_ref()].iasa
    }

    pub fn first_interruptible(&self, entity_def: &EntityDef) -> bool {
        self.frame == entity_def.actions[self.action.as_ref()].iasa
    }

    pub fn last_frame(&self, entity_def: &EntityDef) -> bool {
        self.frame >= entity_def.actions[self.action.as_ref()].frames.len() as i64 - 1
    }

    pub fn past_last_frame(&self, entity_def: &EntityDef) -> bool {
        self.frame >= entity_def.actions[self.action.as_ref()].frames.len() as i64
    }

    pub fn debug_string(&self, entity_defs: &KeyedContextVec<EntityDef>, index: EntityKey) -> String {
        let entity_def = &entity_defs[self.entity_def_key.as_ref()];
        let action = &entity_def.actions[self.action.as_ref()];
        let last_action_frame = action.frames.len() as u64 - 1;
        let iasa = action.iasa;

        format!("Entity: {:?}  \"{}\"  hitlag: {:?}  action: {}  frame: {}/{}  frame no restart: {}  IASA: {}",
            index, self.entity_def_key, self.hitlag, self.action, self.frame, last_action_frame, self.frame_no_restart, iasa)
    }

    pub fn get_action<T: FromStr>(&self) -> Option<T> {
        T::from_str(self.action.as_ref()).ok()
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


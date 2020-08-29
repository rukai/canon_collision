use canon_collision_lib::entity_def::{EntityDef, ActionFrame};

use crate::entity::{EntityKey, StepContext};

use num_traits::{FromPrimitive, ToPrimitive};
use std::marker::PhantomData;

pub struct ActionState<T: FromPrimitive + ToPrimitive> {
    pub action:            u64,
    pub set_action_called: bool,
    pub new_action:        bool,

    // frame count values:
    // == -1 doesnt correspond to a frame in the fighter data, used for the action logic triggered directly after action state transition, must never be in this state after action_step()
    // >=  0  corresponds to a frame in the fighter data, used for the regular action logic step on each game frame
    pub frame:             i64,
    pub frame_no_restart:  i64,

    pub hitlist:           Vec<EntityKey>,
    pub _action_type:      PhantomData<T>,
}

impl <T: FromPrimitive + ToPrimitive> ActionState<T> {
    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        if entity_def.actions.len() > self.action as usize {
            let frames = &entity_def.actions[self.action as usize].frames;
            if frames.len() > self.frame as usize {
                return Some(&frames[self.frame as usize]);
            }
        }
        None
    }

    pub fn set_action(&mut self, context: &StepContext, action: T) {
        let action = action.to_u64().unwrap();
        self.frame = 0;
        self.hitlist.clear();
        self.set_action_called = true;

        if self.action != action {
            self.frame = -1;
            self.frame_no_restart = -1;
            self.action = action;

            //self.frame_step(context); // TODO: Maybe store this in a variable to use later?
            let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as i64 - 1;
            // TODO: move this assert somewhere earlier, maybe the fighter loading code?
            assert_ne!(last_action_frame, -1, "A subaction has a length of 0");
            self.frame = last_action_frame.min(self.frame + 1); // +1 instead of =0 so that frame_step can skip frames if it wants to
            self.frame_no_restart += 1;
        }
    }

    pub fn frame_step(&mut self) {
        // TODO: lots of other stuff

        if !self.set_action_called { // action_expired() can call set_action()
            self.frame += 1;
        }
        if !self.new_action {
            self.frame_no_restart += 1;
        }
    }

    pub fn get_action(&self) -> Option<T> {
        T::from_u64(self.action)
    }
}

enum ActionResult {
    SetAction(u64),
    SetFrame(u64),
}

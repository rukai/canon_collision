use canon_collision_lib::fighter::{Fighter, ActionFrame};

use treeflection::{Node, NodeRunner, NodeToken};

#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct SimpleProjectile {
    pub entity_def_key: String,
    pub action: u64,
    pub frame: i64,
    pub angle: f32,
    pub x: f32,
    pub y: f32,
}

impl SimpleProjectile {
    pub fn get_fighter_frame<'a>(&self, fighter: &'a Fighter) -> Option<&'a ActionFrame> {
        if fighter.actions.len() > self.action as usize {
            let fighter_frames = &fighter.actions[self.action as usize].frames;
            if fighter_frames.len() > self.frame as usize {
                return Some(&fighter_frames[self.frame as usize]);
            }
        }
        None
    }
}

use crate::entity::EntityKey;

// Describes the player location by offsets from other locations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Location {
    Surface { platform_i: usize, x: f32 },
    GrabbedLedge { platform_i: usize, d_x: f32, d_y: f32, logic: LedgeLogic }, // player.face_right determines which edge on the platform
    GrabbedByPlayer (EntityKey),
    Airbourne { x: f32, y: f32 },
}

impl Location {
    pub fn public_bps_xy(&self, entities: &Entities, fighters: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        let bps_xy = match self.location {
            Location::Surface { platform_i, x } => {
                if let Some(platform) = surfaces.get(platform_i) {
                    platform.plat_x_to_world_p(x)
                } else {
                    (0.0, 0.0)
                }
            }
            Location::GrabbedLedge { platform_i, d_x, d_y, .. } => {
                if let Some(platform) = surfaces.get(platform_i) {
                    let (ledge_x, ledge_y) = if self.face_right {
                        platform.left_ledge()
                    } else {
                        platform.right_ledge()
                    };
                    (ledge_x + self.relative_f(d_x), ledge_y + d_y)
                } else {
                    (0.0, 0.0)
                }
            }
            Location::GrabbedByPlayer (entity_i) => {
                if let Some(player) = entities.get(entity_i) {
                    if let Some(fighter_frame) = self.get_entity_frame(&fighters[self.entity_def_key.as_ref()]) {
                        let (grabbing_x, grabbing_y) = player.grabbing_xy(entities, fighters, surfaces);
                        let grabbed_x = self.relative_f(fighter_frame.grabbed_x);
                        let grabbed_y = fighter_frame.grabbed_y;
                        (grabbing_x - grabbed_x, grabbing_y - grabbed_y)
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                }
            }
            Location::Airbourne { x, y } => {
                (x, y)
            }
        };

        match &self.hitlag {
            &Hitlag::Launch { wobble_x, .. } => {
                (bps_xy.0 + wobble_x, bps_xy.1)
            }
            _ => {
                bps_xy
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedgeLogic {
    Hog,
    Share,
    Trump
}


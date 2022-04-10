use cgmath::Vector3;
use std::collections::HashMap;

#[rustfmt::skip]
pub fn get_hurtboxes() -> HashMap<String, Vec<HurtBox>> {
    let mut hurtboxes = HashMap::new();

    hurtboxes.insert(
        "Toriel.cbor".into(),
        vec!(
            HurtBox::new("Hips",       0.0, 2.2, 0.0, 1.0, 0.0),
            HurtBox::new("Waist",      0.0, 2.2, 0.0, 1.2, 0.3),
            HurtBox::new("Chest",      0.0, 2.2, 0.0, 1.6, 0.3),
            HurtBox::new("Head",       0.0, 2.8, 0.0, 1.4, 0.0),
            HurtBox::new("Snout",      0.0, 1.0, 0.0, 1.1, 0.0),

            HurtBox::new("Thigh.L",    4.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("Thigh.R",    4.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("Shin.L",     4.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("Shin.R",     4.0, 1.0, 0.0, 0.0, 0.0),

            HurtBox::new("Shoulder.L", 0.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("Shoulder.R", 0.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("Arm.L",      4.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("Arm.R",      4.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("ForeArm.L",  4.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("ForeArm.R",  4.0, 1.0, 0.0, 0.0, 0.0),
        )
    );

    hurtboxes.insert(
        "Dave.cbor".into(),
        vec!(
            HurtBox::new("Head",      0.0, 1.0, 0.0, 0.0, 0.0),
            HurtBox::new("ForeArm.L", 1.0, 1.0, 0.0, 0.0, 0.0),
        )
    );

    hurtboxes
}

pub struct HurtBox {
    /// The name of the bone the hurtbox is attached to
    pub bone: String,
    /// Multiple hurtboxes are attached along the axis of the bone, every radius a new hurtbox is placed until bone_length
    pub bone_length: f32,
    /// Radius of the hurtbox
    pub radius: f32,
    /// Offset of the hurtbox from the bone, in bone space
    pub offset: Vector3<f32>,
}

impl HurtBox {
    fn new(
        bone: &str,
        bone_length: f32,
        radius: f32,
        offset_x: f32,
        offset_y: f32,
        offset_z: f32,
    ) -> HurtBox {
        HurtBox {
            bone: bone.into(),
            bone_length,
            radius,
            offset: Vector3::new(offset_x, offset_y, offset_z),
        }
    }
}

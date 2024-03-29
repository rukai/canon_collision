#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Particle {
    pub color: [f32; 3],
    pub counter: u32,
    pub counter_max: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub angle: f32,
    pub p_type: ParticleType,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ParticleType {
    AirJump,
    Hit {
        knockback: f32,
        damage: f32,
    },
    Spark {
        x_vel: f32,
        y_vel: f32,
        z_vel: f32,
        size: f32,
        angle_vel: f32,
    },
}

impl Default for ParticleType {
    fn default() -> Self {
        ParticleType::AirJump
    }
}

impl Particle {
    /// returns true if should delete self
    pub fn step(&mut self) -> bool {
        self.counter += 1;
        match self.p_type.clone() {
            ParticleType::Spark {
                x_vel,
                y_vel,
                z_vel,
                angle_vel,
                ..
            } => {
                self.x += x_vel;
                self.y += y_vel;
                self.z += z_vel;
                self.angle += angle_vel;
            }
            _ => {}
        }
        self.counter > self.counter_max
    }

    #[allow(dead_code)] // Needed for headless build
    pub fn counter_mult(&self) -> f32 {
        self.counter as f32 / self.counter_max as f32
    }
}

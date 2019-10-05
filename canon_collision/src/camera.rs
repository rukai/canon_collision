use canon_collision_lib::fighter::Fighter;
use canon_collision_lib::stage::Stage;
use canon_collision_lib::geometry::Rect;
use crate::player::Player;

use cgmath::{Matrix4, Point3, Vector3};
use winit::event::VirtualKeyCode;
use winit_input_helper::Camera as CameraWinitInputHelper;
use winit_input_helper::WinitInputHelper;
use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Node)]
pub struct Camera {
    aspect_ratio:       f32,
    pub zoom:           f32,
    pub pan:            (f32, f32),
    pub rect:           Rect,
    pub control_state:  CameraControlState,
    pub transform_mode: TransformMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Node)]
pub enum CameraControlState {
    Manual,
    Auto,
}

impl Default for CameraControlState {
    fn default() -> Self {
        CameraControlState::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Node)]
pub enum TransformMode {
    Dev,
    Play,
}

impl Default for TransformMode {
    fn default() -> Self {
        TransformMode::Dev
    }
}

impl Camera {
    pub fn new() -> Camera {
        Camera {
            aspect_ratio:   1.0,
            zoom:           100.0,
            pan:            (0.0, 0.0),
            rect:           Rect { x1: -10.0, y1: -10.0, x2: 10.0, y2: 10.0 },
            control_state:  CameraControlState::Auto,
            transform_mode: TransformMode::Dev,
        }
    }

    pub fn update_os_input(&mut self, os_input: &WinitInputHelper<()>) {
        // set manual/automatic camera control
        if os_input.mouse_pressed(2) || os_input.scroll_diff() != 0.0 {
            self.control_state = CameraControlState::Manual;
        }
        else if os_input.key_pressed(VirtualKeyCode::Back) {
            self.control_state = CameraControlState::Auto;
        }

        if let CameraControlState::Manual = self.control_state {
            match self.transform_mode {
                TransformMode::Dev => {
                    // pan camera
                    if os_input.mouse_held(2) {
                        let mouse_diff = os_input.mouse_diff();
                        self.rect.x1 += mouse_diff.0 as f32;
                        self.rect.x2 += mouse_diff.0 as f32;
                        self.rect.y1 += mouse_diff.1 as f32;
                        self.rect.y2 += mouse_diff.1 as f32;
                        println!("x1 {} mouse_diff.0 {}", self.rect.x1, mouse_diff.0);

                        self.pan.0 += mouse_diff.0 as f32;
                        self.pan.1 -= mouse_diff.1 as f32;
                    }

                    // zoom camera
                    self.rect.x1 -= os_input.scroll_diff() * 4.0;
                    self.rect.x2 += os_input.scroll_diff() * 4.0;
                    self.rect.y1 -= os_input.scroll_diff() * 4.0;
                    self.rect.y2 += os_input.scroll_diff() * 4.0;
                }
                TransformMode::Play => unimplemented!(),
            }
        }
    }

    pub fn update(&mut self, os_input: &WinitInputHelper<()>, players: &[Player], fighters: &KeyedContextVec<Fighter>, stage: &Stage) {
        if let CameraControlState::Auto = self.control_state {
            if let Some((width, height)) = os_input.resolution() {
                self.aspect_ratio = width as f32 / height as f32;
            }

            // initialise cam_area using only the first player
            let mut player_iter = players.iter();
            let mut cam_area = match player_iter.next() {
                Some(player) => player.cam_area(&stage.camera, players, fighters, &stage.surfaces),
                None => {
                    self.pan = (0.0, 0.0);
                    self.zoom = 100.0;
                    self.rect = Rect { x1: -10.0, y1: -10.0, x2: 10.0, y2: 10.0 };
                    // TODO: I can use this to debug a specific camera state
                    //self.rect = Rect { x1: -100.0, y1: -100.0, x2: 100.0, y2: 100.0 };
                    //self.rect = Rect { x1: -100.0, y1: -200.0, x2: 100.0, y2: 0.0 };
                    //self.rect = Rect { x1: -100.0, y1: -150.0, x2: 100.0, y2: 50.0 };
                    return;
                }
            };

            // grow cam_area to cover all other players
            for player in player_iter {
                let next_area = player.cam_area(&stage.camera, players, fighters, &stage.surfaces);
                cam_area.x1 = cam_area.x1.min(next_area.left());
                cam_area.x2 = cam_area.x2.max(next_area.right());
                cam_area.y1 = cam_area.y1.min(next_area.bot());
                cam_area.y2 = cam_area.y2.max(next_area.top());
            }

            // grow cam_area to fill aspect ratio
            let mut width  = (cam_area.x1 - cam_area.x2).abs();
            let mut height = (cam_area.y1 - cam_area.y2).abs();
            if width / height > self.aspect_ratio {
                height = width / self.aspect_ratio;

                let avg_vertical = (cam_area.y2 + cam_area.y1) / 2.0;
                cam_area.y2 = avg_vertical + height / 2.0;
                cam_area.y1 = avg_vertical - height / 2.0;
            }
            else {
                width = height * self.aspect_ratio;

                let avg_horizontal = (cam_area.x2 + cam_area.x1) / 2.0;
                cam_area.x2 = avg_horizontal + width / 2.0;
                cam_area.x1 = avg_horizontal - width / 2.0;
            }

            // push aspect_ratio changes back so it doesnt go past the stage camera area
            let cam_max = &stage.camera;
            if cam_area.x1 < cam_max.left() {
                let diff = cam_area.x1 - cam_max.left();
                cam_area.x1 -= diff;
                cam_area.x2 -= diff;
            }
            else if cam_area.x2 > cam_max.right() {
                let diff = cam_area.x2 - cam_max.right();
                cam_area.x1 -= diff;
                cam_area.x2 -= diff;
            }
            if cam_area.y1 < cam_max.bot() {
                let diff = cam_area.y1 - cam_max.bot();
                cam_area.y1 -= diff;
                cam_area.y2 -= diff;
            }
            else if cam_area.y2 > cam_max.top() {
                let diff = cam_area.y2 - cam_max.top();
                cam_area.y1 -= diff;
                cam_area.y2 -= diff;
            }

            // set new camera values
            let dest_pan_x = -((cam_area.x1 + cam_area.x2) / 2.0);
            let dest_pan_y = -((cam_area.y1 + cam_area.y2) / 2.0);
            let dest_zoom = width / 2.0;

            let diff_pan_x = dest_pan_x - self.pan.0;
            let diff_pan_y = dest_pan_y - self.pan.1;
            let diff_zoom = dest_zoom - self.zoom;

            self.pan.0 += diff_pan_x / 10.0;
            self.pan.1 += diff_pan_y / 10.0;
            self.zoom += diff_zoom / 10.0;

            self.rect = cam_area;
        }
    }

    pub fn transform(&self) -> Matrix4<f32> {
        match self.transform_mode {
            TransformMode::Dev => {
                let width = (self.rect.x1 - self.rect.x2).abs();
                let height = (self.rect.x1 - self.rect.x2).abs();
                let proj = cgmath::ortho(-width / 2.0, width / 2.0, -height / 2.0, height / 2.0, -1000000.0, 1000000.0);
                let camera_target = Point3::new(
                    (self.rect.x1 + self.rect.x2) / 2.0,
                    (self.rect.y1 + self.rect.y2) / 2.0,
                    0.0
                );
                let camera_location = Point3::new(
                    (self.rect.x1 + self.rect.x2) / 2.0,
                    (self.rect.y1 + self.rect.y2) / 2.0,
                    1.0
                );
                let view = Matrix4::look_at(camera_location, camera_target, Vector3::new(0.0, 1.0, 0.0));
                let aspect_ratio = Matrix4::from_nonuniform_scale(1.0, self.aspect_ratio, 1.0);
                aspect_ratio * proj * view
            }
            TransformMode::Play => unimplemented!(),
        }
    }

    pub fn for_winit_helper(&self) -> CameraWinitInputHelper {
        CameraWinitInputHelper {
            zoom: self.zoom,
            pan:  self.pan,
        }
    }
}

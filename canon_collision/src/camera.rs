use canon_collision_lib::fighter::Fighter;
use canon_collision_lib::stage::Stage;
use canon_collision_lib::geometry::Rect;
use crate::player::Player;

use cgmath::{Matrix4, Point3, Vector3, Transform};
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;
use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Node)]
pub struct Camera {
    aspect_ratio:       f32,
    window_width:       f32,
    window_height:      f32,
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
            window_width:   1.0,
            window_height:  1.0,
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
                        self.rect.x1 -= mouse_diff.0 as f32;
                        self.rect.x2 -= mouse_diff.0 as f32;
                        self.rect.y1 += mouse_diff.1 as f32;
                        self.rect.y2 += mouse_diff.1 as f32;
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
                self.window_width = width as f32;
                self.window_height = height as f32;
                self.aspect_ratio = width as f32 / height as f32;
            }

            // initialise new_rect using only the first player
            let mut player_iter = players.iter();
            let mut new_rect = match player_iter.next() {
                Some(player) => player.cam_area(&stage.camera, players, fighters, &stage.surfaces),
                None => {
                    self.rect = Rect { x1: -200.0, y1: -200.0, x2: 200.0, y2: 200.0 };
                    return;
                }
            };

            // grow new_rect to cover all other players
            for player in player_iter {
                let next_area = player.cam_area(&stage.camera, players, fighters, &stage.surfaces);
                new_rect.x1 = new_rect.x1.min(next_area.left());
                new_rect.x2 = new_rect.x2.max(next_area.right());
                new_rect.y1 = new_rect.y1.min(next_area.bot());
                new_rect.y2 = new_rect.y2.max(next_area.top());
            }

            // grow new_rect to fill aspect ratio
            let mut width  = (new_rect.x1 - new_rect.x2).abs();
            let mut height = (new_rect.y1 - new_rect.y2).abs();
            if width / height > self.aspect_ratio {
                height = width / self.aspect_ratio;

                let avg_vertical = (new_rect.y2 + new_rect.y1) / 2.0;
                new_rect.y2 = avg_vertical + height / 2.0;
                new_rect.y1 = avg_vertical - height / 2.0;
            }
            else {
                width = height * self.aspect_ratio;

                let avg_horizontal = (new_rect.x2 + new_rect.x1) / 2.0;
                new_rect.x2 = avg_horizontal + width / 2.0;
                new_rect.x1 = avg_horizontal - width / 2.0;
            }

            // push aspect_ratio changes back so it doesnt go past the stage camera area
            let cam_max = &stage.camera;
            if new_rect.x1 < cam_max.left() {
                let diff = new_rect.x1 - cam_max.left();
                new_rect.x1 -= diff;
                new_rect.x2 -= diff;
            }
            else if new_rect.x2 > cam_max.right() {
                let diff = new_rect.x2 - cam_max.right();
                new_rect.x1 -= diff;
                new_rect.x2 -= diff;
            }
            if new_rect.y1 < cam_max.bot() {
                let diff = new_rect.y1 - cam_max.bot();
                new_rect.y1 -= diff;
                new_rect.y2 -= diff;
            }
            else if new_rect.y2 > cam_max.top() {
                let diff = new_rect.y2 - cam_max.top();
                new_rect.y1 -= diff;
                new_rect.y2 -= diff;
            }

            // set new camera values
            let diff_x1 = new_rect.x1 - self.rect.x1;
            let diff_x2 = new_rect.x2 - self.rect.x2;
            let diff_y1 = new_rect.y1 - self.rect.y1;
            let diff_y2 = new_rect.y2 - self.rect.y2;
            self.rect.x1 += diff_x1 / 10.0;
            self.rect.x2 += diff_x2 / 10.0;
            self.rect.y1 += diff_y1 / 10.0;
            self.rect.y2 += diff_y2 / 10.0;
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

    /// Convert a mouse point to the corresponding in game point
    pub fn mouse_to_game(&self, mouse_point: (f32, f32)) -> Option<(f32, f32)> {
        let normalized_x = mouse_point.0 / self.window_width * 2.0 - 1.0;
        let normalized_y = mouse_point.1 / self.window_height * -2.0 + 1.0;
        self.transform()
            .inverse_transform()
            .map(|x| x.transform_point(Point3::new(normalized_x, normalized_y, 0.0)))
            .map(|v| (v.x, v.y))
    }
}

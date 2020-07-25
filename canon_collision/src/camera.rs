use crate::entity::Entity;

use canon_collision_lib::fighter::Fighter;
use canon_collision_lib::stage::Stage;
use canon_collision_lib::geometry::Rect;

use cgmath::{Matrix4, Point3, Vector3, Transform, Rad, Quaternion};
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;
use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec};
use std::f32::consts;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Node)]
pub struct Camera {
    aspect_ratio:       f32,
    window_width:       f32,
    window_height:      f32,
    pub rect:           Rect,
    pub control_state:  CameraControlState,
    pub transform_mode: TransformMode,
    /// Only used when TransformMode::Dev and CameraControlState::Manual
    freelook_location:  (f32, f32, f32),
    /// Uses spherical coordinates to represent the freelook cameras direction
    /// https://en.wikipedia.org/wiki/Spherical_coordinate_system
    /// https://threejs.org/docs/#api/en/math/Spherical
    /// polar angle from the y (up) axis
    freelook_phi: f32,
    /// equator angle around the y (up) axis.
    freelook_theta: f32,
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
            aspect_ratio:      1.0,
            window_width:      1.0,
            window_height:     1.0,
            rect:              Rect { x1: -10.0, y1: -10.0, x2: 10.0, y2: 10.0 },
            control_state:     CameraControlState::Auto,
            transform_mode:    TransformMode::Play,
            freelook_location: (0.0, 0.0, 0.0),
            freelook_phi:      0.0,
            freelook_theta:    0.0,
        }
    }

    pub fn update_os_input(&mut self, os_input: &WinitInputHelper) {
        // set manual/automatic camera control
        if os_input.mouse_pressed(2) || os_input.scroll_diff() != 0.0 ||
            (!self.dev_mode() && (os_input.key_pressed(VirtualKeyCode::W) || os_input.key_pressed(VirtualKeyCode::A) || os_input.key_pressed(VirtualKeyCode::S) || os_input.key_pressed(VirtualKeyCode::D)))
        {
            self.control_state = CameraControlState::Manual;
        }
        else if os_input.key_pressed(VirtualKeyCode::Back) {
            self.control_state = CameraControlState::Auto;
        }
        else if os_input.key_pressed(VirtualKeyCode::Escape) {
            self.transform_mode = match self.transform_mode {
                TransformMode::Dev  => TransformMode::Play,
                TransformMode::Play => TransformMode::Dev,
            };
        }

        match self.control_state {
            CameraControlState::Manual => {
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
                    TransformMode::Play => {
                        // rotate camera
                        if os_input.mouse_held(2) {
                            let mouse_diff = os_input.mouse_diff();
                            self.freelook_theta -= mouse_diff.0 / 300.0;
                            self.freelook_phi   += mouse_diff.1 / 300.0;
                        }

                        // clamp direction
                        let small = 0.00001;
                        if self.freelook_phi > consts::PI - small {
                            self.freelook_phi = consts::PI - small;
                        }
                        else if self.freelook_phi < small {
                            self.freelook_phi = small;
                        }
                        if self.freelook_theta > consts::PI * 2.0 {
                            self.freelook_theta = 0.0;
                        }
                        else if self.freelook_theta < 0.0 {
                            self.freelook_theta = consts::PI * 2.0;
                        }

                        // wasd camera
                        let distance = if os_input.held_shift() { 5.0 } else { 1.0 };
                        let location_inputs = Vector3::new(
                            0.0,
                            0.0,
                            if os_input.key_held(VirtualKeyCode::S) { distance } else if os_input.key_held(VirtualKeyCode::W) { -distance } else { 0.0 },
                        );
                        let rotate_matrix: Matrix4<f32> = Quaternion::from_arc(Vector3::new(0.0, 0.0, -1.0), self.freelook_direction(), None).into();
                        let location_offset = rotate_matrix.transform_vector(location_inputs);
                        self.freelook_location.0 += location_offset.x;
                        self.freelook_location.1 += location_offset.y;
                        self.freelook_location.2 += location_offset.z;

                        let location_inputs = Vector3::new(
                            if os_input.key_held(VirtualKeyCode::A) { distance } else if os_input.key_held(VirtualKeyCode::D) { -distance } else { 0.0 },
                            0.0,
                            0.0,
                        );
                        let rotate_matrix = Matrix4::from_angle_y(Rad(self.freelook_theta));
                        let location_offset = rotate_matrix.transform_vector(location_inputs);
                        self.freelook_location.0 += location_offset.x;
                        self.freelook_location.1 += location_offset.y;
                        self.freelook_location.2 += location_offset.z;

                        // TODO: use controller input to rotate around individual players or the entire scene while paused
                    }
                }
            }
            // Write auto state to freelook data.
            // This is not needed at all for the auto camera logic.
            // However it allows the freelook camera to continue from the location of the auto camera
            //
            // This logic is copy pasted from the transform() method.
            CameraControlState::Auto => {
                let camera_location = self.get_camera_location();

                // write to freelook state
                self.freelook_location = (camera_location.x, camera_location.y, camera_location.z);
                self.freelook_phi = consts::PI / 2.0;
                self.freelook_theta = consts::PI;
            }
        }
    }

    fn get_camera_location(&self) -> Point3<f32> {
        let width = (self.rect.x1 - self.rect.x2).abs();
        let height = (self.rect.x1 - self.rect.x2).abs();
        let middle_x = (self.rect.x1 + self.rect.x2) / 2.0;
        let middle_y = (self.rect.y1 + self.rect.y2) / 2.0;

        // camera distance
        let radius = (self.rect.y2 - middle_y).max(self.rect.x2 - middle_x);
        let mut camera_distance = radius / (Camera::fov_rad() / 2.0).tan();
        let rect_aspect = width / height;
        if rect_aspect > self.aspect_ratio {
            if self.aspect_ratio > 1.0 {
                camera_distance /= self.aspect_ratio;
            }
            else {
                camera_distance *= self.aspect_ratio;
            }
        }
        else if width > height {
            camera_distance /= rect_aspect;
        }

        Point3::new(middle_x, middle_y, camera_distance)
    }

    pub fn debug_print(&self) -> Vec<String> {
        match (&self.transform_mode, &self.control_state) {
            (TransformMode::Dev,  CameraControlState::Manual) => vec!("Press Backspace to leave manual camera mode. Press Esc to leave developer mode".into()),
            (TransformMode::Play, CameraControlState::Manual) => vec!("Press Backspace to leave manual camera mode.".into()),
            (TransformMode::Dev,  CameraControlState::Auto)   => vec!("Press Esc to leave developer mode.".into()),
            (TransformMode::Play, CameraControlState::Auto)   => vec!(),
        }
    }

    pub fn dev_mode(&self) -> bool {
        match self.transform_mode {
            TransformMode::Dev  => true,
            TransformMode::Play => false,
        }
    }

    pub fn update(&mut self, os_input: &WinitInputHelper, players: &[Entity], fighters: &KeyedContextVec<Fighter>, stage: &Stage) {
        // process new resolution
        if let Some((width, height)) = os_input.resolution() {
            self.window_width = width as f32;
            self.window_height = height as f32;
            self.aspect_ratio = width as f32 / height as f32;
        }

        if let CameraControlState::Auto = self.control_state {
            // initialise new_rect using only the first player
            let mut player_iter = players.iter();
            let new_rect = player_iter
                .next()
                .and_then(|x| x.cam_area(&stage.camera, players, fighters, &stage.surfaces));
            let mut new_rect = match new_rect {
                Some(rect) => rect,
                None => {
                    self.rect = Rect { x1: -200.0, y1: -200.0, x2: 200.0, y2: 200.0 };
                    return;
                }
            };

            // grow new_rect to cover all other players
            for player in player_iter {
                if let Some(next_area) = player.cam_area(&stage.camera, players, fighters, &stage.surfaces) {
                    new_rect.x1 = new_rect.x1.min(next_area.left());
                    new_rect.x2 = new_rect.x2.max(next_area.right());
                    new_rect.y1 = new_rect.y1.min(next_area.bot());
                    new_rect.y2 = new_rect.y2.max(next_area.top());
                }
            }

            // grow new_rect to fill aspect ratio
            let mut width  = (new_rect.x1 - new_rect.x2).abs();
            let mut height = (new_rect.y1 - new_rect.y2).abs();
            if width / height > self.aspect_ratio { // if new_rect AR is wider than the screen AR
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
        let width = (self.rect.x1 - self.rect.x2).abs();
        let height = (self.rect.x1 - self.rect.x2).abs();
        let middle_x = (self.rect.x1 + self.rect.x2) / 2.0;
        let middle_y = (self.rect.y1 + self.rect.y2) / 2.0;

        let aspect_ratio = Matrix4::from_nonuniform_scale(1.0, self.aspect_ratio, 1.0);

        match self.transform_mode {
            TransformMode::Dev => {
                // TODO: Apparently the near z plane should be positive, but that breaks things :/
                let proj = cgmath::ortho(-width / 2.0, width / 2.0, -height / 2.0, height / 2.0, -20000.0, 20000.0);
                let camera_target   = Point3::new(middle_x, middle_y, 0.0);
                let camera_location = Point3::new(middle_x, middle_y, 2000.0);
                let view = Matrix4::look_at(camera_location, camera_target, Vector3::new(0.0, 1.0, 0.0));
                aspect_ratio * proj * view
            }
            TransformMode::Play => {
                // projection matrix
                // Specifying the aspect ratio as an argument here messes up the logic for fitting the rect in the camera.
                // So we just implement aspect ratio in a seperate matrix, which doesnt have this issue for some reason
                let proj = cgmath::perspective(Rad(Camera::fov_rad()), 1.0, 1.0, 20000.0);

                match self.control_state {
                    CameraControlState::Auto => {
                        // camera points
                        let camera_target   = Point3::new(middle_x, middle_y, 0.0);
                        let camera_location = self.get_camera_location();

                        // view matrix
                        let view = Matrix4::look_at(camera_location, camera_target, Vector3::new(0.0, 1.0, 0.0));
                        aspect_ratio * proj * view
                    }
                    CameraControlState::Manual => {
                        let camera_location = Point3::new(
                            self.freelook_location.0,
                            self.freelook_location.1,
                            self.freelook_location.2,
                        );
                        let view = Matrix4::look_at_dir(camera_location, self.freelook_direction(), Vector3::new(0.0, 1.0, 0.0));
                        aspect_ratio * proj * view
                    }
                }
            }
        }
    }

    fn freelook_direction(&self) -> Vector3<f32> {
        Vector3::new(
            self.freelook_phi.sin() * self.freelook_theta.sin(),
            self.freelook_phi.cos(),
            self.freelook_phi.sin() * self.freelook_theta.cos(),
        )
    }

    fn fov_rad() -> f32 {
        let fov = 40.0;
        fov * consts::PI / 180.0
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

use egui::{vec2, Pos2, Vec2};
use graph_format::Point;
use nalgebra::{Matrix4, Orthographic3, Similarity3, Vector3};

/// 2D planar camera
pub struct Camera {
    pub transf: Similarity3<f32>,
    pub ortho: Orthographic3<f32>,
    pub size: Vec2,
}

impl Camera {
    pub fn new(center: Point) -> Camera {
        let transf = Similarity3::new(
            Vector3::new(-center.x, -center.y, 0.0),
            Vector3::new(0.0, 0.0, 0.0),
            1.0,
        )
        .append_scaling(0.1);
        Camera {
            transf,
            ortho: Camera::create_orthographic(1, 1),
            size: vec2(1.0, 1.0),
        }
    }

    /// Computes the 4x4 transformation matrix.
    pub fn get_matrix(&self) -> Matrix4<f32> {
        self.ortho.to_homogeneous() * self.transf.to_homogeneous()
    }

    pub fn get_inverse_matrix(&self) -> Matrix4<f32> {
        self.get_matrix().try_inverse().unwrap()
    }

    fn create_orthographic(width: u32, height: u32) -> Orthographic3<f32> {
        let hw = width as f32 / 2.0;
        let hh = height as f32 / 2.0;
        Orthographic3::new(-hw, hw, -hh, hh, -1.0, 1.0)
    }

    pub fn set_window_size(&mut self, size: Vec2) {
        self.size = size;
        self.ortho = Camera::create_orthographic(size.x as u32, size.y as u32);
    }

    /// Zooms the view in or out around the specified mouse location.
    pub fn zoom(&mut self, dy: f32, mouse: Pos2) {
        let zoom_speed = 1.1;
        let s = if dy > 0.0 {
            zoom_speed
        } else {
            1.0 / zoom_speed
        };
        let diffpoint = nalgebra::Point3::new(
            mouse.x as f32 - self.ortho.right(),
            mouse.y as f32 - self.ortho.top(),
            0.0,
        );
        let before = self.transf.inverse_transform_point(&diffpoint);
        self.transf.append_scaling_mut(s);
        let after = self.transf.inverse_transform_point(&diffpoint);
        let scale = self.transf.scaling();
        self.transf
            .append_translation_mut(&nalgebra::Translation3::new(
                (after.x - before.x) * scale,
                -(after.y - before.y) * scale,
                0.0,
            ));
    }

    /// Pans the view.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.transf
            .append_translation_mut(&nalgebra::Translation3::new(dx, -dy, 0.0));
    }

    pub fn rotate(&mut self, rot: f32) {
        // TODO: fuck quaternions all my homies uses euler angles
        //let center = self.transf.inverse_transform_point(&nalgebra::Point3::new(0.0, 0.0, 0.0));
        self.transf
            .append_rotation_wrt_center_mut(&nalgebra::UnitQuaternion::from_euler_angles(
                0.0, 0.0, rot,
            ));
    }
}

use egui::{vec2, Pos2, Vec2};
use graph_format::Point;
use graph_format::nalgebra::{Matrix4, Orthographic3, Point3, Similarity3, Translation3, UnitQuaternion, Vector3};

pub type CamXform = Similarity3<f32>;

/// 2D planar camera
#[derive(Copy, Clone)]
pub struct Camera {
    pub transf: CamXform,
    pub ortho: Orthographic3<f32>,
    pub size: Vec2,
}

impl Camera {
    pub fn new(center: Point) -> Camera {
        let transf = CamXform::new(
            Vector3::new(-center.x, -center.y, 0.0),
            Vector3::new(0.0, 0.0, 0.0),
            1.0,
        );
        //.append_scaling(0.2);
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
        self.transf.append_scaling_mut(if size.x < size.y {
            size.x / self.size.x
        } else {
            size.y / self.size.y
        });
        self.size = size;
        self.ortho = Camera::create_orthographic(size.x as u32, size.y as u32);
    }

    /// Zooms the view in or out around the specified mouse location.
    pub fn zoom(&mut self, scaling: f32, mouse: Pos2) {
        let diffpoint = Point3::new(
            mouse.x - self.ortho.right(),
            mouse.y - self.ortho.top(),
            0.0,
        );
        let before = self.transf.inverse_transform_point(&diffpoint);
        self.transf.append_scaling_mut(scaling);
        let after = self.transf.inverse_transform_point(&diffpoint);
        let diff = after - before;
        let diff_transf = self.transf.transform_vector(&diff);
        self.transf
            .append_translation_mut(&Translation3::new(
                diff_transf.x,
                -diff_transf.y,
                0.0,
            ));
    }

    /// Pans the view.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.transf
            .append_translation_mut(&Translation3::new(dx, -dy, 0.0));
    }
    pub fn rotate(&mut self, rot: f32) {
        self.transf
            .append_rotation_mut(&UnitQuaternion::from_euler_angles(
                0.0, 0.0, -rot,
            ));
    }
}

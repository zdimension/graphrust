use egui::{Pos2, Vec2};
use graph_format::nalgebra::{
    Matrix4, Orthographic3, Point3, Similarity3, Translation3, UnitQuaternion, Vector3,
};
use graph_format::Point;

pub type CamXform = Similarity3<f32>;

/// 2D planar camera
#[derive(Copy, Clone)]
pub struct Camera {
    pub transf: CamXform,
    pub ortho: Orthographic3<f32>,
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

    pub fn get_major_axis(size: Vec2) -> f32 {
        if size.x < size.y {
            size.x
        } else {
            size.y
        }
    }

    pub fn with_window_size(mut self, size: Vec2) -> Self {
        self.transf
            .append_scaling_mut((if size.x < size.y { size.x } else { size.y }).max(1.0));
        self.ortho = Camera::create_orthographic(size.x as u32, size.y as u32);
        self
    }

    /// Zooms the view in or out around the specified mouse location (which should be centered around view origin).
    pub fn zoom(&mut self, scaling: f32, mouse: Pos2) {
        let diffpoint = Point3::new(mouse.x, mouse.y, 0.0);
        let before = self.transf.inverse_transform_point(&diffpoint);
        self.transf.append_scaling_mut(scaling);
        let after = self.transf.inverse_transform_point(&diffpoint);
        let diff = after - before;
        let diff_transf = self.transf.transform_vector(&diff);
        self.transf
            .append_translation_mut(&Translation3::new(diff_transf.x, -diff_transf.y, 0.0));
    }

    /// Pans the view.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.transf
            .append_translation_mut(&Translation3::new(dx, -dy, 0.0));
    }
    pub fn rotate(&mut self, rot: f32) {
        self.transf
            .append_rotation_mut(&UnitQuaternion::from_euler_angles(0.0, 0.0, -rot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graph_format::nalgebra::Point3;
    use graph_format::Point;

    #[test]
    fn test_zoom_at_origin() {
        let center = Point { x: 0.0, y: 0.0 };
        let mut camera = Camera::new(center);
        let initial_matrix = camera.get_matrix();
        let p1 = Point3::new(1.0, 0.0, 0.0);
        let p1_before = camera.transf.transform_point(&p1);
        camera.zoom(2.0, Pos2 { x: 0.0, y: 0.0 });
        let p1_after = camera.transf.transform_point(&p1);
        assert_eq!(p1_after, Point3::new(2.0, 0.0, 0.0));
        let zoomed_matrix = camera.get_matrix();
        assert_ne!(initial_matrix, zoomed_matrix);
        assert_eq!(camera.transf.scaling(), 2.0);
    }

    #[test]
    fn test_zoom_at_one() {
        let center = Point { x: 0.0, y: 0.0 };
        let mut camera = Camera::new(center);
        let initial_matrix = camera.get_matrix();
        let p1 = Point3::new(1.0, 0.0, 0.0);
        let p1_before = camera.transf.transform_point(&p1);
        camera.zoom(2.0, Pos2 { x: 1.0, y: 0.0 });
        let p1_after = camera.transf.transform_point(&p1);
        assert_eq!(p1_after, Point3::new(1.0, 0.0, 0.0));
        let zoomed_matrix = camera.get_matrix();
        assert_ne!(initial_matrix, zoomed_matrix);
        assert_eq!(camera.transf.scaling(), 2.0);
    }
}

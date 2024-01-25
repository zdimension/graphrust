use nalgebra::Vector2;
use speedy::{Readable, Writable};

// 24bpp color structure
#[derive(Copy, Clone, Readable, Writable)]
#[repr(C)]
pub struct Color3b {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

// same but f32
#[derive(Copy, Clone)]
pub struct Color3f {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3b {
    pub fn to_f32(self) -> Color3f {
        Color3f {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
        }
    }
}

impl Color3f {
    pub fn new(r: f32, g: f32, b: f32) -> Color3f {
        Color3f { r, g, b }
    }

    pub fn average(&self, other: Color3f) -> Color3f {
        Color3f {
            r: (self.r + other.r) / 2.0,
            g: (self.g + other.g) / 2.0,
            b: (self.b + other.b) / 2.0,
        }
    }
}

/// 2D point/vector.
#[derive(Copy, Clone, Readable, Writable)]
#[repr(C)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl From<Point> for Vector2<f32> {
    fn from(p: Point) -> Vector2<f32> {
        Vector2::new(p.x, p.y)
    }
}

impl From<Vector2<f32>> for Point {
    fn from(v: Vector2<f32>) -> Point {
        Point { x: v.x, y: v.y }
    }
}

impl Point {
    pub fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    /// Returns the unit vector with angle theta.
    pub fn polar(theta: f32) -> Point {
        Point {
            x: theta.cos(),
            y: theta.sin(),
        }
    }

    /// Returns the distance between the point and the origin.
    pub fn norm(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns the canonical orthogonal vector.
    pub fn ortho(&self) -> Point {
        Point {
            x: -self.y,
            y: self.x,
        }
    }

    /// Normalizes the vector.
    pub fn normalized(&self) -> Point {
        *self / self.norm()
    }
}

impl std::ops::Add for Point {
    type Output = Point;

    fn add(self, other: Point) -> Point {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Sub for Point {
    type Output = Point;

    fn sub(self, other: Point) -> Point {
        Point {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Mul<f32> for Point {
    type Output = Point;

    fn mul(self, other: f32) -> Point {
        Point {
            x: self.x * other,
            y: self.y * other,
        }
    }
}

impl std::ops::Div<f32> for Point {
    type Output = Point;

    fn div(self, other: f32) -> Point {
        Point {
            x: self.x / other,
            y: self.y / other,
        }
    }
}

#[derive(Readable, Writable)]
pub struct NodeStore {
    pub position: Point,
    pub size: f32,
    pub class: u16,
    pub offset_id: u32,
    pub offset_name: u32,
}

#[derive(Readable, Writable)]
pub struct EdgeStore {
    pub a: u32,
    pub b: u32,
}

#[cfg(target_pointer_width = "32")]
pub type LenType = u64;

#[cfg(target_pointer_width = "64")]
pub type LenType = usize;

#[derive(Readable, Default)]
#[cfg_attr(target_pointer_width = "64", derive(Writable))]
pub struct GraphFile {
    pub class_count: u16,
    #[speedy(length = class_count)]
    pub classes: Vec<Color3b>,

    pub node_count: LenType,
    #[speedy(length = node_count)]
    pub nodes: Vec<NodeStore>,

    pub edge_count: LenType,
    #[speedy(length = edge_count)]
    pub edges: Vec<EdgeStore>,

    pub ids_size: LenType,
    #[speedy(length = ids_size)]
    pub ids: Vec<u8>,

    pub names_size: LenType,
    #[speedy(length = names_size)]
    pub names: Vec<u8>,
}

use nalgebra::{Vector2, Vector4};
pub use speedy::{Readable, Writable};
use std::iter::Sum;

pub use nalgebra;

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
#[repr(C)]
pub struct Color3f {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3b {
    pub fn new(r: u8, g: u8, b: u8) -> Color3b {
        Color3b { r, g, b }
    }

    pub fn to_f32(self) -> Color3f {
        Color3f {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
        }
    }

    pub fn to_u32(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
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

    pub fn to_u8(self) -> Color3b {
        Color3b {
            r: (self.r * 255.0) as u8,
            g: (self.g * 255.0) as u8,
            b: (self.b * 255.0) as u8,
        }
    }
}

/// 2D point/vector.
#[derive(Copy, Clone, Readable, Writable, Debug)]
#[repr(C)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Sum for Point {
    fn sum<I: Iterator<Item=Point>>(iter: I) -> Point {
        iter.fold(Point::new(0.0, 0.0), |a, b| a + b)
    }
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

impl From<Point> for Vector4<f32> {
    fn from(p: Point) -> Vector4<f32> {
        Vector4::new(p.x, p.y, 0.0, 1.0)
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

    /// Returns the distance between the point and the origin squared.
    pub fn norm_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Returns the distance between the point and the origin.
    pub fn norm(&self) -> f32 {
        self.norm_squared().sqrt()
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

    pub fn to_array(&self) -> [f32; 2] {
        [self.x, self.y]
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
    pub total_edge_count: u16,
    pub edge_count: u16,
    #[speedy(length = edge_count)]
    pub edges: Vec<u32>,
}

#[derive(Readable, Writable, Hash, PartialEq, Eq, Copy, Clone)]
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

    pub ids_size: LenType,
    #[speedy(length = ids_size)]
    pub ids: Vec<u8>,

    pub names_size: LenType,
    #[speedy(length = names_size)]
    pub names: Vec<u8>,
}

impl GraphFile {
    pub fn get_adjacency(&self) -> Vec<Vec<u32>> {
        let mut persons: Vec<_> = self.nodes.iter().map(|n| Vec::with_capacity(n.total_edge_count as usize)).collect();
        for (i, n) in self.nodes.iter().enumerate() {
            for e in n.edges.iter().copied() {
                persons[i].push(e);
                persons[e as usize].push(i as u32);
            }
        }
        persons
    }
}

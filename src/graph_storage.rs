use nalgebra::Vector2;
use speedy::{Readable};

// 24bpp color structure
#[derive(Copy, Clone)]
#[derive(Readable)]
pub struct Color3b
{
    r: u8,
    g: u8,
    b: u8,
}

// same but f32
#[derive(Copy, Clone)]
pub struct Color3f
{
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3b
{
    pub fn to_f32(&self) -> Color3f
    {
        Color3f {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
        }
    }
}

impl Color3f
{
    fn new(r: f32, g: f32, b: f32) -> Color3f
    {
        Color3f {
            r,
            g,
            b,
        }
    }

    pub fn average(&self, other: Color3f) -> Color3f
    {
        Color3f {
            r: (self.r + other.r) / 2.0,
            g: (self.g + other.g) / 2.0,
            b: (self.b + other.b) / 2.0,
        }
    }
}

unsafe impl glium::vertex::Attribute for Color3f
{
    fn get_type() -> glium::vertex::AttributeType
    {
        glium::vertex::AttributeType::F32F32F32
    }
}


#[derive(Copy, Clone)]
#[derive(Readable)]
pub struct Point
{
    pub x: f32,
    pub y: f32,
}

impl Into<Vector2<f32>> for Point
{
    fn into(self) -> Vector2<f32>
    {
        Vector2::new(self.x, self.y)
    }
}

impl Into<Point> for Vector2<f32>
{
    fn into(self) -> Point
    {
        Point {
            x: self.x,
            y: self.y,
        }
    }
}

unsafe impl glium::vertex::Attribute for Point
{
    fn get_type() -> glium::vertex::AttributeType
    {
        glium::vertex::AttributeType::F32F32
    }
}

impl Point
{
    pub fn new(x: f32, y: f32) -> Point
    {
        Point { x, y }
    }

    pub fn polar(r: f32, theta: f32) -> Point
    {
        Point { x: r * theta.cos(), y: r * theta.sin() }
    }

    pub fn norm(&self) -> f32
    {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn ortho(&self) -> Point
    {
        Point { x: -self.y, y: self.x }
    }

    pub fn normalized(&self) -> Point
    {
        *self / self.norm()
    }
}

impl std::ops::Add for Point
{
    type Output = Point;

    fn add(self, other: Point) -> Point
    {
        Point { x: self.x + other.x, y: self.y + other.y }
    }
}

impl std::ops::Sub for Point
{
    type Output = Point;

    fn sub(self, other: Point) -> Point
    {
        Point { x: self.x - other.x, y: self.y - other.y }
    }
}

impl std::ops::Mul<f32> for Point
{
    type Output = Point;

    fn mul(self, other: f32) -> Point
    {
        Point { x: self.x * other, y: self.y * other }
    }
}

impl std::ops::Div<f32> for Point
{
    type Output = Point;

    fn div(self, other: f32) -> Point
    {
        Point { x: self.x / other, y: self.y / other }
    }
}


#[derive(Readable)]
pub struct NodeStore
{
    pub position: Point,
    pub size: f32,
    pub class: u16,
    pub offset_id: u32,
    pub offset_name: u32,
}

#[derive(Readable)]
pub struct EdgeStore
{
    pub a: u32,
    pub b: u32,
}

#[derive(Readable)]
pub struct GraphFile
{
    pub class_count: u16,
    #[speedy(length = class_count)]
    pub classes: Vec<Color3b>,

    pub node_count: u64,
    #[speedy(length = node_count)]
    pub nodes: Vec<NodeStore>,

    pub edge_count: u64,
    #[speedy(length = edge_count)]
    pub edges: Vec<EdgeStore>,

    pub ids_size: u64,
    #[speedy(length = ids_size)]
    pub ids: Vec<u8>,

    pub names_size: u64,
    #[speedy(length = names_size)]
    pub names: Vec<u8>,
}

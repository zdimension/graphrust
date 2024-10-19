use crate::app::{Person, PersonVertex, Vertex};
use graph_format::{Color3b, Point};

/// Draws a line between a and b with the specified thickness and color.
/// Result is a list of vertices to be used as a GL TriangleList.
pub fn create_rectangle(
    a: Point,
    b: Point,
    color_a: Color3b,
    color_b: Color3b,
    size: f32,
) -> [Vertex; 6] {
    let ortho = (b - a).ortho().normalized() * size;
    let v0 = a + ortho;
    let v1 = a - ortho;
    let v2 = b - ortho;
    let v3 = b + ortho;
    [
        Vertex::new(v0, color_a),
        Vertex::new(v1, color_a),
        Vertex::new(v2, color_b),
        Vertex::new(v2, color_b),
        Vertex::new(v3, color_b),
        Vertex::new(v0, color_a),
    ]
}

/// Draws a circle with the specified radius and color.
/// Result is a list of vertices to be used as a GL TriangleFan.
pub fn create_circle_fan(center: Point, radius: f32, color: Color3b) -> Vec<Vertex> {
    const NUM_SEGMENTS: usize = 32;

    vec![Vertex::new(center, color)]
        .into_iter()
        .chain((0..=NUM_SEGMENTS).map(|i| {
            let angle = i as f32 * 2.0 * std::f32::consts::PI / NUM_SEGMENTS as f32;
            Vertex::new(center + Point::polar(angle) * radius, color)
        }))
        .collect()
}

/// Draws a circle with the specified radius and color.
/// Result is a list of vertices to be used as a GL TriangleList.
pub fn create_circle_tris(center: Point, radius: f32, color: Color3b) -> Vec<Vertex> {
    const NUM_SEGMENTS: usize = 32;

    let verts = create_circle_fan(center, radius, color);

    (0..NUM_SEGMENTS)
        .flat_map(|i| [verts[0], verts[i], verts[i + 1]])
        .chain([verts[0], verts[NUM_SEGMENTS], verts[1]])
        .collect()
}

pub fn create_node_vertex(p: &Person) -> PersonVertex {
    PersonVertex::new(
        p.position,
        p.neighbors.len() as u16,
        p.modularity_class,
    )
}

pub fn create_edge_vertices(pa: &Person, pb: &Person) -> [PersonVertex; 6] {
    let a = pa.position;
    let b = pb.position;
    const EDGE_HALF_WIDTH: f32 = 0.75;
    let ortho = (b - a).ortho().normalized() * EDGE_HALF_WIDTH;
    let v0 = a + ortho;
    let v1 = a - ortho;
    let v2 = b - ortho;
    let v3 = b + ortho;
    let x = [(v0, pa), (v1, pa), (v2, pb), (v2, pb), (v3, pb), (v0, pa)];
    x.map(|(pos, node)| PersonVertex::new(pos, node.neighbors.len() as u16, node.modularity_class))
}
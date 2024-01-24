use crate::app::Vertex;
use crate::graph_storage::{Color3f, Point};

/// Draws a line between a and b with the specified thickness and color.
/// Result is a list of vertices to be used as a GL TriangleList.
pub fn create_rectangle(
    a: Point,
    b: Point,
    color_a: Color3f,
    color_b: Color3f,
    size: f32,
) -> Vec<Vertex> {
    let ortho = (b - a).ortho().normalized() * size;
    let v0 = a + ortho;
    let v1 = a - ortho;
    let v2 = b - ortho;
    let v3 = b + ortho;
    vec![
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
pub fn create_circle_fan(center: Point, radius: f32, color: Color3f) -> Vec<Vertex> {
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
pub fn create_circle_tris(center: Point, radius: f32, color: Color3f) -> Vec<Vertex> {
    const NUM_SEGMENTS: usize = 32;

    let verts = create_circle_fan(center, radius, color);

    (0..NUM_SEGMENTS)
        .flat_map(|i| [verts[0], verts[i], verts[i + 1]])
        .chain([verts[0], verts[NUM_SEGMENTS], verts[1]])
        .collect()
}

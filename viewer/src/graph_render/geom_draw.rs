use crate::graph_render::QuadVertex;
use graph_format::Point;

pub const VERTS_PER_NODE: usize = 6;
pub const VERTS_PER_EDGE: usize = 6;

/// Creates a template quad for instanced rendering
/// The quad is centered at origin with size calculated based on node degree in the shader
pub fn create_quad_template() -> [QuadVertex; VERTS_PER_NODE] {
    // Size scaling will be handled per-instance by the vertex shader
    // Create a unit quad from -1 to 1 with texture coordinates
    const MIN_SIZE: f32 = 12.0;
    const MAX_SIZE: f32 = 100.0;
    let half_size = MAX_SIZE * 0.5; // Use max size as template, shader scales down
    
    let tl = (Point::new(-half_size, half_size), Point::new(-1.0, 1.0));
    let tr = (Point::new(half_size, half_size), Point::new(1.0, 1.0));
    let br = (Point::new(half_size, -half_size), Point::new(1.0, -1.0));
    let bl = (Point::new(-half_size, -half_size), Point::new(-1.0, -1.0));
    
    // Two triangles forming a quad
    [
        QuadVertex::new(tl.0, tl.1),
        QuadVertex::new(bl.0, bl.1),
        QuadVertex::new(br.0, br.1),
        QuadVertex::new(br.0, br.1),
        QuadVertex::new(tr.0, tr.1),
        QuadVertex::new(tl.0, tl.1),
    ]
}

/// Creates a template edge quad for instanced rendering
/// The quad will be transformed by the shader to connect two points
pub fn create_edge_quad_template() -> [QuadVertex; VERTS_PER_EDGE] {
    // Unit quad from (0,0) to (1,0) with half-width of 1
    // The shader will transform this to the actual edge
    let v0 = (Point::new(0.0, 1.0), Point::new(0.0, 0.0));
    let v1 = (Point::new(0.0, -1.0), Point::new(0.0, 0.0));
    let v2 = (Point::new(1.0, -1.0), Point::new(0.0, 0.0));
    let v3 = (Point::new(1.0, 1.0), Point::new(0.0, 0.0));
    
    // Two triangles forming a quad
    [
        QuadVertex::new(v0.0, v0.1),
        QuadVertex::new(v1.0, v1.1),
        QuadVertex::new(v2.0, v2.1),
        QuadVertex::new(v2.0, v2.1),
        QuadVertex::new(v3.0, v3.1),
        QuadVertex::new(v0.0, v0.1),
    ]
}
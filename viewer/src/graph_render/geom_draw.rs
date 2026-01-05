use crate::app::Person;
use crate::graph_render::PersonVertex;
use graph_format::Point;

pub const VERTS_PER_NODE: usize = 6;
pub const VERTS_PER_EDGE: usize = 6;

/// Creates a template quad for instanced rendering
/// The quad is centered at origin with size calculated based on node degree in the shader
pub fn create_quad_template() -> [PersonVertex; VERTS_PER_NODE] {
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
        PersonVertex::with_tex_coord(tl.0, 0, 0, tl.1),
        PersonVertex::with_tex_coord(bl.0, 0, 0, bl.1),
        PersonVertex::with_tex_coord(br.0, 0, 0, br.1),
        PersonVertex::with_tex_coord(br.0, 0, 0, br.1),
        PersonVertex::with_tex_coord(tr.0, 0, 0, tr.1),
        PersonVertex::with_tex_coord(tl.0, 0, 0, tl.1),
    ]
}

/// Creates a template edge quad for instanced rendering
/// The quad will be transformed by the shader to connect two points
pub fn create_edge_quad_template() -> [PersonVertex; VERTS_PER_EDGE] {
    // Unit quad from (0,0) to (1,0) with half-width of 1
    // The shader will transform this to the actual edge
    let v0 = (Point::new(0.0, 1.0), Point::new(0.0, 0.0));
    let v1 = (Point::new(0.0, -1.0), Point::new(0.0, 0.0));
    let v2 = (Point::new(1.0, -1.0), Point::new(0.0, 0.0));
    let v3 = (Point::new(1.0, 1.0), Point::new(0.0, 0.0));
    
    // Two triangles forming a quad
    [
        PersonVertex::with_tex_coord(v0.0, 0, 0, v0.1),
        PersonVertex::with_tex_coord(v1.0, 0, 0, v1.1),
        PersonVertex::with_tex_coord(v2.0, 0, 0, v2.1),
        PersonVertex::with_tex_coord(v2.0, 0, 0, v2.1),
        PersonVertex::with_tex_coord(v3.0, 0, 0, v3.1),
        PersonVertex::with_tex_coord(v0.0, 0, 0, v0.1),
    ]
}

pub fn create_edge_vertices(pa: &Person, pb: &Person) -> [PersonVertex; VERTS_PER_EDGE] {
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
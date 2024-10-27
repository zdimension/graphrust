use crate::app::{Person, PersonVertex};

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
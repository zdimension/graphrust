// Edge vertex shader

struct Uniforms {
    projection: mat4x4<f32>,
    degfilter: u32,
    opacity: f32,
}

struct ClassColors {
    colors: array<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> class_colors: ClassColors;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) edge_pos_a: vec2<f32>,
    @location(3) edge_pos_b: vec2<f32>,
    @location(4) edge_deg_class_a: u32,
    @location(5) edge_deg_class_b: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

fn unpack_color(color: u32) -> vec3<f32> {
    return vec3<f32>(
        f32((color >> 16u) & 0xFFu) / 255.0,
        f32((color >> 8u) & 0xFFu) / 255.0,
        f32(color & 0xFFu) / 255.0
    );
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Transform the unit quad to connect edge_pos_a to edge_pos_b
    let edge_vec = in.edge_pos_b - in.edge_pos_a;
    let edge_len = length(edge_vec);
    let edge_dir = edge_vec / edge_len;
    let edge_ortho = vec2<f32>(-edge_dir.y, edge_dir.x);
    
    let EDGE_HALF_WIDTH = 0.75;
    // position.x goes from 0 to 1, position.y is -1 to 1 for width
    let final_pos = in.edge_pos_a + edge_dir * (in.position.x * edge_len) + edge_ortho * (in.position.y * EDGE_HALF_WIDTH);
    
    // Use appropriate endpoint data based on position along edge
    var deg: u32;
    var class_: u32;
    if (in.position.x < 0.5) {
        deg = in.edge_deg_class_a & 0xFFFFu;
        class_ = in.edge_deg_class_a >> 16u;
    } else {
        deg = in.edge_deg_class_b & 0xFFFFu;
        class_ = in.edge_deg_class_b >> 16u;
    }
    
    let low = uniforms.degfilter & 0xFFFFu;
    let high = uniforms.degfilter >> 16u;
    
    if (deg < low || deg > high) {
        // Discard by placing far outside clip space
        out.position = vec4<f32>(0.0, 0.0, -1000.0, 1.0);
        out.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    } else {
        out.position = uniforms.projection * vec4<f32>(final_pos, 0.0, 1.0);
        let scale = sqrt(f32(min(deg, 1000u)) / 1000.0);
        out.color = vec4<f32>(unpack_color(class_colors.colors[class_]), min(1.0, uniforms.opacity * (1.0 + 1.2 * scale)));
    }
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}

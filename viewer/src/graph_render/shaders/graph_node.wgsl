// Node vertex shader

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
    @location(2) instance_pos: vec2<f32>,
    @location(3) instance_deg_and_class: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
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
    
    let deg = in.instance_deg_and_class & 0xFFFFu;
    let class_ = in.instance_deg_and_class >> 16u;
    
    let low = uniforms.degfilter & 0xFFFFu;
    let high = uniforms.degfilter >> 16u;
    
    if (deg < low || deg > high) {
        // Discard by placing far outside clip space
        out.position = vec4<f32>(0.0, 0.0, -1000.0, 1.0);
        out.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        out.tex_coord = vec2<f32>(0.0);
    } else {
        // Calculate size based on degree
        let scale = sqrt(f32(min(deg, 1000u)) / 1000.0);
        let min_size = 12.0;
        let max_size = 100.0;
        let size_scale = ((max_size - min_size) * scale + min_size) / max_size;
        let final_pos = in.instance_pos + in.position * size_scale;
        
        out.position = uniforms.projection * vec4<f32>(final_pos, 0.0, 1.0);
        out.color = vec4<f32>(unpack_color(class_colors.colors[class_]), min(1.0, uniforms.opacity * (1.0 + 1.2 * scale)));
        out.tex_coord = in.tex_coord;
    }
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Convert tex_coord from [-1, 1] to distance from center
    let dist = dot(in.tex_coord, in.tex_coord);
    let RAD = 1.0;
    let BORDER = 0.2;
    let INNER = RAD - BORDER;
    
    if (dist > 1.0) {
        discard;
    } else if (dist > RAD - BORDER) {
        return vec4<f32>(in.color.rgb * 0.3, smoothstep(0.0, 0.02, RAD - dist) * in.color.a);
    } else {
        return vec4<f32>(mix(in.color.rgb * 0.3, in.color.rgb, smoothstep(0.0, 0.02, INNER - dist)), in.color.a);
    }
}

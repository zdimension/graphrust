precision mediump float;

layout (location = 0) in vec2 position;
layout (location = 1) in uint deg_and_class;
layout (location = 2) in vec2 tex_coord;
layout (location = 3) in vec2 instance_pos;
layout (location = 4) in uint instance_deg_and_class;
out vec4 v_color;
out vec2 v_tex_coord;
uniform mat4 u_projection;
uniform uint u_degfilter;
uniform float opacity;
uniform uint u_class_colors[NUM_CLASSES];
const float neg_infinity = uintBitsToFloat(0xFF800000u);
const float nan = intBitsToFloat(int(0xFFC00000u));
vec3 unpack_color(uint color) {
    return vec3(
    float((color >> 16) & 0xFFu) / 255.0,
    float((color >> 8) & 0xFFu) / 255.0,
    float(color & 0xFFu) / 255.0
    );
}
void main()
{
    // For instanced node rendering, use instance attributes; otherwise use vertex attributes
    bool is_instanced = gl_InstanceID >= 0 && instance_deg_and_class > 0u;
    uint deg = is_instanced ? (instance_deg_and_class & 0xFFFFu) : (deg_and_class & 0xFFFFu);
    uint class_ = is_instanced ? (instance_deg_and_class >> 16) : (deg_and_class >> 16);
    
    uint low = u_degfilter & 0xFFFFu;
    uint high = u_degfilter >> 16;
    if (deg < low || deg > high) {
        // alpha=-inf so when blended all vertices have alpha=-inf
        // it's clamped to 0 anyway after the fragment shader
        v_color = vec4(0.0, 0.0, 0.0, neg_infinity);
        // set position to nan so the vertex gets culled out of existence and the whole primitive is scrapped
        gl_Position = vec4(nan, nan, nan, nan);
        v_tex_coord = vec2(0.0);
    } else {
        vec2 final_pos;
        if (is_instanced) {
            // Calculate size based on degree for instanced nodes
            float scale = sqrt(float(min(deg, 1000u)) / 1000.0);
            const float min_size = 12.0;
            const float max_size = 100.0;
            float size_scale = ((max_size - min_size) * scale + min_size) / max_size;
            final_pos = instance_pos + position * size_scale;
        } else {
            // For edges, use position as-is
            final_pos = position;
        }
        
        gl_Position = u_projection * vec4(final_pos, 0.0, 1.0);
        float scale = sqrt(float(min(deg, 1000u)) / 1000.0);
        v_color = vec4(unpack_color(u_class_colors[class_]), min(1.0, opacity * (1.0 + 1.2 * scale)));
        v_tex_coord = tex_coord;
    }
}
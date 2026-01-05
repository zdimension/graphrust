precision mediump float;

layout (location = 0) in vec2 position;
layout (location = 1) in vec2 tex_coord;
layout (location = 2) in vec2 instance_pos;
layout (location = 3) in uint instance_deg_and_class;
out vec4 v_color;
out vec2 v_tex_coord;
uniform mat4 u_projection;
uniform uint u_degfilter;
uniform float opacity;
uniform uint u_class_colors[NUM_CLASSES];

void main()
{
    uint deg = instance_deg_and_class & 0xFFFFu;
    uint class_ = instance_deg_and_class >> 16;
    
    uint low = u_degfilter & 0xFFFFu;
    uint high = u_degfilter >> 16;
    
    if (deg < low || deg > high) {
        v_color = vec4(0.0, 0.0, 0.0, neg_infinity);
        gl_Position = vec4(nan, nan, nan, nan);
        v_tex_coord = vec2(0.0);
    } else {
        // Calculate size based on degree
        float scale = sqrt(float(min(deg, 1000u)) / 1000.0);
        const float min_size = 12.0;
        const float max_size = 100.0;
        float size_scale = ((max_size - min_size) * scale + min_size) / max_size;
        vec2 final_pos = instance_pos + position * size_scale;
        
        gl_Position = u_projection * vec4(final_pos, 0.0, 1.0);
        v_color = vec4(unpack_color(u_class_colors[class_]), min(1.0, opacity * (1.0 + 1.2 * scale)));
        v_tex_coord = tex_coord;
    }
}

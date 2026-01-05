precision mediump float;

layout (location = 0) in vec2 position;
layout (location = 1) in vec2 tex_coord;
layout (location = 2) in vec2 edge_pos_a;
layout (location = 3) in vec2 edge_pos_b;
layout (location = 4) in uint edge_deg_class_a;
layout (location = 5) in uint edge_deg_class_b;
out vec4 v_color;
out vec2 v_tex_coord;
uniform mat4 u_projection;
uniform uint u_degfilter;
uniform float opacity;
uniform uint u_class_colors[NUM_CLASSES];

void main()
{
    // Transform the unit quad to connect edge_pos_a to edge_pos_b
    vec2 edge_vec = edge_pos_b - edge_pos_a;
    float edge_len = length(edge_vec);
    vec2 edge_dir = edge_vec / edge_len;
    vec2 edge_ortho = vec2(-edge_dir.y, edge_dir.x);
    
    const float EDGE_HALF_WIDTH = 0.75;
    // position.x goes from 0 to 1, position.y is -1 to 1 for width
    vec2 final_pos = edge_pos_a + edge_dir * (position.x * edge_len) + edge_ortho * (position.y * EDGE_HALF_WIDTH);
    
    // Use appropriate endpoint data based on position along edge
    uint deg, class_;
    if (position.x < 0.5) {
        deg = edge_deg_class_a & 0xFFFFu;
        class_ = edge_deg_class_a >> 16;
    } else {
        deg = edge_deg_class_b & 0xFFFFu;
        class_ = edge_deg_class_b >> 16;
    }
    
    uint low = u_degfilter & 0xFFFFu;
    uint high = u_degfilter >> 16;
    
    if (deg < low || deg > high) {
        v_color = vec4(0.0, 0.0, 0.0, neg_infinity);
        gl_Position = vec4(nan, nan, nan, nan);
        v_tex_coord = vec2(0.0);
    } else {
        gl_Position = u_projection * vec4(final_pos, 0.0, 1.0);
        float scale = sqrt(float(min(deg, 1000u)) / 1000.0);
        v_color = vec4(unpack_color(u_class_colors[class_]), min(1.0, opacity * (1.0 + 1.2 * scale)));
        v_tex_coord = tex_coord;
    }
}

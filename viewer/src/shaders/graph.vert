precision mediump float;

in vec2 position;
in vec3 color;
in uint deg_and_class;
out vec4 v_color;
uniform mat4 u_projection;
uniform uint u_degfilter;
uniform float opacity;
const float neg_infinity = uintBitsToFloat(0xFF800000u);
void main()
{
    uint deg = deg_and_class & 0xFFFFu;
    uint class_ = deg_and_class >> 16;
    uint low = u_degfilter & 0xFFFFu;
    uint high = u_degfilter >> 16;
    if (deg < low || deg > high) {
        v_color = vec4(0.0, 0.0, 0.0, neg_infinity);
    } else {
        v_color = vec4(color, opacity);
    }
    gl_Position = u_projection * vec4(position, 0.0, 1.0);
    gl_PointSize = 16.0 * -u_projection[2][2];
}
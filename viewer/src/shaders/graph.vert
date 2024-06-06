precision mediump float;

layout (location = 0) in vec2 position;
layout (location = 1) in vec3 color;
layout (location = 2) in uint deg_and_class;
out vec4 v_color;
uniform mat4 u_projection;
uniform uint u_degfilter;
uniform float opacity;
const float neg_infinity = uintBitsToFloat(0xFF800000u);
const float nan = intBitsToFloat(int(0xFFC00000u));
void main()
{
    uint deg = deg_and_class & 0xFFFFu;
    uint class_ = deg_and_class >> 16;
    uint low = u_degfilter & 0xFFFFu;
    uint high = u_degfilter >> 16;
    if (deg < low || deg > high) {
        // alpha=-inf so when blended all points have alpha=-inf
        // it's clamped to 0 anyway after the fragment shader
        v_color = vec4(0.0, 0.0, 0.0, neg_infinity);
        // set position to nan so the vertex gets culled out of existence and the whole primitive is scrapped
        gl_Position = vec4(nan, nan, nan, nan);
    } else {
        gl_Position = u_projection * vec4(position, 0.0, 1.0);
        float scale = sqrt(float(min(deg, 1000u)) / 1000.0);
        v_color = vec4(color, min(1.0, opacity * (1.0 + 1.2 * scale)));
        const float min_size = 12.0;
        const float max_size = 100.0;
        float size = (max_size - min_size) * scale + min_size;
        gl_PointSize = size * -u_projection[2][2];
    }
}
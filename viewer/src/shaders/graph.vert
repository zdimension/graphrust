precision mediump float;

in vec2 position;
in vec3 color;
out vec4 v_color;
uniform mat4 u_projection;

void main()
{
    v_color = vec4(color, 1.0);
    gl_Position = u_projection * vec4(position, 0.0, 1.0);
    gl_PointSize = 2.0;
}
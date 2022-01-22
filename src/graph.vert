#version 140

in vec2 position;
in vec3 color;
out vec3 v_color;
uniform mat4 matrix;

void main()
{
    v_color = color;
    gl_Position = matrix * vec4(position, 0.0, 1.0);
}
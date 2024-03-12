precision mediump float;

in vec4 v_color;
out vec4 color;

void main()
{
    color = v_color;

    color.rgb *= color.a;
}
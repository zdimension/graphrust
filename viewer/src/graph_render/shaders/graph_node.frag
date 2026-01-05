precision mediump float;

in vec4 v_color;
in vec2 v_tex_coord;
out vec4 color;

void main()
{
    // Convert tex_coord from [-1, 1] to distance from center
    float dist = dot(v_tex_coord, v_tex_coord);
    const float RAD = 1.0;
    const float BORDER = 0.2;
    const float INNER = RAD - BORDER;
    if (dist > 1.0)
        discard;
    else if (dist > RAD - BORDER)
        color = vec4(v_color.rgb * 0.3, smoothstep(0.0, 0.02, RAD - dist) * v_color.a);
    else
        color = vec4(mix(v_color.rgb * 0.3, v_color.rgb, smoothstep(0.0, 0.02, INNER - dist)), v_color.a);
}
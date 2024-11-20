precision mediump float;

in vec4 v_color;
out vec4 color;

void main()
{
    //color = v_color;
    float dist = dot(gl_PointCoord-0.5, gl_PointCoord-0.5);
    const float RAD = 0.25;
    const float BORDER = 0.05;
    const float INNER = RAD - BORDER;
    if (dist>0.25)
    discard;
    else if (dist > RAD - BORDER)
    color = vec4(v_color.rgb * 0.3, smoothstep(0.0, 0.005, RAD - dist) * v_color.a);
    else
    color = vec4(mix(v_color.rgb * 0.3, v_color.rgb, smoothstep(0.0, 0.005, INNER - dist)), v_color.a);
}
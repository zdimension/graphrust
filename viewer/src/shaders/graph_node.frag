precision mediump float;

in vec4 v_color;
out vec4 color;

void main()
{
    //color = v_color;
    float dist = dot(gl_PointCoord-0.5,gl_PointCoord-0.5);
    if(dist>0.25)
        discard;
    else if (dist > 0.2)
        color = vec4(v_color.rgb * 0.3, v_color.a);
    else
        color = v_color;
}
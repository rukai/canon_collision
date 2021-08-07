#version 440

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform Data {
    mat4 u_transform;
    float u_current_frame;
};

layout(set = 0, binding = 1) uniform texture2D u_texture;
layout(set = 0, binding = 2) uniform sampler u_sampler;

void main() {
    float flow = u_current_frame / 1700;
    float swirl_x = sin(v_uv.y + u_current_frame / 800) * 0.3;
    float swirl_y = sin(v_uv.x + u_current_frame / 400) * 0.1;
    vec2 uv = vec2(
        v_uv.x + swirl_x,
        v_uv.y + swirl_y - flow
    );
    f_color = texture(sampler2D(u_texture, u_sampler), uv);

    // at usual camera values is roughly between 0 and 1
    float nice_depth = (1 - gl_FragCoord.z) * 151;

    // fade to dark
    //if (nice_depth < 0.2) {
    //    f_color *= nice_depth * 5;
    //}

    // fade to light
    if (nice_depth < 0.15) {
        f_color = mix(vec4(0.871, 0.4, 0.2, 1.0), f_color, nice_depth * 7);
    }
}

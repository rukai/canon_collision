#version 450

layout(location = 0) in vec4 position;
layout(location = 0) out vec4 v_color;

layout(set = 0, binding = 0) uniform Data {
    mat4 transformation;
} uniforms;

void main() {
    vec4 result = uniforms.transformation * position;
    gl_Position = vec4(result[0], result[1] * -1.0, result[2], result[3]); // positive is up

    v_color = vec4(1.0, 0.0, 0.0, 1.0);
}

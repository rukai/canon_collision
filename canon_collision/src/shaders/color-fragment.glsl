#version 450

layout(location = 0) in vec4 v_color;
layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform Data {
    mat4 transformation;
} uniforms;

void main() {
    f_color = v_color;
}

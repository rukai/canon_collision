#version 440

layout(location = 0) in vec4 a_position;
layout(location = 1) in vec4 a_color;
layout(location = 0) out vec4 v_color;

layout(set = 0, binding = 0) uniform Data {
    mat4 u_transform;
};

void main() {
    gl_Position = u_transform * a_position;

    v_color = a_color;
}

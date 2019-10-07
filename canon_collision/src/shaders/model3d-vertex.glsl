#version 450

layout(location = 0) in vec4 a_position;
layout(location = 1) in vec2 a_uv;
layout(location = 0) out vec2 v_uv;

layout(set = 0, binding = 0) uniform Data {
    mat4 u_transform;
};

void main() {
    gl_Position = u_transform * a_position;
    gl_Position.y *= -1;

    v_uv = a_uv;
}

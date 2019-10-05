#version 450

layout(location = 0) in vec4 a_position;
layout(location = 0) out vec4 v_color;

layout(set = 0, binding = 0) uniform Data {
    mat4 u_transform;
};

void main() {
    gl_Position = u_transform * a_position;
    gl_Position.y *= -1;

    v_color = vec4(1.0, 0.0, 0.0, 1.0);
}

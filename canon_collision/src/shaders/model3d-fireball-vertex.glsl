#version 440

layout(location = 0) in vec4  a_position;
layout(location = 1) in vec2  a_uv;
layout(location = 2) in uvec4 a_joints;
layout(location = 3) in vec4  a_weights;

layout(location = 0) out vec2 v_uv;

layout(set = 0, binding = 0) uniform Data {
    mat4 u_transform;
    mat4 u_joint_transforms[500];
    float u_frame_count;
};

void main() {
    mat4 skin_transform =
        a_weights.x * u_joint_transforms[a_joints.x] +
        a_weights.y * u_joint_transforms[a_joints.y] +
        a_weights.z * u_joint_transforms[a_joints.z] +
        a_weights.w * u_joint_transforms[a_joints.w];

    vec4 flamed_position = skin_transform * a_position;

    flamed_position.z += sin(a_position.y + u_frame_count / 10) * max(a_position.y, 0) / 6;
    flamed_position.z += cos(a_position.y + u_frame_count / 25 + 1) * max(a_position.y, 0) / 9;
    flamed_position.z += sin(a_position.y + u_frame_count / 200 + 0.5) * max(a_position.y, 0) / 13;

    flamed_position.x += cos(a_position.y + u_frame_count / 30) * max(a_position.y, 0) / 8;

    flamed_position.y += sin(a_position.z + u_frame_count / 10) * max(a_position.y, 0) / 7;
    flamed_position.y += cos(a_position.x + u_frame_count / 25 + 1) * max(a_position.y, 0) / 14;

    gl_Position = u_transform * flamed_position;

    v_uv = a_uv;
}

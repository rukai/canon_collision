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

    gl_Position = u_transform * skin_transform * a_position;

    v_uv = a_uv;
}

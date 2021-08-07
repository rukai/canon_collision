struct VertexOutput {
    [[location(0)]] uv: vec2<f32>;
    [[builtin(position)]] position: vec4<f32>;
};

[[block]]
struct Locals {
    transform: mat4x4<f32>;
    joint_transforms: array<mat4x4<f32>, 500>;
    frame_count: f32;
};
[[group(0), binding(0)]]
var locals: Locals;

[[stage(vertex)]]
fn vs_main_animated(
    [[location(0)]] position: vec4<f32>,
    [[location(1)]] uv: vec2<f32>,
    [[location(2)]] joints: vec4<u32>,
    [[location(3)]] weights: vec4<f32>,
) -> VertexOutput { 
    var out: VertexOutput;
    let skin_transform: mat4x4<f32> =
        weights.x * locals.joint_transforms[joints.x] +
        weights.y * locals.joint_transforms[joints.y] +
        weights.z * locals.joint_transforms[joints.z] +
        weights.w * locals.joint_transforms[joints.w];

    out.position = locals.transform * skin_transform * position;
    out.uv = uv;
    return out;
}

[[group(0), binding(1)]]
var texture: texture_2d<f32>;
[[group(0), binding(2)]]
var sampler: sampler;

[[stage(fragment)]]
fn fs_standard_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return textureSample(texture, sampler, in.uv);
}

// TODO: implement:
//*  fireball-vertex
//*  static-vertex
//*  lava-fragment
// Not sure if I can combine them all in here or not...

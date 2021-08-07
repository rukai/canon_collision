struct VertexOutput {
    [[location(0)]] edge: f32;
    [[location(1)]] render_id: u32;
    [[builtin(position)]] position: vec4<f32>;
};

[[block]]
struct Locals {
    edge_color: vec4<f32>;
    color: vec4<f32>;
    transform: mat4x4<f32>;
};
[[group(0), binding(0)]]
var locals: Locals;

[[stage(vertex)]]
fn vs_main(
    [[location(0)]] position: vec2<f32>,
    [[location(1)]] edge: f32,
    [[location(2)]] render_id: u32,
) -> VertexOutput { 
    var out: VertexOutput;
    out.position = locals.transform * vec4<f32>(position, 0.0, 1.0);
    out.edge = edge;
    out.render_id = render_id;
    return out;
}

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    let e: f32 = fwidth(in.edge);
    if (in.render_id == 0u32) {
        return locals.color;
    }
    elseif (in.render_id == 1u32) {
        let value: f32 = smoothStep(0.8 - e, 0.8 + e, in.edge);
        // TODO: this mix fails on wgpu latest release but is fine on latest master (I can see a passing test there)
        //return mix(locals.color, locals.edge_color, 0.1);
        return vec4<f32>(0.0, 0.0, 1.0, 1.0);
    }
    elseif (in.render_id == 2u32) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0);
    }
    elseif (in.render_id == 3u32) {
        return vec4<f32>(0.76, 0.106, 0.843, 1.0);
    }
    elseif (in.render_id == 4u32) {
        if (in.edge > 0.8) {
            let a: vec4<f32> = locals.edge_color;
            return vec4<f32>(a[0], a[1], a[2], 0.5);
        }
        else {
            let a: vec4<f32> = locals.color;
            return vec4<f32>(a[0], a[1], a[2], 0.3);
        }
    }
    elseif (in.render_id == 5u32) {
        return vec4<f32>(0.52, 0.608, 0.756, 1.0);
    }
    elseif (in.render_id == 6u32) {
        return vec4<f32>(0.0, 0.64, 0.0, 1.0);
    }
    elseif (in.render_id == 7u32) {
        return vec4<f32>(0.8, 0.8, 0.8, 1.0);
    }
    elseif (in.render_id == 8u32) {
        return vec4<f32>(0.0, 0.0, 1.0, 1.0);
    }
    else {
        // use magenta as error
        return vec4<f32>(1.0, 0.0, 1.0, 1.0);
    }
}

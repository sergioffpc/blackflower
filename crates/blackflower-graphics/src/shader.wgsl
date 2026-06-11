// Vertex stage applies model * view_proj to transform vertex positions.
// Fragment stage outputs the per-face color.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct ModelTransform {
    model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

// Per-instance model matrices. Indexed by the instance index of the
// single instanced draw call. A storage buffer (not uniform) so it can
// hold far more entries than the 64 KB uniform window allows.
@group(1) @binding(0) var<storage, read> models: array<ModelTransform>;


struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance: u32) -> VertexOutput {
    var out: VertexOutput;
    let model = models[instance].model;
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

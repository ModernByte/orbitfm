struct Camera {
    view_proj: mat4x4<f32>,
    light_direction: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var earth_texture: texture_2d<f32>;
@group(0) @binding(2) var earth_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4(input.position, 1.0);
    output.normal = input.normal;
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let light = normalize(camera.light_direction.xyz);
    let diffuse = max(dot(normal, light), 0.0);
    let color = textureSample(earth_texture, earth_sampler, input.uv);
    let shaded = color.rgb * (0.25 + 0.75 * diffuse);
    return vec4(shaded, 1.0);
}

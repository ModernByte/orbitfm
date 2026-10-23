struct Camera {
    view_proj: mat4x4<f32>,
    light_direction: vec4<f32>,
    camera_position: vec4<f32>,
    pulse: vec4<f32>,
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
    @location(2) world_position: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4(input.position, 1.0);
    output.normal = input.normal;
    output.uv = input.uv;
    output.world_position = input.position;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let light = normalize(camera.light_direction.xyz);
    let diffuse = max(dot(normal, light), 0.0);
    let color = textureSample(earth_texture, earth_sampler, input.uv);
    let shaded = color.rgb * (0.25 + 0.75 * diffuse);

    // Сигнал эфира расширяет и усиливает Fresnel-обод атмосферы.
    let pulse = clamp(camera.pulse.x, 0.0, 1.0);
    let view = normalize(camera.camera_position.xyz - input.world_position);
    let facing = clamp(dot(normal, view), 0.0, 1.0);
    let falloff = mix(3.2, 1.8, pulse);
    let fresnel = pow(1.0 - facing, falloff);
    let rim_strength = fresnel * (0.14 + 0.55 * pulse);
    let rim_color = mix(
        vec3<f32>(0.35, 0.60, 1.0),
        vec3<f32>(0.35, 0.95, 0.85),
        pulse,
    );
    return vec4(shaded + rim_color * rim_strength, 1.0);
}

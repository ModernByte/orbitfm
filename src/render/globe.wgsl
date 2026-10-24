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

/// Дыхание планеты в такт эфиру: доли процента радиуса, чтобы не ломать
/// геометрию и не спорить с попаданием луча в сферу радиусом 1.
const PULSE_SCALE: f32 = 0.005;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pulse = clamp(camera.pulse.x, 0.0, 1.0);
    let world_position = input.position * (1.0 + PULSE_SCALE * pulse);
    output.clip_position = camera.view_proj * vec4(world_position, 1.0);
    output.normal = input.normal;
    output.uv = input.uv;
    output.world_position = world_position;
    return output;
}

/// Линейный sRGB → кодированные значения. Цель wgpu у egui — `Bgra8Unorm`
/// (без аппаратного sRGB-энкода), а цвета egui приходят уже в sRGB, поэтому
/// кодирование делаем руками: без него Земля уходит в почти чёрный.
fn encode_srgb(linear: vec3<f32>) -> vec3<f32> {
    let lo = linear * 12.92;
    let hi = 1.055 * pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return clamp(select(hi, lo, linear <= vec3<f32>(0.0031308)), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let light = normalize(camera.light_direction.xyz);
    let color = textureSample(earth_texture, earth_sampler, input.uv);

    // Half-lambert: ночная сторона остаётся читаемой, день получает тёплый key.
    let wrapped = dot(normal, light) * 0.5 + 0.5;
    let key = pow(wrapped, 1.6);
    let warm = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.0, 0.92, 0.82), key);
    var shaded = color.rgb * (0.42 + 0.62 * key) * warm;
    // Лёгкий подъём сатурации вместо умножения яркости: клиппинг ниже.
    let luma = dot(shaded, vec3<f32>(0.2126, 0.7152, 0.0722));
    shaded = mix(vec3<f32>(luma), shaded, 1.12);

    // Сигнал эфира расширяет и усиливает Fresnel-обод атмосферы.
    let pulse = clamp(camera.pulse.x, 0.0, 1.0);
    let view = normalize(camera.camera_position.xyz - input.world_position);
    let facing = clamp(dot(normal, view), 0.0, 1.0);
    let rim = 1.0 - facing;
    let fresnel = pow(rim, mix(3.2, 1.8, pulse));
    let rim_strength = fresnel * (0.12 + 0.4 * pulse);
    let rim_color = mix(
        vec3<f32>(0.35, 0.60, 1.0),
        vec3<f32>(0.35, 0.95, 0.85),
        pulse,
    );
    // Узкая кромка: обод виден и там, где текстура уходит за горизонт сферы.
    let edge = pow(rim, 9.0) * 0.2;
    return vec4(encode_srgb(shaded + rim_color * (rim_strength + edge)), 1.0);
}

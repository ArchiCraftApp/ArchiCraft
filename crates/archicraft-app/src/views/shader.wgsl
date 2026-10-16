// 3D view shading (VIEW-050). Colours arrive as sRGB; lighting is computed in
// linear space and the result is written back as sRGB, which is what egui
// expects from the texture it composites.

struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    // x: 1 = hidden-line mode (faces take the background colour, unlit)
    // y: ambient light
    params: vec4<f32>,
    background: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = u.view_proj * vec4<f32>(v.position, 1.0);
    out.normal = v.normal;
    out.color = v.color;
    return out;
}

fn linear_from_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c < vec3<f32>(0.04045));
}

fn srgb_from_linear(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c < vec3<f32>(0.0031308));
}

@fragment
fn fs_fill(in: VertexOut) -> @location(0) vec4<f32> {
    if (u.params.x > 0.5) {
        return vec4<f32>(u.background.rgb, 1.0);
    }
    let n = normalize(in.normal);
    let key = max(dot(n, normalize(u.light_dir.xyz)), 0.0);
    let fill = max(dot(n, normalize(vec3<f32>(-0.5, 0.7, 0.2))), 0.0) * 0.2;
    let sky = 0.6 + 0.4 * n.z;
    let shade = u.params.y * sky + key * 0.6 + fill;
    let rgb = srgb_from_linear(clamp(linear_from_srgb(in.color.rgb) * shade, vec3<f32>(0.0), vec3<f32>(1.0)));
    return vec4<f32>(rgb * in.color.a, in.color.a);
}

@fragment
fn fs_line(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
}

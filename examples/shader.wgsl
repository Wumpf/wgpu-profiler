struct VertexOutput {
    @location(0) coord: vec2<f32>,
    @location(1) instance: f32,
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    var x: f32 = f32(((vertex_index + 2u) / 3u) % 2u);
    var y: f32 = f32(((vertex_index + 1u) / 3u) % 2u);
    out.coord = vec2<f32>(x, y);

    x = x - f32(instance_index % 2u);
    y = y - f32(instance_index / 2u);
    out.pos = vec4<f32>(x, y, 0.0, 1.0);

    out.instance = f32(instance_index);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var c: vec2<f32> = vec2<f32>(-0.79, 0.15);
    if (in.instance == 0.0) {
        c = vec2<f32>(-1.476, 0.0);
    }
    if (in.instance == 1.0) {
        c = vec2<f32>(0.28, 0.008);
    }
    if (in.instance == 2.0) {
        c = vec2<f32>(-0.12, -0.77);
    }

    var max_iter: u32 = 200u;
    var z: vec2<f32> = (in.coord.xy - vec2<f32>(0.5, 0.5)) * 3.0;

    var i: u32 = 0u;
    loop {
        if (i >= max_iter) {
            break;
        }
        z = vec2<f32>(z.x * z.x - z.y * z.y, z.x * z.y + z.y * z.x) + c;
        if (dot(z, z) > 4.0) {
            break;
        }
        continuing {
            i = i + 1u;
        }
    }

    var t: f32 = f32(i) / f32(max_iter);
    return vec4<f32>(t * 3.0, t * 3.0 - 1.0, t * 3.0 - 2.0, 1.0);
    //return vec4<f32>(in.coord.x, in.coord.y, 0.0, 1.0);
}

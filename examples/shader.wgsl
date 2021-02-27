[[builtin(vertex_index)]]
var<in> in_vertex_index: u32;
[[builtin(instance_index)]]
var<in> in_instance_index: u32;
[[builtin(position)]]
var<out> out_pos: vec4<f32>;
[[location(0)]]
var<out> out_coord: vec2<f32>;
[[location(1)]]
var<out> out_instance: f32;

[[stage(vertex)]]
fn vs_main() {
    var x: f32 = f32(((i32(in_vertex_index) + 2u) / 3u) % 2u);
    var y: f32 = f32(((i32(in_vertex_index) + 1u) / 3u) % 2u);
    out_coord = vec2<f32>(x, y, 0.0, 1.0);

    x = x - f32(in_instance_index % 2u);
    y = y - f32(in_instance_index / 2u);
    out_pos = vec4<f32>(x, y, 0.0, 1.0);

    out_instance = f32(in_instance_index);
}

[[location(0)]]
var<in> in_coord: vec2<f32>;
[[location(1)]]
var<in> in_instance: f32;
[[location(0)]]
var<out> out_color: vec4<f32>;

[[stage(fragment)]]
fn fs_main() {
    var c: vec2<f32> = vec2<f32>(-0.79, 0.15);
    if (in_instance == 0.0) {
        c = vec2<f32>(-1.476, 0.0);
    }
    if (in_instance == 1.0) {
        c = vec2<f32>(0.28, 0.008);
    }
    if (in_instance == 2.0) {
        c = vec2<f32>(-0.12, -0.77);
    }

    var max_iter: i32 = 200;
    var z: vec2<f32> = (in_coord.xy - vec2<f32>(0.5, 0.5)) * 3.0;

    var i: i32 = 0;
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
    out_color = vec4<f32>(t * 3.0, t * 3.0 - 1.0, t * 3.0 - 2.0, 1.0);
    //out_color = vec4<f32>(in_coord.x, in_coord.y, 0.0, 1.0);
}

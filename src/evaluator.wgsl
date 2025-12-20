fn func(input: vec2f) -> f32 {
    const freq_scale = 5.0;
    const height_scale = 2.0;
    let x = input.x * freq_scale;
    let y = input.y * freq_scale;
    let r = sqrt(x*x + y*y);
    if (r == 0) {
        return height_scale;
    }
    var out = height_scale*sin(r)/r;
    return out;
}

@group(0) @binding(0) var<storage, read_write> vertex_buffer: array<f32>;

const ELEMENT_SIZE = 6;

@compute @workgroup_size(256)
fn evaluate(@builtin(global_invocation_id) gid: vec3u)
{
    let e: u32 = gid.x*ELEMENT_SIZE;
    if (e + ELEMENT_SIZE) > arrayLength(&vertex_buffer) {
        return;
    }
    let input = vec2f(vertex_buffer[e], vertex_buffer[e + 1]);
    let value = func(input);
    vertex_buffer[e + 2] = value;

    let color = (value + 0.5) * 0.4;
    // TODO
    vertex_buffer[e + 3] = color;
    vertex_buffer[e + 4] = color;
    vertex_buffer[e + 5] = color;
}

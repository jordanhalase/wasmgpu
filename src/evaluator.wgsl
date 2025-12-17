fn func(input: vec2f) -> f32 {
    var out = 0.15*(input.x*input.x + input.y*input.y);
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
    vertex_buffer[e + 2] = func(input);

    // TODO
    //vertex_buffer[e + 3] = f32(gid.x)/f32(grid.resolution.y-1);
    //vertex_buffer[e + 4] = f32(gid.y)/f32(grid.resolution.x-1);
    //vertex_buffer[e + 5] = 0.0;
}

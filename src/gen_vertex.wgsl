const ELEMENT_SIZE = 6;
const GRID_SIZE = 32; // TODO: Get from host

@group(0) @binding(0) var<storage, read_write> vertex_buffer: array<f32>;

// TODO: Set from host code
const xy_min = -1.0;
const xy_max =  1.0;

@compute @workgroup_size(16, 16)
fn generate_vertex_buffer(@builtin(global_invocation_id) gid: vec3u)
{
    let i: u32 = (gid.y*GRID_SIZE + gid.x);
    let e: u32 = i*ELEMENT_SIZE;
    if ((e + ELEMENT_SIZE) > arrayLength(&vertex_buffer)) {
        return;
    }
    vertex_buffer[e    ] = mix(xy_min, xy_max, f32(gid.x)/GRID_SIZE);
    vertex_buffer[e + 1] = mix(xy_max, xy_min, f32(gid.y)/GRID_SIZE);
    vertex_buffer[e + 2] = 0.0;
    vertex_buffer[e + 3] = 0.3;
    vertex_buffer[e + 4] = 0.3;
    vertex_buffer[e + 5] = 0.3;
}

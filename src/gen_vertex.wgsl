struct GeneratorUniform {
    resolution: vec2u,
    x_range: vec2f,
    y_range: vec2f,
}

@group(0) @binding(0) var<storage, read_write> vertex_buffer: array<f32>;
@group(0) @binding(1) var<uniform> grid: GeneratorUniform;

const ELEMENT_SIZE = 6;

@compute @workgroup_size(16, 16)
fn generate_vertex_buffer(@builtin(global_invocation_id) gid: vec3u)
{
    let i: u32 = (gid.y*grid.resolution.x + gid.x);
    let e: u32 = i*ELEMENT_SIZE;
    if ((e + ELEMENT_SIZE) > arrayLength(&vertex_buffer)) {
        return;
    }
    vertex_buffer[e    ] = mix(grid.x_range.x, grid.x_range.y, f32(gid.x)/f32(grid.resolution.x));
    vertex_buffer[e + 1] = mix(grid.y_range.y, grid.y_range.x, f32(gid.y)/f32(grid.resolution.y));
    vertex_buffer[e + 2] = 0.0;
    vertex_buffer[e + 3] = f32(gid.x)/f32(grid.resolution.x-1);
    vertex_buffer[e + 4] = f32(gid.y)/f32(grid.resolution.y-1);
    vertex_buffer[e + 5] = 0.0;
}

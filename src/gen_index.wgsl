const ELEMENT_SIZE = 6;
const GRID_SIZE = 32; // TODO: Get from host

@group(0) @binding(0) var<storage, read_write> index_buffer: array<u32>;

@compute @workgroup_size(16, 16)
fn generate_index_buffer(@builtin(global_invocation_id) gid: vec3u)
{
    // i0 i2 -> 0 1 2 2 1 3
    // i1 i3
    let i0: u32 = (gid.y*GRID_SIZE + gid.x);
    let i1: u32 = (i0 + GRID_SIZE);
    let i2: u32 = (i0 + 1);

    if (gid.x + 1 >= GRID_SIZE) {
        return;
    }

    if (gid.y + GRID_SIZE >= GRID_SIZE*GRID_SIZE) {
        return;
    }

    let i3: u32 = i1 + 1;
    let e = ELEMENT_SIZE * (gid.y*(GRID_SIZE - 1) + gid.x);
    if ((e + ELEMENT_SIZE) > arrayLength(&index_buffer)) {
        return;
    }
    index_buffer[e    ] = i0;
    index_buffer[e + 1] = i1;
    index_buffer[e + 2] = i2;
    index_buffer[e + 3] = i2;
    index_buffer[e + 4] = i1;
    index_buffer[e + 5] = i3;
}

@group(0) @binding(0) var<storage, read_write> data: array<u32>;

@compute @workgroup_size(32) fn compute_main(@builtin(global_invocation_id) gid: vec3u)
{
    let i = gid.y*32 + gid.x;
    data[i] = i;
}

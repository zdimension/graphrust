// Common shader functions and constants

fn unpack_color(color: u32) -> vec3<f32> {
    return vec3<f32>(
        f32((color >> 16u) & 0xFFu) / 255.0,
        f32((color >> 8u) & 0xFFu) / 255.0,
        f32(color & 0xFFu) / 255.0
    );
}

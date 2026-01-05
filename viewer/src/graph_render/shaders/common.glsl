// Common shader functions and constants

const float neg_infinity = uintBitsToFloat(0xFF800000u);
const float nan = intBitsToFloat(int(0xFFC00000u));

vec3 unpack_color(uint color) {
    return vec3(
        float((color >> 16) & 0xFFu) / 255.0,
        float((color >> 8) & 0xFFu) / 255.0,
        float(color & 0xFFu) / 255.0
    );
}

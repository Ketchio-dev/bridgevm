#version 450

// Half-viewport triangle with no vertex buffer: covers the framebuffer region
// where x_ndc + y_ndc <= 0 (the top-left half in framebuffer coordinates).
void main() {
    const vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, 1.0)
    );
    gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
}

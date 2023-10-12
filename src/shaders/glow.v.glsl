// SPDX-License-Identifier: LGPL-3.0-or-later OR MPL-2.0
// This file is a part of `piet-hardware`.
//
// `piet-hardware` is free software: you can redistribute it and/or modify it under the
// terms of either:
//
// * GNU Lesser General Public License as published by the Free Software Foundation, either
//   version 3 of the License, or (at your option) any later version.
// * Mozilla Public License as published by the Mozilla Foundation, version 2.
//
// `piet-hardware` is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
// PURPOSE. See the GNU Lesser General Public License or the Mozilla Public License for more
// details.
//
// You should have received a copy of the GNU Lesser General Public License and the Mozilla
// Public License along with `piet-hardware`. If not, see <https://www.gnu.org/licenses/>.

// Vertex shader for OpenGL.
// Assume that the appropriate version of OpenGL is already set.

#ifdef GL_ES
precision mediump float;
#endif

// Vertex shader takes inputs of this form:
// struct Vertex {
//     position: [f32; 2],
//     uv: [f32; 2],
//     color: [u8; 4],
// }
in vec2 aPosition;
in vec2 aUv;
in vec4 aColor;

// Fragment shader expects color, texture coordinates and mask coordinates.
out vec4 fRgbaColor;
out vec2 fTexCoord;
out vec2 fMaskCoord;

// Uniforms:
// - transform: 3x3 matrix for transforming vertices.
// - viewportSize: size of the viewport in pixels.
uniform mat3 uTransform;
uniform vec2 uViewportSize;

void main() {
    // Transform the vertex position.
    vec3 pos = uTransform * vec3(aPosition, 1.0);
    pos /= pos.z;

    // Transform the vertex position to clip space.
    gl_Position = vec4(
        (2.0 * pos.x / uViewportSize.x) - 1.0,
        1.0 - (2.0 * pos.y / uViewportSize.y),
        0.0,
        1.0
    );

    // Transform to mask-space coordinates.
    fMaskCoord = vec2(
        pos.x / uViewportSize.x,
        pos.y / uViewportSize.y
    );

    // Pass through the texture coordinates and color.
    fTexCoord = aUv;
    fRgbaColor = aColor / 255.0;
}

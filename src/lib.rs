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

//! A GPU-accelerated backend for piet that uses the [`glow`] crate.
//!
//! [`glow`]: https://crates.io/crates/glow

pub use glow;
pub use piet_hardware::piet;

use glow::HasContext;

use piet::IntoBrush;
use piet_hardware::piet::{kurbo, Error as Pierror};

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::mem;
use std::rc::Rc;

macro_rules! c {
    ($e:expr) => {{
        ($e) as f32
    }};
}

const VERTEX_SHADER: &str = include_str!("./shaders/glow.v.glsl");
const FRAGMENT_SHADER: &str = include_str!("./shaders/glow.f.glsl");

#[derive(Debug, Clone, Copy)]
enum Uniforms {
    Transform = 0,
    ViewportSize = 1,
    ImageTexture = 2,
    MaskTexture = 3,
}

impl Uniforms {
    fn as_index(self) -> usize {
        self as usize
    }

    fn as_name(self) -> &'static str {
        match self {
            Uniforms::Transform => "uTransform",
            Uniforms::ViewportSize => "uViewportSize",
            Uniforms::ImageTexture => "uImage",
            Uniforms::MaskTexture => "uMask",
        }
    }
}

const UNIFORM_COUNT: usize = 4;
const UNIFORMS: [Uniforms; UNIFORM_COUNT] = [
    Uniforms::Transform,
    Uniforms::ViewportSize,
    Uniforms::ImageTexture,
    Uniforms::MaskTexture,
];

use Uniforms::*;

/// A wrapper around a `glow` context.
struct GpuContext<H: HasContext + ?Sized> {
    /// A compiled shader program for rendering.
    render_program: H::Program,

    /// The uniform locations.
    uniforms: Box<[H::UniformLocation]>,

    /// Do we need to check the indices?
    check_indices: bool,

    /// The underlying context.
    context: Rc<H>,
}

impl<H: HasContext + ?Sized> fmt::Debug for GpuContext<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuContext")
            .field("robust", &!self.check_indices)
            .finish_non_exhaustive()
    }
}

impl<H: HasContext + ?Sized> GpuContext<H> {
    fn uniform(&self, uniform: Uniforms) -> &H::UniformLocation {
        self.uniforms.get(uniform.as_index()).unwrap()
    }
}

impl<H: HasContext + ?Sized> Drop for GpuContext<H> {
    fn drop(&mut self) {
        unsafe {
            self.context.delete_program(self.render_program);
        }
    }
}

/// A wrapper around a `glow` texture.
struct GlTexture<H: HasContext + ?Sized> {
    /// The OpenGL context.
    context: Rc<H>,

    /// The underlying texture.
    texture: H::Texture,
}

impl<H: HasContext + ?Sized> Drop for GlTexture<H> {
    fn drop(&mut self) {
        unsafe {
            self.context.delete_texture(self.texture);
        }
    }
}

/// A wrapper around a `glow` vertex buffer.
struct GlVertexBuffer<H: HasContext + ?Sized> {
    /// The context.
    context: Rc<H>,

    /// The underlying vertex buffer.
    vbo: H::Buffer,

    /// The index buffer.
    ebo: H::Buffer,

    /// The vertex array object.
    vao: H::VertexArray,

    /// The number of indices.
    num_indices: Cell<usize>,
}

impl<H: HasContext + ?Sized> Drop for GlVertexBuffer<H> {
    fn drop(&mut self) {
        unsafe {
            self.context.delete_buffer(self.vbo);
            self.context.delete_buffer(self.ebo);
            self.context.delete_vertex_array(self.vao);
        }
    }
}

#[derive(Debug)]
struct GlError(String);

impl From<String> for GlError {
    fn from(s: String) -> Self {
        GlError(s)
    }
}

impl fmt::Display for GlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gl error: {}", self.0)
    }
}

impl std::error::Error for GlError {}

impl<H: HasContext + ?Sized> piet_hardware::GpuContext for GpuContext<H> {
    type Device = ();
    type Queue = ();
    type Texture = GlTexture<H>;
    type VertexBuffer = GlVertexBuffer<H>;
    type Error = GlError;

    fn clear(&mut self, _: &(), _: &(), color: piet_hardware::piet::Color) {
        let (r, g, b, a) = color.as_rgba();

        unsafe {
            self.context.clear_color(c!(r), c!(g), c!(b), c!(a));
            self.context.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        unsafe {
            self.context.flush();
        }

        Ok(())
    }

    fn create_texture(
        &mut self,
        _: &(),
        interpolation: piet_hardware::piet::InterpolationMode,
        repeat: piet_hardware::RepeatStrategy,
    ) -> Result<Self::Texture, Self::Error> {
        unsafe {
            let texture = self.context.create_texture().gl_err()?;

            // Bind the texture.
            self.context.bind_texture(glow::TEXTURE_2D, Some(texture));
            let _guard = CallOnDrop(|| {
                self.context.bind_texture(glow::TEXTURE_2D, None);
            });

            let (min_filter, mag_filter) = match interpolation {
                piet::InterpolationMode::NearestNeighbor => (glow::NEAREST, glow::NEAREST),
                piet::InterpolationMode::Bilinear => (glow::LINEAR, glow::LINEAR),
            };

            self.context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                min_filter as i32,
            );
            self.context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                mag_filter as i32,
            );

            let (wrap_s, wrap_t) = match repeat {
                piet_hardware::RepeatStrategy::Color(_color) => {
                    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm32")))]
                    {
                        let (r, g, b, a) = _color.as_rgba();
                        self.context.tex_parameter_f32_slice(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_BORDER_COLOR,
                            &[c!(r), c!(g), c!(b), c!(a)],
                        );
                    }

                    (glow::CLAMP_TO_BORDER, glow::CLAMP_TO_BORDER)
                }
                piet_hardware::RepeatStrategy::Repeat => (glow::REPEAT, glow::REPEAT),
                piet_hardware::RepeatStrategy::Clamp => (glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE),
                _ => panic!("unsupported repeat strategy: {repeat:?}"),
            };

            self.context
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_s as i32);
            self.context
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_t as i32);

            gl_error(&*self.context);

            Ok(GlTexture {
                context: self.context.clone(),
                texture,
            })
        }
    }

    fn write_texture(
        &mut self,
        _: &(),
        _: &(),
        texture: &Self::Texture,
        (width, height): (u32, u32),
        format: piet::ImageFormat,
        data: Option<&[u8]>,
    ) {
        let data_width = match format {
            piet::ImageFormat::Grayscale => 1,
            piet::ImageFormat::Rgb => 3,
            piet::ImageFormat::RgbaSeparate | piet::ImageFormat::RgbaPremul => 4,
            _ => panic!("unsupported image format: {format:?}"),
        };

        if let Some(data) = data {
            let total_len = usize::try_from(width)
                .ok()
                .and_then(|width| width.checked_mul(height.try_into().ok()?))
                .and_then(|total| total.checked_mul(data_width.try_into().ok()?))
                .expect("image data too large");
            assert_eq!(data.len(), total_len);
        }

        let grayscale_data;
        let mut data = data;

        unsafe {
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(texture.texture));
            let _guard = CallOnDrop(|| {
                self.context.bind_texture(glow::TEXTURE_2D, None);
            });

            let (internal_format, format, data_type) = match format {
                piet::ImageFormat::Grayscale => {
                    // TODO: Figure out the best way of working around grayscale being broken.
                    grayscale_data =
                        data.map(|data| data.iter().flat_map(|&v| [v, v, v]).collect::<Vec<_>>());
                    data = grayscale_data.as_deref();

                    (glow::RGB8, glow::RGB, glow::UNSIGNED_BYTE)
                }
                piet::ImageFormat::Rgb => (glow::RGB8, glow::RGB, glow::UNSIGNED_BYTE),
                piet::ImageFormat::RgbaPremul => (glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE),
                piet::ImageFormat::RgbaSeparate => (glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE),
                _ => panic!("unsupported image format: {format:?}"),
            };

            // Set texture parameters.
            self.context.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);

            self.context.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal_format as i32,
                width as i32,
                height as i32,
                0,
                format,
                data_type,
                data,
            );
        }

        gl_error(&*self.context);
    }

    fn write_subtexture(
        &mut self,
        _: &(),
        _: &(),
        texture: &Self::Texture,
        (x, y): (u32, u32),
        (width, height): (u32, u32),
        format: piet_hardware::piet::ImageFormat,
        data: &[u8],
    ) {
        let data_width = match format {
            piet::ImageFormat::Grayscale => 1,
            piet::ImageFormat::Rgb => 3,
            piet::ImageFormat::RgbaSeparate | piet::ImageFormat::RgbaPremul => 4,
            _ => panic!("unsupported image format: {format:?}"),
        };

        let total_len = (width * height * data_width) as usize;
        assert_eq!(data.len(), total_len);

        unsafe {
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(texture.texture));
            let _guard = CallOnDrop(|| {
                self.context.bind_texture(glow::TEXTURE_2D, None);
            });

            let (format, data_type) = match format {
                piet::ImageFormat::Grayscale => (glow::RED, glow::UNSIGNED_BYTE),
                piet::ImageFormat::Rgb => (glow::RGB, glow::UNSIGNED_BYTE),
                piet::ImageFormat::RgbaPremul => (glow::RGBA, glow::UNSIGNED_BYTE),
                piet::ImageFormat::RgbaSeparate => (glow::RGBA, glow::UNSIGNED_BYTE),
                _ => panic!("unsupported image format: {format:?}"),
            };

            self.context.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                x as i32,
                y as i32,
                width as i32,
                height as i32,
                format,
                data_type,
                glow::PixelUnpackData::Slice(data),
            );
        }

        gl_error(&*self.context);
    }

    fn set_texture_interpolation(
        &mut self,
        _: &(),
        texture: &Self::Texture,
        interpolation: piet_hardware::piet::InterpolationMode,
    ) {
        unsafe {
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(texture.texture));
            let _guard = CallOnDrop(|| {
                self.context.bind_texture(glow::TEXTURE_2D, None);
            });

            let (min_filter, mag_filter) = match interpolation {
                piet::InterpolationMode::NearestNeighbor => (glow::NEAREST, glow::NEAREST),
                piet::InterpolationMode::Bilinear => (glow::LINEAR, glow::LINEAR),
            };

            self.context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                min_filter as i32,
            );
            self.context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                mag_filter as i32,
            );
        }
    }

    fn capture_area(
        &mut self,
        _: &(),
        _: &(),
        texture: &Self::Texture,
        offset: (u32, u32),
        size: (u32, u32),
        scale: f64,
    ) -> Result<(), Self::Error> {
        let (x, y) = offset;
        let (width, height) = size;

        // Scale up by the bitmap scale.
        let (x, y, width, height) = (
            (x as f64 * scale).floor() as u32,
            (y as f64 * scale).floor() as u32,
            (width as f64 * scale).ceil() as u32,
            (height as f64 * scale).ceil() as u32,
        );

        // Create a buffer to hold the pixels.
        // A little bit more at the end is allocated for the inversion.
        let buffer_size = (width * (height + 1) * 4) as usize;
        let mut pixels = vec![0u8; buffer_size];
        let scratch_start = pixels.len() - (width * 4) as usize;

        // Read the pixels, making sure to invert the y axis.
        unsafe {
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(texture.texture));
            let _guard = CallOnDrop(|| {
                self.context.bind_texture(glow::TEXTURE_2D, None);
            });

            self.context.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            self.context.read_pixels(
                x as i32,
                y as i32,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut pixels),
            );
        }

        // If we got an error, propagate it.
        if unsafe { self.context.get_error() } != glow::NO_ERROR {
            // TODO: If this is an MSAA texture, maybe try the framebuffer trick.
            gl_error(&*self.context);
            return Err(GlError("failed to read pixels".into()));
        }

        // Invert the y axis, making sure to use the little bit at the end of the buffer as
        // temporary storage.
        let stride = width as usize * 4;
        for row in 0..(height / 2) as usize {
            let top = row * stride;
            let bottom = (height as usize - row - 1) * stride;

            pixels.copy_within(top..top + stride, scratch_start);
            pixels.copy_within(bottom..bottom + stride, top);
            pixels.copy_within(scratch_start..scratch_start + stride, bottom);
        }

        // Upload the pixels to the texture.
        unsafe {
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(texture.texture));
            let _guard = CallOnDrop(|| {
                self.context.bind_texture(glow::TEXTURE_2D, None);
            });

            self.context.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as _,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(&pixels),
            );
        }

        Ok(())
    }

    fn max_texture_size(&mut self, _: &()) -> (u32, u32) {
        unsafe {
            let size = self.context.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
            (size as u32, size as u32)
        }
    }

    fn create_vertex_buffer(&mut self, _: &()) -> Result<Self::VertexBuffer, Self::Error> {
        use piet_hardware::Vertex;

        unsafe {
            let vbo = self.context.create_buffer().gl_err()?;
            let ebo = self.context.create_buffer().gl_err()?;
            let vao = self.context.create_vertex_array().gl_err()?;

            // Bind the buffers.
            self.context.bind_vertex_array(Some(vao));
            let _guard = CallOnDrop(|| {
                self.context.bind_vertex_array(None);
            });
            self.context.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            self.context
                .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));

            // Set up vertex attributes.
            let vertex_attributes = [
                (
                    "aPosition",
                    2,
                    glow::FLOAT,
                    bytemuck::offset_of!(Vertex, pos),
                ),
                ("aUv", 2, glow::FLOAT, bytemuck::offset_of!(Vertex, uv)),
                (
                    "aColor",
                    4,
                    glow::UNSIGNED_BYTE,
                    bytemuck::offset_of!(Vertex, color),
                ),
            ];

            let stride = std::mem::size_of::<Vertex>() as i32;
            for (name, size, data_type, offset) in vertex_attributes {
                let location = self
                    .context
                    .get_attrib_location(self.render_program, name)
                    .ok_or_else(|| {
                        GlError(format!("failed to get attribute location for {name}"))
                    })?;

                self.context.enable_vertex_attrib_array(location);
                self.context.vertex_attrib_pointer_f32(
                    location,
                    size,
                    data_type,
                    false,
                    stride,
                    offset as i32,
                );
            }

            gl_error(&*self.context);

            Ok(GlVertexBuffer {
                context: self.context.clone(),
                vbo,
                ebo,
                vao,
                num_indices: Cell::new(0),
            })
        }
    }

    fn write_vertices(
        &mut self,
        _: &(),
        _: &(),
        buffer: &Self::VertexBuffer,
        vertices: &[piet_hardware::Vertex],
        indices: &[u32],
    ) {
        // Make sure we don't cause undefined behavior on platforms without robust buffer access.
        if self.check_indices {
            assert!(indices.iter().all(|&i| i < vertices.len() as u32));
        } else {
            debug_assert!(indices.iter().all(|&i| i < vertices.len() as u32));
        }

        unsafe {
            self.context.bind_vertex_array(Some(buffer.vao));
            let _guard = CallOnDrop(|| {
                self.context.bind_vertex_array(None);
            });

            self.context.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(vertices),
                glow::DYNAMIC_DRAW,
            );

            self.context.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(indices),
                glow::DYNAMIC_DRAW,
            );

            gl_error(&*self.context);

            buffer.num_indices.set(indices.len());
        }
    }

    fn push_buffers(
        &mut self,
        _: &(),
        _: &(),
        vertex_buffer: &Self::VertexBuffer,
        current_texture: &Self::Texture,
        mask_texture: &Self::Texture,
        transform: &piet_hardware::piet::kurbo::Affine,
        size: (u32, u32),
    ) -> Result<(), Self::Error> {
        unsafe {
            // Use our program.
            self.context.use_program(Some(self.render_program));
            let _unbind_program = CallOnDrop(|| {
                self.context.use_program(None);
            });

            // Set viewport size.
            self.context.viewport(0, 0, size.0 as i32, size.1 as i32);
            self.context.uniform_2_f32(
                Some(self.uniform(ViewportSize)),
                size.0 as f32,
                size.1 as f32,
            );

            // Set the transform.
            let [a, b, c, d, e, f] = transform.as_coeffs();
            let transform = [
                c!(a),
                c!(b),
                c!(0.0),
                c!(c),
                c!(d),
                c!(0.0),
                c!(e),
                c!(f),
                c!(1.0),
            ];
            self.context.uniform_matrix_3_f32_slice(
                Some(self.uniform(Transform)),
                false,
                &transform,
            );

            // Set the image texture.
            self.context.active_texture(glow::TEXTURE1);
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(current_texture.texture));
            self.context
                .uniform_1_i32(Some(self.uniform(ImageTexture)), 1);

            // Set the mask texture.
            self.context.active_texture(glow::TEXTURE0);
            self.context
                .bind_texture(glow::TEXTURE_2D, Some(mask_texture.texture));
            self.context
                .uniform_1_i32(Some(self.uniform(MaskTexture)), 0);

            // Enable blending.
            self.context.enable(glow::BLEND);
            self.context
                .blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
            self.context.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            self.context.enable(glow::MULTISAMPLE);

            // Set the vertex array.
            self.context.bind_vertex_array(Some(vertex_buffer.vao));
            let _unbind_vao = CallOnDrop(|| {
                self.context.bind_vertex_array(None);
            });

            // Draw the triangles.
            self.context.draw_elements(
                glow::TRIANGLES,
                vertex_buffer.num_indices.get() as i32,
                glow::UNSIGNED_INT,
                0,
            );

            gl_error(&*self.context);

            Ok(())
        }
    }
}

/// A wrapper around a [`glow`] context with cached information.
#[derive(Debug)]
pub struct GlContext<H: HasContext + ?Sized> {
    text: Text,
    source: piet_hardware::Source<GpuContext<H>>,
}

impl<H: HasContext + ?Sized> GlContext<H> {
    /// Create a new [`GlContext`] from a [`glow`] context.
    ///
    /// # Safety
    ///
    /// The context must be current while calling new, and the context must be current
    /// when this type is dropped.
    pub unsafe fn new(context: H) -> Result<Self, Pierror>
    where
        H: Sized,
    {
        // Get the current version.
        let version = context.version();

        // Check that the version is supported.
        let has_supported_version = if version.is_embedded {
            version.major >= 3
        } else {
            version.major >= 4 || (version.major >= 3 && version.minor >= 3)
        };
        if !has_supported_version {
            return Err(Pierror::BackendError(
                "OpenGL version 3.3 (or 3.0 ES) or higher is required".into(),
            ));
        }

        let shader_header = if version.is_embedded {
            "#version 300 es"
        } else {
            "#version 330 core"
        };

        let format_shader = |shader| format!("{shader_header}\n{shader}");

        // Create a program to use for text rendering.
        let program = compile_program(
            &context,
            &format_shader(VERTEX_SHADER),
            &format_shader(FRAGMENT_SHADER),
        )
        .map_err(|e| Pierror::BackendError(e.into()))?;

        // Get the uniform locations.
        let uniforms = UNIFORMS
            .iter()
            .map(|uniform| {
                context
                    .get_uniform_location(program, uniform.as_name())
                    .ok_or_else(|| {
                        Pierror::BackendError(
                            format!("failed to get uniform location for {}", uniform.as_name())
                                .into(),
                        )
                    })
            })
            .collect::<Result<Box<[_]>, _>>()?;

        let robust_buffer = context
            .supported_extensions()
            .contains("GL_ARB_robust_buffer_access_behavior")
            || context
                .supported_extensions()
                .contains("GL_KHR_robust_buffer_access_behavior");

        piet_hardware::Source::new(
            GpuContext {
                context: Rc::new(context),
                uniforms,
                check_indices: !robust_buffer,
                render_program: program,
            },
            &(),
            &(),
        )
        .map(|source| GlContext {
            text: Text(source.text().clone()),
            source,
        })
    }

    /// Get a reference to the underlying [`glow`] context.
    pub fn context(&self) -> &H {
        &self.source.context().context
    }

    /// Get a render context.
    ///
    /// # Safety
    ///
    /// The context must be current while calling this method, as well as any of the
    /// [`piet::RenderContext`] methods.
    pub unsafe fn render_context(&mut self, width: u32, height: u32) -> RenderContext<'_, H> {
        RenderContext {
            context: self.source.render_context(&(), &(), width, height),
            text: &mut self.text,
        }
    }
}

/// The whole point.
pub struct RenderContext<'a, H: HasContext + ?Sized> {
    context: piet_hardware::RenderContext<'a, 'static, 'static, GpuContext<H>>,
    text: &'a mut Text,
}

impl<'a, H: HasContext + ?Sized> RenderContext<'a, H> {
    /// Get the tolerance for flattening curves.
    #[inline]
    pub fn tolerance(&self) -> f64 {
        self.context.tolerance()
    }

    /// Set the tolerance for flattening curves.
    #[inline]
    pub fn set_tolerance(&mut self, tolerance: f64) {
        self.context.set_tolerance(tolerance)
    }

    /// Get the current bitmap scale.
    #[inline]
    pub fn bitmap_scale(&self) -> f64 {
        self.context.bitmap_scale()
    }

    /// Set the current bitmap scale.
    #[inline]
    pub fn set_bitmap_scale(&mut self, scale: f64) {
        self.context.set_bitmap_scale(scale)
    }
}

impl<H: HasContext + ?Sized> piet::RenderContext for RenderContext<'_, H> {
    type Brush = Brush<H>;

    type Text = Text;

    type TextLayout = TextLayout;

    type Image = Image<H>;

    fn status(&mut self) -> Result<(), Pierror> {
        self.context.status()
    }

    fn solid_brush(&mut self, color: piet::Color) -> Self::Brush {
        Brush(self.context.solid_brush(color))
    }

    fn gradient(
        &mut self,
        gradient: impl Into<piet::FixedGradient>,
    ) -> Result<Self::Brush, Pierror> {
        self.context.gradient(gradient).map(Brush)
    }

    fn clear(&mut self, region: impl Into<Option<kurbo::Rect>>, color: piet::Color) {
        self.context.clear(region, color)
    }

    fn stroke(&mut self, shape: impl kurbo::Shape, brush: &impl IntoBrush<Self>, width: f64) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.context.stroke(shape, &brush.as_ref().0, width)
    }

    fn stroke_styled(
        &mut self,
        shape: impl kurbo::Shape,
        brush: &impl IntoBrush<Self>,
        width: f64,
        style: &piet::StrokeStyle,
    ) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.context
            .stroke_styled(shape, &brush.as_ref().0, width, style)
    }

    fn fill(&mut self, shape: impl kurbo::Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.context.fill(shape, &brush.as_ref().0)
    }

    fn fill_even_odd(&mut self, shape: impl kurbo::Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.context.fill_even_odd(shape, &brush.as_ref().0)
    }

    fn clip(&mut self, shape: impl kurbo::Shape) {
        self.context.clip(shape)
    }

    fn text(&mut self) -> &mut Self::Text {
        self.text
    }

    fn draw_text(&mut self, layout: &Self::TextLayout, pos: impl Into<kurbo::Point>) {
        self.context.draw_text(&layout.0, pos)
    }

    fn save(&mut self) -> Result<(), Pierror> {
        self.context.save()
    }

    fn restore(&mut self) -> Result<(), Pierror> {
        self.context.restore()
    }

    fn finish(&mut self) -> Result<(), Pierror> {
        self.context.finish()?;

        // Free all of the resources as well.
        self.context.source_mut().gpu_flushed();

        Ok(())
    }

    fn transform(&mut self, transform: kurbo::Affine) {
        self.context.transform(transform)
    }

    fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: piet::ImageFormat,
    ) -> Result<Self::Image, Pierror> {
        self.context
            .make_image(width, height, buf, format)
            .map(Image)
    }

    fn draw_image(
        &mut self,
        image: &Self::Image,
        dst_rect: impl Into<kurbo::Rect>,
        interp: piet::InterpolationMode,
    ) {
        self.context.draw_image(&image.0, dst_rect, interp)
    }

    fn draw_image_area(
        &mut self,
        image: &Self::Image,
        src_rect: impl Into<kurbo::Rect>,
        dst_rect: impl Into<kurbo::Rect>,
        interp: piet::InterpolationMode,
    ) {
        self.context
            .draw_image_area(&image.0, src_rect, dst_rect, interp)
    }

    fn capture_image_area(
        &mut self,
        src_rect: impl Into<kurbo::Rect>,
    ) -> Result<Self::Image, Pierror> {
        self.context.capture_image_area(src_rect).map(Image)
    }

    fn blurred_rect(&mut self, rect: kurbo::Rect, blur_radius: f64, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || rect);
        self.context
            .blurred_rect(rect, blur_radius, &brush.as_ref().0)
    }

    fn current_transform(&self) -> kurbo::Affine {
        self.context.current_transform()
    }
}

/// The brush type.
#[derive(Debug)]
pub struct Brush<H: HasContext + ?Sized>(piet_hardware::Brush<GpuContext<H>>);

impl<H: HasContext + ?Sized> Clone for Brush<H> {
    fn clone(&self) -> Self {
        Brush(self.0.clone())
    }
}

impl<H: HasContext + ?Sized> IntoBrush<RenderContext<'_, H>> for Brush<H> {
    fn make_brush<'a>(
        &'a self,
        _piet: &mut RenderContext<'_, H>,
        _bbox: impl FnOnce() -> kurbo::Rect,
    ) -> Cow<'a, Brush<H>> {
        Cow::Borrowed(self)
    }
}

/// The image type.
#[derive(Debug)]
pub struct Image<H: HasContext + ?Sized>(piet_hardware::Image<GpuContext<H>>);

impl<H: HasContext + ?Sized> Clone for Image<H> {
    fn clone(&self) -> Self {
        Image(self.0.clone())
    }
}

impl<H: HasContext + ?Sized> piet::Image for Image<H> {
    fn size(&self) -> kurbo::Size {
        self.0.size()
    }
}

/// The text layout type.
#[derive(Debug, Clone)]
pub struct TextLayout(piet_hardware::TextLayout);

impl piet::TextLayout for TextLayout {
    fn size(&self) -> kurbo::Size {
        self.0.size()
    }

    fn line_text(&self, line_number: usize) -> Option<&str> {
        self.0.line_text(line_number)
    }

    fn line_metric(&self, line_number: usize) -> Option<piet::LineMetric> {
        self.0.line_metric(line_number)
    }

    fn line_count(&self) -> usize {
        self.0.line_count()
    }

    fn hit_test_point(&self, point: kurbo::Point) -> piet::HitTestPoint {
        self.0.hit_test_point(point)
    }

    fn trailing_whitespace_width(&self) -> f64 {
        self.0.trailing_whitespace_width()
    }

    fn image_bounds(&self) -> kurbo::Rect {
        self.0.image_bounds()
    }

    fn text(&self) -> &str {
        self.0.text()
    }

    fn hit_test_text_position(&self, idx: usize) -> piet::HitTestPosition {
        self.0.hit_test_text_position(idx)
    }
}

/// The text layout builder type.
#[derive(Debug)]
pub struct TextLayoutBuilder(piet_hardware::TextLayoutBuilder);

impl piet::TextLayoutBuilder for TextLayoutBuilder {
    type Out = TextLayout;

    fn max_width(self, width: f64) -> Self {
        Self(self.0.max_width(width))
    }

    fn alignment(self, alignment: piet::TextAlignment) -> Self {
        Self(self.0.alignment(alignment))
    }

    fn default_attribute(self, attribute: impl Into<piet::TextAttribute>) -> Self {
        Self(self.0.default_attribute(attribute))
    }

    fn range_attribute(
        self,
        range: impl std::ops::RangeBounds<usize>,
        attribute: impl Into<piet::TextAttribute>,
    ) -> Self {
        Self(self.0.range_attribute(range, attribute))
    }

    fn build(self) -> Result<Self::Out, Pierror> {
        Ok(TextLayout(self.0.build()?))
    }
}

/// The text engine type.
#[derive(Debug, Clone)]
pub struct Text(piet_hardware::Text);

impl Text {
    /// Get the DPI scale.
    pub fn dpi(&self) -> f64 {
        self.0.dpi()
    }

    /// Set the DPI scale.
    pub fn set_dpi(&mut self, dpi: f64) {
        self.0.set_dpi(dpi)
    }
}

impl piet::Text for Text {
    type TextLayoutBuilder = TextLayoutBuilder;
    type TextLayout = TextLayout;

    fn font_family(&mut self, family_name: &str) -> Option<piet::FontFamily> {
        self.0.font_family(family_name)
    }

    fn load_font(&mut self, data: &[u8]) -> Result<piet::FontFamily, Pierror> {
        self.0.load_font(data)
    }

    fn new_text_layout(&mut self, text: impl piet::TextStorage) -> Self::TextLayoutBuilder {
        TextLayoutBuilder(self.0.new_text_layout(text))
    }
}

fn compile_program<H: HasContext + ?Sized>(
    context: &H,
    vertex_shader: &str,
    fragment_shader: &str,
) -> Result<H::Program, GlError> {
    unsafe {
        let vertex_shader = compile_shader(context, glow::VERTEX_SHADER, vertex_shader)?;
        let fragment_shader = compile_shader(context, glow::FRAGMENT_SHADER, fragment_shader)?;

        let program = context.create_program().gl_err()?;
        let _call_on_drop = CallOnDrop(|| context.delete_program(program));

        context.attach_shader(program, vertex_shader);
        context.attach_shader(program, fragment_shader);
        let _unlink_shaders = CallOnDrop(|| {
            context.detach_shader(program, vertex_shader);
            context.detach_shader(program, fragment_shader);
            context.delete_shader(vertex_shader);
            context.delete_shader(fragment_shader);
        });
        context.link_program(program);

        if !context.get_program_link_status(program) {
            let log = context.get_program_info_log(program);
            return Err(GlError(log));
        }

        mem::forget(_call_on_drop);
        Ok(program)
    }
}

unsafe fn compile_shader<H: HasContext + ?Sized>(
    context: &H,
    shader_type: u32,
    source: &str,
) -> Result<H::Shader, GlError> {
    let shader = context.create_shader(shader_type).gl_err()?;
    let _call_on_drop = CallOnDrop(|| context.delete_shader(shader));

    context.shader_source(shader, source);
    context.compile_shader(shader);

    if !context.get_shader_compile_status(shader) {
        let log = context.get_shader_info_log(shader);
        return Err(GlError(log));
    }

    mem::forget(_call_on_drop);
    Ok(shader)
}

fn gl_error(h: &(impl HasContext + ?Sized)) {
    let err = unsafe { h.get_error() };

    if err != glow::NO_ERROR {
        let error_str = match err {
            glow::INVALID_ENUM => "GL_INVALID_ENUM",
            glow::INVALID_VALUE => "GL_INVALID_VALUE",
            glow::INVALID_OPERATION => "GL_INVALID_OPERATION",
            glow::STACK_OVERFLOW => "GL_STACK_OVERFLOW",
            glow::STACK_UNDERFLOW => "GL_STACK_UNDERFLOW",
            glow::OUT_OF_MEMORY => "GL_OUT_OF_MEMORY",
            glow::INVALID_FRAMEBUFFER_OPERATION => "GL_INVALID_FRAMEBUFFER_OPERATION",
            glow::CONTEXT_LOST => "GL_CONTEXT_LOST",
            _ => "Unknown GL error",
        };

        tracing::error!("GL error: {}", error_str)
    }
}

trait ResultExt<T, E> {
    fn gl_err(self) -> Result<T, GlError>;
}

impl<T, E: Into<GlError>> ResultExt<T, E> for Result<T, E> {
    fn gl_err(self) -> Result<T, GlError> {
        self.map_err(Into::into)
    }
}

struct CallOnDrop<F: FnMut()>(F);

impl<F: FnMut()> Drop for CallOnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

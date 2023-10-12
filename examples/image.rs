// SPDX-License-Identifier: LGPL-3.0-or-later OR MPL-2.0
// This file is a part of `piet-glow`.
//
// `piet-glow` is free software: you can redistribute it and/or modify it under the terms of
// either:
//
// * GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
// * Mozilla Public License as published by the Mozilla Foundation, version 2.
//
// `piet-glow` is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License or the Mozilla Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License and the Mozilla
// Public License along with `piet-glow`. If not, see <https://www.gnu.org/licenses/>.

//! An example with a basic usage of the library.

#[path = "util/setup_context.rs"]
mod util;

use piet::kurbo::Rect;
use piet::RenderContext as _;

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init();
    // Get the test image at $CRATE_ROOT/examples/assets/test-image.png
    let manifest_root = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(manifest_root).join("examples/assets/oranges.jpg");
    let image = image::open(path)?.to_rgba8();

    // Convert the image to a byte buffer.
    let size = image.dimensions();
    let image_data = image.into_raw();

    let mut image_handle = None;
    util::with_renderer(move |render_context, width, height| {
        // Create an image and draw it.
        let image = image_handle.get_or_insert_with(|| {
            render_context
                .make_image(
                    size.0 as _,
                    size.1 as _,
                    &image_data,
                    piet::ImageFormat::RgbaSeparate,
                )
                .unwrap()
        });

        render_context.draw_image(
            image,
            Rect::new(0.0, 0.0, width as f64, height as f64),
            piet::InterpolationMode::Bilinear,
        );
    })
}

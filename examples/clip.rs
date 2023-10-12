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

use piet::kurbo::{Circle, Rect};
use piet::{Color, FixedLinearGradient, GradientStop, RenderContext as _};

const RAINBOW: &[Color] = &[
    Color::rgb8(0xff, 0x00, 0x00),
    Color::rgb8(0xff, 0x7f, 0x00),
    Color::rgb8(0xff, 0xff, 0x00),
    Color::rgb8(0x00, 0xff, 0x00),
    Color::rgb8(0x00, 0x00, 0xff),
    Color::rgb8(0x4b, 0x00, 0x82),
    Color::rgb8(0x94, 0x00, 0xd3),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init();
    let mut brush = None;

    util::with_renderer(move |render_context, width, height| {
        let brush = brush.get_or_insert_with(|| {
            render_context
                .gradient(FixedLinearGradient {
                    start: (0.0, 0.0).into(),
                    end: (1000.0, 1000.0).into(),
                    stops: RAINBOW
                        .iter()
                        .enumerate()
                        .map(|(i, color)| GradientStop {
                            pos: i as f32 / (RAINBOW.len() - 1) as f32,
                            color: *color,
                        })
                        .collect::<Vec<_>>(),
                })
                .unwrap()
        });

        render_context.clear(None, piet::Color::WHITE);
        render_context.clip(Circle::new(
            (width as f64 / 2.0, height as f64 / 2.0),
            100.0,
        ));

        let rect = Rect::from_center_size(
            (width as f64 / 2.0, height as f64 / 2.0),
            (width as f64, 100.0),
        );

        render_context.fill(rect, brush);

        render_context.finish().unwrap();
        render_context.status().unwrap();
    })
}

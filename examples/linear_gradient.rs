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

use piet::kurbo::{Circle, Line, Point, Rect, Vec2};
use piet::{Color, FixedLinearGradient, GradientStop, RenderContext as _};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init();

    let mut i = 0;
    let mut outline = None;

    util::with_renderer(move |render_context, width, height| {
        i += 1;

        let center = Point::new(150.0, 200.0);
        let radius = 100.0;
        let angle = i as f64 / 100.0;

        let offset = Vec2::new(angle.cos(), angle.sin()) * radius;
        let start = center + offset;
        let end = center - offset;

        let gradient = FixedLinearGradient {
            start,
            end,
            stops: create_gradient_stops(),
        };
        let brush = render_context.gradient(gradient).unwrap();

        render_context.fill(Rect::new(0.0, 0.0, width as _, height as _), &brush);

        let outline = outline.get_or_insert_with(|| render_context.solid_brush(Color::BLACK));
        render_context.fill(Circle::new(start, 15.0), outline);
        render_context.fill(Circle::new(end, 15.0), outline);
        render_context.stroke(Line::new(start, end), outline, 5.0);

        render_context.finish().unwrap();
        render_context.status().unwrap();
    })
}

fn create_gradient_stops() -> Vec<GradientStop> {
    vec![
        GradientStop {
            pos: 0.0,
            color: Color::rgb(1.0, 0.0, 0.0),
        },
        GradientStop {
            pos: 0.2,
            color: Color::rgb(1.0, 1.0, 0.0),
        },
        GradientStop {
            pos: 0.4,
            color: Color::rgb(0.0, 1.0, 0.0),
        },
        GradientStop {
            pos: 0.6,
            color: Color::rgb(0.0, 1.0, 1.0),
        },
        GradientStop {
            pos: 0.8,
            color: Color::rgb(0.0, 0.0, 1.0),
        },
        GradientStop {
            pos: 1.0,
            color: Color::rgb(1.0, 0.0, 1.0),
        },
    ]
}

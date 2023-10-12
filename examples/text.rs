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

use piet::{RenderContext as _, Text, TextLayoutBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init();
    let mut layout = None;
    let mut last_width = 0;

    util::with_renderer(move |render_context, width, _height| {
        render_context.clear(None, piet::Color::WHITE);

        let layout = if layout.is_none() || width != last_width {
            layout.insert({
                render_context
                    .text()
                    .new_text_layout(TEXT)
                    .max_width(width as f64)
                    .text_color(piet::Color::rgb(0.1, 0.1, 0.1))
                    .build()
                    .expect("failed to build text layout")
            })
        } else {
            #[allow(clippy::unnecessary_unwrap)]
            layout.as_mut().unwrap()
        };
        last_width = width;

        render_context.draw_text(layout, (10.0, 10.0));
    })
}

const TEXT: &str = "the quick brown fox jumps over the lazy dog
1234567890~-=+{};:'<>?
ThE QuicK brown fox Jumps Over The laZy d0g
قبل هو أمدها مشارف ارتكبها, فصل لم زهاء التقليدي. الى ثم ديسمبر 
経だルぴぞ月職カオ宣清こぼトそ積7旬タウ社改オ案処がやく途国地
❤️💀🔥😊";

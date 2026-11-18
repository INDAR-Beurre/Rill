// Every colour in the interface, as a light/dark pair.
//
// The values come from the Mac original's Design.swift (neutral greys, one
// step apart), kept as pairs so the whole window resolves against one
// appearance: light, dark, or whatever the desktop is doing.
use crate::store::Look;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Blend towards another colour; t = 0 is self, 1 is other.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let f = |a: u8, b: u8| {
            (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
        };
        Rgb(f(self.0, other.0), f(self.1, other.1), f(self.2, other.2))
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

fn g(light: f32, dark: f32) -> (Rgb, Rgb) {
    let v = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    (Rgb(v(light), v(light), v(light)), Rgb(v(dark), v(dark), v(dark)))
}

fn same(c: Rgb) -> (Rgb, Rgb) {
    (c, c)
}

fn pair(light: Rgb, dark: Rgb) -> (Rgb, Rgb) {
    (light, dark)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub ground: Rgb,
    pub ink: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub hairline: Rgb,
    pub wash: Rgb,
    pub hover: Rgb,
    pub side_hover: Rgb,
    pub side_tile: Rgb,
    pub side_pin_live: Rgb,
    pub card: Rgb,
    /// The one alarming colour in the browser: used for the close button's
    /// hover mark and nothing else.
    pub danger: Rgb,
}

impl Palette {
    pub fn for_look(look: Look, dark_desktop: bool) -> Palette {
        let dark = match look {
            Look::Light => false,
            Look::Dark => true,
            Look::System => dark_desktop,
        };
        let pick = |(l, d): (Rgb, Rgb)| if dark { d } else { l };
        Palette {
            ground: pick(g(1.0, 0.043)),
            ink: pick(g(0.09, 0.93)),
            muted: pick(pair(Rgb(0x6E, 0x6E, 0x6E), Rgb(0x9A, 0x9A, 0x9A))),
            faint: pick(g(0.83, 0.44)),
            hairline: pick(pair(Rgb(0xE8, 0xE8, 0xE8), Rgb(0x2A, 0x2A, 0x2A))),
            wash: pick(pair(Rgb(0xEF, 0xEF, 0xEF), Rgb(0x3A, 0x3A, 0x3A))),
            hover: pick(pair(Rgb(0xF6, 0xF6, 0xF6), Rgb(0x27, 0x27, 0x27))),
            side_hover: pick(pair(Rgb(0xEF, 0xEF, 0xEE), Rgb(0x2C, 0x2C, 0x2C))),
            side_tile: pick(pair(Rgb(0xF6, 0xF6, 0xF6), Rgb(0x2A, 0x2A, 0x2A))),
            side_pin_live: pick(pair(Rgb(0xDB, 0xDB, 0xD9), Rgb(0x4A, 0x4A, 0x4A))),
            card: pick(pair(Rgb(0xFF, 0xFF, 0xFF), Rgb(0x0B, 0x0B, 0x0B))),
            danger: pick(same(Rgb(0xC0, 0x39, 0x2B))),
        }
    }

    /// The column's own ground: the page's white in the Mac build.
    pub fn side_bg(&self) -> Rgb {
        self.card
    }


    pub fn is_dark(&self) -> bool {
        self.ground.0 < 128
    }
}

/// The font stack: the Mac build's SF Pro, then the closest thing each Linux
/// install is likely to have. Inter and Adwaita Sans are SF's near neighbours.
pub const FONT: &str =
    "\"SF Pro Text\", \"SF Pro\", Inter, \"Adwaita Sans\", \"Noto Sans\", \"Segoe UI\", sans-serif";
pub const MONO: &str = "\"SF Mono\", \"JetBrains Mono\", \"DejaVu Sans Mono\", monospace";

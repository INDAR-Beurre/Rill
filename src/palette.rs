// Every colour in the interface, as a light/dark pair.
//
// The values come from the Mac original's Design.swift (neutral greys, one
// step apart), kept as pairs so the whole window resolves against one
// appearance: light, dark, or whatever the desktop is doing.
use crate::store::Look;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
    /// Blend towards another colour; t = 0 is self, 1 is other.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
        Rgb(f(self.0, other.0), f(self.1, other.1), f(self.2, other.2))
    }
}

fn g(light: f32, dark: f32) -> Rgb {
    let v = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(v(light), v(light), v(light))
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
    pub pin_live: Rgb,
    pub hover: Rgb,
    pub frame: Rgb,
    pub side_wash: Rgb,
    pub side_hover: Rgb,
    pub side_tile: Rgb,
    pub side_pin_live: Rgb,
    pub safe: Rgb,
    pub unsafe_: Rgb,
    pub card: Rgb,
    pub light_red: Rgb,
    pub light_amber: Rgb,
    pub light_green: Rgb,
}

macro_rules! palette {
    ($($field:ident => ($l:expr, $d:expr)),* $(,)?) => {
        impl Palette {
            pub fn for_look(look: Look, dark_desktop: bool) -> Palette {
                let dark = match look {
                    Look::Light => false,
                    Look::Dark => true,
                    Look::System => dark_desktop,
                };
                let pick = |l: Rgb, d: Rgb| if dark { d } else { l };
                Palette { $($field: { let (l, d) = ($l, $d); pick(l, d) }),* }
            }
        }
    };
}

// Light values are the greys of the Mac build; dark values are its dark pairs.
palette! {
    ground        => (g(1.0, 1.0),     g(0.11, 0.11)),
    ink           => (g(0.09, 0.09),   g(0.93, 0.93)),
    muted         => (Rgb(0x6E, 0x6E, 0x6E), Rgb(0x9A, 0x9A, 0x9A)),
    faint         => (g(0.83, 0.83),   g(0.32, 0.32)),
    hairline      => (g(0.91, 0.91),   g(0.20, 0.20)),
    wash          => (g(0.937, 0.937), Rgb(0x35, 0x35, 0x35)),
    pin_live      => (g(0.90, 0.90),   g(0.21, 0.21)),
    hover         => (g(0.965, 0.965), Rgb(0x22, 0x22, 0x22)),
    frame         => (Rgb(0xEF, 0xEF, 0xEE), Rgb(0x14, 0x14, 0x14)),
    side_wash     => (Rgb(0xE9, 0xE9, 0xE8), Rgb(0x2E, 0x2E, 0x2E)),
    side_hover    => (Rgb(0xEF, 0xEF, 0xEE), Rgb(0x26, 0x26, 0x26)),
    side_tile     => (Rgb(0xF6, 0xF6, 0xF6), Rgb(0x24, 0x24, 0x24)),
    side_pin_live => (Rgb(0xDB, 0xDB, 0xD9), Rgb(0x44, 0x44, 0x44)),
    safe          => (Rgb(0x14, 0x80, 0x3D), Rgb(0x4A, 0xDE, 0x80)),
    unsafe_       => (Rgb(0xB5, 0x54, 0x0A), Rgb(0xFA, 0xBF, 0x24)),
    card          => (Rgb(0xFF, 0xFF, 0xFF), Rgb(0x0B, 0x0B, 0x0B)),
    light_red     => (Rgb(0xEC, 0x6A, 0x5E), Rgb(0xEC, 0x6A, 0x5E)),
    light_amber   => (Rgb(0xF5, 0xBF, 0x4F), Rgb(0xF5, 0xBF, 0x4F)),
    light_green   => (Rgb(0x61, 0xC5, 0x54), Rgb(0x61, 0xC5, 0x54)),
}

impl Palette {
    /// The column's own greys: on the frame ground beside the page's card.
    pub fn side_bg(&self) -> Rgb {
        // The column is the page's white in the Mac build; beside the card it
        // keeps a hair of the frame so the card still reads as a card.
        if self.is_dark() { self.card } else { self.card }
    }
    /// A pinned square's tint, and the one the live pin wears.
    pub fn pin(&self, live: bool) -> Rgb {
        if live { self.side_pin_live } else { self.side_tile }
    }
    pub fn row(&self, live: bool, hover: bool) -> Rgb {
        if live { self.side_wash } else if hover { self.side_hover } else { self.side_bg() }
    }
    pub fn is_dark(&self) -> bool {
        // A palette's ground decides the appearance it belongs to.
        self.ground.0 < 128
    }
}

/// The font stack: the Mac build's SF Pro, then the closest thing each Linux
/// install is likely to have. Inter and Adwaita Sans are SF's near neighbours.
pub const FONT: &str = "\"SF Pro Text\", \"SF Pro\", Inter, \"Adwaita Sans\", \"Noto Sans\", \"Segoe UI\", sans-serif";
pub const MONO: &str = "\"SF Mono\", \"JetBrains Mono\", \"DejaVu Sans Mono\", monospace";

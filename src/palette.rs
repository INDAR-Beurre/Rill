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
    /// hover mark and nothing else. The two appearances do not share a value:
    /// a single #C0392B reads 5.44:1 on white but only 3.62:1 on the dark
    /// ground, which is not enough for a mark you are meant to notice before
    /// you press it.
    pub danger: Rgb,
}

/// The WCAG 2.1 contrast ratio between two colours: 1.0 for the same colour,
/// 21.0 for black on white. A text pair wants 4.5, body text 7, a boundary
/// that has to be seen wants 3, and a quiet hairline 1.5.
///
/// It lives outside the tests because it is the yardstick, not one of the
/// things it measures: anything that wants to check a pair, now or later,
/// should count with the same arithmetic rather than a second copy of it.
#[allow(dead_code)]
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let channel = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = |c: Rgb| {
        0.2126 * channel(c.0) + 0.7152 * channel(c.1) + 0.0722 * channel(c.2)
    };
    let (x, y) = (luminance(a), luminance(b));
    let (high, low) = if x > y { (x, y) } else { (y, x) };
    (high + 0.05) / (low + 0.05)
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
            // #6E6E6E read 5.10:1 on white but only 4.43:1 on the pressed
            // wash and 3.68:1 on a live pin, and muted is the colour of text
            // in both. #5E5E5E reads 6.48:1 on white, 5.64:1 on the wash,
            // 4.68:1 on a live pin. The dark went the other way for the same
            // reason: #9A9A9A was 3.15:1 for the letter on a live pin, and
            // #A6A6A6 is 4.53:1 there and still 8.09:1 on the ground.
            muted: pick(pair(Rgb(0x5E, 0x5E, 0x5E), Rgb(0xA6, 0xA6, 0xA6))),
            // The one the critic caught at 2.30:1. #6C6C6C now reads 5.25:1
            // on white and 4.86:1 on a hovered row; #8E8E8E reads 6.01:1 on
            // the dark ground and 4.56:1 on its hover.
            faint: pick(pair(Rgb(0x6C, 0x6C, 0x6C), Rgb(0x8E, 0x8E, 0x8E))),
            // #E8E8E8 was 1.23:1 against white, which is not a line you can
            // see. #D2D2D2 is 1.51:1 and #323232 is 1.54:1 on the dark
            // ground: quiet, but a boundary that exists.
            hairline: pick(pair(Rgb(0xD2, 0xD2, 0xD2), Rgb(0x32, 0x32, 0x32))),
            wash: pick(pair(Rgb(0xEF, 0xEF, 0xEF), Rgb(0x3A, 0x3A, 0x3A))),
            hover: pick(pair(Rgb(0xF6, 0xF6, 0xF6), Rgb(0x27, 0x27, 0x27))),
            side_hover: pick(pair(Rgb(0xEF, 0xEF, 0xEE), Rgb(0x2C, 0x2C, 0x2C))),
            side_tile: pick(pair(Rgb(0xF6, 0xF6, 0xF6), Rgb(0x2A, 0x2A, 0x2A))),
            // A live pin carries the same muted letter as a hovering one, so
            // the fill has to stay light enough to read against: #4A4A4A gave
            // that letter 3.15:1 and is now #3C3C3C at 4.53:1, still a
            // 1.30:1 step up from the tile it sits in.
            side_pin_live: pick(pair(Rgb(0xDB, 0xDB, 0xD9), Rgb(0x3C, 0x3C, 0x3C))),
            card: pick(pair(Rgb(0xFF, 0xFF, 0xFF), Rgb(0x0B, 0x0B, 0x0B))),
            // #C0392B is 5.44:1 on white, so light keeps it. On the dark
            // ground it was 3.62:1; #D25041 is 4.65:1 there and still carries
            // white at 4.23:1, which is what the close mark is drawn in.
            danger: pick(pair(Rgb(0xC0, 0x39, 0x2B), Rgb(0xD2, 0x50, 0x41))),
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

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = Rgb(255, 255, 255);

    fn light() -> Palette {
        Palette::for_look(Look::Light, false)
    }

    fn dark() -> Palette {
        Palette::for_look(Look::Dark, false)
    }

    /// Every pair that is text on a background, named after the rule in
    /// css.rs that creates it. The background is a palette field, not a hex
    /// literal, so this test measures what the palette actually hands out.
    fn text_pairs(p: &Palette) -> Vec<(&'static str, Rgb, Rgb, f64)> {
        let side = p.side_bg();
        vec![
            ("body ink on the page ground", p.ink, p.ground, 7.0),
            ("body ink on the sidebar", p.ink, side, 7.0),
            ("muted on the page ground", p.muted, p.ground, 4.5),
            ("muted on the sidebar", p.muted, side, 4.5),
            ("muted on a pressed fill (keycaps, entries)", p.muted, p.wash, 4.5),
            ("muted on a hovered control", p.muted, p.hover, 4.5),
            ("muted on a hovered pin", p.muted, p.side_hover, 4.5),
            ("muted on a pin tile", p.muted, p.side_tile, 4.5),
            ("muted on a live pin", p.muted, p.side_pin_live, 4.5),
            ("faint on the page ground", p.faint, p.ground, 4.5),
            ("faint on the sidebar", p.faint, side, 4.5),
            ("faint on a hovered row", p.faint, p.hover, 4.5),
            ("ink on a pressed fill", p.ink, p.wash, 4.5),
            ("ink on a hovered control", p.ink, p.hover, 4.5),
            ("danger on the page ground", p.danger, p.ground, 4.5),
            ("danger on the sidebar", p.danger, side, 4.5),
        ]
    }

    #[test]
    fn contrast_is_the_wcag_ratio() {
        // The anchors a ratio has to hit: identical colours are 1, black on
        // white is the maximum 21, and #767676 on white is the 4.5 boundary.
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 1e-9);
        assert!((contrast(Rgb(0, 0, 0), WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Rgb(0x76, 0x76, 0x76), WHITE) - 4.54).abs() < 0.01);
        // Symmetric, as a ratio has to be.
        let a = Rgb(0x5E, 0x5E, 0x5E);
        assert!((contrast(a, WHITE) - contrast(WHITE, a)).abs() < 1e-9);
    }

    #[test]
    fn every_text_pair_passes_wcag() {
        for (appearance, p) in [("light", light()), ("dark", dark())] {
            for (what, ink, ground, need) in text_pairs(&p) {
                let ratio = contrast(ink, ground);
                assert!(
                    ratio >= need,
                    "{appearance}: {what} is {ratio:.2}:1, needs {need}:1 ({} on {})",
                    ink.hex(),
                    ground.hex()
                );
            }
        }
    }

    #[test]
    fn the_field_edge_and_the_divider_are_not_invisible() {
        // These four are not palette fields: css.rs writes them as hex, which
        // is why they are literals here rather than fields of the struct. The
        // test is a tripwire on that file, not a second opinion about it: the
        // css builder owns those tokens, and 1.18:1 is the number that made an
        // edge nobody could see. If those hexes move, move these with them.
        //
        // A hairline is allowed to be quiet, so 1.5 is the floor. All four
        // clear it: 2.08 and 2.68 for the field edge, 1.51 and 1.78 for the
        // divider.
        let field_border = [Rgb(0xB4, 0xB4, 0xB2), Rgb(0x5E, 0x5E, 0x5E)];
        let field_fill = [Rgb(0xFF, 0xFF, 0xFF), Rgb(0x1A, 0x1A, 0x1A)];
        let divider = [Rgb(0xD2, 0xD2, 0xD0), Rgb(0x3C, 0x3C, 0x3C)];
        let palettes = [light(), dark()];
        for (i, appearance) in ["light", "dark"].into_iter().enumerate() {
            let ratio = contrast(field_border[i], field_fill[i]);
            assert!(
                ratio >= 1.5,
                "{appearance}: the field edge is {ratio:.2}:1 against its own fill"
            );
            let ratio = contrast(divider[i], palettes[i].ground);
            assert!(
                ratio >= 1.5,
                "{appearance}: the divider is {ratio:.2}:1 against the page"
            );
        }
    }

    #[test]
    fn the_quiet_boundaries_can_still_be_seen() {
        // hairline is the palette's own line: the paned separator on hover,
        // and the fill behind a pressed control. 1.23:1 is invisible; 1.5 is
        // the least a line can be and still be a line.
        for (appearance, p) in [("light", light()), ("dark", dark())] {
            let ratio = contrast(p.hairline, p.ground);
            assert!(ratio >= 1.5, "{appearance}: hairline on ground is {ratio:.2}:1");
            let ratio = contrast(p.side_pin_live, p.side_tile);
            assert!(
                ratio >= 1.2,
                "{appearance}: a live pin is only {ratio:.2}:1 against a sleeping one"
            );
        }
    }

    #[test]
    fn text_stays_quiet_instead_of_turning_black() {
        // A test that only asks for contrast can be passed by making every
        // word black. These three are close in spirit but still three steps
        // apart, and the app is meant to read as grey.
        for (appearance, p) in [("light", light()), ("dark", dark())] {
            let ink = contrast(p.ink, p.ground);
            let muted = contrast(p.muted, p.ground);
            let faint = contrast(p.faint, p.ground);
            assert!(ink > muted, "{appearance}: muted is as loud as body ink");
            assert!(muted > faint, "{appearance}: faint is as loud as muted");
            assert!(
                muted < 10.0,
                "{appearance}: muted on ground is {muted:.2}:1, that is not quiet"
            );
        }
    }

    #[test]
    fn the_close_mark_reads_on_its_fill() {
        // css.rs paints the close mark #FFFFFF on the danger fill.
        for (appearance, p) in [("light", light()), ("dark", dark())] {
            let ratio = contrast(WHITE, p.danger);
            assert!(
                ratio >= 3.0,
                "{appearance}: white on {} is {ratio:.2}:1",
                p.danger.hex()
            );
        }
    }
}

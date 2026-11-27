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

const fn hex(value: u32) -> Rgb {
    Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
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
        let pick = |light, night| hex(if dark { night } else { light });
        Palette {
            ground: pick(0xF7F7F5, 0x1A1816),
            ink: pick(0x272724, 0xEFECE6),
            muted: pick(0x595A52, 0xBAB4AA),
            faint: pick(0x67685E, 0xA8A298),
            hairline: pick(0xC6C6C0, 0x4E4A44),
            wash: pick(0xF0F0ED, 0x3B3733),
            hover: pick(0xF3F3F0, 0x312E2A),
            side_hover: pick(0xF1F1EE, 0x34312D),
            side_tile: pick(0xFAFAF8, 0x2B2825),
            side_pin_live: pick(0xDEDED8, 0x3F3B36),
            card: pick(0xFFFFFF, 0x211F1C),
            danger: pick(0xAF392C, 0xE07463),
        }
    }

    pub fn side_bg(&self) -> Rgb {
        self.ground
    }

    pub fn accent(&self) -> Rgb {
        hex(if self.is_dark() { 0xF69268 } else { 0xB74525 })
    }

    pub fn accent_ink(&self) -> Rgb {
        hex(if self.is_dark() { 0x261D16 } else { 0xFFFFFF })
    }

    pub fn surface(&self) -> Rgb {
        hex(if self.is_dark() { 0x272522 } else { 0xFFFFFF })
    }

    pub fn tint(&self) -> Rgb {
        hex(if self.is_dark() { 0x362A24 } else { 0xF4DDCF })
    }

    pub fn is_dark(&self) -> bool {
        self.ground.0 < 128
    }
}

/// The four colours reading mode paints with. It sets its own page inside the
/// document, so its pairs are measured on `ground`, not on the browser's.
#[derive(Clone, Copy, Debug)]
pub struct Reader {
    pub ground: Rgb,
    pub ink: Rgb,
    pub muted: Rgb,
    pub rule: Rgb,
    pub paper: Rgb,
}

impl Palette {
    /// Reading mode's tokens, taken from this palette rather than written as
    /// hexes in assets/reader.js, where `#EAEAEA` on white was 1.20:1 and a
    /// rule nobody could see. `hairline` is the palette's line: 1.51:1 light,
    /// 1.54:1 on the dark ground. Body ink keeps its 7:1, the byline keeps
    /// the 4.5:1 `muted` already has on the card.
    pub fn reader(&self) -> Reader {
        Reader {
            ground: self.card,
            ink: self.ink,
            muted: self.muted,
            rule: self.hairline,
            paper: self.wash,
        }
    }
}

pub const FONT: &str =
    "\"Adwaita Sans\", \"Noto Sans\", Cantarell, Inter, sans-serif";
pub const MONO: &str = "\"Adwaita Mono\", \"JetBrains Mono\", \"DejaVu Sans Mono\", monospace";

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
        let palettes = [light(), dark()];
        for (index, appearance) in ["light", "dark"].into_iter().enumerate() {
            let palette = palettes[index];
            let field_border = crate::css::quiet::pick(crate::css::quiet::FIELD_BORDER, palette.is_dark());
            let ratio = contrast(field_border, palette.surface());
            assert!(
                ratio >= 1.5,
                "{appearance}: the field edge is {ratio:.2}:1 against its own fill"
            );
            let ratio = contrast(palette.hairline, palette.ground);
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
    fn reading_mode_has_a_rule_you_can_see() {
        // assets/reader.js used #EAEAEA on white for its rule: 1.20:1, the
        // 1.2 the critic caught. The helper now hands out palette values, so
        // this measures the same floors as the browser's own page.
        for (appearance, p) in [("light", light()), ("dark", dark())] {
            let r = p.reader();
            assert_eq!(r.ground, p.card);
            let rule = contrast(r.rule, r.ground);
            assert!(
                rule >= 1.5,
                "{appearance}: the reading rule is {rule:.2}:1 on {}",
                r.ground.hex()
            );
            let muted = contrast(r.muted, r.ground);
            assert!(muted >= 4.5, "{appearance}: the reading byline is {muted:.2}:1");
            let ink = contrast(r.ink, r.ground);
            assert!(ink >= 7.0, "{appearance}: the reading body is {ink:.2}:1");
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

    #[test]
    fn accent_actions_have_readable_labels() {
        for palette in [light(), dark()] {
            assert!(contrast(palette.accent_ink(), palette.accent()) >= 4.5);
            assert!(contrast(palette.accent(), palette.surface()) >= 3.0);
        }
    }
}

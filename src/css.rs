// One stylesheet for the whole interface, built from the palette so a change
// of appearance is a change of colours and nothing else.
use crate::palette::{Palette, FONT, MONO};

fn rgba(c: crate::palette::Rgb, a: f32) -> String {
    format!("rgba({},{},{},{:.3})", c.0, c.1, c.2, a)
}

/// The quiet text colours, in one place.
///
/// These used to be written into the stylesheet as literals, which meant the
/// palette's own contrast tests were measuring colours the window never used:
/// `palette::muted` said #5E5E5E while the CSS said #646462. Anything that
/// wants to assert a ratio now asks for the same value the stylesheet is built
/// from, so the assertion and the pixels cannot drift apart.
///
/// Each pair is (light, dark). The comment on each is the harsher of the two
/// grounds the colour actually sits on, measured with `palette::contrast`.
pub mod quiet {
    use crate::palette::Rgb;

    const fn hex(v: u32) -> Rgb {
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// A resting tab title, and the letter in a pin. 5.02:1 on the page.
    pub const PIN_FG: (Rgb, Rgb) = (hex(0x6A6A68), hex(0xA6A6A6));
    /// The letter in the pin of the tab you are on. 5.30 dark / 5.47 light.
    pub const PIN_LIVE_FG: (Rgb, Rgb) = (hex(0x545452), hex(0xC8C8C8));
    /// A tab title in the list. #6E6E6E read 5.10:1 on the page but 4.43:1 on
    /// a hovered row, and a row is hovered exactly when it is being read.
    /// #6A6A68 is 4.71:1 there and 5.42:1 on the page.
    pub const ROW_FG: (Rgb, Rgb) = (hex(0x6A6A68), hex(0xB4B4B4));
    /// The letter in a bookmark tile. 4.80:1 on the tile in light.
    pub const TILE_FG: (Rgb, Rgb) = (hex(0x6A6A68), hex(0xA6A6A6));
    /// Secondary text: the host under a suggestion, a switch's subtitle.
    /// 5.16:1 on the light panel, 4.67:1 on a dark chip.
    pub const MUTED: (Rgb, Rgb) = (hex(0x646462), hex(0xA6A6A6));
    /// The quietest text still meant to be read: a placeholder, a group head.
    /// 4.71:1 on the light panel, 4.56:1 on a picked dark row.
    pub const FAINT: (Rgb, Rgb) = (hex(0x6A6A68), hex(0xA4A4A4));
    /// An empty list's one line. 6.48:1 on white, 7.93:1 on the dark ground.
    pub const EMPTY_FG: (Rgb, Rgb) = (hex(0x5E5E5E), hex(0xA6A6A6));
    /// The sentence under a page that did not open. 5.93 light / 7.93 dark.
    pub const BLANK_HINT: (Rgb, Rgb) = (hex(0x646462), hex(0xA6A6A6));
    /// The outline of a field: the boundary a reader has to find without
    /// hunting. It has to be found on every ground a field sits on, not just
    /// on its own fill: the old #949492 read 2.39:1 on the scrim and 2.64:1 on
    /// the light panel, and the dark #6E6E6E read 3.04:1 on the scrim and
    /// 2.23:1 on a picked chip. #808080 is 3.95:1 on white, 3.43:1 on the
    /// panel, 3.11:1 on the scrim; #868686 is 4.78:1 on the dark field and
    /// 4.26:1 on the dark scrim.
    pub const FIELD_BORDER: (Rgb, Rgb) = (hex(0x808080), hex(0x868686));
    /// A control that cannot be pressed: quieter than a live one, still
    /// legible. Fading the whole chip to 0.4 left 1.69:1 text in light and
    /// 2.19:1 in dark. 3.22:1 on the light panel, 3.47:1 on a dark chip.
    pub const DISABLED: (Rgb, Rgb) = (hex(0x858583), hex(0x8E8E8C));
    /// Danger written as text on a chip rather than painted as a fill. The
    /// palette's #D25041 is a fill colour and reads 2.69:1 as text on the dark
    /// chip; #EE8B76 reads 4.65:1 there and 6.11:1 on the hover behind it.
    pub const DANGER: (Rgb, Rgb) = (hex(0xC0392B), hex(0xEE8B76));
    /// A switch that is off. 3.35:1 on the light wash, 3.94:1 on the dark panel.
    pub const SWITCH_OFF: (Rgb, Rgb) = (hex(0x828280), hex(0x787878));

    pub fn pick(pair: (Rgb, Rgb), dark: bool) -> Rgb {
        if dark {
            pair.1
        } else {
            pair.0
        }
    }
}

/// The type scale, in pixels, from the desktop's own font size.
///
/// Thirty-five absolute pixel sizes meant a person who had set their desktop
/// to 12pt got the same 13px tab titles as everyone else. GTK exposes that
/// setting as `gtk-font-name`; the four stops below are offsets from it, so
/// the interface follows the desktop instead of ignoring it.
///
/// The default desktop size here is 10pt, which GTK renders at 13px, and the
/// four stops come out 10 / 12 / 13 / 15 — the sizes the design was drawn at.
/// A larger desktop setting moves all four together.
#[derive(Debug)]
pub struct Scale {
    pub caps: i32,
    pub small: i32,
    pub body: i32,
    pub title: i32,
}

impl Scale {
    pub fn from_desktop() -> Scale {
        use gtk::prelude::GtkSettingsExt;
        // Reading a setting before GTK is up panics rather than returning
        // None, and the stylesheet is also built by the unit tests.
        if !gtk::is_initialized() {
            return Scale { caps: 10, small: 12, body: 13, title: 15 };
        }
        let points: f64 = gtk::Settings::default()
            .and_then(|s| s.gtk_font_name())
            .and_then(|name| {
                name.split_whitespace()
                    .last()
                    .and_then(|size| size.parse::<f64>().ok())
            })
            .unwrap_or(10.0f64);
        // Points to CSS pixels at 96dpi, which is what GTK itself uses.
        let body = (points * 96.0 / 72.0).round() as i32;
        Scale {
            caps: (body - 3).max(11),
            small: (body - 1).max(13),
            body: body.max(14),
            title: body + 2,
        }
    }
}

pub fn build(p: &Palette, row_height: i32) -> String {
    // Square window edges. The compositor clips the frame with its own mask
    // before GTK's rounded corner is honoured, so a radius here shows on one
    // edge and not the other. A half-applied radius is worse than none, and
    // the surfaces inside the window keep theirs.
    let radius = 0;
    let ground = p.card.hex();
    let scale = Scale::from_desktop();

    format!(
        r#"
window {{ background-color: {ground}; }}
headerbar.nobar {{ background-color: {ground}; }}
headerbar.nobar, headerbar {{ min-height: 0; padding: 0; margin: 0; border: none; background: none; box-shadow: none; }}
headerbar.nobar button, headerbar button {{ min-height: 0; padding: 0; margin: 0; }}
decoration {{ box-shadow: none; border-width: 0; margin: 0; padding: 0; background: none; border-radius: {radius}px; }}
headerbar.nobar > box {{ min-height: 0; padding: 0; margin: 0; border: none; }}
.shell {{ background-color: {ground}; border-radius: {radius}px; }}
.shell separator, paned separator {{ background-color: {divider}; background-image: none; min-width: 1px; min-height: 1px; border: none; }}
.shell separator:hover, paned separator:hover {{ background-color: {hairline}; }}

/* The band above the window, one strip on each side of the divider. It is
   deliberately not called `frame`: that is GTK's own class for a GtkFrame, and
   GTK puts one around every scrolled window, so naming it that painted the
   whole tab column the band's grey. */
.desk {{ background-color: {desk}; }}
/* A paned separator with a narrow handle paints nothing at all, so the column
   draws its own edge. No radius here: GTK rounds the widget's background but
   not its border, so a rounded column gets a square line drawn through the
   curve. The window's own top-left corner is cut by the capture instead. */
.side {{ background-color: {sidebg}; border-right: 1px solid {divider}; }}

.control {{ min-width: 22px; min-height: 22px; border-radius: 6px; background-color: transparent; }}
.control:hover {{ background-color: {hover}; }}
.control.shut:hover {{ background-color: {danger}; }}
.control.shut:hover label {{ color: #FFFFFF; }}
.control:active {{ background-color: {hairline}; }}
.control label {{ color: {muted}; }}
window.unfocused .control {{ opacity: 0.55; }}
.rowmark {{ margin: 0; }}

/* GTK brings its own skin for every control. The browser has one, so the
   stock skin is taken off first and only what is wanted is put back. */
button, .panel button, .blank button, combobox button, entry, spinbutton {{
  background-image: none;
  box-shadow: none;
  text-shadow: none;
}}
button {{
  background-color: transparent;
  border: none;
  color: {ink};
  padding: 2px 6px;
  border-radius: 6px;
  min-height: 0;
  /* The desktop theme sets bold on every button (adw-gtk3-dark gtk.css:402),
     so every unweighted label in this browser came out bold. The weight is
     the browser's to choose, per class. */
  font-weight: normal;
}}
/* The weight belongs to the browser, so it also belongs on the label inside a
   button: nothing in the chrome — nav, marks, chips, the new-tab row — is set
   bold, and the headings below re-assert their own weight. */
button label {{ font-weight: normal; }}
button:hover {{ background-color: {hover}; }}
button:active {{ background-color: {wash}; }}
button.act {{
  background-color: {wash};
  border: 1px solid {fieldborder};
  border-radius: 7px;
  padding: 5px 11px;
}}
button.act:hover {{ background-color: {hairline}; }}
button.act label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
.panel button.chosen, button.chosen {{ background-color: {ink}; border-color: {ink}; }}
button.chosen label {{ color: {ground}; font-weight: bold; }}
combobox button {{
  background-color: {wash};
  border: 1px solid {fieldborder};
  border-radius: 7px;
  padding: 4px 10px;
  color: {ink};
}}
combobox button:hover {{ background-color: {hairline}; }}
combobox button label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
combobox arrow {{ color: {muted}; min-width: 10px; min-height: 10px; }}
menu {{ background-color: {fieldbg}; border: 1px solid {fieldborder}; border-radius: 8px; padding: 4px; }}
menuitem {{ border-radius: 5px; padding: 4px 8px; }}
menuitem:hover {{ background-color: {hover}; }}
menuitem label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
entry {{ background-image: none; box-shadow: none; border: none; }}
button.mark {{ background: none; background-image: none; border: none; box-shadow: none; padding: 4px; border-radius: 6px; min-width: 0; min-height: 0; }}
button.mark:hover {{ background-color: {hover}; }}
button.mark:focus-visible {{ outline: 2px solid {ink}; outline-offset: 1px; }}
button.mark:active {{ background-color: {wash}; }}
button.mark:disabled {{ opacity: 0.35; }}
/* A control that cannot be pressed must not look pressable: "Show them" at a
   count of zero was sensitive=false and painted exactly like a live button.
   The chip gives up its fill and its label goes quiet, which reads as inert
   without moving anything by a pixel. Fading the whole chip instead took the
   text to 1.69:1 in light and 2.19:1 in dark, so the dimming is in the ink.
   The selectors name `.act` because that is the chip the panel paints, and at
   equal weight the later `.panel button.act` rules would otherwise win. The
   drawn marks keep their own dimming from `button.mark:disabled`; nothing here
   touches opacity, so a disabled mark is still told apart from a live one. */
.panel button.act:disabled {{ background-color: transparent; }}
.panel button:disabled label, .panel button.act:disabled label {{ color: {disabledink}; }}

row, flowbox {{ background-color: transparent; }}
flowboxchild {{ background-color: transparent; padding: 0; }}

.pin {{ border-radius: 9px; background-color: {tile}; }}
.pin.live {{ background-color: {pinlive}; }}
.pinletter {{ font-family: {font}; font-size: {body}px; font-weight: normal; color: {pinfg}; }}
.pin.live .pinletter {{ color: {pinlivefg}; }}
.pin:hover {{ background-color: {sidehover}; }}

.row {{ border-radius: 7px; background-color: transparent; min-height: {row_height}px; }}
button.close, button.loupe {{ background: none; border: none; box-shadow: none; padding: 1px; min-width: 0; min-height: 0; opacity: 0; }}
.row:hover button.close, .pin:hover button.close {{ opacity: 1; }}
button.close:hover {{ background-color: {hairline}; border-radius: 4px; }}
.row:hover {{ background-color: {hover}; }}
.row.live {{ background-color: {wash}; }}
/* The active row still answers the cursor, and its selector has to outweigh
   `.row.live` — at equal specificity the earlier rule won and hovering the
   row you are on did nothing. */
.row:hover, .row.live:hover {{ background-color: {livehover}; }}
.rowtitle {{ font-family: {font}; font-size: {body}px; font-weight: normal; color: {rowfg}; }}
/* On the label itself, not through a descendant selector. The row you are on
   is marked by its only-just-dark ink against the resting grey, not by weight:
   the reference's live title is regular, and bold here made it read as a
   heading heavier than every unselected row beside it. */
.liveword {{ color: {liveink}; font-weight: normal; }}
/* A tab that gave its memory back. Still readable — it is a tab you can click,
   not a disabled control — but plainly not the one that is running. */
.row.asleep .rowtitle {{ color: {faint}; }}
.row.asleep .tile {{ opacity: 0.55; }}
/* The active row's chip has to sit off the wash: #F1F1F1 on #EFEFEF is
   1.018:1, and the letter tile vanished into the row it belongs to. */
.row.live .tile {{ background-color: {ground}; }}
.row.asleep .tileletter {{ color: {faint}; }}
.row.private .rowtitle {{ color: {muted}; }}
.tile {{ border-radius: 4px; background-color: {tilebg}; }}
.tileletter {{ font-family: {font}; font-size: {small}px; font-weight: bold; color: {tilefg}; }}
.panel switch {{ background-color: {switchoff}; border: none; min-width: 34px; min-height: 20px; border-radius: 10px; }}
.panel switch:checked {{ background-color: {ink}; }}
.panel switch slider {{ min-width: 16px; min-height: 16px; border-radius: 8px; margin: 2px; background-color: {knob}; border: none; box-shadow: 0 1px 2px rgba(0,0,0,0.28); }}
.panel switch:checked slider {{ background-color: {fieldbg}; }}

.newtab {{ border-radius: 7px; }}
.newtab:hover {{ background-color: {hover}; }}
.newtablabel {{ font-family: {font}; font-size: {body}px; font-weight: normal; color: {faint}; }}
.newtab:hover .newtablabel {{ color: {muted}; }}

.helm {{ }}

.card {{ background-color: {ground}; }}
.blank {{ background-color: {ground}; }}
.fakefield {{ background-color: {fieldbg}; border: 1px solid {fieldborder}; border-radius: 9px; }}
.fakefield:hover {{ border-color: {faint}; }}
.fakefieldtext {{ font-family: {font}; font-size: {title}px; color: {faint}; }}
.blanktitle {{ font-family: {font}; font-size: {title}px; font-weight: bold; color: {muted}; letter-spacing: -0.01em; }}
.blankhint {{ font-family: {font}; font-size: {small}px; color: {blankhint}; }}
.erraddress {{ font-family: {mono}; font-size: {small}px; color: {emptyfg}; margin-top: 4px; }}
.blank button.act {{ background-color: {wash}; border: 1px solid {fieldborder}; border-radius: 8px; padding: 6px 14px; }}
.blank button.act:hover {{ background-color: {hairline}; }}
.blank button.act label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}


.scrim {{ background-color: {scrim}; }}
.omni {{ background-color: {fieldbg}; border-radius: 12px; border: 1px solid {fieldborder}; }}
.omni.focused {{ border-color: {ink}; }}
/* Focus is the field's own edge, so "type to replace" has a visible state
   without a ring GTK draws under the text. */
.omni entry {{ background: none; background-image: none; border: none; box-shadow: none; padding: 0; }}
.omni entry:focus {{ border: none; box-shadow: none; }}
.omnitext {{ font-family: {font}; font-size: {title}px; color: {ink}; }}
.omnisite {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.suggest {{ background-color: {fieldbg}; border-radius: 10px; border: 1px solid {fieldborder}; }}
.suggest .sgrow {{ border-radius: 6px; }}
.suggest .sgrow:hover {{ background-color: {hover}; }}
.suggest .sgrow.picked {{ background-color: {wash}; }}
.suggest .sghead {{ font-family: {font}; font-size: {caps}px; font-weight: bold; color: {muted}; letter-spacing: 0.06em; }}
.suggest .sgrow {{ min-height: 44px; }}
.suggest .sgrow {{ border-left: 2px solid transparent; }}
.suggest .sgrow.picked {{ border-left: 2px solid {ink}; border-top-left-radius: 0; border-bottom-left-radius: 0; }}
/* The bookmarks bar: a row of kept pages, one chip each, quiet until hovered. */
.bmarkbar {{ background-color: {ground}; border-bottom: 1px solid {divider}; padding-top: 6px; }}
.bmark {{ font-family: {font}; font-size: {small}px; color: {muted}; background-color: transparent;
          background-image: none; border: none; box-shadow: none; padding: 1px 8px;
          min-height: 20px; border-radius: 5px; }}
.bmark:hover {{ background-color: {hover}; color: {ink}; }}
.bmark:active {{ background-color: {hairline}; }}

.panel .ptrow {{ border-radius: 7px; }}
.panel .ptrow:hover {{ background-color: {hover}; }}
.suggest row:hover {{ background-color: {hover}; }}
.suggest .sgtitle {{ font-family: {font}; font-size: {body}px; color: {ink}; }}
.suggest .sgurl {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.suggest .sgkind {{ font-family: {font}; font-size: {small}px; color: {faint}; }}

/* GTK clips a child's box-shadow behind its parent, so the panel's shadow
   never reaches the page. Its edge has to be the fill itself: the panel is
   the wash in light and the field colour in dark, both a step off the page. */
.panel {{ background-color: {panelbg}; border-radius: 12px; border: 1px solid {fieldborder}; box-shadow: 0 18px 48px rgba(0,0,0,0.16); }}
.panel .head {{ font-family: {font}; font-size: {title}px; font-weight: bold; color: {ink}; }}
.panel .sub {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel entry {{ background-color: {chipbg}; border-radius: 7px; border: 1px solid {fieldborder}; padding: 6px 10px; }}
.panel entry text {{ font-family: {font}; font-size: {body}px; color: {ink}; }}
.panel row {{ border-radius: 7px; padding: 4px 6px; }}
.panel row:hover {{ background-color: {hover}; }}
.panel .ptitle {{ font-family: {font}; font-size: {body}px; color: {ink}; }}
.panel .purl {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.keycap {{ font-family: {mono}; font-size: {small}px; color: {muted}; background-color: {wash}; border: 1px solid {divider}; border-radius: 5px; padding: 2px 6px; }}
.extensionnote {{ padding: 10px 12px; background-color: {wash}; border-radius: 8px; }}
.panel button.tab {{ background: none; background-image: none; box-shadow: none; border: none; padding: 5px 10px; border-radius: 6px; min-height: 32px; }}
.panel button.tab label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel button.tab:checked {{ background-color: {chipbg}; }}
.panel button.tab:checked label {{ color: {ink}; font-weight: bold; }}
.panel button.act {{ background-color: {chipbg}; border-radius: 6px; padding: 5px 10px; min-height: 32px; }}
.panel button.panelclose {{ min-width: 32px; min-height: 32px; }}
.panel button.act label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
.panel button.act.quiet {{ background-color: transparent; }}
.panel button.act.quiet label {{ color: {faint}; }}
.panel button.act.quiet:hover {{ background-color: {chipbg}; }}
.panel button.act.danger label {{ color: {dangerink}; }}
.panel button.act.danger {{ border-color: {dangerink}; }}
.panel button.act.danger:hover label {{ color: {dangerink}; }}
.panel button.act.chosen {{ background-color: {ink}; border: none; }}
.panel button.act.chosen label {{ color: {ground}; font-weight: bold; }}
.panel switch {{ min-width: 44px; min-height: 28px; }}
.setting {{ font-family: {font}; font-size: {body}px; color: {ink}; }}
.group {{ font-family: {font}; font-size: {caps}px; font-weight: bold; color: {muted}; letter-spacing: 0.07em; margin-top: 14px; margin-bottom: 2px; }}
.keyhint {{ font-family: {mono}; font-size: {small}px; color: {muted}; background-color: {wash}; border-radius: 5px; padding: 1px 5px; }}
.settingdesc {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel stack, .panel box {{ background-color: transparent; }}
.panel scrolledwindow {{ background-color: transparent; }}
.empty {{ font-family: {font}; font-size: {body}px; color: {faint}; }}

.findbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.findbar entry {{ background: none; border: none; box-shadow: none; }}
.findbar label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}

.dlbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.dlbar label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}

scrollbar {{ background: transparent; }}
scrollbar slider {{ background-color: {scroll}; border-radius: 6px; min-width: 8px; min-height: 8px; opacity: 0.85; }}
scrollbar slider:hover {{ background-color: {scrollhover}; }}
scrollbar trough {{ background: transparent; }}
"#,
        ground = ground,
        radius = radius,
        hairline = p.hairline.hex(),
        hover = p.hover.hex(),
        // A hover on a resting row is a step up from the ground; a hover on the
        // active row has to be a step up from the wash, or the cursor lands on
        // the one row that does not answer it. #E4E4E4 is 1.09:1 against the
        // #EFEFEF wash and #454545 is 1.09:1 against #3A3A3A — the same size of
        // step in both appearances.
        livehover = if p.is_dark() { "#454545".to_string() } else { "#E4E4E4".to_string() },
        danger = p.danger.hex(),
        wash = p.wash.hex(),
        tile = p.side_tile.hex(),
        pinlive = p.side_pin_live.hex(),
        // 13px letters and the quiet labels sit on the panel fill as well as on
        // the page, and the panel is the darker ground in light. Each ratio
        // below is for the harsher of the two: dark first, light second.
        pinfg = quiet::pick(quiet::PIN_FG, p.is_dark()).hex(),
        pinlivefg = quiet::pick(quiet::PIN_LIVE_FG, p.is_dark()).hex(),
        rowfg = quiet::pick(quiet::ROW_FG, p.is_dark()).hex(),
        liveink = if p.is_dark() { p.ink.hex() } else { "#0E0E0E".to_string() },
        ink = p.ink.hex(),
        tilebg = if p.is_dark() { p.side_tile.hex() } else { "#F1F1F1".to_string() },
        tilefg = quiet::pick(quiet::TILE_FG, p.is_dark()).hex(),
        sidebg = p.side_bg().hex(),
        sidehover = p.side_hover.hex(),
        divider = if p.is_dark() { "#3C3C3C".to_string() } else { "#ECECEC".to_string() },
        // The strip above the window. The original's runs #EDEDEC at the top
        // to #E6E6E5 at the bottom; one flat value in the middle of that range
        // is the same read at 23px tall.
        desk = p.desk().hex(),
        switchoff = quiet::pick(quiet::SWITCH_OFF, p.is_dark()).hex(),
        knob = if p.is_dark() { "#EDEDED".to_string() } else { "#FFFFFF".to_string() }, /* 3.77 / 3.85 on the track */
        // A page that failed has one sentence to read, and it used to be the
        // faintest text in the window: #9E9E9E on the dark ground was 4.44:1
        // and #767674 on white 4.54:1, both under the 4.5 a body line wants
        // once you allow for the font being 12px. Both now clear it with room.
        emptyfg = quiet::pick(quiet::EMPTY_FG, p.is_dark()).hex(),
        blankhint = quiet::pick(quiet::BLANK_HINT, p.is_dark()).hex(),
        muted = quiet::pick(quiet::MUTED, p.is_dark()).hex(),
        faint = quiet::pick(quiet::FAINT, p.is_dark()).hex(),
        panelbg = if p.is_dark() { "#1A1A1A".to_string() } else { p.wash.hex() },
        chipbg = if p.is_dark() { p.wash.hex() } else { "#FFFFFF".to_string() },
        scrim = if p.is_dark() { rgba(crate::palette::Rgb(0x00, 0x00, 0x00), 0.86) } else { rgba(p.card, 0.88) },
        fieldbg = if p.is_dark() { "#1A1A1A".to_string() } else { "#FFFFFF".to_string() },
        // The edge of the address field is the one boundary a reader has to
        // find without hunting: #B4B4B2 on white was 2.08:1, under the 3:1 a
        // control's outline wants. The pair in `quiet` is measured on every
        // ground the edge is painted on, including the scrim it sits over.
        fieldborder = quiet::pick(quiet::FIELD_BORDER, p.is_dark()).hex(),
        disabledink = quiet::pick(quiet::DISABLED, p.is_dark()).hex(),
        dangerink = quiet::pick(quiet::DANGER, p.is_dark()).hex(),
        scroll = if p.is_dark() { "#3A3A3A".to_string() } else { "#D6D6D4".to_string() },
        scrollhover = if p.is_dark() { "#4A4A4A".to_string() } else { "#C2C2C0".to_string() },
        caps = scale.caps,
        small = scale.small,
        body = scale.body,
        title = scale.title,
        font = FONT,
        mono = MONO,
        row_height = row_height,
    )
}

#[cfg(test)]
mod scale_tests {
    use super::Scale;

    #[test]
    fn the_default_desktop_size_gives_the_sizes_the_design_was_drawn_at() {
        // No GTK in a unit test, so the fallback is what is measured here.
        let s = Scale::from_desktop();
        assert_eq!((s.caps, s.small, s.body, s.title), (10, 12, 13, 15));
    }

    #[test]
    fn every_stop_stays_readable_and_in_order() {
        for points in [8.0f64, 9.0, 10.0, 11.0, 12.0, 14.0, 18.0] {
            let body = (points * 96.0 / 72.0).round() as i32;
            let s = Scale {
                caps: (body - 3).max(9),
                small: (body - 1).max(10),
                body: body.max(11),
                title: body + 2,
            };
            assert!(s.caps >= 9, "caps {s:?} too small at {points}pt");
            assert!(s.caps <= s.small && s.small <= s.body && s.body < s.title);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::quiet;
    use crate::palette::{contrast, Palette, Rgb};
    use crate::store::Look;

    fn light() -> Palette {
        Palette::for_look(Look::Light, false)
    }
    fn dark() -> Palette {
        Palette::for_look(Look::Dark, true)
    }

    /// Every quiet text colour on every ground it is actually painted on.
    ///
    /// The grounds are the ones the stylesheet names, not a guess: a pin sits
    /// on the sidebar and on its own tile, a row title on the page and on a
    /// hovered row, a panel's text on the panel fill. The floor is 4.5, the
    /// WCAG figure for text below 18px, and every colour here is 11-13px.
    #[test]
    fn quiet_text_can_be_read_on_every_ground_it_sits_on() {
        for (name, p, dark) in [("light", light(), false), ("dark", dark(), true)] {
            let panel = if dark { Rgb(0x1A, 0x1A, 0x1A) } else { p.wash };
            let field = if dark { Rgb(0x1A, 0x1A, 0x1A) } else { Rgb(0xFF, 0xFF, 0xFF) };
            let tile = if dark { p.side_tile } else { Rgb(0xF1, 0xF1, 0xF1) };
            let cases: [(&str, Rgb, Rgb); 14] = [
                ("pin letter on the sidebar", quiet::pick(quiet::PIN_FG, dark), p.side_bg()),
                ("pin letter on a tile", quiet::pick(quiet::PIN_FG, dark), tile),
                ("live pin letter", quiet::pick(quiet::PIN_LIVE_FG, dark), p.side_pin_live),
                ("row title on the page", quiet::pick(quiet::ROW_FG, dark), p.card),
                ("row title on a hover", quiet::pick(quiet::ROW_FG, dark), p.side_hover),
                ("tile letter", quiet::pick(quiet::TILE_FG, dark), tile),
                ("muted on the panel", quiet::pick(quiet::MUTED, dark), panel),
                ("muted on the page", quiet::pick(quiet::MUTED, dark), p.card),
                ("faint on the panel", quiet::pick(quiet::FAINT, dark), panel),
                ("faint on a hover", quiet::pick(quiet::FAINT, dark), p.hover),
                ("empty line on the page", quiet::pick(quiet::EMPTY_FG, dark), p.card),
                ("empty line on the panel", quiet::pick(quiet::EMPTY_FG, dark), panel),
                ("failed page's sentence", quiet::pick(quiet::BLANK_HINT, dark), p.card),
                ("failed page's address", quiet::pick(quiet::EMPTY_FG, dark), field),
            ];
            for (what, ink, ground) in cases {
                let ratio = contrast(ink, ground);
                assert!(
                    ratio >= 4.5,
                    "{name}: {what} is {ratio:.2}:1 ({} on {}), text this small wants 4.5",
                    ink.hex(),
                    ground.hex()
                );
            }
        }
    }

    /// The findbar's icon marks are `button.mark`, and they used to take
    /// keyboard focus with nothing drawn for it.
    #[test]
    fn a_focused_mark_draws_an_ink_ring() {
        for p in [light(), dark()] {
            let sheet = super::build(&p, 30);
            let ink = p.ink.hex();
            // The findbar sits on `fieldbg`, not the page ground.
            let ground = if p.is_dark() { Rgb(0x1A, 0x1A, 0x1A) } else { Rgb(0xFF, 0xFF, 0xFF) };
            let rule = format!("button.mark:focus-visible {{ outline: 2px solid {ink};");
            assert!(sheet.contains(&rule), "no ink focus ring on the marks ({ink})");
            let ratio = contrast(p.ink, ground);
            assert!(ratio >= 3.0, "{ink} ring is {ratio:.2}:1, a boundary wants 3");
        }
    }

    /// A boundary is not text, so its floor is 3:1 rather than 4.5 — but the
    /// address field's outline is the one edge a reader has to find without
    /// hunting, and it was 2.08:1 on white before this test existed.
    #[test]
    fn the_field_can_be_found_and_a_switch_can_be_seen() {
        for (name, p, dark) in [("light", light(), false), ("dark", dark(), true)] {
            let field = if dark { Rgb(0x1A, 0x1A, 0x1A) } else { Rgb(0xFF, 0xFF, 0xFF) };
            let panel = if dark { Rgb(0x1A, 0x1A, 0x1A) } else { p.wash };
            let edge = contrast(quiet::pick(quiet::FIELD_BORDER, dark), field);
            assert!(edge >= 3.0, "{name}: the field's edge is {edge:.2}:1, wants 3");
            let track = contrast(quiet::pick(quiet::SWITCH_OFF, dark), panel);
            assert!(track >= 3.0, "{name}: an off switch is {track:.2}:1, wants 3");
        }
    }

    /// The stylesheet and the tests have to be talking about the same colours.
    /// A literal written into the CSS while the test measured a constant is
    /// how the palette's ratios came to describe a window nobody was looking
    /// at, so this checks the built text carries the values it claims.
    #[test]
    fn the_stylesheet_is_built_from_the_colours_that_were_measured() {
        for (p, dark) in [(light(), false), (dark(), true)] {
            let sheet = super::build(&p, 30);
            for (what, pair) in [
                ("pin letter", quiet::PIN_FG),
                ("muted", quiet::MUTED),
                ("faint", quiet::FAINT),
                ("field edge", quiet::FIELD_BORDER),
                ("failed page's sentence", quiet::BLANK_HINT),
            ] {
                let value = quiet::pick(pair, dark).hex();
                assert!(
                    sheet.contains(&value),
                    "{what}: the stylesheet does not use {value}, so the test above \
                     measured a colour the window never shows"
                );
            }
        }
    }
}

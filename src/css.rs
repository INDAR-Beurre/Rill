// One stylesheet for the whole interface, built from the palette so a change
// of appearance is a change of colours and nothing else.
use crate::palette::{Palette, FONT, MONO};

fn rgba(c: crate::palette::Rgb, a: f32) -> String {
    format!("rgba({},{},{},{:.3})", c.0, c.1, c.2, a)
}

pub fn build(p: &Palette, _composited: bool, row_height: i32) -> String {
    let radius = 12;
    let ground = p.card.hex();
    format!(
        r#"
window {{ background-color: {ground}; }}
headerbar.nobar {{ background-color: {ground}; }}
headerbar.nobar, headerbar {{ min-height: 0; padding: 0; margin: 0; border: none; background: none; box-shadow: none; }}
headerbar.nobar button, headerbar button {{ min-height: 0; padding: 0; margin: 0; }}
decoration {{ box-shadow: none; border-width: 0; margin: 0; padding: 0; background: none; border-radius: {radius}px; }}
headerbar.nobar > box {{ min-height: 0; padding: 0; margin: 0; border: none; }}
.shell {{ background-color: {ground}; border-radius: {radius}px; }}
.shell > separator, paned > separator {{ background-color: {divider}; background-image: none; min-width: 1px; min-height: 1px; border: none; }}
.shell > separator:hover, paned > separator:hover {{ background-color: {hairline}; }}

.side {{ background-color: {sidebg}; }}

.light {{ min-width: 15px; min-height: 15px; border-radius: 15px; background-color: {hairline}; }}
.light.red {{ background-color: {red}; }}
.light.amber {{ background-color: {amber}; }}
.light.green {{ background-color: {green}; }}
window.unfocused .light.red, window.unfocused .light.amber, window.unfocused .light.green {{ background-color: {resting}; }}

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
}}
button:hover {{ background-color: {hover}; }}
button:active {{ background-color: {wash}; }}
button.act {{
  background-color: {wash};
  border: 1px solid {fieldborder};
  border-radius: 7px;
  padding: 5px 11px;
}}
button.act:hover {{ background-color: {hairline}; }}
button.act label {{ font-family: {font}; font-size: 12px; color: {ink}; }}
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
combobox button label {{ font-family: {font}; font-size: 12px; color: {ink}; }}
combobox arrow {{ color: {muted}; min-width: 10px; min-height: 10px; }}
menu {{ background-color: {fieldbg}; border: 1px solid {fieldborder}; border-radius: 8px; padding: 4px; }}
menuitem {{ border-radius: 5px; padding: 4px 8px; }}
menuitem:hover {{ background-color: {hover}; }}
menuitem label {{ font-family: {font}; font-size: 12px; color: {ink}; }}
entry {{ background-image: none; box-shadow: none; border: none; }}
button.mark {{ background: none; background-image: none; border: none; box-shadow: none; padding: 4px; border-radius: 6px; min-width: 0; min-height: 0; }}
button.mark:hover {{ background-color: {hover}; }}
button.mark:active {{ background-color: {wash}; }}
button.mark:disabled {{ opacity: 0.35; }}

row, flowbox {{ background-color: transparent; }}
flowboxchild {{ background-color: transparent; padding: 0; }}

.pin {{ border-radius: 9px; background-color: {tile}; }}
.pin.live {{ background-color: {pinlive}; }}
.pinletter {{ font-family: {font}; font-size: 13px; font-weight: normal; color: {pinfg}; }}
.pin.live .pinletter {{ color: {muted}; }}
.pin:hover {{ background-color: {sidehover}; }}

.row {{ border-radius: 7px; background-color: transparent; min-height: {row_height}px; }}
button.close, button.loupe {{ background: none; border: none; box-shadow: none; padding: 1px; min-width: 0; min-height: 0; opacity: 0; }}
.row:hover button.close, .pin:hover button.close {{ opacity: 1; }}
button.close:hover {{ background-color: {hairline}; border-radius: 4px; }}
.row:hover {{ background-color: {hover}; }}
.row.live {{ background-color: {wash}; }}
.rowtitle {{ font-family: {font}; font-size: 13px; font-weight: normal; color: {rowfg}; }}
.row.live .rowtitle {{ color: {liveink}; font-weight: bold; }}
.tile {{ border-radius: 4px; background-color: {tilebg}; }}
.tileletter {{ font-family: {font}; font-size: 11px; font-weight: bold; color: {tilefg}; }}
.panel switch, .panel switch slider {{ min-width: 34px; min-height: 20px; border-radius: 10px; }}
.panel switch {{ background-color: {switchoff}; border: none; }}
.panel switch:checked {{ background-color: {ink}; }}
.panel switch slider {{ background-color: {knob}; border: none; box-shadow: 0 1px 2px rgba(0,0,0,0.28); }}
.panel switch:checked slider {{ background-color: {fieldbg}; }}

.newtab {{ border-radius: 7px; }}
.newtab:hover {{ background-color: {hover}; }}
.newtablabel {{ font-family: {font}; font-size: 13px; font-weight: normal; color: {faint}; }}
.newtab:hover .newtablabel {{ color: {muted}; }}

.helm {{ }}

.card {{ background-color: {ground}; }}
.blank {{ background-color: {ground}; }}
.blanktitle {{ font-family: {font}; font-size: 15px; font-weight: bold; color: {muted}; letter-spacing: -0.01em; }}
.blankhint {{ font-family: {font}; font-size: 12px; color: {blankhint}; }}
.erraddress {{ font-family: {mono}; font-size: 11px; color: {emptyfg}; margin-top: 4px; }}
.blank button.act {{ background-color: {wash}; border: 1px solid {fieldborder}; border-radius: 8px; padding: 6px 14px; }}
.blank button.act:hover {{ background-color: {hairline}; }}
.blank button.act label {{ font-family: {font}; font-size: 12px; color: {ink}; }}


.scrim {{ background-color: {scrim}; }}
.omni {{ background-color: {fieldbg}; border-radius: 12px; border: 1px solid {fieldborder}; box-shadow: 0 8px 24px rgba(0,0,0,0.10); }}
.omni entry {{ background: none; background-image: none; border: none; box-shadow: none; padding: 0; }}
.omni entry:focus {{ border: none; box-shadow: none; }}
.omnitext {{ font-family: {font}; font-size: 15px; color: {ink}; }}
.omnisite {{ font-family: {font}; font-size: 12px; color: {muted}; }}
.suggest {{ background-color: {fieldbg}; border-radius: 10px; border: 1px solid {fieldborder}; box-shadow: 0 10px 28px rgba(0,0,0,0.12); }}
.suggest .sgrow {{ border-radius: 6px; }}
.suggest .sgrow:hover {{ background-color: {hover}; }}
.suggest .sgrow.picked {{ background-color: {wash}; }}
.suggest .sgrow.picked .sgtitle {{ font-weight: bold; }}
.suggest .sghead {{ font-family: {font}; font-size: 10px; font-weight: bold; color: {muted}; letter-spacing: 0.06em; }}
.suggest .sgrow {{ min-height: 44px; }}
.suggest .sgrow {{ border-left: 2px solid transparent; }}
.suggest .sgrow.picked {{ border-left: 2px solid {ink}; border-top-left-radius: 0; border-bottom-left-radius: 0; }}
.panel .ptrow {{ border-radius: 7px; }}
.panel .ptrow:hover {{ background-color: {hover}; }}
.suggest row:hover {{ background-color: {hover}; }}
.suggest .sgtitle {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.suggest .sgurl {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.suggest .sgkind {{ font-family: {font}; font-size: 11px; color: {faint}; }}

.panel {{ background-color: {fieldbg}; border-radius: 12px; border: 1px solid {fieldborder}; box-shadow: 0 18px 48px rgba(0,0,0,0.16); }}
.panel .head {{ font-family: {font}; font-size: 15px; font-weight: bold; color: {ink}; }}
.panel .sub {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.panel entry {{ background-color: {wash}; border-radius: 7px; border: 1px solid {fieldborder}; padding: 6px 10px; }}
.panel entry text {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.panel row {{ border-radius: 7px; padding: 4px 6px; }}
.panel row:hover {{ background-color: {hover}; }}
.panel .ptitle {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.panel .purl {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.keycap {{ font-family: {mono}; font-size: 11px; color: {muted}; background-color: {wash}; border-radius: 5px; padding: 2px 6px; }}
.panel button.tab {{ background: none; background-image: none; box-shadow: none; border: none; padding: 5px 10px; border-radius: 6px; }}
.panel button.tab label {{ font-family: {font}; font-size: 12px; color: {muted}; }}
.panel button.tab:checked {{ background-color: {wash}; }}
.panel button.tab:checked label {{ color: {ink}; font-weight: bold; }}
.panel button.act {{ background-color: {wash}; border: none; border-radius: 6px; padding: 5px 10px; }}
.panel button.act label {{ font-family: {font}; font-size: 12px; color: {ink}; }}
.panel switch {{ }}
.setting {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.group {{ font-family: {font}; font-size: 10px; font-weight: bold; color: {muted}; letter-spacing: 0.07em; margin-top: 14px; margin-bottom: 2px; }}
.keyhint {{ font-family: {mono}; font-size: 11px; color: {muted}; background-color: {wash}; border-radius: 5px; padding: 1px 5px; }}
.settingdesc {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.panel stack, .panel box {{ background-color: transparent; }}
.panel scrolledwindow {{ background-color: transparent; }}
.empty {{ font-family: {font}; font-size: 13px; color: {faint}; }}

.findbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.findbar entry {{ background: none; border: none; box-shadow: none; }}
.findbar label {{ font-family: {font}; font-size: 11px; color: {muted}; }}

.dlbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.dlbar label {{ font-family: {font}; font-size: 12px; color: {ink}; }}

scrollbar {{ background: transparent; }}
scrollbar slider {{ background-color: {scroll}; border-radius: 6px; min-width: 6px; min-height: 6px; }}
scrollbar slider:hover {{ background-color: {scrollhover}; }}
scrollbar trough {{ background: transparent; }}
"#,
        ground = ground,
        radius = radius,
        hairline = p.hairline.hex(),
        red = p.light_red.hex(),
        amber = p.light_amber.hex(),
        green = p.light_green.hex(),
        resting = p.faint.hex(),
        hover = p.hover.hex(),
        wash = p.wash.hex(),
        tile = p.side_tile.hex(),
        pinlive = p.side_pin_live.hex(),
        pinfg = if p.is_dark() { "#6E6E6E".to_string() } else { "#C3C3C3".to_string() },
        rowfg = if p.is_dark() { p.ink.mix(p.muted, 0.35).hex() } else { "#7C7C7C".to_string() },
        liveink = if p.is_dark() { p.ink.hex() } else { "#0E0E0E".to_string() },
        ink = p.ink.hex(),
        tilebg = if p.is_dark() { p.side_tile.hex() } else { "#F1F1F1".to_string() },
        tilefg = if p.is_dark() { p.muted.hex() } else { "#7C7C7C".to_string() },
        sidebg = p.side_bg().hex(),
        sidehover = p.side_hover.hex(),
        divider = if p.is_dark() { "#242424".to_string() } else { "#ECECEC".to_string() },
        switchoff = if p.is_dark() { "#2C2C2C".to_string() } else { "#DCDCDA".to_string() },
        knob = if p.is_dark() { "#EDEDED".to_string() } else { "#FFFFFF".to_string() },
        emptyfg = if p.is_dark() { "#9A9A9A".to_string() } else { "#8A8A8A".to_string() },
        blankhint = if p.is_dark() { "#6E6E6E".to_string() } else { "#9E9E9E".to_string() },
        muted = p.muted.hex(),
        faint = if p.is_dark() { p.faint.hex() } else { "#ABABA9".to_string() },
        scrim = if p.is_dark() { rgba(crate::palette::Rgb(0x00, 0x00, 0x00), 0.86) } else { rgba(p.card, 0.88) },
        fieldbg = if p.is_dark() { "#1A1A1A".to_string() } else { "#FFFFFF".to_string() },
        fieldborder = if p.is_dark() { "#2A2A2A".to_string() } else { "#E6E6E4".to_string() },
        scroll = if p.is_dark() { "#3A3A3A".to_string() } else { "#D6D6D4".to_string() },
        scrollhover = if p.is_dark() { "#4A4A4A".to_string() } else { "#C2C2C0".to_string() },
        font = FONT,
        mono = MONO,
        row_height = row_height,
    )
}

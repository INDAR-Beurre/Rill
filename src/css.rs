// One stylesheet for the whole interface, built from the palette so a change
// of appearance is a change of colours and nothing else.
use crate::palette::{Palette, FONT, MONO};

fn rgba(c: crate::palette::Rgb, a: f32) -> String {
    format!("rgba({},{},{},{:.3})", c.0, c.1, c.2, a)
}

pub fn build(p: &Palette, composited: bool, row_height: i32) -> String {
    let radius = if composited { 12 } else { 0 };
    let shell_bg = if composited { "transparent".to_string() } else { p.card.hex() };
    let ground = p.card.hex();
    format!(
        r#"
window {{ background-color: {shell_bg}; }}
window.csd, .csd {{ padding: 0; margin: 0; border-width: 0; }}
headerbar.nobar, headerbar {{ min-height: 0; padding: 0; margin: 0; border: none; background: none; box-shadow: none; }}
headerbar.nobar button, headerbar button {{ min-height: 0; padding: 0; margin: 0; }}
decoration {{ box-shadow: none; border-width: 0; margin: 0; padding: 0; background: none; border-radius: {radius}px; }}
.csd {{ box-shadow: none; }}
headerbar.nobar > box {{ min-height: 0; padding: 0; margin: 0; border: none; }}
.shell {{ background-color: {ground}; border-radius: {radius}px; }}
.shell > separator {{ background-color: {hairline}; min-width: 1px; min-height: 1px; }}

.side {{ background-color: {sidebg}; border-right: 1px solid {hairline}; }}
.side.noline {{ border-right: none; }}

.lights {{ }}
.light {{ min-width: 14px; min-height: 14px; border-radius: 14px; background-color: {hairline}; }}
.light.red {{ background-color: {red}; }}
.light.amber {{ background-color: {amber}; }}
.light.green {{ background-color: {green}; }}
window.unfocused .light.red, window.unfocused .light.amber, window.unfocused .light.green {{ background-color: {resting}; }}
.lightlabel {{ color: {resting_ink}; font-family: {font}; font-size: 9px; font-weight: bold; }}

button.mark {{ background: none; background-image: none; border: none; box-shadow: none; padding: 4px; border-radius: 6px; min-width: 0; min-height: 0; }}
button.mark:hover {{ background-color: {hover}; }}
button.mark:active {{ background-color: {wash}; }}
button.mark:disabled {{ opacity: 0.35; }}

row, flowbox {{ background-color: transparent; }}
flowboxchild {{ background-color: transparent; padding: 0; }}

.pin {{ border-radius: 9px; background-color: {tile}; }}
.pin.live {{ background-color: {pinlive}; }}
.pinletter {{ font-family: {font}; font-size: 13px; font-weight: normal; color: {pinfg}; }}
.pin:hover {{ background-color: {pinlive}; }}

.row {{ border-radius: 7px; background-color: transparent; min-height: {row_height}px; }}
button.close, button.loupe {{ background: none; border: none; box-shadow: none; padding: 1px; min-width: 0; min-height: 0; opacity: 0; }}
.row:hover button.close, .pin:hover button.close {{ opacity: 1; }}
button.close:hover {{ background-color: {hairline}; border-radius: 4px; }}
.row:hover {{ background-color: {hover}; }}
.row.live {{ background-color: {wash}; }}
.rowtitle {{ font-family: {font}; font-size: 13px; font-weight: normal; color: {rowfg}; }}
.row.live .rowtitle {{ color: {ink}; font-weight: bold; }}
.tile {{ border-radius: 4px; background-color: {tilebg}; }}
.tileletter {{ font-family: {font}; font-size: 11px; font-weight: bold; color: {tilefg}; }}
.panel switch, .panel switch slider {{ min-width: 34px; min-height: 20px; border-radius: 10px; }}
.panel switch {{ background-color: {switchoff}; border: none; }}
.panel switch:checked {{ background-color: {ink}; }}
.panel switch slider {{ background-color: {fieldbg}; border: none; box-shadow: 0 1px 2px rgba(0,0,0,0.18); }}
.panel switch:checked slider {{ background-color: {fieldbg}; }}
.rowlivepin {{ color: {muted}; }}

.newtab {{ border-radius: 7px; }}
.newtab:hover {{ background-color: {hover}; }}
.newtablabel {{ font-family: {font}; font-size: 13px; font-weight: normal; color: {faint}; }}
.newtab:hover .newtablabel {{ color: {muted}; }}

.helm {{ }}
.hairline {{ background-color: {hairline}; }}

.card {{ background-color: {ground}; }}
.blank {{ background-color: {ground}; }}
.blanktitle {{ font-family: {font}; font-size: 15px; font-weight: bold; color: {muted}; letter-spacing: -0.01em; }}
.blankhint {{ font-family: {font}; font-size: 12px; color: {blankhint}; }}
.empty {{ font-family: {font}; font-size: 13px; color: {emptyfg}; }}

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
.panel .ptrow {{ border-radius: 7px; }}
.panel .ptrow:hover {{ background-color: {hover}; }}
.suggest row:hover {{ background-color: {hover}; }}
.suggest .sgtitle {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.suggest .sgurl {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.suggest .sgkind {{ font-family: {font}; font-size: 11px; color: {faint}; }}

.panel {{ background-color: {fieldbg}; border-radius: 12px; border: 1px solid {fieldborder}; }}
.panel .head {{ font-family: {font}; font-size: 15px; font-weight: bold; color: {ink}; }}
.panel .sub {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.panel entry {{ background-color: {wash}; border-radius: 7px; border: 1px solid {fieldborder}; padding: 6px 10px; }}
.panel entry text {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.panel row {{ border-radius: 7px; padding: 4px 6px; }}
.panel row:hover {{ background-color: {hover}; }}
.panel .ptitle {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.panel .purl {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.panel button.tab {{ background: none; border: none; padding: 5px 10px; border-radius: 6px; }}
.panel button.tab label {{ font-family: {font}; font-size: 12px; color: {muted}; }}
.panel button.tab:checked {{ background-color: {wash}; }}
.panel button.tab:checked label {{ color: {ink}; font-weight: bold; }}
.panel button.act {{ background-color: {wash}; border: none; border-radius: 6px; padding: 5px 10px; }}
.panel button.act label {{ font-family: {font}; font-size: 12px; color: {ink}; }}
.panel switch {{ }}
.setting {{ font-family: {font}; font-size: 13px; color: {ink}; }}
.settingdesc {{ font-family: {font}; font-size: 11px; color: {muted}; }}
.panel stack, .panel box {{ background-color: transparent; }}
.panel scrolledwindow {{ background-color: transparent; }}
.empty {{ font-family: {font}; font-size: 13px; color: {faint}; }}

.findbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.findbar entry {{ background: none; border: none; box-shadow: none; }}
.findbar label {{ font-family: {font}; font-size: 11px; color: {muted}; }}

.dlbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.dlbar label {{ font-family: {font}; font-size: 12px; color: {ink}; }}
.errorbar {{ background-color: {fieldbg}; border-radius: 9px; border: 1px solid {fieldborder}; }}
.errorbar label {{ font-family: {font}; font-size: 12px; color: {ink}; }}

scrollbar {{ background: transparent; }}
scrollbar slider {{ background-color: {scroll}; border-radius: 6px; min-width: 6px; min-height: 6px; }}
scrollbar slider:hover {{ background-color: {scrollhover}; }}
scrollbar trough {{ background: transparent; }}
.mono {{ font-family: {mono}; }}
.tooltip {{ background-color: {tooltipbg}; }}
"#,
        shell_bg = shell_bg,
        ground = ground,
        radius = radius,
        hairline = p.hairline.hex(),
        red = p.light_red.hex(),
        amber = p.light_amber.hex(),
        green = p.light_green.hex(),
        resting = p.faint.hex(),
        resting_ink = p.muted.hex(),
        hover = p.hover.hex(),
        wash = p.wash.hex(),
        tile = p.side_tile.hex(),
        pinlive = p.side_pin_live.hex(),
        pinfg = p.muted.hex(),
        rowfg = if p.is_dark() { p.ink.mix(p.muted, 0.35).hex() } else { "#5E5E5E".to_string() },
        ink = p.ink.hex(),
        tilebg = if p.is_dark() { p.side_tile.hex() } else { "#F1F1F1".to_string() },
        tilefg = if p.is_dark() { p.muted.hex() } else { "#7C7C7C".to_string() },
        sidebg = p.side_bg().hex(),
        switchoff = if p.is_dark() { "#2C2C2C".to_string() } else { "#DCDCDA".to_string() },
        emptyfg = if p.is_dark() { "#9A9A9A".to_string() } else { "#8A8A8A".to_string() },
        blankhint = if p.is_dark() { "#6E6E6E".to_string() } else { "#9E9E9E".to_string() },
        muted = p.muted.hex(),
        faint = if p.is_dark() { p.faint.hex() } else { "#ABABA9".to_string() },
        scrim = if p.is_dark() { rgba(p.ground, 0.62) } else { rgba(p.card, 0.74) },
        fieldbg = if p.is_dark() { "#1A1A1A".to_string() } else { "#FFFFFF".to_string() },
        fieldborder = if p.is_dark() { "#2A2A2A".to_string() } else { "#E6E6E4".to_string() },
        scroll = if p.is_dark() { "#3A3A3A".to_string() } else { "#D6D6D4".to_string() },
        scrollhover = if p.is_dark() { "#4A4A4A".to_string() } else { "#C2C2C0".to_string() },
        tooltipbg = if p.is_dark() { "#222222".to_string() } else { "#FFFFFF".to_string() },
        font = FONT,
        mono = MONO,
        row_height = row_height,
    )
}

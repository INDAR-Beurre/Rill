use crate::palette::{Palette, FONT, MONO};

pub mod quiet {
    use crate::palette::Rgb;

    const fn hex(value: u32) -> Rgb {
        Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    pub const PIN_FG: (Rgb, Rgb) = (hex(0x4B4C45), hex(0xD3CEC5));
    pub const PIN_LIVE_FG: (Rgb, Rgb) = (hex(0x24261F), hex(0xFFFFFF));
    pub const ROW_FG: (Rgb, Rgb) = (hex(0x4B4C45), hex(0xCBC5BC));
    pub const TILE_FG: (Rgb, Rgb) = (hex(0x595A52), hex(0xBAB4AA));
    pub const MUTED: (Rgb, Rgb) = (hex(0x595A52), hex(0xBAB4AA));
    pub const FAINT: (Rgb, Rgb) = (hex(0x62635A), hex(0xA8A298));
    pub const EMPTY_FG: (Rgb, Rgb) = MUTED;
    pub const BLANK_HINT: (Rgb, Rgb) = MUTED;
    pub const FIELD_BORDER: (Rgb, Rgb) = (hex(0x818177), hex(0x79746C));
    pub const DISABLED: (Rgb, Rgb) = (hex(0x74766A), hex(0x969087));
    pub const DANGER: (Rgb, Rgb) = (hex(0xAF392C), hex(0xF29C88));
    pub const SWITCH_OFF: (Rgb, Rgb) = (hex(0x818177), hex(0x6E6962));

    pub fn pick(pair: (Rgb, Rgb), dark: bool) -> Rgb {
        if dark { pair.1 } else { pair.0 }
    }
}

#[derive(Debug)]
pub struct Scale {
    /// Captions, counts and key hints. Follows the desktop font size like the
    /// other roles, so captions grow with large-text settings instead of
    /// staying at a fixed 12px. At the default 10pt desktop font it is 12px,
    /// the same size the captions had before.
    pub caption: i32,
    pub small: i32,
    pub body: i32,
    pub title: i32,
    /// Display type: brand marks, page and panel headings, empty-state
    /// titles. These used to be fixed pixels while the text around them
    /// followed the desktop setting. Each role keeps its old 10pt size and
    /// grows with the body size from there.
    pub brand: i32,
    pub blank_title: i32,
    pub start_headline: i32,
    pub start_entry: i32,
    pub panel_heading: i32,
    pub panel_heading_compact: i32,
    pub panel_rail_brand: i32,
    pub panel_empty_title: i32,
    pub welcome_title: i32,
    #[allow(dead_code)]
    pub welcome_card_title: i32,
}

impl Scale {
    pub fn from_points(points: f64) -> Scale {
        let body = ((points * 96.0 / 72.0).round() as i32).max(14);
        // The number is the size this role had at the default 10pt desktop
        // font. Offsetting from body keeps the whole ramp in step.
        let role = |at_default: i32| body + at_default - 14;
        Scale {
            caption: (body - 2).max(12),
            small: (body - 1).max(13),
            body,
            title: body + 4,
            brand: role(16),
            blank_title: role(24),
            start_headline: role(20),
            start_entry: role(15),
            panel_heading: role(24),
            panel_heading_compact: role(20),
            panel_rail_brand: role(16),
            panel_empty_title: role(18),
            welcome_title: role(22),
            welcome_card_title: role(22),
        }
    }

    pub fn from_desktop() -> Scale {
        use gtk::prelude::GtkSettingsExt;
        let points = if gtk::is_initialized() {
            gtk::Settings::default()
                .and_then(|settings| settings.gtk_font_name())
                .and_then(|name| name.split_whitespace().last()?.parse::<f64>().ok())
                .unwrap_or(10.0)
        } else {
            10.0
        };
        Self::from_points(points)
    }
}

pub fn build(palette: &Palette, row_height: i32) -> String {
    let scale = Scale::from_desktop();
    let dark = palette.is_dark();
    // Dark shadows over white web pages read as heavy black halos at 0.38;
    // 0.22 keeps the lift without the smudge.
    let shadow = if dark { "rgba(0,0,0,0.24)" } else { "rgba(32,30,26,0.12)" };
    // Chrome-style super-round pill hierarchy with generous concentric sheets:
    //   r_pill = 999px (true capsules/circles: buttons, tabs, search, address, chips, menu rows)
    //   r_xl   =  20px (floating panels, omnibox, welcome card, dialogs)
    //   r_lg   =  16px (viewport frame, popovers, menus, suggest cards, callouts)
    //   r_md   =  14px (medium option cards & toggle containers)
    //   r_sm   =  10px (nested previews & subtle inner frames)
    let r_pill = 999;
    let r_xl = 20;
    let r_lg = 16;
    let r_md = 14;
    let r_sm = 10;
    format!(
        r#"
window {{ background-color: {ground}; color: {ink}; font-family: {font}; font-size: {body}px; }}
headerbar.nobar, headerbar {{ min-height: 0; padding: 0; margin: 0; border: none; background: none; box-shadow: none; }}
headerbar.nobar > box, headerbar button {{ min-height: 0; padding: 0; margin: 0; border: none; }}
decoration {{ box-shadow: none; border: none; margin: 0; padding: 0; background: none; border-radius: 0; }}
.shell {{ background-color: {ground}; }}
.shell separator, paned separator {{ background-color: {divider}; background-image: none; min-width: 1px; min-height: 1px; border: none; }}
.shell separator:hover, paned separator:hover {{ background-color: {accent}; }}
separator.sitecard-rule {{ background-color: {divider}; background-image: none; min-height: 1px; border: none; }}
separator.topbar-sep {{ background-color: {divider}; background-image: none; min-width: 1px; border: none; }}
.side {{ background-color: {ground}; }}
.colstrip {{ background-color: {ground}; }}
.colstrip:hover {{ background-color: {tint}; }}
.topbar, .topband, .toolbar {{ background-color: {ground}; }}
.brand-word {{ font-family: {font}; font-size: {brand}px; font-weight: 600; letter-spacing: -0.3px; color: {ink}; }}
.sidebar-section, .sidebar-count {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.sidebar-count {{ letter-spacing: 0; padding: 0; }}

button, entry, spinbutton, combobox button {{ background-image: none; box-shadow: none; text-shadow: none; }}
button {{ background-color: transparent; border: 1px solid transparent; color: {ink}; padding: 4px 12px; border-radius: {r_pill}px; min-height: 0; font-weight: normal; }}
button label {{ font-family: {font}; font-weight: normal; }}
button:hover {{ background-color: {hover}; }}
button:active {{ background-color: {wash}; }}
button:focus, entry:focus, switch:focus, .row:focus, .pin:focus, .toptab:focus, .toppin:focus, .ptrow:focus, .sgrow:focus {{ outline: 2px solid {accent}; outline-offset: 2px; }}
button:disabled label {{ color: {disabled}; }}
button.mark {{ background-color: transparent; border: none; padding: 4px; border-radius: {r_pill}px; color: {muted}; min-width: 0; min-height: 0; }}
button.mark:hover {{ background-color: {hover}; color: {ink}; }}
button.mark:focus {{ outline: 2px solid {accent}; outline-offset: 1px; }}
button.mark:disabled {{ opacity: 0.35; }}

.toptabs-scroll, .toptabs-scroll viewport {{ background-color: transparent; border: none; }}
.toptab {{ border-radius: {r_pill}px; border: 1px solid transparent; background-color: transparent; min-height: 28px; color: {muted}; }}
.toptab:hover {{ background-color: {sidehover}; color: {ink}; box-shadow: {pillhover}; }}
.toptab:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
.toptab.live {{ background-color: {surface}; border-color: {divider}; box-shadow: {tabshadow}; color: {ink}; }}
.toptab.live:hover {{ background-color: {card}; border-color: {fieldborder}; }}
.toptab.drag-over, .toppin.drag-over, .row.drag-over, .pin.drag-over {{ border-color: {accent}; background-color: {tint}; }}
.toptab.dragging, .toppin.dragging, .row.dragging, .pin.dragging {{ opacity: 0.55; }}
.toptab-title {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {rowfg}; }}
.toptab.asleep .toptab-title {{ color: {faint}; }}
.toptab.asleep .favicon, .toptab.asleep .rowmark {{ opacity: 0.55; }}
.toptab.keep-awake {{ border-color: {accent}; }}
.toptab.private.live {{ border-color: {fieldborder}; }}
.toppin {{ border: 1px solid transparent; border-radius: {r_sm}px; background-color: transparent; min-width: 30px; min-height: 30px; }}
.toppin:hover {{ background-color: {sidehover}; border-color: {divider}; box-shadow: {pillhover}; }}
.toppin:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
.toppin.live {{ background-color: {surface}; border-color: {divider}; box-shadow: {tabshadow}; }}
.toppin.keep-awake, .pin.keep-awake {{ border-color: {accent}; }}
.toppin.private.live, .pin.private.live {{ border-color: {fieldborder}; }}
button.space-pill {{ background-color: transparent; border: 1px solid transparent; border-radius: {r_pill}px; padding: 2px 8px; min-height: 24px; color: {muted}; }}
button.space-pill:hover {{ background-color: {hover}; border-color: {divider}; color: {ink}; }}
button.space-pill:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
.space-pill-label {{ font-family: {font}; font-size: {caption}px; font-weight: 500; color: {muted}; }}
button.space-dot, button.space-menu-button {{ background-color: transparent; border: 1px solid transparent; border-radius: {r_pill}px; padding: 0; min-height: 26px; min-width: 26px; color: {faint}; box-shadow: none; }}
button.space-dot:hover, button.space-menu-button:hover {{ background-color: {sidehover}; color: {ink}; }}
button.space-dot:active, button.space-menu-button:active {{ background-color: {wash}; }}
button.space-dot.active {{ color: {accent}; background-color: {tint}; border-color: {divider}; }}
.space-meta {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
button.act.danger {{ color: {dangerink}; border-color: {divider}; }}
button.act.danger:hover {{ background-color: {danger}; color: #FFFFFF; border-color: {danger}; }}
button.top-newtab {{ min-width: 28px; min-height: 28px; border-radius: {r_pill}px; padding: 0; color: {muted}; }}
button.top-newtab:hover {{ background-color: {sidehover}; color: {ink}; }}
button.top-newtab:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}

button.newtab-row {{ background-color: transparent; border: none; color: {muted}; border-radius: {r_pill}px; padding: 6px 12px; min-height: 20px; }}
button.newtab-row label.newtab-label {{ color: {muted}; font-size: {small}px; font-weight: 500; }}
button.newtab-row:hover {{ background-color: {sidehover}; color: {ink}; }}
button.newtab-row:active {{ background-color: {wash}; }}
button.sidebar-action {{ color: {muted}; padding: 6px 12px; border-radius: {r_pill}px; min-height: 20px; }}
button.sidebar-action label {{ font-size: {small}px; color: {muted}; }}
button.sidebar-action:hover, button.sidebar-tool:hover {{ background-color: {sidehover}; color: {ink}; }}
button.sidebar-tool {{ min-width: 24px; min-height: 24px; border-radius: {r_pill}px; padding: 0; }}
button.chrome-tool {{ min-width: 28px; min-height: 28px; border-radius: {r_pill}px; padding: 0; }}
button.chrome-tool:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
button.chrome-tool.kept, button.chrome-tool.reading {{ color: {accent}; background-color: transparent; }}
button.chrome-tool.kept:hover, button.chrome-tool.reading:hover {{ background-color: {tint}; }}
.location {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 0 4px; min-height: 30px; box-shadow: none; }}
.location:hover {{ border-color: {fieldborder}; box-shadow: {pillhover}; }}
.location.loading {{ border-color: {fieldborder}; }}
.location.omni-open {{ border-color: {accent}; box-shadow: {focusring}; }}
button.site-info {{ padding: 4px 8px; color: {muted}; border-radius: {r_pill}px; min-width: 26px; min-height: 26px; }}
button.site-info.protected {{ color: {accent}; }}
button.address-button {{ border-radius: {r_pill}px; padding: 4px 12px 4px 4px; }}
button.address-button:hover {{ background-color: transparent; }}
.address-label {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {ink}; }}
.address-path {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.control {{ min-width: 26px; min-height: 26px; border-radius: {r_pill}px; background-color: transparent; color: {muted}; }}
.control:hover {{ background-color: {hover}; color: {ink}; }}
.control:active {{ background-color: {wash}; }}
.control.shut:hover {{ background-color: {danger}; color: #FFFFFF; }}
window.unfocused .control {{ opacity: 0.6; }}

.row {{ border-radius: {r_pill}px; border: 1px solid transparent; background-color: transparent; min-height: {row_height}px; color: {muted}; }}
.row:hover {{ background-color: {sidehover}; box-shadow: {pillhover}; }}
.row.live {{ background-color: {surface}; border: 1px solid {divider}; box-shadow: {tabshadow}; color: {ink}; }}
.row.live:hover {{ background-color: {card}; border-color: {fieldborder}; }}
.row:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
.rowtitle {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {rowfg}; }}
.liveword {{ color: {ink}; font-weight: 600; }}
.row-subtitle {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}
.row.private .row-subtitle {{ color: {accent}; }}
.row.asleep .rowtitle {{ color: {faint}; }}
.row.asleep .favicon, .row.asleep .rowmark {{ opacity: 0.55; }}
.row.keep-awake {{ border-color: {divider}; }}
.row.keep-awake .row-subtitle {{ color: {accent}; }}
button.close, button.loupe {{ background-color: transparent; border: none; border-radius: {r_pill}px; box-shadow: none; padding: 2px; min-width: 0; min-height: 0; opacity: 0; color: {muted}; }}
button.close.rowclose {{ opacity: 1; }}
.pin:hover button.close {{ opacity: 1; }}
button.close:hover {{ background-color: {wash}; color: {ink}; }}
.tile {{ border-radius: {r_pill}px; background-color: {tilebg}; }}
.tileletter {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {tilefg}; }}
.pin {{ border: 1px solid transparent; border-radius: {r_sm}px; background-color: {tilebg}; min-width: 36px; min-height: 36px; }}
.pin:hover {{ background-color: {sidehover}; border-color: {divider}; }}
.pin.live {{ background-color: {pinlive}; border-color: {accent}; box-shadow: {tabshadow}; }}
.pinletter {{ font-family: {font}; font-size: {caption}px; font-weight: 600; letter-spacing: -0.02em; color: {pinfg}; }}
.pin.live .pinletter, .toppin.live .pinletter {{ color: {pinlivefg}; font-weight: 700; }}
row, flowbox, flowboxchild {{ background-color: transparent; padding: 0; }}
viewport {{ background-color: transparent; }}
.card {{ background-color: {ground}; }}
.viewport-frame {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_lg}px; }}
.viewport-frame.fullscreen {{ border: none; border-radius: 0; }}
.viewport, .blank, .error-page {{ background-color: transparent; }}
.progress {{ color: {accent}; background-color: transparent; }}
.bmarkbar {{ background-color: {ground}; padding-top: 0; padding-bottom: 0; }}
.bmark {{ font-family: {font}; font-size: {caption}px; font-weight: 500; color: {muted}; padding: 3px 10px; min-height: 22px; border-radius: {r_pill}px; border: 1px solid transparent; }}
.bmark:hover {{ background-color: {hover}; color: {ink}; border-color: {divider}; }}
.bmark:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
.bmark-search {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 0 10px; min-height: 24px; box-shadow: none; }}
.bmark-search:hover {{ border-color: {fieldborder}; }}
.bmark-search.focused {{ border-color: {accent}; box-shadow: {focusring}; }}
.bmark-search entry {{ background-color: transparent; border: none; outline: none; box-shadow: none; padding: 2px 0; min-height: 0; font-family: {font}; font-size: {caption}px; color: {ink}; }}
.bmark-search entry:focus {{ border: none; outline: none; box-shadow: none; }}
.bmark-suggest-card {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_lg}px; box-shadow: 0 8px 24px {shadow}; }}
.bmark-suggest-card .suggest {{ padding: 6px; }}
.bmark-suggest-card .sgrow {{ border-radius: {r_pill}px; border: 1px solid transparent; min-height: 38px; color: {muted}; }}
.bmark-suggest-card .sgrow:hover {{ background-color: {hover}; }}
.bmark-suggest-card .sgrow.picked {{ background-color: {wash}; border-color: transparent; color: {ink}; }}
.blanktitle {{ font-family: {font}; font-size: {blank_title}px; font-weight: 600; color: {ink}; letter-spacing: -0.4px; }}
.blankhint {{ font-family: {font}; font-size: {body}px; color: {blankhint}; }}
.error-content {{ padding: 24px; }}
.error-glyph {{ color: {dangerink}; }}
.erraddress {{ font-family: {mono}; font-size: {small}px; color: {emptyfg}; margin-top: 8px; }}

.start-page, .start-page viewport {{ background-color: transparent; border: none; }}
.start-canvas {{ background-color: transparent; }}
.start-private-pill {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 5px 14px; box-shadow: {startshadow}; }}
.start-private-icon {{ color: {muted}; }}
.start-private-badge {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {ink}; }}
.start-private-note {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_lg}px; padding: 10px 16px; box-shadow: {startshadow}; }}
.start-private-copy {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.start-search {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 5px 7px 5px 18px; box-shadow: {startshadow}; }}
.start-search:hover {{ border-color: {fieldborder}; }}
.start-search.focused {{ border-color: {accent}; box-shadow: {startshadowfocus}; }}
.start-search.has-text {{ border-color: {accent}; }}
.start-search entry {{ border: none; outline: none; box-shadow: none; background-color: transparent; font-family: {font}; font-size: {body}px; color: {ink}; padding: 6px 0; }}
.start-search entry:focus {{ border: none; outline: none; box-shadow: none; }}
button.start-search-submit {{ background-color: transparent; color: {muted}; border: none; border-radius: {r_pill}px; min-width: 30px; min-height: 30px; padding: 6px; }}
button.start-search-submit .start-arrow {{ color: {muted}; }}
button.start-search-submit:hover {{ background-color: {hover}; color: {ink}; }}
button.start-search-submit:hover .start-arrow {{ color: {ink}; }}
.start-search.has-text button.start-search-submit {{ background-color: {accent}; color: {accentink}; }}
.start-search.has-text button.start-search-submit .start-arrow {{ color: {accentink}; }}
.start-search.has-text button.start-search-submit:hover {{ background-color: {accenthover}; }}
.start-links, .start-link-slot {{ background-color: transparent; padding: 0; }}
.start-suggest {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_lg}px; box-shadow: 0 6px 20px {shadow}; }}
.start-suggest .suggest {{ padding: 6px; }}
.start-suggest .sgrow {{ border-radius: {r_pill}px; border: 1px solid transparent; min-height: 40px; color: {muted}; }}
.start-suggest .sgrow:hover {{ background-color: {hover}; }}
.start-suggest .sgrow.picked {{ background-color: {wash}; border-color: transparent; color: {ink}; }}
.start-page .tile {{ background-color: {hover}; border-radius: {r_pill}px; }}
.start-page button:hover .tile {{ background-color: {wash}; }}
button.start-chip {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 6px 14px 6px 8px; min-height: 34px; box-shadow: {startshadow}; }}
button.start-chip.start-chip-icon-only {{ padding: 0; min-width: 44px; min-height: 44px; border-radius: {r_pill}px; }}
button.start-chip.start-chip-detailed {{ padding: 6px 16px 6px 10px; min-height: 42px; }}
button.start-chip:hover {{ background-color: {hover}; border-color: {fieldborder}; box-shadow: {startshadowfocus}; }}
button.start-chip:active {{ background-color: {wash}; box-shadow: {pressshadow}; }}
.start-chip-title {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {ink}; }}
.start-chip-url {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}

.omni-block {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_xl}px; box-shadow: 0 10px 28px {shadow}; }}
.omni-block.focused {{ border-color: {fieldborder}; box-shadow: 0 12px 32px {shadow}, {focusring}; }}
.omni {{ background-color: transparent; border-radius: {r_xl}px {r_xl}px 0 0; border-bottom: 1px solid {divider}; color: {ink}; }}
.omni.focused {{ border-bottom-color: {fieldborder}; }}
.omni entry {{ background-color: transparent; border: none; box-shadow: none; padding: 0; }}
.omni entry:focus {{ outline: none; border: none; box-shadow: none; }}
.omnitext {{ font-family: {font}; font-size: {start_entry}px; color: {ink}; }}
.omni-search-icon {{ color: {muted}; }}
.omnisite {{ font-family: {font}; font-size: {small}px; color: {accent}; }}
.omnisite-pill {{ padding: 3px 10px; background-color: {tint}; border-radius: {r_pill}px; }}
.omnisite-pill.insecure {{ background-color: {tint}; border: 1px solid {dangerink}; }}
.omnisite-pill.insecure .omnisite {{ color: {dangerink}; font-weight: 600; }}
.omnisite-pill.destination {{ background-color: {wash}; border: 1px solid {divider}; }}
.omnisite-pill.destination .omnisite {{ color: {muted}; }}
.omni-escape {{ font-family: {mono}; font-size: {caption}px; color: {muted}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 2px 8px; }}
.omni-footer {{ border-top: 1px solid {divider}; padding: 8px 16px; }}
.omni-footer label {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
.suggest {{ background-color: transparent; padding: 8px; }}
.suggest .sgrow {{ border-radius: {r_pill}px; border: 1px solid transparent; min-height: 44px; color: {muted}; }}
.suggest .sgrow:hover {{ background-color: {hover}; }}
.suggest .sgrow.picked {{ background-color: {wash}; border-color: transparent; color: {ink}; }}
.sghead {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.sgtitle {{ font-family: {font}; font-size: {body}px; color: {ink}; }}
.sgurl {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.sghost {{ font-family: {font}; font-size: {caption}px; font-weight: 500; color: {rowfg}; }}
.sgpath {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
.sgkind {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}

.panel {{ background-color: {surface}; border-radius: {r_xl}px; border: 1px solid {divider}; box-shadow: 0 10px 28px {shadow}; }}
.panel.panel-fullpage {{ background-color: {surface}; border-radius: {r_lg}px; border: 1px solid {divider}; box-shadow: none; }}
.panel .head, .panel-heading {{ font-family: {font}; font-size: {panel_heading}px; font-weight: 600; letter-spacing: -0.4px; color: {ink}; }}
.panel .sub, .panel-description {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel .panel-rail {{ background-color: {ground}; border-right: 1px solid {divider}; border-radius: 19px 0 0 19px; }}
.panel.panel-fullpage .panel-rail {{ border-radius: 15px 0 0 15px; }}
.panel .panel-rail-brand {{ font-family: {font}; font-size: {panel_rail_brand}px; font-weight: 600; letter-spacing: -0.3px; color: {ink}; }}
.panel button.tab {{ background-color: transparent; border: 1px solid transparent; border-radius: {r_pill}px; padding: 6px 12px; min-height: 22px; color: {muted}; }}
.panel button.tab label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel button.tab:hover {{ background-color: {hover}; }}
.panel button.tab:checked {{ background-color: {tint}; border-color: transparent; color: {accent}; box-shadow: none; }}
.panel button.tab:checked label {{ color: {ink}; font-weight: 600; }}
.panel .settings-toc {{ border-top: 1px solid {divider}; margin-top: 10px; padding-top: 12px; }}
.panel .settings-toc-heading {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; margin-bottom: 4px; padding-left: 10px; }}
.panel button.settings-toc-item {{ background-color: transparent; border: none; border-radius: {r_sm}px; padding: 5px 10px; min-height: 20px; color: {muted}; }}
.panel button.settings-toc-item label {{ font-family: {font}; font-size: {small}px; color: {muted}; font-weight: 400; }}
.panel button.settings-toc-item:hover {{ background-color: {hover}; }}
.panel button.settings-toc-item:hover label {{ color: {ink}; }}
.panel button.settings-toc-item.active-section {{ background-color: {hover}; }}
.panel button.settings-toc-item.active-section label {{ color: {ink}; font-weight: 600; }}
.panel-section {{ background-color: transparent; border: none; border-top: 1px solid {divider}; padding-top: 20px; }}
.panel-section.first-section {{ border-top: none; padding-top: 4px; }}
.panel-setting-row {{ padding: 12px 0; border-bottom: 1px solid {divider}; }}
.panel-setting-row:last-child, .panel-setting-row.panel-keyword-form {{ border-bottom: none; }}
.panel-appearance {{ padding: 6px 0 8px; }}
.panel entry, .dlbar entry, popover.browser-menu entry {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 6px 14px; font-family: {font}; font-size: {body}px; color: {ink}; }}
.panel entry:focus, .dlbar entry:focus, popover.browser-menu entry:focus {{ border-color: {fieldborder}; }}
.panel entry text, .dlbar entry text, popover.browser-menu entry text {{ color: {ink}; font-family: {font}; font-size: {body}px; }}
.panel .panel-filter-box {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 6px 14px; }}
.panel .panel-filter-box.focused {{ border-color: {fieldborder}; }}
.panel .panel-filter-icon {{ color: {muted}; }}
.panel .panel-filter-box entry.panel-filter {{ background-color: transparent; border: none; box-shadow: none; padding: 2px 0; min-height: 0; }}
.panel .panel-filter-box entry.panel-filter:focus {{ border: none; outline: none; box-shadow: none; }}
.panel .ptrow {{ border-radius: {r_md}px; padding: 10px 12px; border: 1px solid transparent; }}
.panel .ptrow:hover {{ background-color: {hover}; }}
.panel .ptitle {{ font-family: {font}; font-size: {body}px; color: {ink}; font-weight: 500; }}
.panel .purl {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel button.panelclose {{ min-width: 28px; min-height: 28px; border-radius: {r_pill}px; }}
.panel button.panel-back-btn {{ background-color: transparent; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 5px 14px 5px 10px; min-height: 28px; color: {ink}; }}
.panel button.panel-back-btn:hover {{ background-color: {hover}; border-color: {fieldborder}; }}
.panel button.panel-back-btn label {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {ink}; }}
.panel-empty-icon {{ color: {muted}; }}
.panel-empty-title {{ font-family: {font}; font-size: {panel_empty_title}px; font-weight: 600; color: {ink}; }}
.panel-empty-description, .empty {{ font-family: {font}; font-size: {small}px; color: {emptyfg}; }}
.group {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; margin-top: 16px; margin-bottom: 8px; }}
.setting {{ font-family: {font}; font-size: {body}px; font-weight: 500; color: {ink}; }}
.settingdesc {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.security-warning {{ color: {dangerink}; font-weight: 600; }}
.sitecard-status {{ background-color: {ground}; border: 1px solid {divider}; border-radius: {r_lg}px; padding: 10px 14px; }}
.sitecard-status.warning {{ border-color: {divider}; background-color: {ground}; }}
.sitecard-status.secure {{ border-color: {divider}; }}
.sitecard-clear {{ margin-top: 8px; }}
.peek-header {{ border-bottom: 1px solid {divider}; }}
.keycap, .keyhint {{ font-family: {mono}; font-size: {caption}px; color: {muted}; background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 2px 9px; }}
.extensionnote {{ padding: 12px 14px; background-color: {ground}; border-radius: {r_lg}px; }}
.compat-badge {{ font-family: {font}; font-size: {caption}px; font-weight: 500; padding: 1px 0; border-radius: 0; background-color: transparent; color: {muted}; border: none; }}
.compat-badge.content {{ color: {ink}; }}
.compat-badge.unsupported {{ color: {dangerink}; }}
.panel box, .panel stack, .panel scrolledwindow {{ background-color: transparent; }}
.panel .panel-rail {{ background-color: {ground}; }}
.panel-rail-caption, .panel-rail-hint .settingdesc {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}
.panel-nav-icon, .panel-section-icon {{ color: {muted}; }}
.panel button.panel-nav-item.on {{ background-color: {tint}; border-color: transparent; color: {accent}; box-shadow: none; }}
.panel button.panel-nav-item.on .panel-nav-icon {{ color: {accent}; }}
.panel.panel-compact .panel-rail {{ border-right: none; border-bottom: 1px solid {divider}; border-radius: 19px 19px 0 0; }}
.panel.panel-fullpage.panel-compact .panel-rail {{ border-radius: 15px 15px 0 0; }}
.panel.panel-compact button.panel-nav-item {{ min-width: 0; padding: 6px 12px; border-radius: {r_pill}px; }}
.panel.panel-compact button.panel-nav-item.on {{ background-color: {tint}; box-shadow: none; }}
.panel .purl.dl-failed {{ color: {dangerink}; font-weight: 600; }}
progressbar.dl-progress {{ min-height: 4px; margin-top: 4px; }}
progressbar.dl-progress trough {{ background-color: {wash}; border: none; border-radius: {r_pill}px; min-height: 4px; }}
progressbar.dl-progress progress {{ background-color: {accent}; border: none; border-radius: {r_pill}px; min-height: 4px; }}
.panel.panel-compact .panel-heading {{ font-size: {panel_heading_compact}px; letter-spacing: -0.3px; }}
.panel .panel-section-heading {{ font-family: {font}; font-size: {title}px; font-weight: 600; letter-spacing: -0.2px; color: {ink}; margin: 0; }}
.panel-section-description {{ font-size: {small}px; color: {muted}; }}
.panel .panel-appearance-option {{ background-color: transparent; border: 1px solid {divider}; padding: 8px; border-radius: {r_md}px; }}
.panel .panel-appearance-option:hover {{ background-color: {hover}; }}
.panel .panel-appearance-option.chosen, .panel .panel-appearance-option:checked {{ background-color: {card}; border-color: {accent}; box-shadow: 0 0 0 1px {accent}; }}
.panel .panel-appearance-option label, .panel .panel-appearance-option.chosen label {{ color: {ink}; font-weight: 500; }}
.panel .panel-appearance-preview {{ color: {divider}; }}
.panel-appearance-label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
.panel-count {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
.panel-list-heading {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.panel-location {{ font-family: {mono}; font-size: {caption}px; color: {muted}; }}
.panel .panel-row-icon {{ border-radius: {r_pill}px; background-color: {ground}; color: {muted}; }}
.panel .ptrow.panel-list-row {{ padding: 8px 12px; }}
.panel .panel-list-row .panel-row-actions button.act {{ border-color: transparent; background-color: transparent; padding: 4px 12px; min-height: 20px; }}
.panel .panel-list-row .panel-row-actions button.act:hover {{ background-color: {hover}; border-color: {divider}; }}
.panel-note {{ padding: 12px 14px; border: 1px solid {divider}; border-radius: {r_lg}px; background-color: {ground}; }}
.panel .panel-about {{ font-family: {font}; font-size: {caption}px; color: {faint}; padding-top: 12px; border-top: 1px solid {divider}; }}
.panel .panel-shortcut-row {{ padding: 8px 0; border-bottom: 1px solid {divider}; }}
.panel .panel-shortcut-key {{ border: 1px solid {divider}; box-shadow: none; background-color: {card}; color: {ink}; padding: 2px 9px; border-radius: {r_pill}px; font-family: {mono}; font-size: {caption}px; text-decoration: none; }}
.panel-empty-tip {{ padding-top: 8px; }}
button.act {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 6px 14px; min-height: 20px; }}
button.act label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
button.act:hover {{ background-color: {hover}; border-color: {fieldborder}; }}
button.act:active {{ background-color: {wash}; }}
button.act.primary {{ background-color: {accent}; border-color: {accent}; color: {accentink}; }}
button.act.primary:hover {{ background-color: {accenthover}; border-color: {accenthover}; }}
button.act.primary label {{ color: {accentink}; font-weight: 600; }}
button.act.chosen {{ background-color: {ink}; border-color: {ink}; }}
button.act.chosen label {{ color: {surface}; font-weight: 600; }}
button.act.quiet {{ background-color: transparent; border-color: transparent; }}
button.act.quiet label {{ color: {muted}; }}
button.act.danger {{ border-color: {dangerink}; }}
button.act.danger label {{ color: {dangerink}; }}
button.act:disabled {{ background-color: transparent; border-color: {divider}; }}
button.act:disabled label {{ color: {disabled}; }}
.panel switch {{ background-color: {switchoff}; border: none; min-width: 38px; min-height: 22px; border-radius: {r_pill}px; }}
.panel switch:checked {{ background-color: {accent}; }}
.panel switch:disabled {{ opacity: 0.45; }}
.panel switch slider {{ min-width: 16px; min-height: 16px; border-radius: {r_pill}px; margin: 3px; background-color: {surface}; border: none; box-shadow: 0 1px 2px rgba(0,0,0,0.2); }}
.panel switch:checked slider {{ background-color: {accentink}; }}
button.pill-switch, button.pill-switch:hover, button.pill-switch:active, button.pill-switch:checked {{ background-color: transparent; border: none; padding: 2px; border-radius: {r_pill}px; min-width: 0; min-height: 0; }}
button.pill-switch:focus {{ outline: 2px solid {accent}; outline-offset: 2px; }}
button.pill-switch:disabled {{ opacity: 0.35; }}
button.engine-picker, button.setting-picker {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 6px 14px; }}
button.engine-picker label, button.setting-picker label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
button.engine-picker:hover, button.engine-picker:checked, button.setting-picker:hover, button.setting-picker:checked {{ background-color: {hover}; border-color: {fieldborder}; }}
popover {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_lg}px; padding: 6px; box-shadow: 0 8px 24px {shadow}; color: {ink}; }}
popover.engine-popover button.act {{ background-color: transparent; border: 1px solid transparent; border-radius: {r_pill}px; padding: 6px 12px; min-height: 20px; }}
popover.engine-popover button.act label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
popover.engine-popover button.act:hover {{ background-color: {hover}; border-color: transparent; }}
popover.engine-popover button.act.chosen {{ background-color: {tint}; border-color: {accent}; }}
popover.engine-popover button.act.chosen label {{ color: {ink}; font-weight: 600; }}
popover.browser-menu {{ padding: 6px; border-radius: {r_lg}px; }}
popover.browser-menu button.menu-row {{ background-color: transparent; border: 1px solid transparent; border-radius: {r_pill}px; padding: 6px 12px; min-height: 26px; }}
popover.browser-menu button.menu-row:hover {{ background-color: {hover}; }}
popover.browser-menu button.menu-row:active {{ background-color: {wash}; }}
popover.browser-menu button.menu-row:disabled label {{ color: {disabled}; }}
popover.browser-menu .menu-row-title {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
popover.browser-menu .menu-row-accel {{ font-family: {mono}; font-size: {caption}px; color: {faint}; }}
popover.browser-menu .menu-zoom-row {{ padding: 4px 8px; }}
popover.browser-menu .menu-zoom-label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
popover.browser-menu button.menu-zoom-btn {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; min-width: 28px; min-height: 26px; padding: 2px 8px; }}
popover.browser-menu button.menu-zoom-btn:hover {{ background-color: {hover}; border-color: {fieldborder}; }}
popover.browser-menu button.menu-zoom-reset {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; min-width: 52px; min-height: 26px; padding: 2px 10px; }}
popover.browser-menu button.menu-zoom-reset:hover {{ background-color: {hover}; border-color: {fieldborder}; }}
popover.browser-menu separator {{ background-color: {divider}; min-height: 1px; margin: 4px 8px; }}
combobox button {{ background-color: {card}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 6px 14px; }}
combobox button label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
combobox arrow {{ color: {muted}; min-width: 10px; min-height: 10px; }}
window.popup, window.search-menu-popup {{ background-color: transparent; background-image: none; box-shadow: none; }}
window.popup decoration, window.search-menu-popup decoration {{ background-color: transparent; box-shadow: none; border: none; margin: 0; padding: 0; }}
menu {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_lg}px; padding: 5px; box-shadow: 0 8px 24px {shadow}; }}
menu separator {{ background-color: {divider}; min-height: 1px; margin: 4px 8px; }}
menuitem {{ border-radius: {r_pill}px; padding: 6px 12px; }}
menuitem:hover {{ background-color: {hover}; }}
menuitem label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
menuitem:disabled label {{ color: {disabled}; }}

.findbar {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_pill}px; padding: 2px 10px; box-shadow: 0 6px 18px {shadow}; }}
.dlbar {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_xl}px; box-shadow: 0 8px 24px {shadow}; }}
.dlbar.toast-pill {{ border-radius: {r_pill}px; padding: 2px 6px; }}
.findbar entry {{ background-color: transparent; border: none; box-shadow: none; }}
.findbar label, .dlbar label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.findbar label.find-zero {{ color: {dangerink}; font-weight: 600; }}
.dlbar.prompt {{ padding: 16px; }}
.dlbar .group {{ margin-top: 12px; }}
.dlbar .setting {{ color: {ink}; }}
.statuspill {{ font-family: {font}; font-size: {caption}px; color: {muted}; padding: 4px 12px; border-radius: {r_pill}px; }}
.welcome-page, .welcome-shell, .welcome-stage, .welcome-hero-outer, .welcome-hero, .welcome-hero-actions, .welcome-header, .welcome-step-stack, .welcome-step-page, .welcome-section, .welcome-footer-wrap, .welcome-actions {{ background-color: transparent; }}
.welcome-choice-icon, .welcome-segment-icon {{ background-color: transparent; }}
.welcome-scroll, .welcome-scroll viewport {{ background-color: transparent; border: none; }}
.welcome-title {{ font-family: {font}; font-size: 28px; font-weight: 700; letter-spacing: -0.6px; color: {ink}; }}
.welcome-step-title {{ font-family: {font}; font-size: {panel_heading}px; font-weight: 600; letter-spacing: -0.4px; color: {ink}; }}
.welcome-short .welcome-title, .welcome-short .welcome-step-title {{ font-size: {welcome_title}px; }}
.welcome-step-meta {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.welcome-description, .welcome-step-sub {{ font-family: {font}; font-size: {body}px; color: {muted}; }}
.welcome-group-label {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {muted}; }}
.welcome-hint, .welcome-foot-hint {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}
.welcome-status {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {accent}; }}
button.welcome-choice {{ background-color: transparent; border: none; border-bottom: 1px solid {divider}; border-radius: 0; padding: 10px 12px; min-height: 52px; box-shadow: none; }}
button.welcome-choice:hover {{ background-color: {hover}; }}
button.welcome-choice:active {{ background-color: {wash}; }}
button.welcome-choice:focus {{ outline: 2px solid {accent}; outline-offset: -2px; }}
button.welcome-choice.chosen {{ background-color: {wash}; border-bottom: 2px solid {accent}; }}
button.welcome-choice .welcome-choice-title {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {ink}; }}
button.welcome-choice .welcome-choice-sub {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
.welcome-choice-icon {{ color: {muted}; }}
button.welcome-choice .welcome-choice-icon {{ color: {muted}; }}
button.welcome-choice.chosen .welcome-choice-icon {{ color: {accent}; }}
.dark .welcome-site-choice image.favicon {{ background-color: rgba(239, 236, 230, 0.90); border-radius: 4px; padding: 1px; }}
.welcome-toggle-row {{ padding: 11px 6px; background-color: transparent; border: none; border-bottom: 1px solid {divider}; border-radius: 0; box-shadow: none; }}
.welcome-toggle-title {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {ink}; }}
.welcome-toggle-sub {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
button.welcome-switch {{ min-width: 44px; min-height: 24px; padding: 0; border: none; border-radius: {r_pill}px; background-color: transparent; box-shadow: none; }}
button.welcome-switch:hover {{ background-color: transparent; }}
button.welcome-switch:focus {{ outline: 2px solid {accent}; outline-offset: 3px; }}
.welcome-import {{ background-color: {surface}; border: 1px solid {divider}; border-radius: {r_lg}px; padding: 11px 18px; box-shadow: none; }}
.welcome-import-title {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {ink}; }}
.welcome-import expander arrow {{ color: {muted}; }}
.welcome-browser-icon {{ color: {muted}; }}
.welcome-browser-name {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
button.welcome-import-btn {{ padding: 5px 14px; min-height: 30px; }}
button.welcome-btn-secondary {{ background-color: {surface}; border: 1px solid {fieldborder}; border-radius: {r_pill}px; padding: 9px 20px; min-height: 40px; box-shadow: none; }}
button.welcome-btn-secondary:hover {{ background-color: {hover}; border-color: {ink}; }}
button.welcome-btn-secondary:active {{ background-color: {wash}; }}
button.welcome-btn-secondary:focus {{ outline: 2px solid {accent}; outline-offset: 2px; }}
button.welcome-btn-secondary .welcome-btn-label, button.welcome-btn-secondary .welcome-btn-icon {{ color: {ink}; font-family: {font}; font-size: {small}px; font-weight: 600; }}
button.welcome-btn-primary {{ background-color: {accent}; border: 1px solid {accent}; border-radius: {r_pill}px; padding: 9px 22px; min-height: 40px; box-shadow: none; }}
button.welcome-btn-primary:hover {{ background-color: {accenthover}; border-color: {accenthover}; }}
button.welcome-btn-primary:active {{ background-color: {accenthover}; border-color: {accenthover}; }}
button.welcome-btn-primary:focus {{ outline: 2px solid {accent}; outline-offset: 2px; }}
button.welcome-btn-primary .welcome-btn-label, button.welcome-btn-primary .welcome-btn-icon {{ color: {accentink}; font-family: {font}; font-size: {small}px; font-weight: 600; }}
button.welcome-skip-link {{ background-color: transparent; border: none; border-radius: {r_pill}px; padding: 4px 12px; min-height: 28px; box-shadow: none; }}
button.welcome-skip-link label {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
button.welcome-skip-link:hover label {{ color: {ink}; }}
button.welcome-skip-link:focus {{ outline: 2px solid {accent}; outline-offset: 2px; }}
.welcome-short button.welcome-choice {{ padding: 7px 10px; min-height: 44px; }}
.welcome-short .welcome-toggle-row {{ padding: 7px 6px; }}
.welcome-short button.welcome-btn-secondary, .welcome-short button.welcome-btn-primary {{ padding: 7px 18px; min-height: 36px; }}
.dark .start-chip image.favicon, .dark .sgrow image.favicon {{ background-color: rgba(239, 236, 230, 0.90); border-radius: 999px; }}
.density-roomy .toptab {{ min-height: 32px; }}
.density-roomy .location {{ min-height: 34px; }}
.density-roomy button.chrome-tool {{ min-width: 32px; min-height: 32px; }}
.customize-preset-group {{ margin-top: 2px; }}
.customize-order-row {{ padding: 7px 2px; border-radius: 0; background-color: transparent; border: none; border-bottom: 1px solid {divider}; }}
.customize-order-row:last-child {{ border-bottom: none; }}
.customize-order-row.customize-order-required {{ border-bottom-color: {divider}; }}
.customize-order-label {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {ink}; }}
button.customize-order-btn {{ padding: 3px 10px; min-height: 22px; }}
button.customize-stepper-btn {{ min-width: 28px; min-height: 26px; padding: 2px 8px; }}
.customize-stepper-value {{ font-family: {mono}; font-size: {caption}px; color: {ink}; }}

entry selection, label selection, textview selection {{ background-color: {tint}; color: {ink}; }}
scrollbar, scrollbar trough {{ background-color: transparent; border: none; }}
scrollbar slider {{ background-color: {fieldborder}; border: none; border-radius: {r_pill}px; min-width: 6px; min-height: 6px; margin: 2px; }}
scrollbar slider:hover {{ background-color: {muted}; }}
button, .control, .row, .toptab, .toppin, .pin, .tile, .colstrip, menuitem, .sgrow, .bmark, .ptrow, .panel-appearance-option, button.welcome-choice, button.welcome-btn-primary, button.welcome-btn-secondary, button.space-pill, button.space-dot, button.space-menu-button, button.start-chip, button.start-search-submit, popover.browser-menu button.menu-row {{ transition: background-color {micro}ms {ease}, color {micro}ms {ease}, border-color {micro}ms {ease}, opacity {micro}ms {ease}, box-shadow {micro}ms {ease}; }}
.location, .start-search, .bmark-search, .omni-block, .omni, .panel .panel-filter-box, .panel entry, .dlbar entry, popover.browser-menu entry, .omnisite-pill {{ transition: background-color {micro}ms {ease}, border-color {micro}ms {ease}, box-shadow {micro}ms {ease}, color {micro}ms {ease}; }}
.rowtitle, .toptab-title, .row-subtitle, .favicon, .pinletter, .space-pill-label, .start-search-icon, .start-arrow, .panel-nav-icon, .welcome-choice-icon {{ transition: color {micro}ms {ease}, opacity {micro}ms {ease}; }}
scrollbar slider, .shell separator, paned separator {{ transition: background-color {micro}ms {ease}, opacity {micro}ms {ease}; }}
progressbar.dl-progress progress {{ transition: background-color {micro}ms {ease}, min-width {micro}ms {ease}; }}
.panel switch {{ transition: background-color {micro}ms {ease}; }}
.panel switch slider {{ transition: background-color {micro}ms {ease}; }}
"#,
        r_pill = r_pill,
        r_xl = r_xl,
        r_lg = r_lg,
        r_md = r_md,
        // CSS transitions share the MICRO role and the coherent Rill 1.3
        // liquid ease-out curve (`motion::curve::CSS_LIQUID`), so hover,
        // focus, pressed, and procedural GdkFrameClock motion feel unified.
        // A reduced-motion desktop gets the final state immediately. This is
        // the CSS half of motion::enabled(), which gates procedural tweens.
        micro = {
            // CSS generation also runs in unit tests without a display. Only ask
            // the motion engine for GTK's preference when a display exists.
            let animated = if gtk::is_initialized() {
                crate::motion::enabled()
            } else {
                true
            };
            if animated { crate::motion::dur::MICRO } else { 0 }
        },
        ease = crate::motion::curve::CSS_LIQUID,
        ground = palette.ground.hex(),
        card = palette.card.hex(),
        surface = palette.surface().hex(),
        ink = palette.ink.hex(),
        muted = quiet::pick(quiet::MUTED, dark).hex(),
        faint = quiet::pick(quiet::FAINT, dark).hex(),
        rowfg = quiet::pick(quiet::ROW_FG, dark).hex(),
        tilefg = quiet::pick(quiet::TILE_FG, dark).hex(),
        pinfg = quiet::pick(quiet::PIN_FG, dark).hex(),
        pinlivefg = quiet::pick(quiet::PIN_LIVE_FG, dark).hex(),
        fieldborder = quiet::pick(quiet::FIELD_BORDER, dark).hex(),
        disabled = quiet::pick(quiet::DISABLED, dark).hex(),
        dangerink = quiet::pick(quiet::DANGER, dark).hex(),
        switchoff = quiet::pick(quiet::SWITCH_OFF, dark).hex(),
        emptyfg = quiet::pick(quiet::EMPTY_FG, dark).hex(),
        blankhint = quiet::pick(quiet::BLANK_HINT, dark).hex(),
        danger = palette.danger.hex(),
        wash = palette.wash.hex(),
        hover = palette.hover.hex(),
        sidehover = palette.side_hover.hex(),
        tabshadow = if dark { "0 1px 3px rgba(0,0,0,0.28)" } else { "0 1px 3px rgba(32,30,26,0.08)" },
        pillhover = if dark { "0 1px 2px rgba(0,0,0,0.18)" } else { "0 1px 3px rgba(32,30,26,0.05)" },
        pressshadow = if dark { "inset 0 1px 2px rgba(0,0,0,0.34)" } else { "inset 0 1px 2px rgba(32,30,26,0.10)" },
        focusring = if dark { "0 0 0 2px rgba(246,146,104,0.24)" } else { "0 0 0 2px rgba(183,69,37,0.16)" },
        startshadow = if dark { "0 1px 3px rgba(0,0,0,0.22)" } else { "0 1px 4px rgba(32,30,26,0.06)" },
        startshadowfocus = if dark { "0 2px 10px rgba(0,0,0,0.34), 0 0 0 2px rgba(246,146,104,0.22)" } else { "0 3px 12px rgba(32,30,26,0.10), 0 0 0 2px rgba(183,69,37,0.14)" },
        tilebg = palette.side_tile.hex(),
        pinlive = palette.side_pin_live.hex(),
        accent = palette.accent().hex(),
        accentink = palette.accent_ink().hex(),
        accenthover = palette.accent().mix(palette.ink, 0.12).hex(),
        tint = palette.tint().hex(),
        divider = palette.hairline.hex(),
        shadow = shadow,
        caption = scale.caption,
        small = scale.small,
        body = scale.body,
        title = scale.title,
        brand = scale.brand,
        blank_title = scale.blank_title,
        start_entry = scale.start_entry,
        panel_heading = scale.panel_heading,
        panel_heading_compact = scale.panel_heading_compact,
        panel_rail_brand = scale.panel_rail_brand,
        panel_empty_title = scale.panel_empty_title,
        welcome_title = scale.welcome_title,
        font = FONT,
        mono = MONO,
        row_height = row_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::contrast;
    use crate::store::Look;

    #[test]
    fn desktop_type_scale_is_readable_and_ordered() {
        for points in [8.0, 10.0, 12.0, 18.0] {
            let scale = Scale::from_points(points);
            assert!(scale.caption >= 12 && scale.small >= 13 && scale.body >= 14);
            assert!(scale.caption <= scale.small, "caption {} above small {} at {points}pt", scale.caption, scale.small);
            assert!(scale.small <= scale.body && scale.body < scale.title);
        }
    }

    #[test]
    fn captions_follow_the_desktop_font_size() {
        // At the default 10pt desktop font, captions keep the 12px they had
        // as a fixed size, so nothing moves for most people.
        assert_eq!(Scale::from_points(10.0).caption, 12);
        // With large text they grow with the rest of the scale, and never
        // shrink as the desktop font gets bigger.
        let mut last = 0;
        for points in [8.0, 10.0, 11.0, 12.0, 14.0, 18.0, 24.0] {
            let caption = Scale::from_points(points).caption;
            assert!(caption >= last, "caption shrank to {caption} at {points}pt");
            last = caption;
        }
        assert!(Scale::from_points(18.0).caption > 12);
    }

    #[test]
    fn no_caption_uses_a_fixed_pixel_size() {
        let css = build(&Palette::for_look(Look::Light, false), 32);
        for size in ["font-size: 10px", "font-size: 11px"] {
            assert!(!css.contains(size), "fixed caption size left: {size}");
        }
    }

    #[test]
    fn the_stylesheet_has_no_unknown_properties() {
        // GTK rejects the whole provider on an unknown property. A single
        // `.error-actions { spacing: 9px; }` in round 70 silently dropped
        // every later rule, including the omnibox, panel and site-card
        // surfaces. Box spacing belongs in Rust, not here.
        let css = build(&Palette::for_look(Look::Light, false), 32);
        assert!(!css.contains("{ spacing:"), "unknown GTK CSS property left");
    }

    #[test]
    fn display_type_follows_the_desktop_font_size() {
        let base = Scale::from_points(10.0);
        assert_eq!((base.brand, base.blank_title, base.start_headline), (16, 24, 20));
        assert_eq!(base.start_entry, 15);
        assert_eq!(
            (base.panel_heading, base.panel_heading_compact, base.panel_empty_title),
            (24, 20, 18)
        );
        assert_eq!((base.welcome_title, base.welcome_card_title), (22, 22));
        let large = Scale::from_points(18.0);
        for (before, after) in [
            (base.brand, large.brand),
            (base.blank_title, large.blank_title),
            (base.start_headline, large.start_headline),
            (base.start_entry, large.start_entry),
            (base.panel_heading, large.panel_heading),
            (base.welcome_card_title, large.welcome_card_title),
        ] {
            assert!(after > before, "display size {before} did not grow at 18pt");
        }
    }

    #[test]
    fn quiet_text_is_measured_on_the_real_surfaces() {
        for dark in [false, true] {
            let palette = Palette::for_look(if dark { Look::Dark } else { Look::Light }, dark);
            for pair in [quiet::MUTED, quiet::FAINT, quiet::ROW_FG, quiet::TILE_FG] {
                let foreground = quiet::pick(pair, dark);
                for background in [palette.ground, palette.card, palette.surface(), palette.hover, palette.side_hover, palette.side_tile] {
                    assert!(contrast(foreground, background) >= 4.5, "{} on {}", foreground.hex(), background.hex());
                }
            }
            assert!(contrast(quiet::pick(quiet::PIN_LIVE_FG, dark), palette.side_pin_live) >= 4.5);
        }
    }

    #[test]
    fn controls_and_accent_labels_pass_contrast_checks() {
        for look in [Look::Light, Look::Dark] {
            let palette = Palette::for_look(look, false);
            let edge = quiet::pick(quiet::FIELD_BORDER, palette.is_dark());
            for background in [palette.card, palette.surface(), palette.ground] {
                assert!(contrast(edge, background) >= 3.0, "edge on {}", background.hex());
            }
            assert!(contrast(palette.accent_ink(), palette.accent()) >= 4.5);
            assert!(contrast(palette.ink, palette.tint()) >= 4.5);
        }
    }

    #[test]
    fn both_themes_share_the_same_complete_ui_system() {
        for look in [Look::Light, Look::Dark] {
            let palette = Palette::for_look(look, false);
            let sheet = build(&palette, 48);
            for class in [".topbar", ".toptab", ".viewport-frame", ".toolbar", ".location", ".row-subtitle", ".panel-rail", ".omni-block", ".colstrip", ".statuspill", "popover.browser-menu", ".compat-badge", ".customize-order-row", ".density-roomy"] {
                assert!(sheet.contains(class), "missing {class}");
            }
            assert!(sheet.contains(&format!("background-color: {}", palette.accent().hex())));
            assert!(sheet.contains("button.mark:focus { outline: 2px solid"));
            assert!(!sheet.contains(":focus-visible"));
            assert!(!sheet.contains("linear-gradient"));
        }
    }

    /// GTK3's CSS parser accepts only the numeric font weights 100 to 900, in
    /// steps of 100. Any other number, such as 650, is a parse error, and a
    /// parse error makes load_from_data throw out the whole stylesheet. In
    /// round 53 one `font-weight: 650` on the wordmark left every panel and
    /// the omnibox with no card surface and the default theme colours, and
    /// the only sign of it was one note line on stderr. This test stops that
    /// from building. Keyword values (normal, bold) are allowed.
    #[test]
    fn every_numeric_font_weight_is_one_gtk3_can_parse() {
        for look in [Look::Light, Look::Dark] {
            let palette = Palette::for_look(look, false);
            let sheet = build(&palette, 48);
            for (index, _) in sheet.match_indices("font-weight:") {
                let value: String = sheet[index + "font-weight:".len()..]
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if let Ok(weight) = value.parse::<u32>() {
                    assert!(
                        (100..=900).contains(&weight) && weight % 100 == 0,
                        "font-weight {weight} is not one GTK3 can parse, so the whole stylesheet would be dropped (near: {})",
                        &sheet[index.saturating_sub(60)..(index + 24).min(sheet.len())]
                    );
                }
            }
        }
    }

    #[test]
    fn motion_css_uses_explicit_properties_and_shared_liquid_curve() {
        for look in [Look::Light, Look::Dark] {
            let palette = Palette::for_look(look, false);
            let sheet = build(&palette, 48);
            assert!(
                sheet.contains(crate::motion::curve::CSS_LIQUID),
                "stylesheet must use shared motion::curve::CSS_LIQUID"
            );
            assert!(!sheet.contains("transition: all"), "never use blanket transition: all");
            assert!(!sheet.contains("transform:"), "GTK3 CSS rejects transform properties");
            assert!(!sheet.contains("@media"), "GTK3 CSS does not support @media queries");
            for selector in [".omni-block.focused", ".location.omni-open", ".start-search.focused", "progressbar.dl-progress progress"] {
                assert!(sheet.contains(selector), "missing motion selector {selector}");
            }
        }
    }
}

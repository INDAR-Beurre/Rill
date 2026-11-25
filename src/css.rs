use crate::palette::{Palette, FONT, MONO};

pub mod quiet {
    use crate::palette::Rgb;

    const fn hex(value: u32) -> Rgb {
        Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    pub const PIN_FG: (Rgb, Rgb) = (hex(0x4B4C45), hex(0xD0D0D0));
    pub const PIN_LIVE_FG: (Rgb, Rgb) = (hex(0x24261F), hex(0xFFFFFF));
    pub const ROW_FG: (Rgb, Rgb) = (hex(0x4B4C45), hex(0xC7C7C7));
    pub const TILE_FG: (Rgb, Rgb) = (hex(0x595A52), hex(0xB5B5B5));
    pub const MUTED: (Rgb, Rgb) = (hex(0x595A52), hex(0xB5B5B5));
    pub const FAINT: (Rgb, Rgb) = (hex(0x62635A), hex(0xA5A5A5));
    pub const EMPTY_FG: (Rgb, Rgb) = MUTED;
    pub const BLANK_HINT: (Rgb, Rgb) = MUTED;
    pub const FIELD_BORDER: (Rgb, Rgb) = (hex(0x818177), hex(0x898989));
    pub const DISABLED: (Rgb, Rgb) = (hex(0x74766A), hex(0x929292));
    pub const DANGER: (Rgb, Rgb) = (hex(0xAF392C), hex(0xF29C88));
    pub const SWITCH_OFF: (Rgb, Rgb) = (hex(0x818177), hex(0x898989));

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
    pub start_headline_wide: i32,
    pub start_headline_narrow: i32,
    pub start_entry: i32,
    pub start_link_title: i32,
    pub panel_heading: i32,
    pub panel_heading_compact: i32,
    pub panel_rail_brand: i32,
    pub panel_empty_title: i32,
    pub welcome_title: i32,
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
            brand: role(20),
            blank_title: role(28),
            start_headline: role(40),
            start_headline_wide: role(44),
            start_headline_narrow: role(32),
            start_entry: role(16),
            start_link_title: role(15),
            panel_heading: role(30),
            panel_heading_compact: role(24),
            panel_rail_brand: role(20),
            panel_empty_title: role(20),
            welcome_title: role(28),
            welcome_card_title: role(30),
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
    let shadow = if dark { "rgba(0,0,0,0.22)" } else { "rgba(44,39,29,0.14)" };
    format!(
        r#"
window {{ background-color: {ground}; color: {ink}; font-family: {font}; font-size: {body}px; }}
headerbar.nobar, headerbar {{ min-height: 0; padding: 0; margin: 0; border: none; background: none; box-shadow: none; }}
headerbar.nobar > box, headerbar button {{ min-height: 0; padding: 0; margin: 0; border: none; }}
decoration {{ box-shadow: none; border: none; margin: 0; padding: 0; background: none; border-radius: 0; }}
.shell {{ background-color: {ground}; }}
.shell separator, paned separator {{ background-color: {divider}; background-image: none; min-width: 1px; min-height: 1px; border: none; }}
.shell separator:hover, paned separator:hover {{ background-color: {accent}; }}
/* The site card's group rules. The card lives on the page overlay, which may not sit inside .shell, so the shell rule above is not enough to reach it. Without a rule of its own the separator falls back to the theme's default look. Same divider colour, 1px tall, and no hover accent, because it is not a handle you can drag. */
separator.sitecard-rule {{ background-color: {divider}; background-image: none; min-height: 1px; border: none; }}
.side {{ background-color: {ground}; border-right: 1px solid {divider}; }}
.colstrip {{ background-color: {ground}; border-right: 1px solid {divider}; }}
.colstrip:hover {{ background-color: {tint}; border-right-color: {accent}; }}
.topband, .toolbar {{ background-color: {ground}; }}
.toolbar {{ border-bottom: 1px solid {divider}; }}
.brand-mark {{ background-color: {accent}; color: {accentink}; border-radius: 7px; }}
.brand-word {{ font-family: {font}; font-size: {brand}px; font-weight: 600; letter-spacing: -0.4px; color: {ink}; }}
.sidebar-section, .sidebar-count {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.sidebar-count {{ letter-spacing: 0; }}
.sidebar-count {{ padding: 0; }}
.sidebar-footer {{ border-top: 1px solid {divider}; padding-top: 12px; }}

button, entry, spinbutton, combobox button {{ background-image: none; box-shadow: none; text-shadow: none; }}
button {{ background-color: transparent; border: 1px solid transparent; color: {ink}; padding: 5px 8px; border-radius: 6px; min-height: 0; font-weight: normal; }}
button label {{ font-family: {font}; font-weight: normal; }}
button:hover {{ background-color: {hover}; }}
button:active {{ background-color: {wash}; }}
button:focus, entry:focus, switch:focus, .row:focus, .ptrow:focus, .sgrow:focus {{ outline: 2px solid {accent}; outline-offset: 2px; }}
button:disabled label {{ color: {disabled}; }}
button.mark {{ background-color: transparent; border: none; padding: 4px; color: {muted}; min-width: 0; min-height: 0; }}
button.mark:hover {{ background-color: {hover}; color: {ink}; }}
button.mark:focus {{ outline: 2px solid {accent}; outline-offset: 1px; }}
button.mark:disabled {{ opacity: 0.35; }}
button.create-tab {{ background-color: {card}; border: 1px solid {divider}; color: {ink}; border-radius: 7px; padding: 8px 11px; min-height: 20px; }}
button.create-tab label {{ color: {ink}; font-size: {small}px; font-weight: 500; }}
button.create-tab:hover {{ background-color: {hover}; }}
button.create-tab .create-hint {{ font-family: {font}; font-size: {caption}px; font-weight: normal; color: {muted}; }}
button.private-tab {{ min-width: 30px; border: 1px solid {divider}; border-radius: 7px; }}
button.sidebar-action {{ color: {muted}; padding: 8px 10px; min-height: 20px; }}
button.sidebar-action label {{ font-size: {small}px; color: {muted}; }}
button.sidebar-action:hover, button.sidebar-tool:hover {{ background-color: {sidehover}; color: {ink}; }}
button.sidebar-tool {{ min-width: 28px; min-height: 28px; }}
button.chrome-tool {{ min-width: 28px; min-height: 28px; border-radius: 6px; }}
button.chrome-tool.kept, button.chrome-tool.reading {{ color: {accent}; background-color: {tint}; }}
.location {{ background-color: {card}; border: 1px solid {divider}; border-radius: 7px; }}
.location:hover {{ border-color: {fieldborder}; }}
button.site-info {{ padding: 9px 10px; color: {muted}; border-radius: 6px 0 0 6px; }}
button.site-info.protected {{ color: {accent}; }}
button.address-button {{ border-radius: 0 6px 6px 0; padding: 9px 12px 9px 0; }}
button.address-button:hover {{ background-color: transparent; }}
.address-label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
.address-path {{ font-family: {mono}; font-size: {caption}px; color: {muted}; }}
.address-hint {{ font-family: {mono}; font-size: {caption}px; color: {muted}; padding: 2px 5px; border: 1px solid {divider}; border-radius: 3px; }}
.control {{ min-width: 28px; min-height: 28px; border-radius: 5px; background-color: transparent; color: {muted}; }}
.control:hover {{ background-color: {hover}; color: {ink}; }}
.control.shut:hover {{ background-color: {danger}; color: #FFFFFF; }}
window.unfocused .control {{ opacity: 0.6; }}

.row {{ border-radius: 6px; border: 1px solid transparent; background-color: transparent; min-height: {row_height}px; color: {muted}; }}
.row:hover {{ background-color: {sidehover}; }}
.row.live {{ background-color: {surface}; border: 1px solid {divider}; box-shadow: inset 3px 0 0 {accent}; color: {ink}; }}
.row.live:hover {{ background-color: {card}; }}
.row:active {{ background-color: {wash}; }}
.rowtitle {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {rowfg}; }}
.liveword {{ color: {ink}; font-weight: 600; }}
.row-subtitle {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}
.row.private .row-subtitle {{ color: {accent}; }}
.row.asleep .rowtitle {{ color: {faint}; }}
.row.asleep .favicon, .row.asleep .rowmark {{ opacity: 0.55; }}
button.close, button.loupe {{ background-color: transparent; border: none; box-shadow: none; padding: 3px; min-width: 0; min-height: 0; opacity: 0; color: {muted}; }}
button.close.rowclose {{ opacity: 1; }}
.pin:hover button.close {{ opacity: 1; }}
button.close:hover {{ background-color: {wash}; color: {ink}; }}
.tile {{ border-radius: 5px; background-color: {tilebg}; }}
.tileletter {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {tilefg}; }}
.pin {{ border: 1px solid {divider}; border-radius: 7px; background-color: {tilebg}; }}
.pin:hover {{ background-color: {sidehover}; }}
.pin.live {{ background-color: {pinlive}; box-shadow: inset 0 -2px 0 {accent}; }}
.pinletter {{ font-family: {font}; font-size: {small}px; font-weight: 600; letter-spacing: -0.02em; color: {pinfg}; }}
.pin.live .pinletter {{ color: {pinlivefg}; font-weight: 600; }}
row, flowbox, flowboxchild {{ background-color: transparent; padding: 0; }}
viewport {{ background-color: transparent; }}
.card, .viewport, .blank, .error-page {{ background-color: {card}; }}
.progress {{ color: {accent}; background-color: transparent; }}
.bmarkbar {{ background-color: {card}; border-bottom: 1px solid {divider}; padding-top: 3px; padding-bottom: 3px; }}
.bmark {{ font-family: {font}; font-size: {small}px; color: {muted}; padding: 4px 9px; min-height: 20px; border-radius: 4px; }}
.bmark:hover {{ background-color: {hover}; color: {ink}; }}
.blanktitle {{ font-family: {font}; font-size: {blank_title}px; font-weight: 600; color: {ink}; letter-spacing: -0.4px; }}
.blankhint {{ font-family: {font}; font-size: {body}px; color: {blankhint}; }}
.error-content {{ padding: 30px; }}
.error-glyph {{ color: {muted}; }}
.erraddress {{ font-family: {mono}; font-size: {small}px; color: {emptyfg}; margin-top: 8px; }}

.start-page, .start-canvas {{ background-color: {card}; }}
.start-eyebrow, .start-session, .start-section-label {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {muted}; }}
.start-brand-mark {{ color: {accent}; }}
.start-session {{ color: {faint}; font-weight: normal; }}
.start-headline {{ font-family: {font}; font-size: {start_headline}px; font-weight: 600; letter-spacing: -0.8px; color: {ink}; }}
.start-wide .start-headline {{ font-size: {start_headline_wide}px; letter-spacing: -0.9px; }}
.start-narrow .start-headline {{ font-size: {start_headline_narrow}px; letter-spacing: -0.5px; }}
.start-intro {{ font-family: {font}; font-size: {body}px; color: {muted}; }}
.start-orbit {{ color: {accent}; background-color: {card}; border-top-color: {ink}; }}
.start-search {{ background-color: {surface}; border: 1px solid {fieldborder}; border-radius: 7px; padding: 5px; }}
.start-search.focused {{ border-color: {accent}; box-shadow: 0 0 0 1px {accent}; }}
.start-search-icon {{ margin-left: 12px; color: {muted}; }}
.start-search entry {{ border: none; outline: none; box-shadow: none; background-color: transparent; font-family: {font}; font-size: {start_entry}px; color: {ink}; padding: 10px 0; }}
.start-search entry:focus {{ border: none; outline: none; box-shadow: none; }}
button.start-search-submit {{ background-color: {accent}; color: {accentink}; border-radius: 4px; min-width: 30px; min-height: 30px; padding: 8px; }}
button.start-search-submit .start-arrow {{ color: {accentink}; }}
button.start-search-submit:hover {{ background-color: {accenthover}; }}
.start-engine {{ font-family: {font}; font-size: {small}px; color: {faint}; }}
.start-rule {{ background-color: {divider}; min-height: 1px; border: none; }}
.start-section-note {{ font-family: {font}; font-size: {small}px; color: {faint}; }}
button.start-section-action {{ padding: 0; border: none; border-radius: 0; }}
button.start-section-action label {{ font-family: {font}; font-size: {caption}px; font-weight: 500; color: {accent}; }}
button.start-section-action:hover {{ background-color: transparent; }}
button.start-section-action:hover label {{ text-decoration: underline; }}
.start-links, .start-link-slot {{ background-color: transparent; padding: 0; }}
button.start-link {{ border: none; border-radius: 0; padding: 12px 0; border-bottom: 1px solid {divider}; }}
button.start-link:hover {{ background-color: {hover}; border-bottom-color: {accent}; }}
.start-icon {{ color: {muted}; }}
.start-link-title {{ font-family: {font}; font-size: {start_link_title}px; font-weight: 600; color: {ink}; }}
.start-link-description {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
.start-empty {{ padding: 16px 0; }}
.start-empty-title {{ font-family: {font}; font-size: {body}px; font-weight: 500; color: {ink}; }}
.start-empty-copy {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
button.start-recent {{ border: none; border-radius: 0; padding: 12px 0; }}
button.start-recent:hover {{ background-color: {hover}; }}
.start-recent-title {{ font-family: {font}; font-size: {small}px; font-weight: 500; color: {ink}; }}
.start-recent-host, .start-recent-age {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
button.start-protection {{ background-color: transparent; border: none; border-radius: 0; padding: 8px 0; }}
button.start-protection:hover {{ background-color: {hover}; }}
.start-protection-icon {{ color: {accent}; }}
.start-protection-title {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {ink}; }}
.start-protection-copy, .start-private-copy {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.start-protection-action {{ font-family: {font}; font-size: {caption}px; font-weight: 500; color: {accent}; }}
.start-protection .start-arrow {{ color: {accent}; }}
.start-private-title {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {ink}; }}
.start-private .start-headline {{ color: {accent}; }}

.omni-block {{ background-color: {surface}; border: 1px solid {fieldborder}; border-radius: 12px; box-shadow: 0 12px 32px {shadow}; }}
.omni {{ background-color: {surface}; border-radius: 11px 11px 0 0; border-bottom: 1px solid {divider}; color: {ink}; }}
.omni.focused {{ border-bottom-color: {accent}; }}
.omni entry {{ background-color: transparent; border: none; box-shadow: none; padding: 0; }}
.omni entry:focus {{ outline: none; border: none; box-shadow: none; }}
.omnitext {{ font-family: {font}; font-size: {entry_size}px; color: {ink}; }}
.omni-search-icon {{ color: {accent}; }}
.omnisite {{ font-family: {font}; font-size: {small}px; color: {accent}; padding: 4px 7px; background-color: {tint}; border-radius: 4px; }}
.omni-escape {{ font-family: {mono}; font-size: {caption}px; color: {muted}; border: 1px solid {divider}; border-radius: 3px; padding: 3px 5px; }}
.omni-footer {{ border-top: 1px solid {divider}; padding: 8px 14px; }}
.omni-footer label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.suggest {{ background-color: {surface}; padding: 8px; }}
.suggest .sgrow {{ border-radius: 6px; min-height: 50px; color: {muted}; }}
.suggest .sgrow:hover {{ background-color: {hover}; }}
.suggest .sgrow.picked {{ background-color: {wash}; color: {ink}; box-shadow: inset 3px 0 0 {accent}, inset 0 0 0 1px {accent}; }}
.sghead {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.sgtitle {{ font-family: {font}; font-size: {body}px; color: {ink}; }}
.sgurl {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.sgkind {{ font-family: {font}; font-size: {small}px; color: {faint}; }}

.panel {{ background-color: {surface}; border-radius: 12px; border: 1px solid {divider}; box-shadow: 0 12px 32px {shadow}; }}
.panel .head, .panel-heading {{ font-family: {font}; font-size: {panel_heading}px; font-weight: 600; letter-spacing: -0.6px; color: {ink}; }}
.panel .sub, .panel-description {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel .panel-rail {{ background-color: {ground}; border-right: 1px solid {divider}; border-radius: 11px 0 0 11px; }}
.panel .panel-rail-brand {{ font-family: {font}; font-size: {panel_rail_brand}px; font-weight: 600; letter-spacing: -0.3px; color: {ink}; }}
.panel button.tab {{ background-color: transparent; border: 1px solid transparent; border-radius: 5px; padding: 9px 10px; min-height: 20px; color: {muted}; }}
.panel button.tab label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel button.tab:hover {{ background-color: {hover}; }}
.panel button.tab:checked {{ background-color: {surface}; border-color: {divider}; color: {accent}; box-shadow: inset 3px 0 0 {accent}; }}
.panel button.tab:checked label {{ color: {ink}; font-weight: 600; }}
.panel-section {{ background-color: transparent; border: none; border-top: 1px solid {divider}; padding-top: 16px; }}
.panel-setting-row {{ padding: 10px 0; border-bottom: 1px solid {divider}; }}
.panel-setting-row:last-child {{ border-bottom: none; }}
.panel-appearance {{ padding: 12px 0 6px; }}
.panel entry, .dlbar entry {{ background-color: {card}; border: 1px solid {fieldborder}; border-radius: 6px; padding: 9px 12px; font-family: {font}; font-size: {body}px; color: {ink}; }}
.panel entry text, .dlbar entry text {{ color: {ink}; font-family: {font}; font-size: {body}px; }}
.panel .ptrow {{ border-radius: 5px; padding: 7px 3px; border-bottom: 1px solid {divider}; }}
.panel .ptrow:hover {{ background-color: {hover}; }}
.panel .ptitle {{ font-family: {font}; font-size: {body}px; color: {ink}; font-weight: 500; }}
.panel .purl {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.panel button.panelclose {{ min-width: 28px; min-height: 28px; }}
.panel-empty-icon {{ color: {accent}; }}
.panel-empty-title {{ font-family: {font}; font-size: {panel_empty_title}px; font-weight: 600; color: {ink}; }}
.panel-empty-description, .empty {{ font-family: {font}; font-size: {small}px; color: {emptyfg}; }}
.group {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; margin-top: 18px; margin-bottom: 8px; }}
.setting {{ font-family: {font}; font-size: {body}px; font-weight: 500; color: {ink}; }}
.settingdesc {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.security-warning {{ color: {dangerink}; font-weight: 600; }}
.sitecard-clear {{ margin-top: 8px; }}
.keycap, .keyhint {{ font-family: {mono}; font-size: {caption}px; color: {muted}; background-color: {card}; border: 1px solid {divider}; border-radius: 3px; padding: 3px 6px; }}
.extensionnote {{ padding: 14px; background-color: {ground}; border-radius: 6px; }}
.panel box, .panel stack, .panel scrolledwindow {{ background-color: transparent; }}
.panel .panel-rail {{ background-color: {ground}; }}
.panel-rail-caption, .panel-rail-hint .settingdesc {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}
.panel-nav-icon, .panel-section-icon {{ color: {muted}; }}
.panel button.panel-nav-item.on {{ background-color: {surface}; border-color: {divider}; color: {accent}; box-shadow: inset 3px 0 0 {accent}; }}
.panel button.panel-nav-item.on .panel-nav-icon {{ color: {accent}; }}
.panel.panel-compact .panel-rail {{ border-right: none; border-bottom: 1px solid {divider}; border-radius: 11px 11px 0 0; }}
.panel.panel-compact button.panel-nav-item {{ min-width: 0; padding: 7px 5px; border-radius: 4px; }}
.panel.panel-compact .panel-heading {{ font-size: {panel_heading_compact}px; letter-spacing: -0.4px; }}
.panel .panel-section-heading {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {muted}; margin: 0; }}
.panel-section-description {{ font-size: {small}px; color: {muted}; }}
.panel .panel-appearance-option {{ background-color: transparent; border: 1px solid {divider}; padding: 10px; border-radius: 6px; }}
.panel .panel-appearance-option:hover {{ background-color: {hover}; }}
.panel .panel-appearance-option.chosen, .panel .panel-appearance-option:checked {{ background-color: {card}; border-color: {accent}; box-shadow: inset 0 -3px 0 {accent}; }}
.panel .panel-appearance-option label, .panel .panel-appearance-option.chosen label {{ color: {ink}; font-weight: 500; }}
.panel .panel-appearance-preview {{ color: {divider}; }}
.panel-appearance-label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
.panel-count {{ font-family: {font}; font-size: {caption}px; color: {muted}; }}
.panel-list-heading {{ font-family: {font}; font-size: {caption}px; font-weight: 600; color: {faint}; }}
.panel-location {{ font-family: {mono}; font-size: {caption}px; color: {muted}; }}
.panel .panel-row-icon {{ border-radius: 5px; background-color: {ground}; color: {muted}; }}
.panel-list-row {{ padding: 10px 0; }}
.panel .panel-list-row .panel-row-actions button.act {{ border-color: transparent; background-color: transparent; padding: 6px 8px; min-height: 18px; }}
.panel .panel-list-row .panel-row-actions button.act:hover {{ background-color: {hover}; border-color: {divider}; }}
.panel-note {{ padding: 14px; border-left: 2px solid {accent}; background-color: {card}; }}
.panel .panel-about {{ font-family: {font}; font-size: {caption}px; color: {faint}; padding-top: 14px; border-top: 1px solid {divider}; }}
.panel .panel-shortcut-row {{ padding: 10px 0; border-bottom: 1px solid {divider}; }}
.panel .panel-shortcut-key {{ border: none; background-color: transparent; color: {ink}; padding: 1px 5px; border-radius: 4px; font-family: {mono}; font-size: {caption}px; text-decoration: none; }}
.panel-empty-tip {{ padding-top: 12px; }}
button.act {{ background-color: {card}; border: 1px solid {fieldborder}; border-radius: 6px; padding: 8px 13px; min-height: 18px; }}
button.act label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
button.act:hover {{ background-color: {hover}; }}
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
.panel switch {{ background-color: {switchoff}; border: none; min-width: 40px; min-height: 24px; border-radius: 12px; }}
.panel switch:checked {{ background-color: {accent}; }}
.panel switch slider {{ min-width: 18px; min-height: 18px; border-radius: 9px; margin: 3px; background-color: {surface}; border: none; box-shadow: 0 1px 3px rgba(0,0,0,0.2); }}
.panel switch:checked slider {{ background-color: {accentink}; }}
combobox button {{ background-color: {card}; border: 1px solid {fieldborder}; border-radius: 6px; padding: 8px 11px; }}
combobox button label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}
combobox arrow {{ color: {muted}; min-width: 10px; min-height: 10px; }}
menu {{ background-color: {surface}; border: 1px solid {divider}; border-radius: 8px; padding: 6px; box-shadow: 0 10px 30px {shadow}; }}
menuitem {{ border-radius: 4px; padding: 7px 10px; }}
menuitem:hover {{ background-color: {hover}; }}
menuitem label {{ font-family: {font}; font-size: {small}px; color: {ink}; }}

.findbar, .dlbar {{ background-color: {surface}; border: 1px solid {fieldborder}; border-radius: 8px; box-shadow: 0 8px 24px {shadow}; }}
.findbar entry {{ background-color: transparent; border: none; box-shadow: none; }}
.findbar label, .dlbar label {{ font-family: {font}; font-size: {small}px; color: {muted}; }}
.dlbar.prompt {{ padding: 18px; }}
.dlbar .group {{ margin-top: 12px; }}
.dlbar .setting {{ color: {ink}; }}
.statuspill {{ font-family: {font}; font-size: {caption}px; color: {muted}; padding: 5px 10px; }}
.welcome-title {{ font-family: {font}; font-size: {welcome_title}px; font-weight: 600; letter-spacing: -0.4px; color: {ink}; }}
.welcome-description {{ font-family: {font}; font-size: {body}px; color: {muted}; }}
.dlbar.welcome-card {{ border-color: {divider}; border-radius: 12px; background-color: {surface}; }}
.dlbar .welcome-eyebrow, .dlbar .welcome-section-title {{ font-family: {font}; font-size: {small}px; font-weight: 600; color: {muted}; }}
.dlbar .welcome-title {{ font-family: {font}; font-size: {welcome_card_title}px; font-weight: 600; letter-spacing: -0.6px; color: {ink}; }}
.dlbar .welcome-description, .dlbar .welcome-shortcut-label {{ font-family: {font}; font-size: {body}px; color: {muted}; }}
.welcome-section {{ border-top: 1px solid {divider}; padding-top: 18px; }}
.dlbar .welcome-shortcut-key {{ font-family: {mono}; font-size: {caption}px; color: {accent}; }}
.dlbar .welcome-hint {{ font-family: {font}; font-size: {caption}px; color: {faint}; }}
.dlbar .welcome-browser-name {{ font-size: {body}px; color: {ink}; }}
.dlbar .welcome-status {{ color: {accent}; }}
.dlbar .welcome-start, .dlbar .welcome-start label {{ color: {accentink}; }}

entry selection, label selection, textview selection {{ background-color: {tint}; color: {ink}; }}
scrollbar, scrollbar trough {{ background-color: transparent; border: none; }}
scrollbar slider {{ background-color: {divider}; border: none; border-radius: 4px; min-width: 6px; min-height: 6px; margin: 2px; }}
scrollbar slider:hover {{ background-color: {faint}; }}
button, .control, .row, .pin, .tile, .colstrip, menuitem, .sgrow {{ transition: background-color {micro}ms {ease}, color {micro}ms {ease}, border-color {micro}ms {ease}, opacity {micro}ms {ease}, box-shadow {micro}ms {ease}; }}
.rowtitle, .row-subtitle, .favicon {{ transition: color {micro}ms {ease}, opacity {micro}ms {ease}; }}
.panel switch {{ transition: background-color {micro}ms {ease}; }}
.panel switch slider {{ transition: background-color {micro}ms {ease}; }}
"#,
        // CSS transitions share the MICRO role and the cubic-out curve that
        // motion::ease_out_cubic uses, so hover states and code-driven fades
        // finish together.
        micro = crate::motion::dur::MICRO,
        ease = "cubic-bezier(0.215, 0.61, 0.355, 1)",
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
        entry_size = scale.title + 2,
        brand = scale.brand,
        blank_title = scale.blank_title,
        start_headline = scale.start_headline,
        start_headline_wide = scale.start_headline_wide,
        start_headline_narrow = scale.start_headline_narrow,
        start_entry = scale.start_entry,
        start_link_title = scale.start_link_title,
        panel_heading = scale.panel_heading,
        panel_heading_compact = scale.panel_heading_compact,
        panel_rail_brand = scale.panel_rail_brand,
        panel_empty_title = scale.panel_empty_title,
        welcome_title = scale.welcome_title,
        welcome_card_title = scale.welcome_card_title,
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
        assert_eq!((base.brand, base.blank_title, base.start_headline), (20, 28, 40));
        assert_eq!((base.start_headline_wide, base.start_headline_narrow), (44, 32));
        assert_eq!((base.start_entry, base.start_link_title), (16, 15));
        assert_eq!(
            (base.panel_heading, base.panel_heading_compact, base.panel_empty_title),
            (30, 24, 20)
        );
        assert_eq!((base.welcome_title, base.welcome_card_title), (28, 30));
        let large = Scale::from_points(18.0);
        for (before, after) in [
            (base.brand, large.brand),
            (base.blank_title, large.blank_title),
            (base.start_headline, large.start_headline),
            (base.start_headline_wide, large.start_headline_wide),
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
            for class in [".brand-word", ".toolbar", ".location", ".row-subtitle", ".panel-rail", ".omni-block", ".colstrip", ".statuspill"] {
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
}

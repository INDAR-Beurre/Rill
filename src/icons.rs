// The small marks the browser draws for itself, in the spirit of the Mac
// build's SF Symbols: optically calibrated monoline geometry on a shared
// 16-unit grid, round caps and joins, drawn at demand so every icon stays
// razor-sharp at 10–48 logical px on both 1x and HiDPI displays.
use crate::palette::Rgb;
use gtk::prelude::*;
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI, TAU};

const GRID: f64 = 16.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Back,
    Forward,
    Reload,
    Home,
    Plus,
    Close,
    Menu,
    Globe,
    Search,
    Up,
    Down,
    Clock,
    Star,
    Tab,
    Minimise,
    Maximise,
    Shield,
    ArrowRight,
    Settings,
    Download,
    Sidebar,
    Command,
    Spark,
    Lock,
    Check,
    Book,
    Stop,
    #[allow(dead_code)]
    Orbit,
    /// A private tab: the mask, not a hat. Two eye holes in a rounded band.
    Mask,
    Warning,
    Offline,
    ShieldAlert,
    Refused,
}

impl Icon {
    /// Directional glyphs that mirror horizontally in right-to-left locales.
    const fn mirrors_in_rtl(self) -> bool {
        matches!(self, Icon::Back | Icon::Forward | Icon::ArrowRight | Icon::Sidebar)
    }
}

/// Optically calibrated stroke width in logical pixels for a given icon size.
/// Keeps small 10–14px marks crisp with open counters while giving 32–48px
/// empty-state and error-page marks calm architectural weight.
pub(crate) fn stroke_width(size: f64) -> f64 {
    if size <= 16.0 {
        (1.22 + (size - 10.0).max(0.0) * (0.38 / 6.0)).clamp(1.15, 1.60)
    } else {
        (1.60 + (size - 16.0) * (0.95 / 32.0)).clamp(1.60, 3.20)
    }
}

/// Read the widget's live CSS foreground color, falling back to `fallback` if transparent.
pub(crate) fn set_source_color(widget: &impl IsA<gtk::Widget>, cr: &cairo::Context, fallback: Rgb) {
    let rgba = widget.style_context().color(widget.state_flags());
    if rgba.alpha() > 0.0 {
        cr.set_source_rgba(rgba.red(), rgba.green(), rgba.blue(), rgba.alpha());
    } else {
        cr.set_source_rgb(
            fallback.0 as f64 / 255.0,
            fallback.1 as f64 / 255.0,
            fallback.2 as f64 / 255.0,
        );
    }
}

pub fn area(kind: Icon, size: i32, color: Rgb) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.connect_draw(move |widget, cr| {
        let w = size as f64;
        // Snap centering offset to an integer logical pixel so odd parent
        // allocations never introduce accidental half-pixel blur.
        let x = ((widget.allocated_width() - size).max(0) as f64 / 2.0).floor();
        let y = ((widget.allocated_height() - size).max(0) as f64 / 2.0).floor();
        let rtl = widget.direction() == gtk::TextDirection::Rtl;
        let _ = cr.save();
        cr.translate(x, y);
        set_source_color(widget, cr, color);
        draw_oriented(cr, kind, w, rtl);
        let _ = cr.restore();
        crate::gtkx::inh(false)
    });
    area
}

fn draw_oriented(cr: &cairo::Context, kind: Icon, w: f64, rtl: bool) {
    let _ = cr.save();
    // Always start from a clean Cairo path so no caller text/shape current point
    // can ever connect a stray stroke into an icon's leading `arc`.
    cr.new_path();
    if rtl && kind.mirrors_in_rtl() {
        cr.translate(w, 0.0);
        cr.scale(-1.0, 1.0);
    }
    cr.set_line_width(stroke_width(w));
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    draw(cr, kind, w);
    let _ = cr.restore();
}

/// Radius for filled circular dots (exclamation points, keyholes, base dots),
/// ensuring they remain legible even at 12–14px without ballooning at 48px.
#[inline]
fn dot_radius(u: f64, sw: f64, units: f64) -> f64 {
    (units * u).max(sw * 0.56)
}

#[inline]
fn fill_dot(cr: &cairo::Context, cx: f64, cy: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(cx, cy, r, 0.0, TAU);
    let _ = cr.fill();
}

/// Closed rectangle with smooth circular-arc corners.
fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 1e-6 {
        cr.rectangle(x, y, w, h);
        return;
    }
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// Closed polygon with per-vertex fillet radii `(x, y, r)` in user pixels.
fn rounded_polygon(cr: &cairo::Context, vertices: &[(f64, f64, f64)]) {
    let n = vertices.len();
    if n < 3 {
        return;
    }
    cr.new_sub_path();
    for i in 0..n {
        let (x0, y0, _) = vertices[(i + n - 1) % n];
        let (x1, y1, r) = vertices[i];
        let (x2, y2, _) = vertices[(i + 1) % n];
        let dx_in = x0 - x1;
        let dy_in = y0 - y1;
        let len_in = dx_in.hypot(dy_in);
        let dx_out = x2 - x1;
        let dy_out = y2 - y1;
        let len_out = dx_out.hypot(dy_out);
        let d = r.min(len_in * 0.38).min(len_out * 0.38).max(0.0);
        if d <= 1e-6 || len_in <= 1e-6 || len_out <= 1e-6 {
            if i == 0 {
                cr.move_to(x1, y1);
            } else {
                cr.line_to(x1, y1);
            }
            continue;
        }
        let sx = x1 + dx_in / len_in * d;
        let sy = y1 + dy_in / len_in * d;
        let ex = x1 + dx_out / len_out * d;
        let ey = y1 + dy_out / len_out * d;
        if i == 0 {
            cr.move_to(sx, sy);
        } else {
            cr.line_to(sx, sy);
        }
        // Quadratic Bezier through vertex `(x1, y1)` elevated to exact cubic.
        let c1x = sx + (x1 - sx) * (2.0 / 3.0);
        let c1y = sy + (y1 - sy) * (2.0 / 3.0);
        let c2x = ex + (x1 - ex) * (2.0 / 3.0);
        let c2y = ey + (y1 - ey) * (2.0 / 3.0);
        cr.curve_to(c1x, c1y, c2x, c2y, ex, ey);
    }
    cr.close_path();
}

/// Shared sculpted heraldic shield silhouette used by `Shield` and `ShieldAlert`.
fn shield_path(cr: &cairo::Context, u: f64) {
    cr.new_sub_path();
    cr.move_to(8.0 * u, 1.85 * u);
    cr.line_to(13.15 * u, 3.75 * u);
    cr.line_to(12.95 * u, 7.85 * u);
    cr.curve_to(12.80 * u, 10.95 * u, 10.75 * u, 13.05 * u, 8.0 * u, 14.15 * u);
    cr.curve_to(5.25 * u, 13.05 * u, 3.20 * u, 10.95 * u, 3.05 * u, 7.85 * u);
    cr.line_to(2.85 * u, 3.75 * u);
    cr.close_path();
}

/// Regular non-self-crossing 5-pointed star vertices in 16-unit coordinates.
/// Alternates outer tips and inner valleys in monotonic 36-degree steps.
fn star_vertices() -> [(f64, f64); 10] {
    let cx = 8.0;
    let cy = 8.40;
    let r_outer = 6.00;
    let r_inner = 2.70;
    let step = PI / 5.0;
    let mut pts = [(0.0, 0.0); 10];
    for (i, slot) in pts.iter_mut().enumerate() {
        let angle = -FRAC_PI_2 + i as f64 * step;
        let r = if i % 2 == 0 { r_outer } else { r_inner };
        *slot = (cx + r * angle.cos(), cy + r * angle.sin());
    }
    pts
}

fn draw(cr: &cairo::Context, kind: Icon, w: f64) {
    let u = w / GRID;
    let c = 8.0 * u;
    let sw = stroke_width(w);

    match kind {
        Icon::Back => {
            // Exact 90-degree chevron optically centered in the 16u cell.
            cr.move_to(10.25 * u, 3.25 * u);
            cr.line_to(5.50 * u, 8.0 * u);
            cr.line_to(10.25 * u, 12.75 * u);
            let _ = cr.stroke();
        }
        Icon::Forward => {
            cr.move_to(5.75 * u, 3.25 * u);
            cr.line_to(10.50 * u, 8.0 * u);
            cr.line_to(5.75 * u, 12.75 * u);
            let _ = cr.stroke();
        }
        Icon::Reload => {
            // 275-degree clockwise arc ending at -45 deg with a tangent-symmetric arrowhead.
            let r = 5.20 * u;
            let a_start = 0.22 * PI;
            let a_end = 1.75 * PI; // -45 deg (upper-right)
            cr.new_sub_path();
            cr.arc(c, c, r, a_start, a_end);
            let _ = cr.stroke();
            let tip_x = c + r * a_end.cos();
            let tip_y = c + r * a_end.sin();
            let barb = 2.40 * u;
            cr.move_to(tip_x, tip_y - barb);
            cr.line_to(tip_x, tip_y);
            cr.line_to(tip_x - barb, tip_y);
            let _ = cr.stroke();
        }
        Icon::Home => {
            // Balanced monoline house silhouette: pitched gable roof and open
            // base walls with subtle inset door arch, crisp at 12-16px.
            cr.move_to(2.35 * u, 7.85 * u);
            cr.line_to(c, 2.75 * u);
            cr.line_to(13.65 * u, 7.85 * u);
            let _ = cr.stroke();

            let r = 1.25 * u;
            cr.new_sub_path();
            cr.move_to(4.05 * u, 6.95 * u);
            cr.line_to(4.05 * u, 13.15 * u - r);
            cr.arc_negative(4.05 * u + r, 13.15 * u - r, r, PI, FRAC_PI_2);
            cr.line_to(11.95 * u - r, 13.15 * u);
            cr.arc_negative(11.95 * u - r, 13.15 * u - r, r, FRAC_PI_2, 0.0);
            cr.line_to(11.95 * u, 6.95 * u);
            let _ = cr.stroke();
        }
        Icon::Plus => {
            cr.move_to(c, 3.0 * u);
            cr.line_to(c, 13.0 * u);
            cr.move_to(3.0 * u, c);
            cr.line_to(13.0 * u, c);
            let _ = cr.stroke();
        }
        Icon::Close => {
            cr.move_to(3.5 * u, 3.5 * u);
            cr.line_to(12.5 * u, 12.5 * u);
            cr.move_to(12.5 * u, 3.5 * u);
            cr.line_to(3.5 * u, 12.5 * u);
            let _ = cr.stroke();
        }
        Icon::Menu => {
            // Generous 4.5u center-to-center vertical air keeps all three bars
            // distinct and crisp even at 12–14px on 1x displays.
            for y in [3.5, 8.0, 12.5] {
                cr.move_to(2.5 * u, y * u);
                cr.line_to(13.5 * u, y * u);
            }
            let _ = cr.stroke();
        }
        Icon::Clock => {
            cr.new_sub_path();
            cr.arc(c, c, 5.75 * u, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(c, 4.70 * u);
            cr.line_to(c, c);
            cr.line_to(10.50 * u, 9.55 * u);
            let _ = cr.stroke();
        }
        Icon::Star => {
            // True non-self-crossing five-point outline star.
            let pts = star_vertices();
            cr.move_to(pts[0].0 * u, pts[0].1 * u);
            for &(px, py) in &pts[1..] {
                cr.line_to(px * u, py * u);
            }
            cr.close_path();
            let _ = cr.stroke();
        }
        Icon::Tab => {
            // Rounded browser window frame with a top tab strip and active-tab divider,
            // pairing cleanly with `Sidebar` and `Command` (never misread as a folder).
            rounded_rect(cr, 2.25 * u, 2.75 * u, 11.5 * u, 10.5 * u, 1.75 * u);
            let _ = cr.stroke();
            let inner_sw = if w <= 16.0 { sw * 0.88 } else { sw * 0.92 };
            cr.set_line_width(inner_sw);
            cr.move_to(2.25 * u, 6.35 * u);
            cr.line_to(13.75 * u, 6.35 * u);
            cr.move_to(7.75 * u, 2.75 * u);
            cr.line_to(7.75 * u, 6.35 * u);
            let _ = cr.stroke();
            cr.set_line_width(sw);
        }
        Icon::Mask => {
            // Sculpted domino eye mask: arched brow, rounded temples, distinct
            // nose-bridge cutout underneath, and two well-spaced eye openings.
            cr.move_to(2.05 * u, 7.65 * u);
            cr.curve_to(2.05 * u, 4.95 * u, 4.75 * u, 4.40 * u, 8.0 * u, 5.25 * u);
            cr.curve_to(11.25 * u, 4.40 * u, 13.95 * u, 4.95 * u, 13.95 * u, 7.65 * u);
            cr.curve_to(13.95 * u, 10.60 * u, 11.40 * u, 11.65 * u, 9.55 * u, 10.55 * u);
            cr.curve_to(8.85 * u, 10.10 * u, 8.45 * u, 9.55 * u, 8.0 * u, 9.55 * u);
            cr.curve_to(7.55 * u, 9.55 * u, 7.15 * u, 10.10 * u, 6.45 * u, 10.55 * u);
            cr.curve_to(4.60 * u, 11.65 * u, 2.05 * u, 10.60 * u, 2.05 * u, 7.65 * u);
            cr.close_path();
            let _ = cr.stroke();

            let eye_r = (0.88 * u).max(sw * 0.50);
            fill_dot(cr, 5.35 * u, 7.75 * u, eye_r);
            fill_dot(cr, 10.65 * u, 7.75 * u, eye_r);
        }
        Icon::Up => {
            cr.move_to(3.25 * u, 10.25 * u);
            cr.line_to(8.0 * u, 5.50 * u);
            cr.line_to(12.75 * u, 10.25 * u);
            let _ = cr.stroke();
        }
        Icon::Down => {
            cr.move_to(3.25 * u, 5.75 * u);
            cr.line_to(8.0 * u, 10.50 * u);
            cr.line_to(12.75 * u, 5.75 * u);
            let _ = cr.stroke();
        }
        Icon::Minimise => {
            cr.move_to(3.0 * u, c);
            cr.line_to(13.0 * u, c);
            let _ = cr.stroke();
        }
        Icon::Maximise => {
            rounded_rect(cr, 3.25 * u, 3.25 * u, 9.5 * u, 9.5 * u, 1.75 * u);
            let _ = cr.stroke();
        }
        Icon::Search => {
            // Optically centered magnifier: handle meets the exact 45-degree
            // outer perimeter of the lens and never enters the circle.
            let lx = 7.00 * u;
            let ly = 7.00 * u;
            let r = 4.50 * u;
            cr.new_sub_path();
            cr.arc(lx, ly, r, 0.0, TAU);
            let _ = cr.stroke();
            let join = r * FRAC_1_SQRT_2;
            cr.move_to(lx + join, ly + join);
            cr.line_to(13.50 * u, 13.50 * u);
            let _ = cr.stroke();
        }
        Icon::Globe => {
            let r = 5.75 * u;
            cr.new_sub_path();
            cr.arc(c, c, r, 0.0, TAU);
            let _ = cr.stroke();
            // Slightly lighter interior graticule preserves crisp open counters at 13–14px.
            let inner_sw = if w <= 16.0 { sw * 0.84 } else { sw * 0.90 };
            cr.set_line_width(inner_sw);
            cr.move_to(c - r, c);
            cr.line_to(c + r, c);
            let _ = cr.stroke();
            // Central meridian ellipse via exact cubic Bezier ellipse approximation.
            let rx = 2.55 * u;
            let k = 0.55228475;
            cr.move_to(c, c - r);
            cr.curve_to(c + rx * k, c - r, c + rx, c - r * k, c + rx, c);
            cr.curve_to(c + rx, c + r * k, c + rx * k, c + r, c, c + r);
            cr.curve_to(c - rx * k, c + r, c - rx, c + r * k, c - rx, c);
            cr.curve_to(c - rx, c - r * k, c - rx * k, c - r, c, c - r);
            cr.close_path();
            let _ = cr.stroke();
            cr.set_line_width(sw);
        }
        Icon::Shield => {
            shield_path(cr, u);
            let _ = cr.stroke();
            // Crisp interior checkmark: "blocking active on secure page".
            cr.set_line_width(sw * 0.92);
            cr.move_to(5.55 * u, 8.05 * u);
            cr.line_to(7.30 * u, 9.80 * u);
            cr.line_to(10.70 * u, 6.20 * u);
            let _ = cr.stroke();
            cr.set_line_width(sw);
        }
        Icon::ArrowRight => {
            cr.move_to(2.75 * u, c);
            cr.line_to(13.0 * u, c);
            cr.move_to(8.75 * u, 3.75 * u);
            cr.line_to(13.0 * u, c);
            cr.line_to(8.75 * u, 12.25 * u);
            let _ = cr.stroke();
        }
        Icon::Settings => {
            // Continuous 6-tooth sculpted mechanical cogwheel with open hub bore.
            let r_out = 6.15 * u;
            let r_in = 4.65 * u;
            let r_hub = 2.15 * u;
            let pitch = TAU / 6.0;
            let half_crown = 0.145; // ~8.3 deg
            let half_root = 0.250; // ~14.3 deg

            cr.new_sub_path();
            for tooth in 0..6 {
                let theta = -FRAC_PI_2 + tooth as f64 * pitch;
                let a0 = theta - half_root;
                let a1 = theta - half_crown;
                let a2 = theta + half_crown;
                let a3 = theta + half_root;
                let next_a0 = theta + pitch - half_root;
                let p0 = (c + r_in * a0.cos(), c + r_in * a0.sin());
                let p1 = (c + r_out * a1.cos(), c + r_out * a1.sin());
                let p2 = (c + r_out * a2.cos(), c + r_out * a2.sin());
                let p3 = (c + r_in * a3.cos(), c + r_in * a3.sin());
                if tooth == 0 {
                    cr.move_to(p0.0, p0.1);
                }
                cr.line_to(p1.0, p1.1);
                cr.line_to(p2.0, p2.1);
                cr.line_to(p3.0, p3.1);
                cr.arc(c, c, r_in, a3, next_a0);
            }
            cr.close_path();
            let _ = cr.stroke();

            cr.new_sub_path();
            cr.arc(c, c, r_hub, 0.0, TAU);
            let _ = cr.stroke();
        }
        Icon::Download => {
            cr.move_to(c, 2.35 * u);
            cr.line_to(c, 9.75 * u);
            cr.move_to(4.85 * u, 6.75 * u);
            cr.line_to(c, 9.90 * u);
            cr.line_to(11.15 * u, 6.75 * u);
            let _ = cr.stroke();

            // Clean rounded U-tray (counter-clockwise 90-degree bottom corners in Y-down Cairo).
            let r = 1.45 * u;
            cr.new_sub_path();
            cr.move_to(2.50 * u, 10.35 * u);
            cr.line_to(2.50 * u, 13.55 * u - r);
            cr.arc_negative(2.50 * u + r, 13.55 * u - r, r, PI, FRAC_PI_2);
            cr.line_to(13.50 * u - r, 13.55 * u);
            cr.arc_negative(13.50 * u - r, 13.55 * u - r, r, FRAC_PI_2, 0.0);
            cr.line_to(13.50 * u, 10.35 * u);
            let _ = cr.stroke();
        }
        Icon::Sidebar => {
            rounded_rect(cr, 2.25 * u, 2.75 * u, 11.5 * u, 10.5 * u, 1.75 * u);
            let _ = cr.stroke();
            cr.move_to(6.05 * u, 2.75 * u);
            cr.line_to(6.05 * u, 13.25 * u);
            let _ = cr.stroke();
        }
        Icon::Command => {
            rounded_rect(cr, 2.25 * u, 2.75 * u, 11.5 * u, 10.5 * u, 1.75 * u);
            let _ = cr.stroke();
            cr.move_to(4.75 * u, 5.85 * u);
            cr.line_to(6.90 * u, 8.0 * u);
            cr.line_to(4.75 * u, 10.15 * u);
            cr.move_to(8.45 * u, 10.15 * u);
            cr.line_to(11.25 * u, 10.15 * u);
            let _ = cr.stroke();
        }
        Icon::Spark => {
            // Crisp four-point concave editorial star.
            let r = 6.00 * u;
            let k = 1.32 * u;
            cr.move_to(c, c - r);
            cr.curve_to(c + k, c - k, c + k, c - k, c + r, c);
            cr.curve_to(c + k, c + k, c + k, c + k, c, c + r);
            cr.curve_to(c - k, c + k, c - k, c + k, c - r, c);
            cr.curve_to(c - k, c - k, c - k, c - k, c, c - r);
            cr.close_path();
            let _ = cr.fill();
        }
        Icon::Lock => {
            // Rounded U-shackle over a rounded rectangular body with legible keyhole dot.
            let sr = 2.55 * u;
            let sy = 5.35 * u;
            cr.new_sub_path();
            cr.move_to(c - sr, 7.30 * u);
            cr.line_to(c - sr, sy);
            cr.arc(c, sy, sr, PI, TAU);
            cr.line_to(c + sr, 7.30 * u);
            let _ = cr.stroke();

            rounded_rect(cr, 3.30 * u, 7.25 * u, 9.40 * u, 6.20 * u, 1.50 * u);
            let _ = cr.stroke();

            fill_dot(cr, c, 10.35 * u, dot_radius(u, sw, 0.72));
        }
        Icon::Check => {
            cr.move_to(3.0 * u, 8.40 * u);
            cr.line_to(6.40 * u, 11.80 * u);
            cr.line_to(13.15 * u, 4.35 * u);
            let _ = cr.stroke();
        }
        Icon::Book => {
            // Open reading book with arched page tops and grounded spine.
            cr.new_sub_path();
            cr.move_to(c, 4.20 * u);
            cr.curve_to(6.60 * u, 3.05 * u, 4.25 * u, 2.95 * u, 2.35 * u, 3.55 * u);
            cr.line_to(2.35 * u, 11.80 * u);
            cr.curve_to(4.25 * u, 11.25 * u, 6.60 * u, 11.40 * u, c, 12.70 * u);
            cr.curve_to(9.40 * u, 11.40 * u, 11.75 * u, 11.25 * u, 13.65 * u, 11.80 * u);
            cr.line_to(13.65 * u, 3.55 * u);
            cr.curve_to(11.75 * u, 2.95 * u, 9.40 * u, 3.05 * u, c, 4.20 * u);
            cr.line_to(c, 12.70 * u);
            let _ = cr.stroke();
        }
        Icon::Stop => {
            rounded_rect(cr, 3.65 * u, 3.65 * u, 8.70 * u, 8.70 * u, 1.65 * u);
            let _ = cr.stroke();
        }
        Icon::Warning => {
            // Rounded equilateral triangle distinct from ShieldAlert.
            let pts = [
                (8.0 * u, 2.35 * u, 1.35 * u),
                (13.65 * u, 12.75 * u, 1.35 * u),
                (2.35 * u, 12.75 * u, 1.35 * u),
            ];
            rounded_polygon(cr, &pts);
            let _ = cr.stroke();

            cr.move_to(c, 5.70 * u);
            cr.line_to(c, 8.45 * u);
            let _ = cr.stroke();
            fill_dot(cr, c, 10.75 * u, dot_radius(u, sw, 0.68));
        }
        Icon::Offline => {
            // Upward Wi-Fi signal fan crossed by a crisp diagonal slash.
            let oy = 12.25 * u;
            cr.new_sub_path();
            cr.arc(c, oy, 7.60 * u, 1.24 * PI, 1.76 * PI);
            let _ = cr.stroke();
            cr.new_sub_path();
            cr.arc(c, oy, 4.55 * u, 1.22 * PI, 1.78 * PI);
            let _ = cr.stroke();
            fill_dot(cr, c, 12.05 * u, dot_radius(u, sw, 0.85));

            cr.move_to(2.55 * u, 2.55 * u);
            cr.line_to(13.45 * u, 13.45 * u);
            let _ = cr.stroke();
        }
        Icon::ShieldAlert => {
            shield_path(cr, u);
            let _ = cr.stroke();
            // Distinct alert exclamation inside the shield contour.
            cr.move_to(c, 4.75 * u);
            cr.line_to(c, 8.20 * u);
            let _ = cr.stroke();
            fill_dot(cr, c, 10.75 * u, dot_radius(u, sw, 0.72));
        }
        Icon::Refused => {
            let r = 5.75 * u;
            cr.new_sub_path();
            cr.arc(c, c, r, 0.0, TAU);
            let _ = cr.stroke();
            let d = r * FRAC_1_SQRT_2;
            cr.move_to(c - d, c + d);
            cr.line_to(c + d, c - d);
            let _ = cr.stroke();
        }
        Icon::Orbit => {
            // Minimalist ringed planet with front-wrapping orbital ellipse.
            let r = 3.10 * u;
            cr.new_sub_path();
            cr.arc(c, c, r, 0.0, TAU);
            let _ = cr.stroke();

            let _ = cr.save();
            cr.translate(c, c);
            cr.rotate(-0.48);
            cr.scale(1.0, 0.36);
            cr.new_sub_path();
            cr.arc(0.0, 0.0, 6.20 * u, -0.34 * PI, 1.34 * PI);
            let _ = cr.restore();
            let _ = cr.stroke();
        }
    }
}

/// A small round button with one of the marks in it.
pub fn button(kind: Icon, size: i32, color: Rgb, class: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.add(&area(kind, size, color));
    button.style_context().add_class(class);
    crate::ui::class(&button, "mark");
    button.set_can_focus(false);
    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Palette;
    use crate::store::Look;

    const ALL: [Icon; 33] = [
        Icon::Back,
        Icon::Forward,
        Icon::Reload,
        Icon::Home,
        Icon::Plus,
        Icon::Close,
        Icon::Menu,
        Icon::Globe,
        Icon::Search,
        Icon::Up,
        Icon::Down,
        Icon::Clock,
        Icon::Star,
        Icon::Tab,
        Icon::Minimise,
        Icon::Maximise,
        Icon::Shield,
        Icon::ArrowRight,
        Icon::Settings,
        Icon::Download,
        Icon::Sidebar,
        Icon::Command,
        Icon::Spark,
        Icon::Lock,
        Icon::Check,
        Icon::Book,
        Icon::Stop,
        Icon::Orbit,
        Icon::Mask,
        Icon::Warning,
        Icon::Offline,
        Icon::ShieldAlert,
        Icon::Refused,
    ];

    fn render_alpha(kind: Icon, size: i32, scale: i32, rtl: bool) -> Vec<u8> {
        let px = size * scale;
        let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, px, px)
            .expect("image surface");
        {
            let cr = cairo::Context::new(&surface).expect("cairo context");
            cr.scale(scale as f64, scale as f64);
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
            draw_oriented(&cr, kind, size as f64, rtl);
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().expect("surface data");
        let mut alpha = Vec::with_capacity((px * px) as usize);
        for y in 0..px as usize {
            for x in 0..px as usize {
                // ARgb32 on little-endian stores bytes as [B, G, R, A].
                alpha.push(data[y * stride + x * 4 + 3]);
            }
        }
        alpha
    }

    fn alpha_bbox(alpha: &[u8], dim: usize, threshold: u8) -> Option<(usize, usize, usize, usize)> {
        let mut min_x = dim;
        let mut min_y = dim;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut found = false;
        for y in 0..dim {
            for x in 0..dim {
                if alpha[y * dim + x] >= threshold {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    found = true;
                }
            }
        }
        found.then_some((min_x, min_y, max_x, max_y))
    }

    #[test]
    fn all_32_icons_render_visibly_across_canonical_sizes_and_scales() {
        for &kind in &ALL {
            for size in [10, 12, 14, 16, 20, 48] {
                for scale in [1, 2] {
                    let dim = (size * scale) as usize;
                    let alpha = render_alpha(kind, size, scale, false);
                    let ink_pixels = alpha.iter().filter(|&&a| a >= 32).count();
                    assert!(
                        ink_pixels >= (scale * scale * 4) as usize,
                        "{kind:?} at {size}px@{scale}x drew only {ink_pixels} ink pixels"
                    );
                    let bbox = alpha_bbox(&alpha, dim, 64)
                        .unwrap_or_else(|| panic!("{kind:?} at {size}px@{scale}x had no opaque ink"));
                    let span_x = bbox.2 - bbox.0 + 1;
                    let span_y = bbox.3 - bbox.1 + 1;
                    let major = span_x.max(span_y) as f64 / dim as f64;
                    assert!(
                        major >= 0.45,
                        "{kind:?} at {size}px@{scale}x is too tiny (major span {major:.2})"
                    );
                    let max_major = if size == 10 && scale == 1 { 1.00 } else { 0.96 };
                    assert!(
                        major <= max_major,
                        "{kind:?} at {size}px@{scale}x risks clipping (major span {major:.2})"
                    );
                }
            }
        }
    }

    #[test]
    fn star_is_a_true_non_self_crossing_five_point_polygon() {
        let pts = star_vertices();
        // Angles must increase strictly monotonically around the circle.
        let mut prev = -FRAC_PI_2 - 0.1;
        for (i, &(x, y)) in pts.iter().enumerate() {
            let mut angle = (y - 8.40).atan2(x - 8.0);
            if angle < prev {
                angle += TAU;
            }
            assert!(angle > prev, "star vertex {i} reversed angle order");
            prev = angle;
        }
        // Check no two non-adjacent edges intersect.
        let seg_intersect = |a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)| -> bool {
            let ccw = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
                (r.1 - p.1) * (q.0 - p.0) > (q.1 - p.1) * (r.0 - p.0)
            };
            ccw(a, c, d) != ccw(b, c, d) && ccw(a, b, c) != ccw(a, b, d)
        };
        let n = pts.len();
        for i in 0..n {
            for j in (i + 2)..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                assert!(
                    !seg_intersect(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]),
                    "star edges {i} and {j} self-intersect"
                );
            }
        }
    }

    #[test]
    fn menu_bars_keep_open_air_between_strokes_at_14px() {
        let size = 14usize;
        let alpha = render_alpha(Icon::Menu, size as i32, 1, false);
        let center_col: Vec<u8> = (0..size).map(|y| alpha[y * size + (size / 2)]).collect();
        // Count distinct opaque horizontal bands along the vertical center line.
        let mut bands = 0usize;
        let mut in_band = false;
        for &a in &center_col {
            if a >= 140 {
                if !in_band {
                    bands += 1;
                    in_band = true;
                }
            } else if a <= 40 {
                in_band = false;
            }
        }
        assert_eq!(
            bands, 3,
            "Menu at 14px should have 3 distinct bars with clear gaps, got {bands}: {center_col:?}"
        );
    }

    #[test]
    fn settings_cog_has_open_center_bore() {
        for size in [14i32, 16, 18] {
            let dim = size as usize;
            let alpha = render_alpha(Icon::Settings, size, 1, false);
            let center = alpha[(dim / 2) * dim + (dim / 2)];
            assert!(
                center <= 32,
                "Settings cog center at {size}px should be open (alpha={center})"
            );
        }
    }

    #[test]
    fn window_controls_have_balanced_optical_size_at_12px() {
        for kind in [Icon::Close, Icon::Minimise, Icon::Maximise] {
            let alpha = render_alpha(kind, 12, 1, false);
            let (x0, y0, x1, y1) = alpha_bbox(&alpha, 12, 64).expect("visible control");
            let width = x1 - x0 + 1;
            let height = y1 - y0 + 1;
            assert!(
                width >= 7,
                "{kind:?} at 12px should span at least 7px wide (got {width}px)"
            );
            if kind != Icon::Minimise {
                assert!(
                    height >= 7,
                    "{kind:?} at 12px should span at least 7px tall (got {height}px)"
                );
            }
        }
    }

    #[test]
    fn security_and_privacy_icons_are_pairwise_distinct() {
        let group = [
            Icon::Lock,
            Icon::Shield,
            Icon::ShieldAlert,
            Icon::Warning,
            Icon::Offline,
            Icon::Refused,
            Icon::Mask,
            Icon::Globe,
            Icon::Check,
        ];
        let rendered: Vec<(Icon, Vec<u8>)> = group
            .iter()
            .map(|&k| (k, render_alpha(k, 16, 2, false)))
            .collect();
        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                let diff: usize = rendered[i]
                    .1
                    .iter()
                    .zip(&rendered[j].1)
                    .map(|(&a, &b)| a.abs_diff(b) as usize)
                    .sum();
                let mean_diff = diff as f64 / rendered[i].1.len() as f64;
                assert!(
                    mean_diff >= 3.5,
                    "{:?} and {:?} are too visually similar (mean alpha diff {mean_diff:.2})",
                    rendered[i].0,
                    rendered[j].0
                );
            }
        }
    }

    #[test]
    fn rtl_mirrors_directional_navigation_icons() {
        let back_rtl = render_alpha(Icon::Back, 16, 2, true);
        let fwd_ltr = render_alpha(Icon::Forward, 16, 2, false);
        let max_diff = back_rtl
            .iter()
            .zip(&fwd_ltr)
            .map(|(&a, &b)| a.abs_diff(b))
            .max()
            .unwrap_or(0);
        assert!(
            max_diff <= 2,
            "Back mirrored in RTL should match Forward in LTR (max diff {max_diff})"
        );
    }

    #[test]
    fn stroke_width_scales_monotonically_with_size() {
        let sizes = [10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 32.0, 48.0];
        for pair in sizes.windows(2) {
            assert!(
                stroke_width(pair[1]) >= stroke_width(pair[0]),
                "stroke width shrank between {} and {}",
                pair[0],
                pair[1]
            );
        }
        assert!((stroke_width(14.0) - 1.473).abs() < 0.05);
        assert!(stroke_width(48.0) >= 2.4);
    }

    fn set_rgb(cr: &cairo::Context, rgb: Rgb, alpha: f64) {
        cr.set_source_rgba(
            rgb.0 as f64 / 255.0,
            rgb.1 as f64 / 255.0,
            rgb.2 as f64 / 255.0,
            alpha,
        );
    }

    /// Generates actual-size 1x/2x light/dark contact sheets and state matrices
    /// when `SEARCH_ICON_SHEET_OUT` is set to an output directory.
    #[test]
    fn render_contact_sheets_when_requested() {
        let Ok(out_dir) = std::env::var("SEARCH_ICON_SHEET_OUT") else {
            return;
        };
        let out = std::path::PathBuf::from(out_dir);
        std::fs::create_dir_all(&out).expect("create sheet output dir");

        let light = Palette::for_look(Look::Light, false);
        let dark = Palette::for_look(Look::Dark, false);

        // 1. Actual-size 1x and 2x contact sheets across 12, 14, 16, 20 logical px on light & dark.
        for scale in [1i32, 2i32] {
            let cols = 8;
            let rows = 4;
            let cell_w = 136.0;
            let cell_h = 64.0;
            let pad = 20.0;
            let header = 34.0;
            let half_h = header + rows as f64 * cell_h + pad;
            let total_w = (pad * 2.0 + cols as f64 * cell_w) as i32;
            let total_h = (half_h * 2.0 + pad) as i32;

            let surface = cairo::ImageSurface::create(
                cairo::Format::ARgb32,
                total_w * scale,
                total_h * scale,
            )
            .expect("sheet surface");
            let cr = cairo::Context::new(&surface).expect("sheet ctx");
            cr.scale(scale as f64, scale as f64);

            for (theme_idx, (label, pal)) in [("Light", light), ("Dark", dark)].iter().enumerate() {
                let y_base = theme_idx as f64 * (half_h + pad);
                set_rgb(&cr, pal.ground, 1.0);
                cr.rectangle(0.0, y_base, total_w as f64, half_h + pad);
                let _ = cr.fill();

                cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
                cr.set_font_size(12.0);
                set_rgb(&cr, pal.ink, 1.0);
                cr.move_to(pad, y_base + 22.0);
                let _ = cr.show_text(&format!(
                    "Search F43 Icon Family — {label} ({scale}x) — Actual 12 / 14 / 16 / 20 logical px"
                ));

                for (idx, &kind) in ALL.iter().enumerate() {
                    let col = idx % cols;
                    let row = idx / cols;
                    let cx = pad + col as f64 * cell_w;
                    let cy = y_base + header + row as f64 * cell_h;

                    // Subtle card surface cell
                    set_rgb(&cr, pal.surface(), 1.0);
                    rounded_rect(&cr, cx + 2.0, cy + 2.0, cell_w - 6.0, cell_h - 6.0, 6.0);
                    let _ = cr.fill();

                    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
                    cr.set_font_size(9.5);
                    set_rgb(&cr, pal.muted, 1.0);
                    cr.move_to(cx + 8.0, cy + 15.0);
                    let _ = cr.show_text(&format!("{kind:?}"));

                    let mut x_cursor = cx + 10.0;
                    for sz in [12i32, 14, 16, 20] {
                        let y_pos = (cy + 38.0 - sz as f64 / 2.0).floor();
                        let _ = cr.save();
                        cr.translate(x_cursor, y_pos);
                        set_rgb(&cr, pal.ink, 1.0);
                        draw_oriented(&cr, kind, sz as f64, false);
                        let _ = cr.restore();
                        x_cursor += sz as f64 + 12.0;
                    }
                }
            }

            let mut file =
                std::fs::File::create(out.join(format!("contact-sheet-{scale}x.png")))
                    .expect("create png");
            surface.write_to_png(&mut file).expect("write png");
        }

        // 2. State matrix (muted/rest, ink/hover, disabled 35%, accent, danger) + 10/32/48px scale strip.
        {
            let total_w = 1080i32;
            let total_h = 860i32;
            let surface = cairo::ImageSurface::create(
                cairo::Format::ARgb32,
                total_w * 2,
                total_h * 2,
            )
            .expect("state surface");
            let cr = cairo::Context::new(&surface).expect("state ctx");
            cr.scale(2.0, 2.0);

            let sample_icons = [
                Icon::Back,
                Icon::Reload,
                Icon::Menu,
                Icon::Close,
                Icon::Minimise,
                Icon::Maximise,
                Icon::Star,
                Icon::Settings,
                Icon::Globe,
                Icon::Search,
                Icon::Tab,
                Icon::Sidebar,
                Icon::Book,
                Icon::Lock,
                Icon::Shield,
                Icon::ShieldAlert,
                Icon::Warning,
                Icon::Offline,
                Icon::Refused,
                Icon::Mask,
            ];

            for (theme_idx, (label, pal)) in [("Light", light), ("Dark", dark)].iter().enumerate() {
                let y_base = theme_idx as f64 * 430.0;
                set_rgb(&cr, pal.ground, 1.0);
                cr.rectangle(0.0, y_base, total_w as f64, 430.0);
                let _ = cr.fill();

                cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
                cr.set_font_size(12.0);
                set_rgb(&cr, pal.ink, 1.0);
                cr.move_to(20.0, y_base + 24.0);
                let _ = cr.show_text(&format!(
                    "States & Scale Semantics — {label} (2x crispness render)"
                ));

                let states: [(&str, Rgb, f64, Rgb); 5] = [
                    ("Rest (muted)", pal.muted, 1.0, pal.surface()),
                    ("Hover (ink)", pal.ink, 1.0, pal.hover),
                    ("Disabled (35%)", pal.muted, 0.35, pal.surface()),
                    ("Accent (kept/shield)", pal.accent(), 1.0, pal.tint()),
                    ("Danger (alert/shut)", pal.danger, 1.0, pal.surface()),
                ];

                for (row_idx, (sname, fg, alpha, bg)) in states.iter().enumerate() {
                    let ry = y_base + 40.0 + row_idx as f64 * 44.0;
                    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
                    cr.set_font_size(10.5);
                    set_rgb(&cr, pal.muted, 1.0);
                    cr.move_to(20.0, ry + 20.0);
                    let _ = cr.show_text(sname);

                    for (i, &kind) in sample_icons.iter().enumerate() {
                        let bx = 175.0 + i as f64 * 44.0;
                        set_rgb(&cr, *bg, 1.0);
                        rounded_rect(&cr, bx, ry + 2.0, 32.0, 32.0, 6.0);
                        let _ = cr.fill();

                        let _ = cr.save();
                        cr.translate((bx + 9.0).floor(), (ry + 11.0).floor());
                        set_rgb(&cr, *fg, *alpha);
                        draw_oriented(&cr, kind, 14.0, false);
                        let _ = cr.restore();
                    }
                }

                // Scale semantics strip: 10px (tab close), 12px (window), 14px (chrome),
                // 16px (section), 18px (rail), 32px (empty), 48px (error).
                let strip_y = y_base + 275.0;
                cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
                cr.set_font_size(11.0);
                set_rgb(&cr, pal.muted, 1.0);
                cr.move_to(20.0, strip_y);
                let _ = cr.show_text("Call-site scale semantics: 10px / 12px / 14px / 16px / 18px / 32px / 48px");

                let showcases: [(Icon, i32, &str); 11] = [
                    (Icon::Close, 10, "10px tab×"),
                    (Icon::Minimise, 12, "12px min"),
                    (Icon::Maximise, 12, "12px max"),
                    (Icon::Close, 12, "12px close"),
                    (Icon::Menu, 14, "14px menu"),
                    (Icon::Star, 14, "14px star"),
                    (Icon::Book, 14, "14px book"),
                    (Icon::Settings, 18, "18px cog"),
                    (Icon::Star, 32, "32px empty"),
                    (Icon::ShieldAlert, 48, "48px tls"),
                    (Icon::Offline, 48, "48px offline"),
                ];
                let mut sx = 24.0;
                for (kind, sz, caption) in showcases {
                    let box_w = (sz as f64 + 34.0).max(76.0);
                    set_rgb(&cr, pal.surface(), 1.0);
                    rounded_rect(&cr, sx, strip_y + 12.0, box_w, 96.0, 8.0);
                    let _ = cr.fill();

                    let _ = cr.save();
                    cr.translate(
                        (sx + (box_w - sz as f64) / 2.0).floor(),
                        (strip_y + 12.0 + (72.0 - sz as f64) / 2.0).floor(),
                    );
                    set_rgb(&cr, pal.ink, 1.0);
                    draw_oriented(&cr, kind, sz as f64, false);
                    let _ = cr.restore();

                    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
                    cr.set_font_size(9.5);
                    set_rgb(&cr, pal.muted, 1.0);
                    cr.move_to(sx + 8.0, strip_y + 98.0);
                    let _ = cr.show_text(caption);

                    sx += box_w + 10.0;
                }
            }

            let mut file = std::fs::File::create(out.join("contact-sheet-states-and-scales.png"))
                .expect("create states png");
            surface.write_to_png(&mut file).expect("write states png");
        }

        // 3. 4x magnified geometry audit sheet of all 32 icons at 16px (drawn at 64px via 4x scale
        //    on the 16px optical stroke weight so every contour & counter is easy to inspect).
        {
            let cols = 8;
            let rows = 4;
            let cell = 112.0;
            let pad = 20.0;
            let total_w = (pad * 2.0 + cols as f64 * cell) as i32;
            let total_h = (pad * 2.0 + 30.0 + rows as f64 * cell) as i32;
            let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, total_w, total_h)
                .expect("audit surface");
            let cr = cairo::Context::new(&surface).expect("audit ctx");
            set_rgb(&cr, dark.ground, 1.0);
            cr.rectangle(0.0, 0.0, total_w as f64, total_h as f64);
            let _ = cr.fill();

            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(13.0);
            set_rgb(&cr, dark.ink, 1.0);
            cr.move_to(pad, 26.0);
            let _ = cr.show_text("Search F43 — All 32 Icons Geometry Audit (16px optical spec @ 4x)");

            for (idx, &kind) in ALL.iter().enumerate() {
                let col = idx % cols;
                let row = idx / cols;
                let cx = pad + col as f64 * cell;
                let cy = 38.0 + row as f64 * cell;

                set_rgb(&cr, dark.surface(), 1.0);
                rounded_rect(&cr, cx + 4.0, cy + 4.0, cell - 8.0, cell - 8.0, 8.0);
                let _ = cr.fill();

                // Faint 16px bounding box guide
                set_rgb(&cr, dark.hairline, 0.35);
                cr.set_line_width(1.0);
                cr.rectangle(cx + 24.0, cy + 16.0, 64.0, 64.0);
                let _ = cr.stroke();

                let _ = cr.save();
                cr.translate(cx + 24.0, cy + 16.0);
                cr.scale(4.0, 4.0);
                set_rgb(&cr, dark.ink, 1.0);
                draw_oriented(&cr, kind, 16.0, false);
                let _ = cr.restore();

                cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
                cr.set_font_size(10.5);
                set_rgb(&cr, dark.muted, 1.0);
                cr.move_to(cx + 12.0, cy + cell - 12.0);
                let _ = cr.show_text(&format!("{kind:?}"));
            }

            let mut file =
                std::fs::File::create(out.join("geometry-audit-4x.png")).expect("create audit png");
            surface.write_to_png(&mut file).expect("write audit png");
        }
    }
}

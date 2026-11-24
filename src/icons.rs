// The small marks the browser draws for itself, in the spirit of the Mac
// build's SF Symbols: one stroke weight, round caps, drawn at demand so they
// stay sharp at any scale.
use crate::palette::Rgb;
use gtk::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Back,
    Forward,
    Reload,
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
    Orbit,
    /// A private tab: the mask, not a hat. Two eye holes in a rounded band.
    Mask,
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
        let x = (widget.allocated_width() - size).max(0) as f64 / 2.0;
        let y = (widget.allocated_height() - size).max(0) as f64 / 2.0;
        cr.translate(x, y);
        set_source_color(widget, cr, color);
        cr.set_line_width(1.5);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        draw(cr, kind, w);
        crate::gtkx::inh(false)
    });
    area
}

fn draw(cr: &cairo::Context, kind: Icon, w: f64) {
    let s = w;
    let c = s / 2.0;
    match kind {
        Icon::Back => {
            cr.move_to(c + s * 0.15, s * 0.22);
            cr.line_to(c - s * 0.13, c);
            cr.line_to(c + s * 0.15, s * 0.78);
            let _ = cr.stroke();
        }
        Icon::Forward => {
            cr.move_to(c - s * 0.15, s * 0.22);
            cr.line_to(c + s * 0.13, c);
            cr.line_to(c - s * 0.15, s * 0.78);
            let _ = cr.stroke();
        }
        Icon::Reload => {
            let r = s * 0.31;
            cr.arc(c, c, r, -0.35 * std::f64::consts::PI, 1.15 * std::f64::consts::PI);
            let _ = cr.stroke();
            let a = -0.35 * std::f64::consts::PI;
            let (x, y) = (c + r * a.cos(), c + r * a.sin());
            cr.move_to(x - s * 0.06, y - s * 0.11);
            cr.line_to(x, y);
            cr.line_to(x + s * 0.10, y - s * 0.06);
            let _ = cr.stroke();
        }
        Icon::Plus => {
            cr.move_to(c, s * 0.24);
            cr.line_to(c, s * 0.76);
            cr.move_to(s * 0.24, c);
            cr.line_to(s * 0.76, c);
            let _ = cr.stroke();
        }
        Icon::Close => {
            cr.move_to(s * 0.32, s * 0.32);
            cr.line_to(s * 0.68, s * 0.68);
            cr.move_to(s * 0.68, s * 0.32);
            cr.line_to(s * 0.32, s * 0.68);
            let _ = cr.stroke();
        }
        Icon::Menu => {
            for (i, y) in [0.34, 0.5, 0.66].iter().enumerate() {
                let inset = if i == 0 { 0.02 } else { 0.0 };
                cr.move_to(s * (0.24 + inset), s * y);
                cr.line_to(s * (0.76 - inset), s * y);
            }
            let _ = cr.stroke();
        }
        Icon::Clock => {
            cr.arc(c, c, s * 0.32, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            cr.move_to(c, c - s * 0.16);
            cr.line_to(c, c);
            cr.line_to(c + s * 0.14, c + s * 0.06);
            let _ = cr.stroke();
        }
        Icon::Star => {
            let fifth = std::f64::consts::PI / 5.0;
            for i in 0..5 {
                let angle = -std::f64::consts::FRAC_PI_2 + (i as f64) * 4.0 * fifth;
                let inner = angle + fifth;
                let outer = (c + s * 0.34 * angle.cos(), c + s * 0.34 * angle.sin());
                let notch = (c + s * 0.14 * inner.cos(), c + s * 0.14 * inner.sin());
                if i == 0 {
                    cr.move_to(outer.0, outer.1);
                } else {
                    cr.line_to(outer.0, outer.1);
                }
                cr.line_to(notch.0, notch.1);
            }
            cr.close_path();
            let _ = cr.stroke();
        }
        Icon::Tab => {
            let w = s * 0.30;
            let h = s * 0.24;
            cr.rectangle(c - w, c - h, w * 2.0, h * 2.0);
            let _ = cr.stroke();
            cr.move_to(c + w * 0.1, c - h);
            cr.line_to(c + w * 0.35, c - h - s * 0.10);
            cr.line_to(c + w * 1.1, c - h - s * 0.10);
            let _ = cr.stroke();
        }
        Icon::Mask => {
            // A band across the eyes with two holes in it. Read at 14px it is
            // a domino mask; read at 40px it is still one.
            let half_w = s * 0.34;
            let half_h = s * 0.17;
            cr.move_to(c - half_w, c - half_h * 0.2);
            cr.curve_to(c - half_w, c - half_h, c - half_w * 0.4, c - half_h, c, c - half_h * 0.7);
            cr.curve_to(c + half_w * 0.4, c - half_h, c + half_w, c - half_h, c + half_w, c - half_h * 0.2);
            cr.curve_to(c + half_w, c + half_h, c + half_w * 0.3, c + half_h, c, c + half_h * 0.55);
            cr.curve_to(c - half_w * 0.3, c + half_h, c - half_w, c + half_h, c - half_w, c - half_h * 0.2);
            cr.close_path();
            let _ = cr.stroke();
            for side in [-1.0_f64, 1.0_f64] {
                cr.arc(c + side * s * 0.16, c - s * 0.01, s * 0.055, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
        }
        Icon::Up => {
            cr.move_to(c - s * 0.17, c + s * 0.08);
            cr.line_to(c, c - s * 0.11);
            cr.line_to(c + s * 0.17, c + s * 0.08);
            let _ = cr.stroke();
        }
        Icon::Down => {
            cr.move_to(c - s * 0.17, c - s * 0.08);
            cr.line_to(c, c + s * 0.11);
            cr.line_to(c + s * 0.17, c - s * 0.08);
            let _ = cr.stroke();
        }
        Icon::Minimise => {
            cr.move_to(c - s * 0.19, c);
            cr.line_to(c + s * 0.19, c);
            let _ = cr.stroke();
        }
        Icon::Maximise => {
            let r = s * 0.17;
            cr.rectangle(c - r, c - r, r * 2.0, r * 2.0);
            let _ = cr.stroke();
        }
        Icon::Search => {
            cr.arc(c, c - s * 0.04, s * 0.26, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            cr.move_to(c + s * 0.15, c + s * 0.15);
            cr.line_to(c + s * 0.28, c + s * 0.28);
            let _ = cr.stroke();
        }
        Icon::Globe => {
            cr.arc(c, c, s * 0.28, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            cr.move_to(c, c - s * 0.28);
            cr.curve_to(c + s * 0.2, c - s * 0.05, c + s * 0.2, c + s * 0.05, c, c + s * 0.28);
            let _ = cr.stroke();
            cr.move_to(c, c - s * 0.28);
            cr.curve_to(c - s * 0.2, c - s * 0.05, c - s * 0.2, c + s * 0.05, c, c + s * 0.28);
            let _ = cr.stroke();
            cr.move_to(c - s * 0.28, c);
            cr.line_to(c + s * 0.28, c);
            let _ = cr.stroke();
        }
        Icon::Shield => {
            cr.move_to(c, s * 0.14);
            cr.line_to(s * 0.79, s * 0.26);
            cr.line_to(s * 0.75, s * 0.57);
            cr.curve_to(s * 0.71, s * 0.72, s * 0.60, s * 0.81, c, s * 0.87);
            cr.curve_to(s * 0.40, s * 0.81, s * 0.29, s * 0.72, s * 0.25, s * 0.57);
            cr.line_to(s * 0.21, s * 0.26);
            cr.close_path();
            let _ = cr.stroke();
            cr.move_to(s * 0.37, s * 0.49);
            cr.line_to(s * 0.47, s * 0.59);
            cr.line_to(s * 0.64, s * 0.40);
            let _ = cr.stroke();
        }
        Icon::ArrowRight => {
            cr.move_to(s * 0.20, c);
            cr.line_to(s * 0.79, c);
            cr.move_to(s * 0.54, s * 0.26);
            cr.line_to(s * 0.79, c);
            cr.line_to(s * 0.54, s * 0.74);
            let _ = cr.stroke();
        }
        Icon::Settings => {
            cr.arc(c, c, s * 0.14, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
            cr.arc(c, c, s * 0.28, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
            for tooth in 0..8 {
                let angle = tooth as f64 * std::f64::consts::FRAC_PI_4;
                cr.move_to(c + s * 0.28 * angle.cos(), c + s * 0.28 * angle.sin());
                cr.line_to(c + s * 0.37 * angle.cos(), c + s * 0.37 * angle.sin());
            }
            let _ = cr.stroke();
        }
        Icon::Download => {
            cr.move_to(c, s * 0.16);
            cr.line_to(c, s * 0.61);
            cr.move_to(s * 0.31, s * 0.44);
            cr.line_to(c, s * 0.63);
            cr.line_to(s * 0.69, s * 0.44);
            cr.move_to(s * 0.22, s * 0.62);
            cr.line_to(s * 0.22, s * 0.80);
            cr.line_to(s * 0.78, s * 0.80);
            cr.line_to(s * 0.78, s * 0.62);
            let _ = cr.stroke();
        }
        Icon::Sidebar => {
            cr.rectangle(s * 0.16, s * 0.22, s * 0.68, s * 0.56);
            cr.move_to(s * 0.40, s * 0.22);
            cr.line_to(s * 0.40, s * 0.78);
            let _ = cr.stroke();
        }
        Icon::Command => {
            cr.rectangle(s * 0.16, s * 0.22, s * 0.68, s * 0.56);
            cr.move_to(s * 0.29, s * 0.40);
            cr.line_to(s * 0.41, c);
            cr.line_to(s * 0.29, s * 0.60);
            cr.move_to(s * 0.52, s * 0.62);
            cr.line_to(s * 0.70, s * 0.62);
            let _ = cr.stroke();
        }
        Icon::Spark => {
            cr.move_to(c, s * 0.10);
            cr.curve_to(s * 0.57, s * 0.40, s * 0.60, s * 0.43, s * 0.90, c);
            cr.curve_to(s * 0.60, s * 0.57, s * 0.57, s * 0.60, c, s * 0.90);
            cr.curve_to(s * 0.43, s * 0.60, s * 0.40, s * 0.57, s * 0.10, c);
            cr.curve_to(s * 0.40, s * 0.43, s * 0.43, s * 0.40, c, s * 0.10);
            let _ = cr.fill();
        }
        Icon::Lock => {
            cr.arc(c, s * 0.40, s * 0.17, std::f64::consts::PI, std::f64::consts::TAU);
            cr.line_to(s * 0.67, s * 0.51);
            cr.move_to(s * 0.33, s * 0.40);
            cr.line_to(s * 0.33, s * 0.51);
            cr.rectangle(s * 0.25, s * 0.49, s * 0.50, s * 0.34);
            let _ = cr.stroke();
            cr.arc(c, s * 0.65, s * 0.03, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
        Icon::Check => {
            cr.move_to(s * 0.23, c);
            cr.line_to(s * 0.42, s * 0.68);
            cr.line_to(s * 0.79, s * 0.28);
            let _ = cr.stroke();
        }
        Icon::Book => {
            cr.move_to(c, s * 0.28);
            cr.curve_to(s * 0.36, s * 0.19, s * 0.23, s * 0.20, s * 0.15, s * 0.24);
            cr.line_to(s * 0.15, s * 0.74);
            cr.curve_to(s * 0.27, s * 0.70, s * 0.39, s * 0.72, c, s * 0.80);
            cr.curve_to(s * 0.61, s * 0.72, s * 0.73, s * 0.70, s * 0.85, s * 0.74);
            cr.line_to(s * 0.85, s * 0.24);
            cr.curve_to(s * 0.77, s * 0.20, s * 0.64, s * 0.19, c, s * 0.28);
            cr.line_to(c, s * 0.80);
            let _ = cr.stroke();
        }
        Icon::Stop => {
            cr.rectangle(s * 0.28, s * 0.28, s * 0.44, s * 0.44);
            let _ = cr.stroke();
        }
        Icon::Orbit => {
            cr.arc(c, c, s * 0.24, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            let _ = cr.save();
            cr.translate(c, c);
            cr.rotate(-0.55);
            cr.scale(1.0, 0.42);
            cr.arc(0.0, 0.0, s * 0.41, 0.0, std::f64::consts::TAU);
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

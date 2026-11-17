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
    Search,
    Book,
    Clock,
    Download,
    Gear,
    Reader,
    Globe,
    Shield,
    Eye,
    Split,
    Pin,
}

pub fn area(kind: Icon, size: i32, color: Rgb) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.connect_draw(move |_, cr| {
        let w = size as f64;
        cr.set_source_rgb(color.0 as f64 / 255.0, color.1 as f64 / 255.0, color.2 as f64 / 255.0);
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
        Icon::Search => {
            cr.arc(c - s * 0.04, c - s * 0.04, s * 0.22, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            cr.move_to(c + s * 0.12, c + s * 0.12);
            cr.line_to(s * 0.74, s * 0.74);
            let _ = cr.stroke();
        }
        Icon::Book => {
            cr.move_to(s * 0.26, s * 0.22);
            cr.line_to(s * 0.62, s * 0.22);
            cr.line_to(s * 0.74, s * 0.32);
            cr.line_to(s * 0.74, s * 0.78);
            cr.line_to(s * 0.26, s * 0.78);
            cr.close_path();
            let _ = cr.stroke();
            cr.move_to(s * 0.62, s * 0.22);
            cr.line_to(s * 0.62, s * 0.33);
            cr.line_to(s * 0.73, s * 0.33);
            let _ = cr.stroke();
        }
        Icon::Clock => {
            cr.arc(c, c, s * 0.28, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            cr.move_to(c, c - s * 0.14);
            cr.line_to(c, c + s * 0.02);
            cr.line_to(c + s * 0.13, c + s * 0.09);
            let _ = cr.stroke();
        }
        Icon::Download => {
            cr.move_to(c, s * 0.24);
            cr.line_to(c, s * 0.6);
            let _ = cr.stroke();
            cr.move_to(c - s * 0.14, s * 0.46);
            cr.line_to(c, s * 0.61);
            cr.line_to(c + s * 0.14, s * 0.46);
            let _ = cr.stroke();
            cr.move_to(s * 0.26, s * 0.76);
            cr.line_to(s * 0.74, s * 0.76);
            let _ = cr.stroke();
        }
        Icon::Gear => {
            cr.arc(c, c, s * 0.13, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            for i in 0..8 {
                let a = i as f64 * std::f64::consts::PI / 4.0;
                cr.move_to(c + s * 0.22 * a.cos(), c + s * 0.22 * a.sin());
                cr.line_to(c + s * 0.31 * a.cos(), c + s * 0.31 * a.sin());
            }
            let _ = cr.stroke();
        }
        Icon::Reader => {
            for (i, y) in [0.3, 0.44, 0.58, 0.72].iter().enumerate() {
                let right = if i == 3 { 0.58 } else { 0.74 };
                cr.move_to(s * 0.26, s * y);
                cr.line_to(s * right, s * y);
            }
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
            cr.move_to(c, s * 0.22);
            cr.line_to(s * 0.72, s * 0.33);
            cr.curve_to(s * 0.72, s * 0.62, s * 0.6, s * 0.72, c, s * 0.79);
            cr.curve_to(s * 0.4, s * 0.72, s * 0.28, s * 0.62, s * 0.28, s * 0.33);
            cr.close_path();
            let _ = cr.stroke();
        }
        Icon::Eye => {
            cr.move_to(s * 0.24, c);
            cr.curve_to(s * 0.38, s * 0.34, s * 0.62, s * 0.34, s * 0.76, c);
            cr.curve_to(s * 0.62, s * 0.66, s * 0.38, s * 0.66, s * 0.24, c);
            let _ = cr.stroke();
            cr.arc(c, c, s * 0.08, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
        }
        Icon::Split => {
            cr.rectangle(s * 0.24, s * 0.28, s * 0.52, s * 0.44);
            let _ = cr.stroke();
            cr.move_to(c, s * 0.28);
            cr.line_to(c, s * 0.72);
            let _ = cr.stroke();
        }
        Icon::Pin => {
            cr.arc(c, c, s * 0.1, 0.0, 2.0 * std::f64::consts::PI);
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
    button.style_context().add_class("mark");
    button.set_can_focus(false);
    button
}

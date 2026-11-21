// VSync-synced GTK3 frame-clock animations for smooth browser transitions.
//
// Animations advance via `WidgetExt::add_tick_callback` using `gdk::FrameClock`
// timestamps so transitions run at the display's native refresh rate. Under
// automated tests (`SEARCH_BENCH`) or when desktop animations are disabled
// (`gtk-enable-animations == false`), transitions finish synchronously in a
// single step with no frame delay.

use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(0) };
    static ACTIVE_TOKENS: RefCell<HashMap<usize, u64>> = RefCell::new(HashMap::new());
}

/// Returns `true` when UI animations should run over time, or `false` when
/// benchmarking (`SEARCH_BENCH`) or when GTK animations are disabled.
pub fn enabled() -> bool {
    if std::env::var_os("SEARCH_BENCH").is_some() {
        return false;
    }
    if gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations()) {
        return false;
    }
    true
}

/// Cubic ease-out curve for `t` clamped to `[0.0, 1.0]`.
pub fn ease_out_cubic(t: f64) -> f64 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Quintic ease-out curve for `t` clamped to `[0.0, 1.0]`.
pub fn ease_out_quint(t: f64) -> f64 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u.powi(5)
}

/// Cubic ease-in curve for `t` clamped to `[0.0, 1.0]`.
pub fn ease_in_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// Linear interpolation between `a` and `b` at factor `t`.
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn widget_key(widget: &impl IsA<gtk::Widget>) -> usize {
    use glib::translate::ToGlibPtr;
    let ptr: *mut gtk::ffi::GtkWidget = widget.upcast_ref::<gtk::Widget>().to_glib_none().0;
    ptr as usize
}

fn next_token(key: usize) -> u64 {
    let token = NEXT_TOKEN.with(|counter| {
        let next = counter.get().wrapping_add(1).max(1);
        counter.set(next);
        next
    });
    ACTIVE_TOKENS.with(|tokens| {
        tokens.borrow_mut().insert(key, token);
    });
    token
}

fn is_current(key: usize, token: u64) -> bool {
    ACTIVE_TOKENS.with(|tokens| tokens.borrow().get(&key).copied() == Some(token))
}

fn clear_if_current(key: usize, token: u64) {
    ACTIVE_TOKENS.with(|tokens| {
        let mut map = tokens.borrow_mut();
        if map.get(&key).copied() == Some(token) {
            map.remove(&key);
        }
    });
}

/// Cancels any active animation on `widget` so its tick callback stops on the
/// next frame without running its completion callback.
pub fn cancel(widget: &impl IsA<gtk::Widget>) {
    let key = widget_key(widget);
    let _ = next_token(key);
}

/// Runs a VSync-synced animation on `widget` over `duration_ms` milliseconds.
///
/// Any prior animation on `widget` is cancelled. `step(t)` receives normalized
/// progress in `[0.0, 1.0]`, and `on_done()` runs once when `t` reaches `1.0`.
pub fn animate(
    widget: &impl IsA<gtk::Widget>,
    duration_ms: u64,
    mut step: impl FnMut(f64) + 'static,
    on_done: impl FnOnce() + 'static,
) {
    let key = widget_key(widget);
    let token = next_token(key);

    if !enabled() || duration_ms == 0 {
        clear_if_current(key, token);
        step(1.0);
        on_done();
        return;
    }

    step(0.0);

    let start_us = Cell::new(None::<i64>);
    let step_cell = RefCell::new(step);
    let done_cell = Cell::new(Some(on_done));
    let duration_us = duration_ms as f64 * 1000.0;

    let _ = widget.add_tick_callback(move |w, clock| {
        if !is_current(key, token) {
            return glib::ControlFlow::Break;
        }

        if start_us.get().is_some() && !w.is_visible() {
            clear_if_current(key, token);
            return glib::ControlFlow::Break;
        }

        let now = clock.frame_time();
        let start = match start_us.get() {
            Some(s) => s,
            None => {
                start_us.set(Some(now));
                now
            }
        };

        let elapsed_us = (now - start).max(0) as f64;
        let t = (elapsed_us / duration_us).clamp(0.0, 1.0);

        step_cell.borrow_mut()(t);

        if t >= 1.0 {
            clear_if_current(key, token);
            if let Some(done) = done_cell.take() {
                done();
            }
            return glib::ControlFlow::Break;
        }

        glib::ControlFlow::Continue
    });
}

/// Fades `widget` opacity from `0.0` to `1.0` using an ease-out cubic curve.
pub fn fade_in(widget: &impl IsA<gtk::Widget>, duration_ms: u64) {
    fade_from(widget, 0.0, 1.0, duration_ms);
}

/// Fades `widget` opacity from `from` to `to` using an ease-out cubic curve.
pub fn fade_from(widget: &impl IsA<gtk::Widget>, from: f64, to: f64, duration_ms: u64) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                w.set_opacity(lerp(from, to, ease_out_cubic(t)));
            }
        },
        || {},
    );
}

/// Fades `widget` opacity from its current value to `0.0` with an ease-in
/// cubic curve, then invokes `on_done`.
pub fn fade_out_then(
    widget: &impl IsA<gtk::Widget>,
    duration_ms: u64,
    on_done: impl FnOnce() + 'static,
) {
    let start_opacity = widget.opacity();
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                w.set_opacity(lerp(start_opacity, 0.0, ease_in_cubic(t)));
            }
        },
        on_done,
    );
}

/// Slides `widget` inside `fixed` from `to_y + offset_y` to `to_y` while fading
/// opacity from `0.0` to `1.0` with a quintic ease-out curve.
pub fn slide_in_fixed(
    fixed: &gtk::Fixed,
    widget: &impl IsA<gtk::Widget>,
    x: i32,
    to_y: i32,
    offset_y: i32,
    duration_ms: u64,
) {
    let fixed_weak = fixed.downgrade();
    let widget_weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let from_y = (to_y + offset_y) as f64;
    let target_y = to_y as f64;

    animate(
        widget,
        duration_ms,
        move |t| {
            let Some(w) = widget_weak.upgrade() else { return };
            let eased = ease_out_quint(t);
            let y = lerp(from_y, target_y, eased).round() as i32;
            if let Some(f) = fixed_weak.upgrade() {
                if w.parent().as_ref() == Some(f.upcast_ref::<gtk::Widget>()) {
                    f.move_(&w, x, y);
                }
            }
            w.set_opacity(eased);
        },
        || {},
    );
}

/// Slides `widget` inside `fixed` from `from_y` to `from_y + offset_y` while
/// fading opacity from its current value to `0.0` with a cubic ease-in curve,
/// then invokes `on_done`.
pub fn slide_out_fixed(
    fixed: &gtk::Fixed,
    widget: &impl IsA<gtk::Widget>,
    x: i32,
    from_y: i32,
    offset_y: i32,
    duration_ms: u64,
    on_done: impl FnOnce() + 'static,
) {
    let start_opacity = widget.opacity();
    let fixed_weak = fixed.downgrade();
    let widget_weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let start_y = from_y as f64;
    let target_y = (from_y + offset_y) as f64;

    animate(
        widget,
        duration_ms,
        move |t| {
            let Some(w) = widget_weak.upgrade() else { return };
            let eased = ease_in_cubic(t);
            let y = lerp(start_y, target_y, eased).round() as i32;
            if let Some(f) = fixed_weak.upgrade() {
                if w.parent().as_ref() == Some(f.upcast_ref::<gtk::Widget>()) {
                    f.move_(&w, x, y);
                }
            }
            w.set_opacity(lerp(start_opacity, 0.0, eased));
        },
        on_done,
    );
}

/// Animates a card or overlay popping into place by reducing `margin_top` from
/// `to_top + offset_y` to `to_top` while fading opacity from `0.0` to `1.0`.
pub fn pop_card_in(widget: &impl IsA<gtk::Widget>, to_top: i32, offset_y: i32, duration_ms: u64) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let from_top = (to_top + offset_y) as f64;
    let target_top = to_top as f64;

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                w.set_margin_top(lerp(from_top, target_top, ease_out_quint(t)).round() as i32);
                w.set_opacity(ease_out_cubic(t));
            }
        },
        || {},
    );
}

/// Animates a bottom toast floating into place by increasing `margin_bottom`
/// from `(to_bottom - offset_y).max(0)` to `to_bottom` while fading opacity
/// from `0.0` to `1.0`.
pub fn float_toast_in(
    widget: &impl IsA<gtk::Widget>,
    to_bottom: i32,
    offset_y: i32,
    duration_ms: u64,
) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let from_bottom = (to_bottom - offset_y).max(0) as f64;
    let target_bottom = to_bottom as f64;

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let eased = ease_out_cubic(t);
                w.set_margin_bottom(lerp(from_bottom, target_bottom, eased).round() as i32);
                w.set_opacity(eased);
            }
        },
        || {},
    );
}

/// Smoothly animates a `gtk::Paned` divider position from `from` to `to` using
/// a quintic ease-out curve, then invokes `on_done`.
pub fn slide_paned(
    paned: &gtk::Paned,
    from: i32,
    to: i32,
    duration_ms: u64,
    on_done: impl FnOnce() + 'static,
) {
    let weak = paned.downgrade();
    let start = from as f64;
    let target = to as f64;

    animate(
        paned,
        duration_ms,
        move |t| {
            if let Some(p) = weak.upgrade() {
                p.set_position(lerp(start, target, ease_out_quint(t)).round() as i32);
            }
        },
        on_done,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn ease_out_cubic_bounds_and_monotonicity() {
        assert!(approx_eq(ease_out_cubic(-0.5), 0.0));
        assert!(approx_eq(ease_out_cubic(0.0), 0.0));
        assert!(approx_eq(ease_out_cubic(0.5), 0.875));
        assert!(approx_eq(ease_out_cubic(1.0), 1.0));
        assert!(approx_eq(ease_out_cubic(1.5), 1.0));

        let mut prev = ease_out_cubic(0.0);
        for i in 1..=20 {
            let cur = ease_out_cubic(i as f64 / 20.0);
            assert!(cur >= prev);
            prev = cur;
        }
    }

    #[test]
    fn ease_out_quint_bounds_and_monotonicity() {
        assert!(approx_eq(ease_out_quint(-1.0), 0.0));
        assert!(approx_eq(ease_out_quint(0.0), 0.0));
        assert!(approx_eq(ease_out_quint(0.5), 1.0 - 0.03125));
        assert!(approx_eq(ease_out_quint(1.0), 1.0));
        assert!(approx_eq(ease_out_quint(2.0), 1.0));

        let mut prev = ease_out_quint(0.0);
        for i in 1..=20 {
            let cur = ease_out_quint(i as f64 / 20.0);
            assert!(cur >= prev);
            prev = cur;
        }
    }

    #[test]
    fn ease_in_cubic_bounds_and_monotonicity() {
        assert!(approx_eq(ease_in_cubic(-0.2), 0.0));
        assert!(approx_eq(ease_in_cubic(0.0), 0.0));
        assert!(approx_eq(ease_in_cubic(0.5), 0.125));
        assert!(approx_eq(ease_in_cubic(1.0), 1.0));
        assert!(approx_eq(ease_in_cubic(1.8), 1.0));

        let mut prev = ease_in_cubic(0.0);
        for i in 1..=20 {
            let cur = ease_in_cubic(i as f64 / 20.0);
            assert!(cur >= prev);
            prev = cur;
        }
    }

    #[test]
    fn lerp_values() {
        assert!(approx_eq(lerp(10.0, 20.0, 0.0), 10.0));
        assert!(approx_eq(lerp(10.0, 20.0, 0.5), 15.0));
        assert!(approx_eq(lerp(10.0, 20.0, 1.0), 20.0));
        assert!(approx_eq(lerp(20.0, 10.0, 0.25), 17.5));
        assert!(approx_eq(lerp(-5.0, 5.0, 0.5), 0.0));
    }
}

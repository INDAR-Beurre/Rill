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
    /// The frame callback behind each running animation and the token that
    /// owns it. Cancelling removes the callback at once, so it does not wait
    /// for its next tick to see that it was cancelled.
    static ACTIVE_CALLBACKS: RefCell<HashMap<usize, (u64, gtk::TickCallbackId)>> = RefCell::new(HashMap::new());
}

/// The decision itself, kept pure so reduced motion can be tested without a
/// display or a process-wide environment variable (Gemini R74 review).
fn should_animate(benching: bool, gtk_animations: bool) -> bool {
    !benching && gtk_animations
}

/// Returns `true` when UI animations should run over time, or `false` when
/// benchmarking (`SEARCH_BENCH`) or when GTK animations are disabled.
pub fn enabled() -> bool {
    should_animate(
        std::env::var_os("SEARCH_BENCH").is_some(),
        gtk::Settings::default().map(|s| s.is_gtk_enable_animations()).unwrap_or(true),
    )
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
// ease_exit replaced this curve on every exit. It stays only as the
// baseline the exit-curve test compares against, so release builds skip it.
#[cfg(test)]
pub fn ease_in_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// Quadratic ease-in for exits, for `t` clamped to `[0.0, 1.0]`.
///
/// Cubic ease-in spent its first third barely moving, so a closing layer
/// looked stuck and then vanished. A quadratic curve starts leaving at once
/// and still speeds up toward the end, so an exit reads as "gone" instead of
/// "stalled, then cut".
pub fn ease_exit(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Linear interpolation between `a` and `b` at factor `t`.
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// One shared set of durations and travel distances, so every layer in the
/// browser moves the same way. Each call site picks a role, not a number.
/// Exits are shorter than entrances: something you dismissed should be out
/// of the way before your eye gets back to the page.
pub mod dur {
    /// Small state changes: a label, a chip, a switch of panel content.
    pub const MICRO: u64 = 120;
    /// A layer arriving: field, panel, find bar, card, toast.
    pub const ENTER: u64 = 180;
    /// The same layer leaving.
    pub const EXIT: u64 = 130;
    /// Something that changes the page layout, such as folding the column.
    pub const LAYOUT: u64 = 220;
    /// How far a layer travels while it enters or leaves, in pixels.
    pub const TRAVEL: i32 = 8;
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

/// Drops the registry entries for `key`, but only while `token` still owns
/// them, so a finished or retired animation never clears its successor.
/// Uses `try_with` because a retiring callback can be dropped while the
/// thread's locals are being torn down at exit.
fn clear_if_current(key: usize, token: u64) {
    let _ = ACTIVE_TOKENS.try_with(|tokens| {
        let mut map = tokens.borrow_mut();
        if map.get(&key).copied() == Some(token) {
            map.remove(&key);
        }
    });
    // A callback that clears itself ends by returning Break, so its id is
    // only dropped here, never removed. It is dropped after the borrow ends.
    let finished = ACTIVE_CALLBACKS
        .try_with(|callbacks| {
            let mut map = callbacks.borrow_mut();
            match map.get(&key) {
                Some((owner, _)) if *owner == token => map.remove(&key),
                _ => None,
            }
        })
        .ok()
        .flatten();
    drop(finished);
}

/// Removes the frame callback of whichever animation is running on `key`.
/// The id is taken out of the registry first and removed only after the
/// borrow ends: removing it drops the closure, and the closure's retirement
/// guard touches the registry again.
fn remove_callback(key: usize) {
    let running = ACTIVE_CALLBACKS.with(|callbacks| callbacks.borrow_mut().remove(&key));
    if let Some((_, id)) = running {
        id.remove();
    }
}

/// Owned by each tick closure. When GTK drops the closure, whether because
/// the animation finished, was cancelled, or its widget was destroyed partway
/// through, the guard clears that animation's registry entries. A destroyed
/// widget therefore leaves no stale key behind for a later widget that
/// happens to be allocated at the same address.
struct Retire {
    key: usize,
    token: u64,
}

impl Drop for Retire {
    fn drop(&mut self) {
        clear_if_current(self.key, self.token);
    }
}

/// Cancels any active animation on `widget` straight away. Its completion
/// callback does not run.
///
/// The token is removed so a callback that is already running sees no match
/// and breaks. The frame callback itself is removed at once, so a cancelled
/// animation does not wake the frame clock again.
/// Returns `true` while an active frame-clock animation owns `widget`.
pub fn is_animating(widget: &impl IsA<gtk::Widget>) -> bool {
    let key = widget_key(widget);
    ACTIVE_TOKENS.with(|tokens| tokens.borrow().contains_key(&key))
}

/// Initial anchor advance credited to the very first frame-clock tick (one 60Hz frame).
const FIRST_TICK_STEP_US: i64 = 16_667;

/// Maximum frame-clock advance credited to the first real inter-frame tick
/// (`tick_index == 1`, right after the initial paint). Native 60Hz ticks
/// (~16.7ms) advance at exact wall-clock speed; a single heavy first-paint
/// layout/rasterization stall cannot consume the entire entrance duration in
/// one tick, while every subsequent tick (`tick_index >= 2`) advances at exact
/// wall-clock delta so sustained slow frames never stretch into slow motion.
const FIRST_PAINT_CLAMP_US: i64 = 33_333;

/// Advances accumulated animation time `elapsed_us` by `delta_us` on tick
/// `tick_index` (`0` = first anchor tick, `1` = first inter-frame tick after
/// initial paint, `2+` = steady ticks). Returns `(next_elapsed_us, t)`.
fn advance_elapsed(elapsed_us: f64, delta_us: i64, duration_us: f64, tick_index: u32) -> (f64, f64) {
    if duration_us <= 0.0 {
        return (0.0, 1.0);
    }
    let non_neg = delta_us.max(0);
    let step = if tick_index == 1 {
        non_neg.min(FIRST_PAINT_CLAMP_US) as f64
    } else {
        non_neg as f64
    };
    let next = (elapsed_us + step).min(duration_us);
    (next, (next / duration_us).clamp(0.0, 1.0))
}

fn telemetry_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SEARCH_MOTION_TELEMETRY").is_some())
}

pub fn cancel(widget: &impl IsA<gtk::Widget>) {
    let key = widget_key(widget);
    ACTIVE_TOKENS.with(|tokens| {
        tokens.borrow_mut().remove(&key);
    });
    remove_callback(key);
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
    // Stop the animation this one replaces now, rather than leaving its
    // callback to tick once more just to find that its token has changed.
    remove_callback(key);
    let token = next_token(key);

    if !enabled() || duration_ms == 0 {
        clear_if_current(key, token);
        step(1.0);
        on_done();
        return;
    }

    step(0.0);

    // Anchor the clock on the first tick, one 60Hz frame behind `now`, instead
    // of reading `frame_clock().frame_time()` up front. While the window is
    // idle or building a panel synchronously, `frame_time()` still holds the
    // previous frame's timestamp (often 100ms+ in the past), which made the
    // first tick jump straight to t=1.0 in a single frame. Anchoring at
    // `now - 16_667` on the first tick avoids both a held t=0 frame and a
    // stale-clock jump, so every entrance interpolates smoothly across frames.
    let last_us: Cell<Option<i64>> = Cell::new(None);
    let elapsed_us: Cell<f64> = Cell::new(0.0);
    let tick_count: Cell<u32> = Cell::new(0);
    let step_cell = RefCell::new(step);
    let done_cell = Cell::new(Some(on_done));
    let duration_us = duration_ms as f64 * 1000.0;
    let retire = Retire { key, token };
    let tele_on = telemetry_enabled();
    let started_at = if tele_on { Some(std::time::Instant::now()) } else { None };
    let first_tick_at: Cell<Option<std::time::Instant>> = Cell::new(None);
    let first_clock_us: Cell<Option<i64>> = Cell::new(None);
    let deltas_ms: RefCell<Vec<f64>> = RefCell::new(Vec::new());
    let widget_label = if tele_on {
        let name = widget.widget_name();
        if name.is_empty() {
            widget.type_().name().to_string()
        } else {
            format!("{}:{name}", widget.type_().name())
        }
    } else {
        String::new()
    };

    let id = widget.add_tick_callback(move |w, clock| {
        // Keeps the guard inside the closure, so the guard drops when the closure does.
        let _owned = &retire;
        if !is_current(key, token) {
            return glib::ControlFlow::Break;
        }

        // If animations are turned off while this one is running, jump to the
        // end state on this tick. Without this check it would play its
        // remaining frames first. The end step and the completion run exactly
        // as on a normal finish, so a layer that was closing still gets
        // removed and a layer that was opening still ends fully shown.
        if !enabled() {
            step_cell.borrow_mut()(1.0);
            clear_if_current(key, token);
            if let Some(done) = done_cell.take() {
                done();
            }
            return glib::ControlFlow::Break;
        }

        // Checked only after the first tick: a widget shown in the same pass
        // that starts its animation may not report visible until then.
        let idx = tick_count.get();
        if idx > 0 && !w.is_visible() {
            clear_if_current(key, token);
            return glib::ControlFlow::Break;
        }
        tick_count.set(idx.saturating_add(1));

        let now = clock.frame_time();
        let delta_us = match last_us.replace(Some(now)) {
            Some(prev) => {
                let raw = now - prev;
                if tele_on {
                    deltas_ms.borrow_mut().push((raw as f64) / 1000.0);
                }
                raw
            }
            None => {
                if tele_on {
                    first_tick_at.set(Some(std::time::Instant::now()));
                    first_clock_us.set(Some(now));
                }
                FIRST_TICK_STEP_US
            }
        };
        let (next_elapsed, t) = advance_elapsed(elapsed_us.get(), delta_us, duration_us, idx);
        elapsed_us.set(next_elapsed);

        step_cell.borrow_mut()(t);

        if t >= 1.0 {
            if tele_on {
                use std::io::Write as _;
                let wall_total_ms = started_at
                    .map(|s| s.elapsed().as_secs_f64() * 1000.0)
                    .unwrap_or(0.0);
                let first_latency_ms = match (started_at, first_tick_at.get()) {
                    (Some(s), Some(f)) => f.saturating_duration_since(s).as_secs_f64() * 1000.0,
                    _ => 0.0,
                };
                let interp_wall_ms = first_tick_at
                    .get()
                    .map(|f| f.elapsed().as_secs_f64() * 1000.0)
                    .unwrap_or(0.0);
                let clock_span_ms = first_clock_us
                    .get()
                    .map(|f| ((now - f).max(0) as f64) / 1000.0)
                    .unwrap_or(0.0);
                let (refresh_interval_us, _) = clock.refresh_info(now);
                let refresh_hz = if refresh_interval_us > 0 {
                    1_000_000.0 / (refresh_interval_us as f64)
                } else {
                    0.0
                };
                let deltas_str = deltas_ms
                    .borrow()
                    .iter()
                    .map(|d| format!("{d:.2}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = writeln!(
                    std::io::stderr(),
                    "{{\"motion_telemetry\":true,\"widget\":\"{widget_label}\",\"duration_ms\":{duration_ms},\"ticks\":{},\"wall_total_ms\":{wall_total_ms:.2},\"first_tick_latency_ms\":{first_latency_ms:.2},\"interp_wall_ms\":{interp_wall_ms:.2},\"clock_span_ms\":{clock_span_ms:.2},\"refresh_hz\":{refresh_hz:.2},\"deltas_ms\":[{deltas_str}]}}",
                    tick_count.get()
                );
            }
            clear_if_current(key, token);
            if let Some(done) = done_cell.take() {
                done();
            }
            return glib::ControlFlow::Break;
        }

        glib::ControlFlow::Continue
    });
    if is_current(key, token) {
        ACTIVE_CALLBACKS.with(|callbacks| {
            callbacks.borrow_mut().insert(key, (token, id));
        });
    }
}

/// Fades `widget` opacity from `0.0` to `1.0` using an ease-out cubic curve.
pub fn fade_in(widget: &impl IsA<gtk::Widget>, duration_ms: u64) {
    fade_from(widget, 0.0, 1.0, duration_ms);
}

/// Fades a direct-draw alpha multiplier from `0.0` to `1.0` and redraws the
/// widget without forcing GTK3 to allocate an offscreen opacity surface.
pub fn fade_alpha(
    widget: &impl IsA<gtk::Widget>,
    alpha: &std::rc::Rc<std::cell::Cell<f64>>,
    duration_ms: u64,
) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let alpha = alpha.clone();
    animate(
        widget,
        duration_ms,
        move |t| {
            alpha.set(ease_out_cubic(t));
            if let Some(w) = weak.upgrade() {
                w.queue_draw();
            }
        },
        || {},
    );
}

/// Fades a direct-draw alpha multiplier from its current value to `0.0` using
/// the shared exit curve (`ease_exit`).
pub fn fade_alpha_out(
    widget: &impl IsA<gtk::Widget>,
    alpha: &std::rc::Rc<std::cell::Cell<f64>>,
    duration_ms: u64,
) {
    let start = alpha.get().clamp(0.0, 1.0);
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let alpha = alpha.clone();
    animate(
        widget,
        duration_ms,
        move |t| {
            alpha.set(lerp(start, 0.0, ease_exit(t)));
            if let Some(w) = weak.upgrade() {
                w.queue_draw();
            }
        },
        || {},
    );
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

/// Fades `widget` opacity from its current value to `0.0` with the exit
/// curve (`ease_exit`), then invokes `on_done`.
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
                w.set_opacity(lerp(start_opacity, 0.0, ease_exit(t)));
            }
        },
        on_done,
    );
}

/// Slides `widget` inside `fixed` from `to_y + offset_y` to `to_y` while fading
/// opacity from `0.0` to `1.0`, then runs `on_done`.
pub fn slide_in_fixed_then(
    fixed: &gtk::Fixed,
    widget: &impl IsA<gtk::Widget>,
    x: i32,
    to_y: i32,
    offset_y: i32,
    duration_ms: u64,
    on_done: impl FnOnce() + 'static,
) {
    let fixed_weak = fixed.downgrade();
    let widget_weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let from_y = (to_y + offset_y) as f64;
    let target_y = to_y as f64;
    // GtkFixed::move_ queues a resize of the whole fixed every time it is called,
    // even when the child stays on the same pixel. An 8px quint entrance spends
    // most of its frames inside the last pixel, so those frames only change the
    // opacity. The move is skipped unless the rounded y is new (Gemini R60 perf).
    let last_y = std::rc::Rc::new(std::cell::Cell::new(i32::MIN));

    animate(
        widget,
        duration_ms,
        move |t| {
            let Some(w) = widget_weak.upgrade() else { return };
            let y = lerp(from_y, target_y, ease_out_quint(t)).round() as i32;
            if y != last_y.get() {
                if let Some(f) = fixed_weak.upgrade() {
                    if w.parent().as_ref() == Some(f.upcast_ref::<gtk::Widget>()) {
                        f.move_(&w, x, y);
                        last_y.set(y);
                    }
                }
            }
            w.set_opacity(ease_out_cubic(t));
        },
        on_done,
    );
}

/// Slides `widget` inside `fixed` from `to_y + offset_y` to `to_y` while fading
/// opacity from `0.0` to `1.0`.
pub fn slide_in_fixed(
    fixed: &gtk::Fixed,
    widget: &impl IsA<gtk::Widget>,
    x: i32,
    to_y: i32,
    offset_y: i32,
    duration_ms: u64,
) {
    slide_in_fixed_then(fixed, widget, x, to_y, offset_y, duration_ms, || {});
}

/// Slides `widget` inside `fixed` from `from_y` to `from_y + offset_y` while
/// fading opacity from its current value to `0.0` with the exit curve
/// (`ease_exit`), then invokes `on_done`.
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
    // Same whole-pixel guard as slide_in_fixed: the widget is only moved, and
    // the fixed only relaid out, on a frame where the rounded y changes. The
    // widget already sits at from_y, so that pixel counts as placed.
    let last_y = std::rc::Rc::new(std::cell::Cell::new(from_y));

    animate(
        widget,
        duration_ms,
        move |t| {
            let Some(w) = widget_weak.upgrade() else { return };
            let eased = ease_exit(t);
            let y = lerp(start_y, target_y, eased).round() as i32;
            if y != last_y.get() {
                if let Some(f) = fixed_weak.upgrade() {
                    if w.parent().as_ref() == Some(f.upcast_ref::<gtk::Widget>()) {
                        f.move_(&w, x, y);
                        last_y.set(y);
                    }
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
    // Changing a margin makes the overlay lay itself out again, so the margin
    // is set only on frames where the card reaches a new whole pixel. That is
    // at most |offset_y| layout passes (8 with dur::TRAVEL) for the whole
    // entrance. On every other frame only the opacity changes, which repaints
    // without a layout.
    let last_top = Cell::new(i32::MIN);

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let top = lerp(from_top, target_top, ease_out_quint(t)).round() as i32;
                if top != last_top.replace(top) {
                    w.set_margin_top(top);
                }
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
    // Same rule as pop_card_in: the margin is set (and the layout runs) only
    // when the toast reaches a new whole pixel. The frames in between change
    // only the opacity.
    let last_bottom = Cell::new(i32::MIN);

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let eased = ease_out_cubic(t);
                let bottom = lerp(from_bottom, target_bottom, eased).round() as i32;
                if bottom != last_bottom.replace(bottom) {
                    w.set_margin_bottom(bottom);
                }
                w.set_opacity(eased);
            }
        },
        || {},
    );
}

/// Smoothly animates a `gtk::Paned` divider position from `from` to `to` using
/// a quintic ease-out curve, then invokes `on_done`.
#[allow(dead_code)]
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

    #[test]
    fn reduced_motion_is_honoured_without_a_display() {
        assert!(should_animate(false, true));
        assert!(!should_animate(false, false));
        assert!(!should_animate(true, true));
        assert!(!should_animate(true, false));
    }

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
    fn ease_exit_bounds_and_monotonicity() {
        assert!(approx_eq(ease_exit(-0.3), 0.0));
        assert!(approx_eq(ease_exit(0.0), 0.0));
        assert!(approx_eq(ease_exit(0.5), 0.25));
        assert!(approx_eq(ease_exit(1.0), 1.0));
        assert!(approx_eq(ease_exit(1.4), 1.0));
        // It leaves sooner than the cubic ease-in it replaced.
        assert!(ease_exit(0.3) > ease_in_cubic(0.3));

        let mut prev = ease_exit(0.0);
        for i in 1..=20 {
            let cur = ease_exit(i as f64 / 20.0);
            assert!(cur >= prev);
            prev = cur;
        }
    }

    #[test]
    fn duration_roles_keep_exits_shorter_than_entrances() {
        assert!(dur::EXIT < dur::ENTER);
        assert!(dur::MICRO <= dur::EXIT);
        assert!(dur::ENTER < dur::LAYOUT);
        assert!(dur::TRAVEL > 0);
    }

    // Lifecycle tests for the token registry that animate() and cancel()
    // share. They use plain keys instead of widgets, so they run without a
    // display and are not skipped when animations are off. Each test uses its
    // own key, and the registry is thread-local, so tests running in parallel
    // cannot see each other's entries.

    fn has_token(key: usize) -> bool {
        ACTIVE_TOKENS.with(|tokens| tokens.borrow().contains_key(&key))
    }

    #[test]
    fn a_replaced_animation_never_clears_the_one_that_replaced_it() {
        let key = 0x5ea4_0001;
        let old = next_token(key);
        let new = next_token(key);
        assert_ne!(old, new);
        assert!(!is_current(key, old), "the replaced animation must see it is stale");
        assert!(is_current(key, new));
        // The old callback finishing or being dropped late leaves the new one alone.
        clear_if_current(key, old);
        assert!(is_current(key, new));
        // The new one finishing removes the entry, so nothing is left behind.
        clear_if_current(key, new);
        assert!(!has_token(key));
    }

    #[test]
    fn a_cancelled_animation_is_no_longer_current_and_leaves_no_token() {
        // cancel() removes the key's token. A callback that is already queued
        // then fails is_current and breaks without running on_done.
        let key = 0x5ea4_0002;
        let token = next_token(key);
        ACTIVE_TOKENS.with(|tokens| {
            tokens.borrow_mut().remove(&key);
        });
        assert!(!is_current(key, token));
        assert!(!has_token(key));
        // Its retirement guard dropping afterwards is harmless.
        drop(Retire { key, token });
        assert!(!has_token(key));
    }

    #[test]
    fn a_dropped_retire_guard_clears_only_its_own_entry() {
        let key = 0x5ea4_0003;
        // A widget destroyed mid-animation drops the closure, and with it the
        // guard, which must remove the stale entry for that address.
        let token = next_token(key);
        drop(Retire { key, token });
        assert!(!has_token(key));
        // A guard from an earlier animation must not clear a later one that
        // reused the same key.
        let first = next_token(key);
        let second = next_token(key);
        drop(Retire { key, token: first });
        assert!(is_current(key, second));
        clear_if_current(key, second);
        assert!(!has_token(key));
    }

    #[test]
    fn tokens_skip_zero_when_the_counter_wraps() {
        let key = 0x5ea4_0004;
        let saved = NEXT_TOKEN.with(|counter| counter.replace(u64::MAX));
        let token = next_token(key);
        assert_eq!(token, 1, "a wrapped counter must not hand out token 0");
        assert!(is_current(key, token));
        clear_if_current(key, token);
        NEXT_TOKEN.with(|counter| counter.set(saved));
        assert!(!has_token(key));
    }

    #[test]
    fn lerp_values() {
        assert!(approx_eq(lerp(10.0, 20.0, 0.0), 10.0));
        assert!(approx_eq(lerp(10.0, 20.0, 0.5), 15.0));
        assert!(approx_eq(lerp(10.0, 20.0, 1.0), 20.0));
        assert!(approx_eq(lerp(20.0, 10.0, 0.25), 17.5));
        assert!(approx_eq(lerp(-5.0, 5.0, 0.5), 0.0));
    }

    #[test]
    fn first_paint_stall_does_not_collapse_entrance_into_one_tick() {
        let dur = dur::ENTER as f64 * 1000.0;
        let mut elapsed = 0.0;
        let mut ticks = 0u32;
        // Simulate tick 0 (anchor 16.7ms), tick 1 (180ms first-paint stall),
        // then steady 60Hz ticks. The entrance must still produce multiple
        // smooth intermediate frames instead of jumping to t=1.0.
        let deltas = [
            FIRST_TICK_STEP_US,
            180_000,
            16_667, 16_667, 16_667, 16_667, 16_667, 16_667, 16_667, 16_667, 16_667,
        ];
        for (idx, delta) in deltas.into_iter().enumerate() {
            let (next, t) = advance_elapsed(elapsed, delta, dur, idx as u32);
            elapsed = next;
            ticks += 1;
            if t >= 1.0 {
                break;
            }
        }
        assert!(ticks >= 8, "stalled entrance collapsed into {ticks} ticks");
        assert!(approx_eq(elapsed, dur));
    }

    #[test]
    fn steady_60hz_ticks_finish_on_the_real_duration() {
        let dur = dur::ENTER as f64 * 1000.0;
        let mut elapsed = 0.0;
        let mut ticks = 0u32;
        while elapsed < dur {
            let (next, _) = advance_elapsed(elapsed, 16_667, dur, ticks);
            elapsed = next;
            ticks += 1;
        }
        // 180ms / 16.667ms = 10.8 -> 11 ticks at 60Hz.
        assert_eq!(ticks, 11);
    }

    #[test]
    fn sustained_slow_80ms_intervals_keep_wall_time_bounded() {
        // Sustained 80ms software frames must NOT stretch a 180ms ENTER to
        // 400ms+. Tick 1 clamps once for first-paint protection; ticks 2+
        // advance by full wall-clock delta, keeping total wall time <= 240ms.
        let dur = dur::ENTER as f64 * 1000.0;
        let mut elapsed = 0.0;
        let mut wall_us = 0i64;
        let mut ticks = 0u32;
        loop {
            let delta = if ticks == 0 { FIRST_TICK_STEP_US } else { 80_000 };
            if ticks > 0 {
                wall_us += delta;
            }
            let (next, t) = advance_elapsed(elapsed, delta, dur, ticks);
            elapsed = next;
            ticks += 1;
            if t >= 1.0 || ticks > 20 {
                break;
            }
        }
        assert_eq!(ticks, 4, "expected 4 ticks at sustained 80ms intervals, got {ticks}");
        assert!(
            wall_us <= 240_000,
            "sustained 80ms frames stretched wall time to {}ms (must stay <= 240ms)",
            wall_us / 1000
        );
        assert!(approx_eq(elapsed, dur));
    }

    #[test]
    fn negative_and_zero_deltas_stay_monotonic_and_safe() {
        let dur = dur::ENTER as f64 * 1000.0;
        let (e0, t0) = advance_elapsed(0.0, FIRST_TICK_STEP_US, dur, 0);
        let (e1, t1) = advance_elapsed(e0, -50_000, dur, 1);
        assert!(approx_eq(e1, e0) && approx_eq(t1, t0), "negative delta moved backwards");
        let (e2, t2) = advance_elapsed(e1, 0, dur, 2);
        assert!(approx_eq(e2, e1) && approx_eq(t2, t1), "zero delta changed progress");
        let (e3, t3) = advance_elapsed(e2, -1, dur, 3);
        assert!(approx_eq(e3, e2) && approx_eq(t3, t2));
        // Zero duration finishes immediately regardless of delta.
        let (ez, tz) = advance_elapsed(0.0, -100, 0.0, 0);
        assert!(approx_eq(ez, 0.0) && approx_eq(tz, 1.0));
    }

    #[test]
    fn reversal_replaces_in_flight_animation_and_keeps_registry_clean() {
        let key = 0x5ea4_0005;
        let enter_token = next_token(key);
        assert!(is_current(key, enter_token));
        // Mid-flight reversal starts an exit animation on the same widget key.
        let exit_token = next_token(key);
        assert!(!is_current(key, enter_token), "reversed enter must be stale");
        assert!(is_current(key, exit_token), "exit must own the key");
        // Stale enter cleanup cannot disturb the active exit.
        clear_if_current(key, enter_token);
        assert!(is_current(key, exit_token));
        // Re-opening cancels the exit and hands ownership to a third token.
        let reopen_token = next_token(key);
        assert!(!is_current(key, exit_token));
        assert!(is_current(key, reopen_token));
        drop(Retire { key, token: exit_token });
        assert!(is_current(key, reopen_token));
        clear_if_current(key, reopen_token);
        assert!(!has_token(key));
    }
}

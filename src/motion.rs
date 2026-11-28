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

/// Evaluates a standard CSS cubic-Bézier timing function `(x1, y1, x2, y2)` at
/// normalized progress `t` in `[0.0, 1.0]`. Used so procedural `GdkFrameClock`
/// animations and GTK3 CSS `cubic-bezier(...)` transitions share the exact
/// same mathematical curves.
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let ax = 3.0 * x1 - 3.0 * x2 + 1.0;
    let bx = 3.0 * x2 - 6.0 * x1;
    let cx = 3.0 * x1;
    let sample_x = |u: f64| ((ax * u + bx) * u + cx) * u;
    let sample_dx = |u: f64| (3.0 * ax * u + 2.0 * bx) * u + cx;

    // Solve B_x(u) = t via Newton-Raphson, falling back to bisection when the
    // slope is flat or leaves [0, 1].
    let mut u = t;
    let mut solved = false;
    for _ in 0..8 {
        let err = sample_x(u) - t;
        if err.abs() < 1e-7 {
            solved = true;
            break;
        }
        let dx = sample_dx(u);
        if dx.abs() < 1e-6 {
            break;
        }
        let next = u - err / dx;
        if !(0.0..=1.0).contains(&next) {
            break;
        }
        u = next;
    }
    if !solved {
        let mut lo = 0.0;
        let mut hi = 1.0;
        u = t;
        for _ in 0..24 {
            let x = sample_x(u);
            if (x - t).abs() < 1e-7 {
                break;
            }
            if x < t {
                lo = u;
            } else {
                hi = u;
            }
            u = 0.5 * (lo + hi);
        }
    }

    let ay = 3.0 * y1 - 3.0 * y2 + 1.0;
    let by = 3.0 * y2 - 6.0 * y1;
    let cy = 3.0 * y1;
    ((ay * u + by) * u + cy) * u
}

/// One coherent easing family shared across CSS and frame-clock transitions
/// (Rill 1.3 liquid motion spec).
pub mod curve {
    /// Primary monotonic settle: fast buoyant lift, long silky deceleration.
    pub const LIQUID: (f64, f64, f64, f64) = (0.22, 1.0, 0.36, 1.0);
    /// Subtle ~5% buoyant settle for spatial physical motion (switches, dots,
    /// floating cards/popovers), inspired by the desktop compositor's fluid
    /// open curve without distracting bounce.
    pub const BUOYANT: (f64, f64, f64, f64) = (0.34, 1.0, 0.28, 1.15);
    /// CSS timing function string matching `LIQUID`.
    pub const CSS_LIQUID: &str = "cubic-bezier(0.22, 1, 0.36, 1)";
    /// CSS timing function string for decisive exits.
    pub const CSS_DISMISS: &str = "cubic-bezier(0.32, 0, 0.67, 0)";
}

/// Primary liquid ease-out curve (`curve::LIQUID`), strictly monotonic in `[0.0, 1.0]`.
pub fn ease_liquid(t: f64) -> f64 {
    cubic_bezier(
        curve::LIQUID.0,
        curve::LIQUID.1,
        curve::LIQUID.2,
        curve::LIQUID.3,
        t,
    )
}

/// Subtle buoyant spatial settle (`curve::BUOYANT`). Never apply directly to
/// opacity; use for spatial coordinates, switch knobs, and indicator radii.
pub fn ease_buoyant(t: f64) -> f64 {
    cubic_bezier(
        curve::BUOYANT.0,
        curve::BUOYANT.1,
        curve::BUOYANT.2,
        curve::BUOYANT.3,
        t,
    )
}

/// Cubic ease-out curve for `t` clamped to `[0.0, 1.0]`.
#[cfg(test)]
pub fn ease_out_cubic(t: f64) -> f64 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Quintic ease-out curve for `t` clamped to `[0.0, 1.0]`.
#[cfg(test)]
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
    /// Small state changes: hover/focus/active transitions, switch glides,
    /// close-button fades, space dots, tab crossfades, and progress sweeps.
    pub const MICRO: u64 = 140;
    /// A layer arriving: omnibox, panel, find bar, card, toast, popover, new tab.
    pub const ENTER: u64 = 200;
    /// The same layer leaving.
    pub const EXIT: u64 = 150;
    /// Structural layout shifts: sidebar/top-tabs toggle, welcome step slide.
    pub const LAYOUT: u64 = 240;
    /// Spatial travel for primary floating layers, in pixels.
    pub const TRAVEL: i32 = 10;
    /// Compact spatial travel for inline dropdowns, popovers, and new tabs.
    pub const TRAVEL_SM: i32 = 5;
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

/// Fades `widget` opacity from `0.0` to `1.0` using the shared liquid curve.
pub fn fade_in(widget: &impl IsA<gtk::Widget>, duration_ms: u64) {
    fade_from(widget, 0.0, 1.0, duration_ms);
}

/// Smoothly fades `widget` opacity from its current opacity to `to` using the
/// shared liquid curve. Skips work when already at `to`.
pub fn fade_opacity_to(widget: &impl IsA<gtk::Widget>, to: f64, duration_ms: u64) {
    let to = to.clamp(0.0, 1.0);
    let from = widget.opacity().clamp(0.0, 1.0);
    if (from - to).abs() < 0.005 {
        cancel(widget);
        widget.set_opacity(to);
        return;
    }
    fade_from(widget, from, to, duration_ms);
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
            alpha.set(ease_liquid(t).clamp(0.0, 1.0));
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
            alpha.set(lerp(start, 0.0, ease_exit(t)).clamp(0.0, 1.0));
            if let Some(w) = weak.upgrade() {
                w.queue_draw();
            }
        },
        || {},
    );
}

/// Fades `widget` opacity from `from` to `to` using the shared liquid curve.
pub fn fade_from(widget: &impl IsA<gtk::Widget>, from: f64, to: f64, duration_ms: u64) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                w.set_opacity(lerp(from, to, ease_liquid(t)).clamp(0.0, 1.0));
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
                w.set_opacity(lerp(start_opacity, 0.0, ease_exit(t)).clamp(0.0, 1.0));
            }
        },
        on_done,
    );
}

/// Slides `widget` inside `fixed` from `to_y + offset_y` to `to_y` with subtle
/// buoyant spatial settle while fading opacity from `0.0` to `1.0`, then runs `on_done`.
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
    // even when the child stays on the same pixel. The move is skipped unless
    // the rounded y is new (Gemini R60 perf).
    let last_y = std::rc::Rc::new(std::cell::Cell::new(i32::MIN));

    animate(
        widget,
        duration_ms,
        move |t| {
            let Some(w) = widget_weak.upgrade() else { return };
            let y = lerp(from_y, target_y, ease_buoyant(t)).round() as i32;
            if y != last_y.get() {
                if let Some(f) = fixed_weak.upgrade() {
                    if w.parent().as_ref() == Some(f.upcast_ref::<gtk::Widget>()) {
                        f.move_(&w, x, y);
                        last_y.set(y);
                    }
                }
            }
            w.set_opacity(ease_liquid(t).clamp(0.0, 1.0));
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
            w.set_opacity(lerp(start_opacity, 0.0, eased).clamp(0.0, 1.0));
        },
        on_done,
    );
}

/// Animates a card or overlay popping into place by settling `margin_top` from
/// `to_top + offset_y` to `to_top` with subtle buoyancy while fading opacity
/// from `0.0` to `1.0`.
pub fn pop_card_in(widget: &impl IsA<gtk::Widget>, to_top: i32, offset_y: i32, duration_ms: u64) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let from_top = (to_top + offset_y).max(0) as f64;
    let target_top = to_top.max(0) as f64;
    // Changing a margin makes the overlay lay itself out again, so the margin
    // is set only on frames where the card reaches a new whole pixel.
    let last_top = Cell::new(i32::MIN);

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let top = lerp(from_top, target_top, ease_buoyant(t)).round().max(0.0) as i32;
                if top != last_top.replace(top) {
                    w.set_margin_top(top);
                }
                w.set_opacity(ease_liquid(t).clamp(0.0, 1.0));
            }
        },
        || {},
    );
}

/// Animates a bottom toast floating into place by settling `margin_bottom`
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
    let target_bottom = to_bottom.max(0) as f64;
    let last_bottom = Cell::new(i32::MIN);

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let bottom = lerp(from_bottom, target_bottom, ease_buoyant(t)).round().max(0.0) as i32;
                if bottom != last_bottom.replace(bottom) {
                    w.set_margin_bottom(bottom);
                }
                w.set_opacity(ease_liquid(t).clamp(0.0, 1.0));
            }
        },
        || {},
    );
}

/// Animates a bottom toast drifting down and fading out symmetrically, then
/// runs `on_done`.
pub fn float_toast_out_then(
    widget: &impl IsA<gtk::Widget>,
    offset_y: i32,
    duration_ms: u64,
    on_done: impl FnOnce() + 'static,
) {
    let start_opacity = widget.opacity();
    let start_bottom = widget.margin_bottom().max(0);
    let target_bottom = (start_bottom - offset_y).max(0) as f64;
    let from_bottom = start_bottom as f64;
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let last_bottom = Cell::new(start_bottom);

    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let eased = ease_exit(t);
                let bottom = lerp(from_bottom, target_bottom, eased).round().max(0.0) as i32;
                if bottom != last_bottom.replace(bottom) {
                    w.set_margin_bottom(bottom);
                }
                w.set_opacity(lerp(start_opacity, 0.0, eased).clamp(0.0, 1.0));
            }
        },
        on_done,
    );
}

/// Smoothly animates a `gtk::Paned` divider position from `from` to `to` using
/// the shared liquid curve, then invokes `on_done`.
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
    let last_pos = Cell::new(i32::MIN);

    animate(
        paned,
        duration_ms,
        move |t| {
            if let Some(p) = weak.upgrade() {
                let pos = lerp(start, target, ease_liquid(t)).round() as i32;
                if pos != last_pos.replace(pos) {
                    p.set_position(pos);
                }
            }
        },
        on_done,
    );
}

/// Smoothly animates a Cairo-drawn scalar cell (such as a pill switch knob or
/// space indicator dot) from its current value to `to` and queues a redraw on
/// each frame.
pub fn tween_cell(
    widget: &impl IsA<gtk::Widget>,
    cell: &std::rc::Rc<std::cell::Cell<f64>>,
    to: f64,
    duration_ms: u64,
    buoyant: bool,
) {
    let from = cell.get();
    if (from - to).abs() < 1e-4 {
        cancel(widget);
        cell.set(to);
        widget.queue_draw();
        return;
    }
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let cell = cell.clone();
    animate(
        widget,
        duration_ms,
        move |t| {
            let factor = if buoyant { ease_buoyant(t) } else { ease_liquid(t) };
            cell.set(lerp(from, to, factor));
            if let Some(w) = weak.upgrade() {
                w.queue_draw();
            }
        },
        || {},
    );
}

/// Smoothly scrolls a `gtk::Adjustment` to `target` using the shared liquid
/// curve, driven by `widget`'s frame clock.
pub fn scroll_to(
    widget: &impl IsA<gtk::Widget>,
    adjust: &gtk::Adjustment,
    target: f64,
    duration_ms: u64,
) {
    let lo = adjust.lower();
    let hi = (adjust.upper() - adjust.page_size()).max(lo);
    let dest = target.clamp(lo, hi);
    let from = adjust.value();
    if (from - dest).abs() < 1.0 {
        cancel(widget);
        adjust.set_value(dest);
        return;
    }
    let adj_weak = adjust.downgrade();
    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(adj) = adj_weak.upgrade() {
                let lo = adj.lower();
                let hi = (adj.upper() - adj.page_size()).max(lo);
                adj.set_value(lerp(from, dest, ease_liquid(t)).clamp(lo, hi));
            }
        },
        || {},
    );
}

/// Reveals a `GtkPopover`'s inner content container with a brisk liquid fade
/// and compact directional settle matching `position`.
pub fn reveal_popover(
    popover: &gtk::Popover,
    content: &impl IsA<gtk::Widget>,
    position: gtk::PositionType,
) {
    if !enabled() {
        content.set_opacity(1.0);
        content.set_margin_top(0);
        content.set_margin_bottom(0);
        return;
    }
    let content_weak = content.upcast_ref::<gtk::Widget>().downgrade();
    let from_top = matches!(position, gtk::PositionType::Top);
    let last_px = Cell::new(i32::MIN);
    let start_offset = dur::TRAVEL_SM as f64;
    let cancel_content = content.upcast_ref::<gtk::Widget>().downgrade();
    popover.connect_closed(move |_| {
        if let Some(w) = cancel_content.upgrade() {
            cancel(&w);
        }
    });
    animate(
        content,
        dur::ENTER,
        move |t| {
            if let Some(w) = content_weak.upgrade() {
                let px = lerp(start_offset, 0.0, ease_buoyant(t)).round().max(0.0) as i32;
                if px != last_px.replace(px) {
                    if from_top {
                        w.set_margin_bottom(px);
                    } else {
                        w.set_margin_top(px);
                    }
                }
                w.set_opacity(ease_liquid(t).clamp(0.0, 1.0));
            }
        },
        || {},
    );
}

/// Reveals an inline container (such as start-page suggestions) smoothly.
pub fn reveal_inline(widget: &impl IsA<gtk::Widget>, offset_y: i32, duration_ms: u64) {
    cancel(widget);
    widget.set_no_show_all(false);
    widget.show_all();
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let start = offset_y.max(0) as f64;
    let last = Cell::new(i32::MIN);
    animate(
        widget,
        duration_ms,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let top = lerp(start, 0.0, ease_buoyant(t)).round().max(0.0) as i32;
                if top != last.replace(top) {
                    w.set_margin_top(top);
                }
                w.set_opacity(ease_liquid(t).clamp(0.0, 1.0));
            }
        },
        || {},
    );
}

/// Hides an inline container smoothly, completing synchronously when reduced
/// motion or `SEARCH_BENCH` is active.
pub fn hide_inline(widget: &impl IsA<gtk::Widget>, duration_ms: u64) {
    if !widget.is_visible() {
        widget.set_no_show_all(true);
        return;
    }
    cancel(widget);
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    fade_out_then(widget, duration_ms, move || {
        if let Some(w) = weak.upgrade() {
            w.set_no_show_all(true);
            w.hide();
            w.set_opacity(1.0);
            w.set_margin_top(0);
        }
    });
}

/// Animates a newly mounted top tab chip or sidebar tab row gliding smoothly
/// into place without altering its container hierarchy.
pub fn reveal_tab(widget: &impl IsA<gtk::Widget>, vertical: bool) {
    if !enabled() {
        widget.set_opacity(1.0);
        return;
    }
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    let start = dur::TRAVEL_SM as f64;
    let last = Cell::new(i32::MIN);
    animate(
        widget,
        dur::ENTER,
        move |t| {
            if let Some(w) = weak.upgrade() {
                let px = lerp(start, 0.0, ease_buoyant(t)).round().max(0.0) as i32;
                if px != last.replace(px) {
                    if vertical {
                        w.set_margin_top(px);
                    } else {
                        w.set_margin_start(px);
                    }
                }
                w.set_opacity(ease_liquid(t).clamp(0.0, 1.0));
            }
        },
        || {},
    );
}

/// Spawns a lightweight pass-through departure pill at `source`'s current
/// bounding box inside `overlay` so tab close feels fluid while state and
/// WebView teardown stay synchronous and leak-free.
pub fn spawn_tab_close_ghost(
    overlay: &gtk::Overlay,
    source: &impl IsA<gtk::Widget>,
    bg: crate::palette::Rgb,
    border: crate::palette::Rgb,
) {
    if !enabled() || !source.is_visible() || !source.is_mapped() {
        return;
    }
    let w = source.allocated_width();
    let h = source.allocated_height();
    if w < 12 || h < 12 {
        return;
    }
    let Some((x, y)) = source.translate_coordinates(overlay, 0, 0) else {
        return;
    };
    let ghost = gtk::DrawingArea::new();
    ghost.set_widget_name("tab-close-ghost");
    ghost.set_size_request(w, h);
    ghost.set_halign(gtk::Align::Start);
    ghost.set_valign(gtk::Align::Start);
    ghost.set_margin_start(x.max(0));
    ghost.set_margin_top(y.max(0));
    ghost.set_can_focus(false);
    let progress = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
    let p_draw = progress.clone();
    ghost.connect_draw(move |area, cr| {
        let p = p_draw.get().clamp(0.0, 1.0);
        let alpha = (1.0 - p) * 0.85;
        if alpha <= 0.01 {
            return crate::gtkx::inh(false);
        }
        let aw = f64::from(area.allocated_width().max(1));
        let ah = f64::from(area.allocated_height().max(1));
        let inset_x = p * 5.0;
        let inset_y = p * 2.5;
        let rw = (aw - inset_x * 2.0).max(4.0);
        let rh = (ah - inset_y * 2.0).max(4.0);
        let r = (rh * 0.5).min(rw * 0.5);
        cr.new_sub_path();
        cr.arc(inset_x + rw - r, inset_y + r, r, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
        cr.arc(inset_x + r, inset_y + r, r, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
        cr.close_path();
        cr.set_source_rgba(
            f64::from(bg.0) / 255.0,
            f64::from(bg.1) / 255.0,
            f64::from(bg.2) / 255.0,
            alpha,
        );
        let _ = cr.fill_preserve();
        cr.set_source_rgba(
            f64::from(border.0) / 255.0,
            f64::from(border.1) / 255.0,
            f64::from(border.2) / 255.0,
            alpha * 0.8,
        );
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        crate::gtkx::inh(false)
    });
    overlay.add_overlay(&ghost);
    overlay.set_overlay_pass_through(&ghost, true);
    ghost.show();

    let ghost_weak = ghost.downgrade();
    let ghost_done = ghost.downgrade();
    let overlay_done = overlay.downgrade();
    let base_y = y.max(0) as f64;
    let last_y = Cell::new(y.max(0));
    animate(
        &ghost,
        dur::EXIT,
        move |t| {
            let eased = ease_exit(t);
            progress.set(eased);
            if let Some(g) = ghost_weak.upgrade() {
                let ny = (base_y - eased * 4.0).round().max(0.0) as i32;
                if ny != last_y.replace(ny) {
                    g.set_margin_top(ny);
                }
                g.queue_draw();
            }
        },
        move || {
            if let (Some(ov), Some(g)) = (overlay_done.upgrade(), ghost_done.upgrade()) {
                if g.parent().as_ref() == Some(ov.upcast_ref::<gtk::Widget>()) {
                    ov.remove(&g);
                }
            }
        },
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
        assert!(dur::TRAVEL_SM > 0 && dur::TRAVEL_SM < dur::TRAVEL);
    }

    #[test]
    fn ease_liquid_is_monotonic_and_fast_out() {
        assert!(approx_eq(ease_liquid(-0.5), 0.0));
        assert!(approx_eq(ease_liquid(0.0), 0.0));
        assert!(approx_eq(ease_liquid(1.0), 1.0));
        assert!(approx_eq(ease_liquid(1.5), 1.0));
        assert!(ease_liquid(0.25) > 0.5, "liquid curve should lift briskly");

        let mut prev = ease_liquid(0.0);
        for i in 1..=40 {
            let cur = ease_liquid(i as f64 / 40.0);
            assert!(cur >= prev - 1e-9, "ease_liquid regressed at step {i}: {cur} < {prev}");
            assert!((0.0..=1.0).contains(&cur));
            prev = cur;
        }
    }

    #[test]
    fn ease_buoyant_has_subtle_restrained_settle_without_excess_bounce() {
        assert!(approx_eq(ease_buoyant(-0.2), 0.0));
        assert!(approx_eq(ease_buoyant(0.0), 0.0));
        assert!(approx_eq(ease_buoyant(1.0), 1.0));
        assert!(approx_eq(ease_buoyant(1.5), 1.0));
        let mut peak = 0.0_f64;
        for i in 1..40 {
            let v = ease_buoyant(i as f64 / 40.0);
            peak = peak.max(v);
        }
        assert!(
            (1.01..=1.08).contains(&peak),
            "buoyant settle peak should stay subtle (1.01..=1.08), got {peak:.4}"
        );
    }

    #[test]
    fn css_curve_tokens_are_valid_gtk_cubic_bezier() {
        for token in [curve::CSS_LIQUID, curve::CSS_DISMISS] {
            assert!(token.starts_with("cubic-bezier(") && token.ends_with(')'));
        }
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
        // 200ms / 16.667ms = 12.0 -> 12 ticks at 60Hz.
        assert_eq!(ticks, 12);
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

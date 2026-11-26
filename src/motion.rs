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
    let start_us: Cell<Option<i64>> = Cell::new(None);
    let ticked = Cell::new(false);
    let step_cell = RefCell::new(step);
    let done_cell = Cell::new(Some(on_done));
    let duration_us = duration_ms as f64 * 1000.0;
    let retire = Retire { key, token };

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
        if ticked.get() && !w.is_visible() {
            clear_if_current(key, token);
            return glib::ControlFlow::Break;
        }
        ticked.set(true);

        let now = clock.frame_time();
        let start = match start_us.get() {
            Some(s) => s,
            None => {
                let s = now.saturating_sub(16_667);
                start_us.set(Some(s));
                s
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
            let eased = ease_out_quint(t);
            let y = lerp(from_y, target_y, eased).round() as i32;
            if y != last_y.get() {
                if let Some(f) = fixed_weak.upgrade() {
                    if w.parent().as_ref() == Some(f.upcast_ref::<gtk::Widget>()) {
                        f.move_(&w, x, y);
                        last_y.set(y);
                    }
                }
            }
            w.set_opacity(eased);
        },
        || {},
    );
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
}

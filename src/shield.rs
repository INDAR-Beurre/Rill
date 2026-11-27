// The ad and tracker blocker: a WebKit content blocker, compiled once at
// launch and enforced inside WebKit's own networking, before a request is
// made. There is no script of ours in the page and nothing to pay per frame.
//
// The rules live in assets/shield/ and are copied to the user's data folder on
// first run, so anyone can read or edit what is blocked.
use crate::files;
use glib::translate::ToGlibPtr;
use glib::{ffi as glib_ffi, gobject_ffi};
use std::collections::BTreeMap;
use std::ffi::{c_void, CString};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use webkit2gtk::{ffi, gio};

thread_local! {
    /// What the last install really achieved: how many lists, and how many
    /// rules inside them. A list that fails to parse contributes nothing and
    /// uses to leave no trace anywhere but stderr.
    static SHIELD_STATE: std::cell::RefCell<ShieldState> = const {
        std::cell::RefCell::new(ShieldState { lists: 0, done: 0, compiled: 0 })
    };
    /// How many managers the shield has been installed into this session.
    ///
    /// A new extension generation builds a new manager, and its lists are the
    /// same lists: the second install applies the filters again but does not
    /// count them, or `state()` would report twice the rules the browser has.
    static INSTALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

static FAILURE_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(crate) fn should_warn_failed(lists: usize, done: usize, compiled: usize) -> bool {
    lists > 0 && done >= lists && compiled == 0
}

fn maybe_warn_failed() {
    let warn = SHIELD_STATE.with(|state| {
        let s = state.borrow();
        should_warn_failed(s.lists, s.done, s.compiled)
    });
    if warn
        && !FAILURE_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        glib::idle_add_local_once(|| {
            if let Some(app) = crate::app::current() {
                app.flash_toast(
                    "Ad-blocker could not start (disk may be full). Browsing without blocking this session.",
                );
            }
        });
    }
}

pub struct ShieldState {
    pub lists: usize,
    pub done: usize,
    pub compiled: usize,
}

/// Lists attempted, lists finished, and rules inside the lists that compiled.
///
/// "Finished" is not "accepted": a list WebKit rejected still counts in `done`,
/// because it has been dealt with, and contributes nothing to `compiled`. A
/// finish with no rules behind it is the shape of a blocker that blocks
/// nothing, and it is the only way to see one from outside.
pub fn state() -> (usize, usize, usize) {
    SHIELD_STATE.with(|state| {
        let state = state.borrow();
        (state.lists, state.done, state.compiled)
    })
}

/// The readable copy of the rules, beside the compiled store.
///
/// The name carries the content hash, so a new EasyList pull lands in a new
/// file instead of leaving last month's rules where anyone reading them would
/// think they were current. The compiled store is named from the same bytes:
/// one hash, one answer about which rules are in force.
fn rules_path() -> std::path::PathBuf {
    files::path_in(&files::data_dir(), &format!("shield-rules-{}.json", lists_key()))
}

/// Every rule list the browser installs, embedded at build time.
///
/// These are uBlock Origin's own lists, EasyList and EasyPrivacy, compiled into
/// the one format this engine can enforce. `tools/build_shield.py` writes them.
pub fn rules() -> &'static [&'static str] {
    crate::shield_assets::FILTERS
}

/// The lists as one JSON array of arrays, for the readable copy on disk.
///
/// Concatenating them, which the first version did, produces `][`, and a file
/// that does not parse is not a file anyone can check the blocker from.
fn joined() -> String {
    let mut all = String::with_capacity(rules().iter().map(|json| json.len() + 1).sum());
    all.push('[');
    for (index, json) in rules().iter().enumerate() {
        if index > 0 {
            all.push(',');
        }
        all.push_str(json.trim());
    }
    all.push(']');
    all
}

/// Make sure the readable copy exists, without reading it back at launch.
/// A write that fails is not a launch failure: the embedded lists are what the
/// store compiles, and this file is only a copy for a person to look at.
fn ensure_rules_json() {
    let path = rules_path();
    if path.exists() {
        return;
    }
    let _ = std::fs::write(&path, joined());
}

/// How many rules a list holds.
///
/// A WebKit content-blocker list is a JSON array of rule objects (each with a
/// `trigger` and an `action`). Counting top-level rule objects directly via
/// `serde_json` without building a `Value` tree counts every rule once —
/// including cosmetic rules carrying both `url-filter` and `selector` — and
/// ignores matching text inside regex or selector strings.
fn rule_count(json: &str) -> usize {
    struct RuleObj;
    impl<'de> serde::Deserialize<'de> for RuleObj {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct ObjVisitor;
            impl<'de> serde::de::Visitor<'de> for ObjVisitor {
                type Value = RuleObj;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a content-blocker rule object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Self::Value, A::Error> {
                    while map
                        .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                        .is_some()
                    {}
                    Ok(RuleObj)
                }
            }
            deserializer.deserialize_map(ObjVisitor)
        }
    }
    struct ArrayCount(usize);
    impl<'de> serde::Deserialize<'de> for ArrayCount {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct SeqVisitor;
            impl<'de> serde::de::Visitor<'de> for SeqVisitor {
                type Value = ArrayCount;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a JSON array of rule objects")
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut count = 0usize;
                    while seq.next_element::<RuleObj>()?.is_some() {
                        count += 1;
                    }
                    Ok(ArrayCount(count))
                }
            }
            deserializer.deserialize_seq(SeqVisitor)
        }
    }
    serde_json::from_str::<ArrayCount>(json)
        .map(|ArrayCount(n)| n)
        .unwrap_or(0)
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn hash_from(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// FNV-1a, 64 bit, written out instead of taken from the standard library.
///
/// `DefaultHasher` promises nothing about its output staying the same between
/// Rust releases. A hash that moves on upgrade either throws away every
/// compiled store or, worse, keeps answering a changed list with a filter built
/// from bytes that are gone. This one has published answers for known inputs,
/// and the tests pin them.
fn hash64(bytes: &[u8]) -> u64 {
    hash_from(FNV_OFFSET, bytes)
}

/// The name one list's compiled filter is cached under.
///
/// A compiled filter is only correct for the exact bytes it was compiled from.
/// Reuse the name across a new EasyList pull and the browser enforces last
/// month's rules forever, with nothing anywhere saying so. So the name is the
/// list's own content hash, plus the index so two identical lists cannot
/// collide. Hex and dashes only, because the store keeps this as a file name.
fn list_ident(index: usize, json: &str) -> String {
    format!("search-shield-{index}-{:016x}", hash64(json.as_bytes()))
}

/// One key for the whole embedded set, for the readable disk copy.
fn lists_key() -> String {
    let mut hash = FNV_OFFSET;
    for json in rules() {
        hash = hash_from(hash, json.as_bytes());
    }
    format!("{hash:016x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Load,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotOutcome {
    /// Another phase (fallback compile or in-flight load/compile) still owns
    /// this list's terminal decision, so `done` must not advance yet.
    Pending,
    /// A compiled filter was delivered for this list.
    Accepted,
    /// Every phase attempted for this list finished without a usable filter.
    Failed,
}

struct Slot {
    /// Whether answers about this list belong in `state()`. Only the first
    /// install of the session counts; later managers get the same filters.
    tally: bool,
    /// How many callbacks have landed on this list. A load and a compile can
    /// each answer for the same list, and one arriving must not look like the
    /// other's answer, so a wait counts answers rather than a flag.
    answers: u64,
    /// How many callbacks have been registered for this list. A wait is for
    /// the answer to its own registration, not merely for any answer.
    registered: u64,
    /// Whether a cached store load is currently in flight.
    load_pending: bool,
    /// Whether a compile/save call is currently in flight.
    save_pending: bool,
    /// Whether the fallback compile phase has been entered (so no further
    /// attempt will be started after pending callbacks settle).
    save_attempted: bool,
    /// A filter for it reached the manager.
    accepted: bool,
    /// Counted into `state()`, once, however many callbacks land on it.
    counted: bool,
    /// Rules already added to `compiled`, so a second answer for the same list
    /// can only raise the count to what the list holds, never past it.
    rules_counted: usize,
    /// How many rules the list held, so the browser can report what it has
    /// rather than assuming a compile that returned means rules that work.
    rules: usize,
    /// The name this list is cached under, so a refusal can say which list was
    /// refused rather than which number it happened to be.
    ident: String,
}

impl Slot {
    fn new(tally: bool, rules: usize, ident: String) -> Self {
        Self {
            tally,
            answers: 0,
            registered: 0,
            load_pending: false,
            save_pending: false,
            save_attempted: false,
            accepted: false,
            counted: false,
            rules_counted: 0,
            rules,
            ident,
        }
    }

    fn on_load_start(&mut self) -> u64 {
        self.load_pending = true;
        self.registered += 1;
        self.registered
    }

    fn on_save_start(&mut self) -> Option<u64> {
        self.save_attempted = true;
        if self.accepted {
            return None;
        }
        self.save_pending = true;
        self.registered += 1;
        Some(self.registered)
    }

    fn on_save_sync_fail(&mut self) -> SlotOutcome {
        self.save_attempted = true;
        self.save_pending = false;
        if !self.accepted && !self.load_pending {
            SlotOutcome::Failed
        } else {
            SlotOutcome::Pending
        }
    }

    fn on_callback(&mut self, phase: Phase, ok: bool) -> (bool, SlotOutcome) {
        self.answers += 1;
        match phase {
            Phase::Load => self.load_pending = false,
            Phase::Save => self.save_pending = false,
        }
        if ok {
            let fresh = !self.accepted;
            self.accepted = true;
            return (fresh, SlotOutcome::Accepted);
        }
        if !self.accepted && self.save_attempted && !self.load_pending && !self.save_pending {
            (false, SlotOutcome::Failed)
        } else {
            (false, SlotOutcome::Pending)
        }
    }
}

/// What a pending callback needs: the slot it fills in, and the manager that
/// must still exist when it lands.
type Handoff = (Arc<Mutex<Slot>>, *mut ffi::WebKitUserContentManager);

fn slot_guard(slot: &Mutex<Slot>) -> std::sync::MutexGuard<'_, Slot> {
    // A panic in a callback must not become a panic in the blocker: the data
    // behind the lock is sound, only a count may be one short.
    slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Pin the manager and the slot where the async callback can find them.
///
/// The callback runs long after the install has moved on, and may arrive after
/// the manager's owner has dropped it, so the manager is referenced here and
/// given back by whoever frees the hand-off.
fn hand_off(slot: &Arc<Mutex<Slot>>, manager: &webkit2gtk::UserContentManager) -> Option<*mut c_void> {
    let raw: *mut ffi::WebKitUserContentManager = manager.to_glib_none().0;
    if raw.is_null() {
        return None;
    }
    unsafe {
        gobject_ffi::g_object_ref(raw as *mut gobject_ffi::GObject);
    }
    Some(Box::into_raw(Box::new((slot.clone(), raw))) as *mut c_void)
}

/// Give back what `hand_off` pinned. Only for a callback, or for a path that
/// knows no callback will ever run.
unsafe fn release(data: *mut c_void) {
    let manager = (*data.cast::<Handoff>()).1;
    if !manager.is_null() {
        gobject_ffi::g_object_unref(manager as *mut gobject_ffi::GObject);
    }
    drop(Box::from_raw(data.cast::<Handoff>()));
}

/// Count one list into `state()`, once, even when it is finished twice.
///
/// A list can be touched twice at launch (a load that missed, then a compile)
/// and either can finish after the launch stopped waiting, so the tally cannot
/// be a plain increment at the end of a function. A filter that arrives for a
/// list already counted as finished without rules upgrades the rule count
/// without counting the list again: that is the load-after-timeout case, and
/// reporting zero rules there would be a lie about what the browser blocks.
fn count(slot: &Mutex<Slot>, accepted: bool) {
    let (tally, first, added) = {
        let mut s = slot_guard(slot);
        let tally = s.tally;
        let first = !s.counted;
        s.counted = true;
        // Only ever upwards: a miss answering after a hit must not take the
        // rules back off a list the manager already has.
        let rules = if accepted { s.rules } else { 0 };
        let added = rules.saturating_sub(s.rules_counted);
        s.rules_counted += added;
        (tally, first, added)
    };
    if !tally {
        return;
    }
    SHIELD_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if first {
            state.done += 1;
        }
        state.compiled += added;
    });
    maybe_warn_failed();
}

fn apply_outcome(slot: &Mutex<Slot>, outcome: SlotOutcome) {
    match outcome {
        SlotOutcome::Pending => {}
        SlotOutcome::Accepted => count(slot, true),
        SlotOutcome::Failed => count(slot, false),
    }
}

/// Report a finished callback, hand any filter to the manager, update `state()`
/// only on terminal outcomes, and free the hand-off.
///
/// This is the only place a filter is applied, so a list that is still
/// compiling when the launch budget runs out is applied when it lands rather
/// than dropped: the window appearing and the shield working are separate
/// promises, and this is the second one being kept.
unsafe fn delivered(
    data: *mut c_void,
    filter: *mut ffi::WebKitUserContentFilter,
    phase: Phase,
) {
    let slot = (*data.cast::<Handoff>()).0.clone();
    let manager = (*data.cast::<Handoff>()).1;
    let (fresh, outcome) = slot_guard(&slot).on_callback(phase, !filter.is_null());
    if !filter.is_null() {
        // A load and a compile can both answer for one list. The manager takes
        // its own reference, so this one is given back either way; adding the
        // same filter twice would be the same rules paid for twice in memory.
        if fresh {
            ffi::webkit_user_content_manager_add_filter(manager, filter);
        }
        // WebKitUserContentFilter is a GBoxed type, not a GObject. Freeing it
        // with g_object_unref reads its first word as a class pointer and
        // logs "g_object_unref: assertion 'G_IS_OBJECT (object)' failed" on
        // every compile. Found with G_DEBUG=fatal-criticals and a backtrace
        // that named this line; the boxed free is the one the type asks for.
        ffi::webkit_user_content_filter_unref(filter);
    }
    // A cache miss on `Phase::Load` before fallback compile finishes is
    // `SlotOutcome::Pending`, so `done` never reaches `lists` while compiles
    // are still running in the background.
    apply_outcome(&slot, outcome);
    release(data);
}

unsafe extern "C" fn saved(
    source: *mut gobject_ffi::GObject,
    res: *mut gio::ffi::GAsyncResult,
    data: *mut c_void,
) {
    let mut err: *mut glib_ffi::GError = std::ptr::null_mut();
    let filter = ffi::webkit_user_content_filter_store_save_finish(
        source as *mut ffi::WebKitUserContentFilterStore,
        res,
        &mut err,
    );
    if filter.is_null() && !err.is_null() {
        let (ident, rules) = {
            let s = slot_guard(&(*data.cast::<Handoff>()).0);
            (s.ident.clone(), s.rules)
        };
        let msg = if (*err).message.is_null() {
            "no message".to_string()
        } else {
            std::ffi::CStr::from_ptr((*err).message).to_string_lossy().to_string()
        };
        // A list WebKit cannot parse fails whole, which is a silent loss of
        // every rule in it. Loud, once, at launch, with the name and size of
        // what was lost so the number is not a mystery later.
        //
        // Not eprintln!: it panics when stderr cannot be written, and this
        // runs inside a C callback, where a panic cannot unwind and aborts the
        // whole browser. That is what happened when /tmp filled up: the save
        // failed with "database or disk is full", this line tried to report it
        // to a log file on the same full disk, the write failed, and the
        // browser died at launch with SIGABRT and no message. A report that
        // cannot be written is dropped; the browser starts without the list.
        {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "search: shield list {ident} did not compile ({msg}); its {rules} rules block nothing"
            );
        }
        glib_ffi::g_error_free(err);
    }
    delivered(data, filter, Phase::Save);
}

unsafe extern "C" fn loaded(
    source: *mut gobject_ffi::GObject,
    res: *mut gio::ffi::GAsyncResult,
    data: *mut c_void,
) {
    let mut err: *mut glib_ffi::GError = std::ptr::null_mut();
    let filter = ffi::webkit_user_content_filter_store_load_finish(
        source as *mut ffi::WebKitUserContentFilterStore,
        res,
        &mut err,
    );
    if !err.is_null() {
        // A miss is not an error: this list has simply never been compiled
        // here, and the caller compiles it now.
        glib_ffi::g_error_free(err);
    }
    delivered(data, filter, Phase::Load);
}

/// The whole launch's budget for waiting on the store, in milliseconds.
///
/// This is the only number that decides how long the window waits for the
/// blocker. Everything past it still happens: each list has a callback that
/// applies its filter the moment it lands, so a slow compile costs the first
/// page it opens, never the launch or the session.
const TOTAL_BUDGET_MS: u64 = 400;

/// How much of that one load may take.
///
/// A store on a local disk either answers quickly or is worth moving on from;
/// every millisecond spent here is a millisecond the window is not on screen.
const LOAD_LIMIT_MS: u64 = 250;

/// How long a launch waits for a first compile.
const SAVE_LIMIT_MS: u64 = 400;

/// Wait for the next answer about this list, but never hold the loop for
/// longer than the caller allows. The list is never dropped by a timeout: the
/// callback is registered, it still adds its filter, and the state it carries
/// is counted by it.
///
/// `target` is the answer number this attempt is waiting for, so a store that
/// missed and a compile started after it cannot read the miss as the compile's
/// own answer.
fn wait_for(slot: &Arc<Mutex<Slot>>, target: u64, limit: Duration) {
    let deadline = Instant::now() + limit;
    loop {
        {
            let s = slot_guard(slot);
            // A filter is a final answer for the list, whichever attempt
            // brought it: no reason to sit out the rest of the budget.
            if s.answers >= target || s.accepted || Instant::now() >= deadline {
                return;
            }
        }
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// What is left of the launch budget, never more than this step's own share.
fn budget(stop: Instant, cap: u64) -> Duration {
    stop.saturating_duration_since(Instant::now())
        .min(Duration::from_millis(cap))
}

/// Compile every rule list and hand them to a content manager.
///
/// Each list is a separate filter: WebKit caps a filter, and uBlock Origin's
/// lists together are far past that cap, so they go in as several. A list
/// already compiled from these exact bytes is loaded instead of compiled, so
/// only the first launch on a profile pays for compiling, and a launch never
/// waits longer than one budget in total for the whole set.
///
/// Returns whether any list is installed or on its way in. A list still
/// compiling when the budget runs out is applied by its own callback, so the
/// shield is never left permanently off by a slow machine.
pub fn install(manager: &webkit2gtk::UserContentManager) -> bool {
    let lists = rules();
    if lists.is_empty() {
        return false;
    }
    // Write the rules out once, so what is blocked can be read and edited
    // without the binary's source. `ensure_rules_json` does that on first
    // launch, and never reads the copy back into a launch.
    ensure_rules_json();
    let store_dir = files::data_dir().join("shield-store");
    let _ = std::fs::create_dir_all(&store_dir);
    let Ok(store_path) = CString::new(store_dir.to_string_lossy().to_string()) else {
        return false;
    };

    SHIELD_STATE.with(|state| state.borrow_mut().lists = lists.len());
    // The first manager of the session is the one whose lists are reported. A
    // manager built after an extension change holds the same rules, and saying
    // so twice would read as twice the shield.
    let tally = INSTALLS.with(|installs| {
        let seen = installs.get() + 1;
        installs.set(seen);
        seen == 1
    });
    let stop = Instant::now() + Duration::from_millis(TOTAL_BUDGET_MS);
    let mut installed = 0;
    for (index, json) in lists.iter().enumerate() {
        if json.trim().is_empty() || !json.trim_start().starts_with('[') {
            note!("search: shield list {index} is not a JSON array; skipping it");
            // Attempted and dealt with: a list the build broke must not leave
            // `finished` short of `lists` forever, or the only signal there is
            // would read as a compile that never ended.
            SHIELD_STATE.with(|state| {
                if tally {
                    state.borrow_mut().done += 1;
                }
            });
            maybe_warn_failed();
            continue;
        }
        // One slot for both attempts: a load and a compile can both finish for
        // the same list, and two filters or two rule counts for one list would
        // report the shield as bigger than it is.
        //
        // The store key is this list's own bytes, so a hit can only ever be a
        // filter compiled from exactly these rules.
        let ident = list_ident(index, json);
        let slot = Arc::new(Mutex::new(Slot::new(tally, rule_count(json), ident.clone())));
        let ready = match load_one(&store_path, &ident, &slot, manager, budget(stop, LOAD_LIMIT_MS)) {
            Some(true) => true,
            Some(false) => save_one(&store_path, &ident, json, &slot, manager, budget(stop, SAVE_LIMIT_MS))
                .unwrap_or(true),
            None => {
                // The load is still in flight and the budget is spent. Compile
                // too, and let whichever answers first win: a hit that arrives
                // late is the same bytes, so the store is not lied to.
                let _ = save_one(&store_path, &ident, json, &slot, manager, budget(stop, SAVE_LIMIT_MS));
                true
            }
        };
        if ready {
            installed += 1;
        }
    }
    if installed == 0 {
        // One line, once: a session with no rules anywhere is the state the
        // whole module exists to avoid, and silence is how it went unnoticed
        // the first time. The path names where to look; the list itself has
        // already said why it was refused.
        note!(
            "search: the shield installed no list from {}; nothing is blocked this session",
            store_path.to_string_lossy()
        );
    }
    maybe_warn_failed();
    installed > 0
}

/// Ask a store that may already hold this list.
///
/// `Some(true)` is a hit, `Some(false)` is no filter stored for these bytes
/// (the one case that must compile), `None` means the answer did not arrive
/// inside the budget and the callback still holds it.
fn load_one(
    store_path: &CString,
    ident: &str,
    slot: &Arc<Mutex<Slot>>,
    manager: &webkit2gtk::UserContentManager,
    wait: Duration,
) -> Option<bool> {
    let Ok(ident) = CString::new(ident) else {
        return Some(false);
    };
    let data = hand_off(slot, manager)?;
    let target = unsafe {
        let store = ffi::webkit_user_content_filter_store_new(store_path.as_ptr());
        if store.is_null() {
            // Nothing was registered: the pins go back, and the caller falls
            // through to a compile that will report its own failure.
            release(data);
            return Some(false);
        }
        let target = slot_guard(slot).on_load_start();
        ffi::webkit_user_content_filter_store_load(
            store,
            ident.as_ptr(),
            std::ptr::null_mut(),
            Some(loaded),
            data,
        );
        gobject_ffi::g_object_unref(store as *mut gobject_ffi::GObject);
        target
    };
    wait_for(slot, target, wait);
    let s = slot_guard(slot);
    if s.accepted {
        Some(true)
    } else if s.answers >= target {
        // A load that answered with no filter: never compiled here.
        Some(false)
    } else {
        None
    }
}

/// Compile one list, waiting no longer than the budget allows.
///
/// `Some(true)` compiled and applied, `Some(false)` refused by WebKit, `None`
/// still compiling, in which case the callback applies it.
fn save_one(
    store_path: &CString,
    ident: &str,
    json: &str,
    slot: &Arc<Mutex<Slot>>,
    manager: &webkit2gtk::UserContentManager,
    wait: Duration,
) -> Option<bool> {
    if slot_guard(slot).accepted {
        slot_guard(slot).save_attempted = true;
        return Some(true);
    }
    let Ok(ident) = CString::new(ident) else {
        let outcome = slot_guard(slot).on_save_sync_fail();
        apply_outcome(slot, outcome);
        return Some(false);
    };
    let Some(data) = hand_off(slot, manager) else {
        let outcome = slot_guard(slot).on_save_sync_fail();
        apply_outcome(slot, outcome);
        return Some(false);
    };
    let bytes = glib::Bytes::from(json.as_bytes());
    let target = unsafe {
        let store = ffi::webkit_user_content_filter_store_new(store_path.as_ptr());
        if store.is_null() {
            // Nothing will call back: give the pins back and mark terminal
            // failure if no store load is still in flight.
            release(data);
            let outcome = slot_guard(slot).on_save_sync_fail();
            apply_outcome(slot, outcome);
            return Some(false);
        }
        let Some(target) = slot_guard(slot).on_save_start() else {
            // The cached store load succeeded before compile started.
            release(data);
            gobject_ffi::g_object_unref(store as *mut gobject_ffi::GObject);
            return Some(true);
        };
        ffi::webkit_user_content_filter_store_save(
            store,
            ident.as_ptr(),
            bytes.to_glib_none().0,
            std::ptr::null_mut(),
            Some(saved),
            data,
        );
        gobject_ffi::g_object_unref(store as *mut gobject_ffi::GObject);
        target
    };

    // A list of tens of thousands of rules takes longer than a hand-written
    // one to compile. This happens once per profile, and even then the loop is
    // only held for as long as a launch can afford.
    wait_for(slot, target, wait);
    let s = slot_guard(slot);
    if s.accepted {
        Some(true)
    } else if s.answers >= target {
        Some(false)
    } else {
        None
    }
}

/// Sites where the shield is off, kept beside the hidden elements.
pub fn allow_path() -> std::path::PathBuf {
    files::path_in(&files::data_dir(), "shield-off.json")
}

/// Whether the blocker is off for this host.
pub fn allowed(host: &str) -> bool {
    allow_list().get(host).copied().unwrap_or(false)
}

pub fn allow_list() -> BTreeMap<String, bool> {
    files::read_json(&allow_path()).unwrap_or_default()
}

/// Turn the blocker off for one host, and on again. The curtain menu uses
/// this; the allowlist it writes is read at load time.
pub fn set_allowed(host: &str, off: bool) {
    let mut list = allow_list();
    if off {
        list.insert(host.to_string(), true);
    } else {
        list.remove(host);
    }
    files::write_json(&allow_path(), &list);
}

pub fn is_allowed(host: &str) -> bool {
    allow_list().contains_key(host)
}

/// A tiny host helper: "news.ycombinator.com" from any address.
/// Preserves bracketed IPv6 literals (`[::1]`, `[2001:db8::1]`), strips
/// userinfo and port, and fails safely to an empty string on malformed
/// authorities.
pub fn host_of(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let (has_scheme, rest) = match trimmed.split_once("://") {
        Some((_, after)) => (true, after),
        None => (false, trimmed),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if host_port.is_empty() {
        return String::new();
    }
    if host_port.starts_with('[') {
        let Some(end) = host_port.find(']') else {
            return String::new();
        };
        let inner = &host_port[1..end];
        let after = &host_port[end + 1..];
        let valid_after = after.is_empty()
            || (after.starts_with(':')
                && after.len() > 1
                && after[1..].chars().all(|c| c.is_ascii_digit()));
        if !valid_after
            || inner.is_empty()
            || !inner.contains(':')
            || !inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return String::new();
        }
        return host_port[..=end].to_ascii_lowercase();
    }
    if host_port.contains('[') || host_port.contains(']') {
        return String::new();
    }
    let host = if let Some((host, port)) = host_port.split_once(':') {
        if has_scheme && (port.is_empty() || !port.chars().all(|c| c.is_ascii_digit())) {
            return String::new();
        }
        host
    } else {
        host_port
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return String::new();
    }
    host.to_lowercase()
}

/// The path and query after the host, for telling same-site tabs apart and
/// ranking a page whose address matches what was typed above one that only
/// happens to have the word in its title. The query is case-sensitive in a
/// URL, so unlike a host it is kept as written; callers lower-case for
/// matching.
pub fn location_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let after_host = rest
        .find(['/', '?', '#'])
        .map(|at| &rest[at..])
        .unwrap_or("/");
    after_host.split('#').next().unwrap_or(after_host).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_keeps_the_path_that_distinguishes_same_host_tabs() {
        assert_eq!(location_of("http://127.0.0.1:8099/office"), "/office");
        assert_eq!(location_of("http://127.0.0.1:8099/titled?t=X"), "/titled?t=X");
        assert_eq!(location_of("http://127.0.0.1:8099"), "/");
        assert_eq!(location_of("about:blank"), "/");
    }

    #[test]
    fn every_rule_list_is_a_json_array() {
        assert!(!rules().is_empty(), "the build produces at least one list");
        for (index, json) in rules().iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(json)
                .unwrap_or_else(|error| panic!("list {index} does not parse: {error}"));
            assert!(
                parsed.as_array().map(|rules| !rules.is_empty()).unwrap_or(false),
                "list {index} is an empty array"
            );
        }
    }

    #[test]
    fn the_lists_block_the_ad_networks_ublock_blocks() {
        // A generated list can compile and still be full of nothing useful.
        // These two are the ones any list has to have.
        let all = rules().join("");
        for host in ["doubleclick", "google-analytics", "googlesyndication"] {
            assert!(all.contains(host), "no rule mentions {host}");
        }
    }

    #[test]
    fn the_cache_key_follows_the_bytes() {
        // Same rules, same key, launch after launch and machine after machine.
        assert_eq!(list_ident(0, "[]"), list_ident(0, "[]"));
        // A new EasyList pull changes the bytes. If the key did not follow, the
        // old compiled filter would answer for the new rules forever.
        assert_ne!(list_ident(0, "[]"), list_ident(0, "[ ]"));
        assert_ne!(list_ident(0, "[]"), list_ident(1, "[]"));
        // Pinned algorithm: changing the hash throws away every compiled store
        // on the machine, so it has to be a deliberate act.
        assert_eq!(hash64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(hash64(b"rules"), hash64(b"rules"));
        assert_ne!(hash64(b"rules A"), hash64(b"rules B"));
    }

    #[test]
    fn the_cache_key_is_a_file_name() {
        // The store keeps the identifier as a name on disk, so a key with a
        // slash or a space in it is a key that cannot be written.
        for (index, json) in rules().iter().enumerate() {
            let ident = list_ident(index, json);
            assert!(
                ident
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{ident} is not a name every file system can hold"
            );
            assert!(ident.len() <= 64, "{ident} is longer than a store key needs to be");
        }
        assert!(lists_key().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_rule_is_counted_once_per_rule_object() {
        assert_eq!(rule_count(""), 0);
        assert_eq!(rule_count("{}"), 0);
        assert_eq!(rule_count(r#"["url-filter","url-filter","selector"]"#), 0);
        assert_eq!(
            rule_count(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}}]"#),
            1
        );
        // A cosmetic rule carries both `url-filter` and `selector` inside one
        // rule object and must be counted once, not twice.
        assert_eq!(
            rule_count(
                r#"[{"trigger":{"url-filter":".*"},"action":{"type":"css-display-none","selector":"a"}}]"#
            ),
            1
        );
        // Key-like text inside regex or selector strings must not inflate count.
        assert_eq!(
            rule_count(
                r#"[{"trigger":{"url-filter":"\"url-filter\""}, "action":{"type":"css-display-none","selector":"[data-k=\"selector\"][data-t=\"trigger\"]"}}]"#
            ),
            1
        );
        assert_eq!(
            rule_count(
                r#"[
                    {"trigger":{"url-filter":"a"},"action":{"type":"block"}},
                    {"trigger":{"url-filter":".*"},"action":{"type":"css-display-none","selector":".ad"}},
                    {"trigger":{"url-filter":"b"},"action":{"type":"block"}}
                ]"#
            ),
            3
        );
    }

    #[test]
    fn the_shield_reports_the_exact_rules_its_lists_hold() {
        let mut parsed_total = 0usize;
        let mut counted_total = 0usize;
        for (index, json) in rules().iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(json)
                .unwrap_or_else(|error| panic!("list {index} does not parse: {error}"));
            let actual = parsed.as_array().map(|a| a.len()).unwrap_or(0);
            let counted = rule_count(json);
            assert_eq!(counted, actual, "list {index} count mismatch");
            parsed_total += actual;
            counted_total += counted;
        }
        assert_eq!(counted_total, parsed_total);
        assert_eq!(counted_total, 15_775);
    }

    #[test]
    fn cache_miss_never_marks_list_finished_before_compile_outcome() {
        // Fresh-profile sequence: all 3 lists miss the compiled store, then
        // compile asynchronously. `done` must stay 0 across all 3 cache misses,
        // rise to 2 when 2 lists finish compiling (never finished=3 with
        // partial rules), and reach (3, 3, 15_775) only once the 3rd compiles.
        let counts = [1usize, 1_774, 14_000];
        let slots: Vec<Mutex<Slot>> = counts
            .iter()
            .enumerate()
            .map(|(i, &n)| Mutex::new(Slot::new(true, n, format!("list-{i}"))))
            .collect();

        SHIELD_STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.lists = slots.len();
            st.done = 0;
            st.compiled = 0;
        });

        for slot in &slots {
            assert_eq!(slot_guard(slot).on_load_start(), 1);
            let (fresh, outcome) = slot_guard(slot).on_callback(Phase::Load, false);
            assert!(!fresh);
            assert_eq!(outcome, SlotOutcome::Pending);
            apply_outcome(slot, outcome);
            assert_eq!(slot_guard(slot).on_save_start(), Some(2));
        }
        assert_eq!(state(), (3, 0, 0));
        assert!(!should_warn_failed(3, 0, 0));

        // Lists 0 and 2 finish compiling while list 1 is still compiling.
        for &idx in &[0usize, 2usize] {
            let (fresh, outcome) = slot_guard(&slots[idx]).on_callback(Phase::Save, true);
            assert!(fresh);
            assert_eq!(outcome, SlotOutcome::Accepted);
            apply_outcome(&slots[idx], outcome);
        }
        assert_eq!(state(), (3, 2, 14_001));

        // List 1 finishes compiling: now all 3 are done with the full 15,775 rules.
        let (fresh, outcome) = slot_guard(&slots[1]).on_callback(Phase::Save, true);
        assert!(fresh);
        assert_eq!(outcome, SlotOutcome::Accepted);
        apply_outcome(&slots[1], outcome);
        assert_eq!(state(), (3, 3, 15_775));
    }

    #[test]
    fn overlapping_load_timeout_and_compile_interleavings_finish_terminally_once() {
        // 1. Load times out, save starts, load misses while save is pending -> Pending,
        //    then save fails -> Failed.
        let mut s = Slot::new(true, 100, "a".into());
        assert_eq!(s.on_load_start(), 1);
        assert_eq!(s.on_save_start(), Some(2));
        assert_eq!(s.on_callback(Phase::Load, false), (false, SlotOutcome::Pending));
        assert_eq!(s.on_callback(Phase::Save, false), (false, SlotOutcome::Failed));

        // 2. Load times out, save starts, save fails while load is still pending -> Pending,
        //    then late load succeeds -> Accepted (fresh).
        let mut s = Slot::new(true, 100, "b".into());
        assert_eq!(s.on_load_start(), 1);
        assert_eq!(s.on_save_start(), Some(2));
        assert_eq!(s.on_callback(Phase::Save, false), (false, SlotOutcome::Pending));
        assert_eq!(s.on_callback(Phase::Load, true), (true, SlotOutcome::Accepted));

        // 3. Load times out, save starts, save fails while load is pending -> Pending,
        //    then late load also misses -> Failed.
        let mut s = Slot::new(true, 100, "c".into());
        assert_eq!(s.on_load_start(), 1);
        assert_eq!(s.on_save_start(), Some(2));
        assert_eq!(s.on_callback(Phase::Save, false), (false, SlotOutcome::Pending));
        assert_eq!(s.on_callback(Phase::Load, false), (false, SlotOutcome::Failed));

        // 4. Both load and save succeed -> only the first filter is `fresh`,
        //    and rules/done are counted once.
        let slot = Mutex::new(Slot::new(true, 1_774, "d".into()));
        SHIELD_STATE.with(|st| {
            let mut st = st.borrow_mut();
            st.lists = 1;
            st.done = 0;
            st.compiled = 0;
        });
        assert_eq!(slot_guard(&slot).on_load_start(), 1);
        assert_eq!(slot_guard(&slot).on_save_start(), Some(2));
        let (f1, o1) = slot_guard(&slot).on_callback(Phase::Load, true);
        assert!(f1);
        apply_outcome(&slot, o1);
        let (f2, o2) = slot_guard(&slot).on_callback(Phase::Save, true);
        assert!(!f2);
        apply_outcome(&slot, o2);
        assert_eq!(state(), (1, 1, 1_774));

        // 5. Load times out, save fails synchronously while load is pending -> Pending,
        //    then late load misses -> Failed.
        let mut s = Slot::new(true, 100, "e".into());
        assert_eq!(s.on_load_start(), 1);
        assert_eq!(s.on_save_sync_fail(), SlotOutcome::Pending);
        assert_eq!(s.on_callback(Phase::Load, false), (false, SlotOutcome::Failed));

        // 6. Load misses immediately, then save fails synchronously -> Failed immediately.
        let mut s = Slot::new(true, 100, "f".into());
        assert_eq!(s.on_load_start(), 1);
        assert_eq!(s.on_callback(Phase::Load, false), (false, SlotOutcome::Pending));
        assert_eq!(s.on_save_sync_fail(), SlotOutcome::Failed);
    }

    #[test]
    fn the_readable_copy_is_json() {
        // Built the same way `rules_json` writes it, without touching anyone's
        // data folder: a test that writes to $HOME is a test that fails on a
        // machine where someone has edited their copy by hand.
        let text = joined();
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("the readable rules do not parse: {error}"));
        assert_eq!(parsed.as_array().map(|lists| lists.len()), Some(rules().len()));
    }

    #[test]
    fn a_host_is_a_host() {
        assert_eq!(host_of("https://news.ycombinator.com/item?id=1"), "news.ycombinator.com");
        assert_eq!(host_of("http://127.0.0.1:8099/plain?x=1"), "127.0.0.1");
        assert_eq!(host_of("https://user:pw@example.com:443/x"), "example.com");
        assert_eq!(host_of("example.com/path"), "example.com");
        assert_eq!(host_of("http://[::1]:8080/p"), "[::1]");
        assert_eq!(host_of("https://[2001:DB8::1]/"), "[2001:db8::1]");
        assert_eq!(
            host_of("https://user:pw@[2001:DB8::1]:8443/a?b=1#c"),
            "[2001:db8::1]"
        );
        assert_eq!(
            host_of("http://[::ffff:127.0.0.1]:8080/x"),
            "[::ffff:127.0.0.1]"
        );
        assert_eq!(host_of("[::1]:8080"), "[::1]");
        assert_eq!(host_of("file:///tmp/x.html"), "");
        assert_eq!(host_of(""), "");
        for malformed in [
            "http://[::1/p",
            "http://[]/p",
            "http://[]:8080/p",
            "http://[::1]evil.com/p",
            "http://[::1]:/p",
            "http://[::1]:abc/p",
            "http://[::1]:80:90/p",
            "http://[example.com]/p",
            "http://[::1*]/p",
            "http://::1/p",
            "http://2001:db8::1/p",
            "http://example.com:abc/p",
            "http://example.com:/p",
            "http://foo]bar/p",
            "http://a[b]/p",
            "http://user@:8080/p",
            "http://exa mple.com/p",
        ] {
            assert_eq!(host_of(malformed), "", "malformed authority {malformed:?} must fail safely");
        }
    }

    #[test]
    fn failure_warning_fires_only_when_all_lists_finish_with_zero_compiled() {
        assert!(!should_warn_failed(0, 0, 0));
        assert!(!should_warn_failed(3, 2, 0));
        assert!(!should_warn_failed(3, 3, 1));
        assert!(should_warn_failed(3, 3, 0));
        assert!(should_warn_failed(1, 2, 0));
    }
}

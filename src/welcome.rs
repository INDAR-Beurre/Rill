//! Full-tab first-run introduction and calm setup flow for Rill.
//!
//! Mounted directly inside `app.ui.stack` as child `"welcome"` (zero modal
//! scrim or centered floating dialog). Built directly on Rill's warm paper /
//! charcoal palette with an asymmetric left-aligned editorial column and an
//! original native Cairo flowing river contour motif with sparse 8x8 Bayer
//! ordered-dither texture on the right, live draft preview, Escape/Skip revert,
//! pure-Cairo pill toggle (zero `GtkSwitch`/`glycin` helpers), and zero network
//! traffic until the user explicitly opts into starter shortcuts.

use crate::app::{App, AppRef};
use crate::icons::Icon;
use crate::palette::Rgb;
use crate::store::{Bookmark, Look, Records};
use gtk::gdk::keys::constants::Escape;
use gtk::prelude::*;
use soup::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const STACK_NAME: &str = "welcome";
const CONTENT_MAX_WIDTH: i32 = 540;
const PILL_GAP: i32 = 14;
const STEP_COUNT: usize = 4; // 0 = Welcome intro, 1 = Appearance & layout, 2 = Search, 3 = Privacy & shortcuts
const FAVICON_MAX_BYTES: usize = 512 * 1024;
const FAVICON_TIMEOUT_SECS: u32 = 2;
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const DITHER_CELL: i32 = 2;

#[derive(Clone, Copy, Debug)]
struct StarterSite {
    title: &'static str,
    host: &'static str,
    url: &'static str,
    icon_url: &'static str,
}

const STARTER_SITES: [StarterSite; 4] = [
    StarterSite {
        title: "Wikipedia",
        host: "wikipedia.org",
        url: "https://www.wikipedia.org/",
        icon_url: "https://www.wikipedia.org/static/apple-touch/wikipedia.png",
    },
    StarterSite {
        title: "GitHub",
        host: "github.com",
        url: "https://github.com/",
        icon_url: "https://github.githubassets.com/favicons/favicon.png",
    },
    StarterSite {
        title: "YouTube",
        host: "youtube.com",
        url: "https://www.youtube.com/",
        icon_url: "https://www.youtube.com/s/desktop/c52980d1/img/favicon_32x32.png",
    },
    StarterSite {
        title: "MDN Web Docs",
        host: "developer.mozilla.org",
        url: "https://developer.mozilla.org/",
        icon_url: "https://developer.mozilla.org/favicon-48x48.png",
    },
];

#[derive(Clone, Debug, PartialEq)]
struct PrefsSnapshot {
    appearance: Look,
    sidebar: bool,
    column_folded: bool,
    search_engine: String,
    engine_suggestions: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct WizardDraft {
    step: usize,
    initial: PrefsSnapshot,
    appearance: Look,
    sidebar: bool,
    search_engine: String,
    engine_suggestions: bool,
    touched_appearance: bool,
    touched_sidebar: bool,
    touched_engine: bool,
    touched_suggestions: bool,
    selected_sites: [bool; 4],
}

impl WizardDraft {
    fn from_snapshot(snap: PrefsSnapshot) -> Self {
        Self {
            step: 0,
            appearance: snap.appearance.clone(),
            sidebar: snap.sidebar,
            search_engine: snap.search_engine.clone(),
            engine_suggestions: snap.engine_suggestions,
            initial: snap,
            touched_appearance: false,
            touched_sidebar: false,
            touched_engine: false,
            touched_suggestions: false,
            selected_sites: [false; 4],
        }
    }

    #[cfg(test)]
    fn set_step(&mut self, next: usize) {
        self.step = next.min(STEP_COUNT - 1);
    }

    fn next_step(&mut self) -> bool {
        if self.step + 1 < STEP_COUNT {
            self.step += 1;
            true
        } else {
            false
        }
    }

    fn prev_step(&mut self) -> bool {
        if self.step > 0 {
            self.step -= 1;
            true
        } else {
            false
        }
    }
}

struct CanvasCache {
    key: (i32, i32, bool),
    surface: cairo::ImageSurface,
}

thread_local! {
    static ACTIVE_CANCEL: RefCell<Option<Rc<RefCell<Option<gtk::gio::Cancellable>>>>> = const { RefCell::new(None) };
    static ACTIVE_DRAFT: RefCell<Option<Rc<RefCell<WizardDraft>>>> = const { RefCell::new(None) };
    static ACTIVE_CANVAS: RefCell<Option<glib::WeakRef<gtk::DrawingArea>>> = const { RefCell::new(None) };
    static FAVICON_SESSION: soup::Session = {
        let session = soup::Session::new();
        session.set_user_agent(BROWSER_UA);
        session
    };
}

/// Escape dismisses the introduction and reverts unconfirmed draft changes,
/// matching every other keyboard-first flow in Rill.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    if event.keyval() != Escape {
        return false;
    }
    if app.ui.stack.visible_child_name().as_deref() != Some(STACK_NAME) {
        return false;
    }
    let Some(root) = app.ui.stack.child_by_name(STACK_NAME) else {
        return false;
    };
    skip(app, &root);
    true
}

/// Whether the full-tab first-run introduction is mounted in the tab stack.
pub fn is_open(app: &App) -> bool {
    app.ui.stack.child_by_name(STACK_NAME).is_some()
}

pub fn dismiss(app: &App) {
    if let Some(root) = app.ui.stack.child_by_name(STACK_NAME) {
        skip(app, &root);
    }
}

/// Repaint the atmospheric canvas on window or theme changes.
pub fn relayout(app: &App) {
    ACTIVE_CANVAS.with(|slot| {
        if let Some(canvas) = slot.borrow().as_ref().and_then(|w| w.upgrade()) {
            canvas.queue_draw();
        }
    });
    if let Some(root) = app.ui.stack.child_by_name(STACK_NAME) {
        root.queue_draw();
    }
}

fn sync_look(app: &App, look: Look) {
    let desktop_dark = app.prefs.borrow().desktop_dark;
    *app.palette.borrow_mut() = crate::palette::Palette::for_look(look, desktop_dark);
    app.pin_signature.replace(None);
    app.top_pin_signature.replace(None);
}

/// Skip / Use defaults / Escape: revert any unconfirmed draft changes, mark
/// welcomed, and transition cleanly to the Start Page.
fn skip(app: &App, root: &gtk::Widget) {
    cancel_in_flight();
    let mut restored_look = None;
    let mut touched_sidebar = false;
    if let Some(draft_rc) = ACTIVE_DRAFT.with(|d| d.borrow_mut().take()) {
        let draft = draft_rc.borrow();
        let mut prefs = app.prefs.borrow_mut();
        if draft.touched_appearance {
            prefs.appearance = draft.initial.appearance.clone();
            restored_look = Some(draft.initial.appearance.clone());
        }
        if draft.touched_sidebar {
            prefs.sidebar = draft.initial.sidebar;
            prefs.column_folded = draft.initial.column_folded;
            touched_sidebar = true;
        }
        if draft.touched_engine {
            prefs.search_engine = draft.initial.search_engine.clone();
        }
        if draft.touched_suggestions {
            prefs.engine_suggestions = draft.initial.engine_suggestions;
        }
        prefs.welcomed = true;
        drop(prefs);
        app.prefs.borrow().save();
    } else {
        app.prefs.borrow_mut().welcomed = true;
        app.prefs.borrow().save();
    }
    teardown(app, root);
    if let Some(look) = restored_look {
        sync_look(app, look);
    }
    if touched_sidebar {
        app.sync_tab_layout();
    }
    app.apply_style();
    app.render();
    focus_after_close(app);
}

/// Finish: persist all confirmed choices, add opted-in starter shortcuts,
/// cache/fetch their official favicons, and transition to the Start Page.
fn finish(
    app: &AppRef,
    root: &gtk::Widget,
    draft: &WizardDraft,
    fetched_icons: &HashMap<&'static str, cairo::ImageSurface>,
) {
    cancel_in_flight();
    ACTIVE_DRAFT.with(|d| *d.borrow_mut() = None);

    {
        let mut prefs = app.prefs.borrow_mut();
        if draft.touched_appearance {
            prefs.appearance = draft.appearance.clone();
        }
        if draft.touched_sidebar {
            prefs.sidebar = draft.sidebar;
            prefs.column_folded = false;
        }
        if draft.touched_engine {
            prefs.search_engine = draft.search_engine.clone();
        }
        if draft.touched_suggestions {
            prefs.engine_suggestions = draft.engine_suggestions;
        }
        prefs.welcomed = true;
    }
    app.prefs.borrow().save();

    let mut chosen_sites = Vec::new();
    for (idx, site) in STARTER_SITES.iter().enumerate() {
        if draft.selected_sites[idx] {
            chosen_sites.push(*site);
        }
    }

    if !chosen_sites.is_empty() {
        let mut added = false;
        {
            let mut state = app.state.borrow_mut();
            for site in &chosen_sites {
                if !state.bookmarks.iter().any(|b| b.url == site.url) {
                    state.bookmarks.push(Bookmark {
                        url: site.url.to_string(),
                        title: site.title.to_string(),
                    });
                    added = true;
                }
            }
            if added {
                Records::save_bookmarks(&state.bookmarks);
            }
        }
        for site in &chosen_sites {
            if let Some(surface) = fetched_icons.get(site.url) {
                let surf_ref: &cairo::Surface = surface;
                app.cache_favicon(site.url, surf_ref);
            } else if app.favicon_for_url(site.url).is_none() {
                let weak = app.weak();
                let site_url = site.url;
                fetch_official_favicon(site.icon_url, None, move |surface| {
                    if let (Some(app), Some(surface)) = (weak.upgrade(), surface) {
                        let surf_ref: &cairo::Surface = &surface;
                        app.cache_favicon(site_url, surf_ref);
                        app.render();
                    }
                });
            }
        }
    }

    teardown(app, root);
    if draft.touched_appearance {
        sync_look(app, draft.appearance.clone());
    }
    if draft.touched_sidebar {
        app.sync_tab_layout();
    }
    app.apply_style();
    app.render();
    focus_after_close(app);
}

fn cancel_in_flight() {
    if let Some(slot) = ACTIVE_CANCEL.with(|c| c.borrow_mut().take()) {
        if let Some(cancel) = slot.borrow_mut().take() {
            cancel.cancel();
        }
    }
}

fn teardown(app: &App, root: &gtk::Widget) {
    crate::motion::cancel(root);
    ACTIVE_CANVAS.with(|d| *d.borrow_mut() = None);
    app.ui.stack.remove(root);
}

fn focus_after_close(app: &App) {
    if app.ui.stack.visible_child_name().as_deref() == Some("blank") {
        app.ui.start.search.grab_focus();
    } else {
        crate::permissions::focus_page(app);
    }
}

pub fn show(app: &AppRef) {
    // Never overwrite a restored non-blank web tab, error page, or private tab.
    let active_is_blank_regular = {
        let state = app.state.borrow();
        let blank = state.active_tab().map(|t| t.url.is_empty()).unwrap_or(true);
        let private = state.active_tab().is_some_and(|t| t.private);
        let has_error = state.errors.contains_key(&state.active);
        blank && !private && !has_error
    };
    if !active_is_blank_regular {
        return;
    }

    if let Some(existing) = app.ui.stack.child_by_name(STACK_NAME) {
        teardown(app, &existing);
    }

    let initial = {
        let prefs = app.prefs.borrow();
        PrefsSnapshot {
            appearance: prefs.appearance.clone(),
            sidebar: prefs.sidebar,
            column_folded: prefs.column_folded,
            search_engine: prefs.search_engine.clone(),
            engine_suggestions: prefs.engine_suggestions,
        }
    };
    let draft = Rc::new(RefCell::new(WizardDraft::from_snapshot(initial)));
    ACTIVE_DRAFT.with(|d| *d.borrow_mut() = Some(draft.clone()));

    let fetched_icons: Rc<RefCell<HashMap<&'static str, cairo::ImageSurface>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let active_cancel: Rc<RefCell<Option<gtk::gio::Cancellable>>> = Rc::new(RefCell::new(None));
    ACTIVE_CANCEL.with(|slot| *slot.borrow_mut() = Some(active_cancel.clone()));

    // Full-tab root mounted directly inside app.ui.stack as "welcome".
    // Retains `welcome-card` + `welcome-page` classes and zero scrim.
    let root = gtk::Overlay::new();
    root.set_widget_name("welcome-card");
    crate::ui::class(&root, "welcome-page");
    crate::ui::class(&root, "welcome-card");
    root.set_hexpand(true);
    root.set_vexpand(true);
    if let Some(access) = root.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Panel);
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Welcome to Rill");
    }

    // Warm paper / charcoal full-tab canvas with an original flowing river
    // topographic contour motif and sparse 8x8 Bayer ordered-dither texture
    // anchored along the right side of the tab.
    let initial_dark = app.palette.borrow().is_dark();
    let canvas = welcome_canvas_drawing_area(initial_dark);
    crate::ui::class(&canvas, "welcome-canvas");
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);
    ACTIVE_CANVAS.with(|slot| *slot.borrow_mut() = Some(canvas.downgrade()));
    root.add(&canvas);

    // Two-zone full-tab layout: scrollable editorial stage + pinned bare footer
    // actions aligned cleanly to the left rail (zero outer box or dock).
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    crate::ui::class(&shell, "welcome-shell");
    shell.set_hexpand(true);
    shell.set_vexpand(true);

    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    crate::ui::class(&scroll, "welcome-scroll");
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_shadow_type(gtk::ShadowType::None);
    scroll.set_propagate_natural_width(false);
    scroll.set_propagate_natural_height(false);
    scroll.set_min_content_width(0);
    scroll.set_min_content_height(0);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_overlay_scrolling(true);
    scroll.set_kinetic_scrolling(true);

    let stage = gtk::Box::new(gtk::Orientation::Vertical, 0);
    crate::ui::class(&stage, "welcome-stage");
    stage.set_hexpand(true);
    stage.set_vexpand(true);
    stage.set_valign(gtk::Align::Fill);

    let stack = gtk::Stack::new();
    crate::ui::class(&stack, "welcome-step-stack");
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(false);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::None);

    // =========================================================================
    // Screen 0 — Asymmetric Editorial Opening Intro ("Welcome to Rill")
    // =========================================================================
    let step0_outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    crate::ui::class(&step0_outer, "welcome-hero-outer");
    step0_outer.set_hexpand(true);
    step0_outer.set_vexpand(true);

    let top_spring = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_spring.set_vexpand(true);
    step0_outer.pack_start(&top_spring, true, true, 0);

    let hero = gtk::Box::new(gtk::Orientation::Vertical, 14);
    crate::ui::class(&hero, "welcome-hero");
    hero.set_hexpand(true);
    hero.set_halign(gtk::Align::Fill);
    hero.set_valign(gtk::Align::Center);
    hero.set_margin_start(88);
    hero.set_margin_end(48);

    let mark = rill_wave_mark(initial_dark);
    crate::ui::class(&mark, "welcome-mark");
    mark.set_halign(gtk::Align::Start);
    hero.pack_start(&mark, false, false, 0);

    let hero_title = left_label("Welcome to Rill", "welcome-title");
    if let Some(access) = hero_title.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
    }
    hero.pack_start(&hero_title, false, false, 0);

    let hero_desc = left_label(
        "A quiet browser built for reading and focus.\nChoose your look, layout, and search engine, or start with defaults.",
        "welcome-description",
    );
    hero_desc.set_max_width_chars(50);
    hero.pack_start(&hero_desc, false, false, 0);

    let hero_actions = gtk::Box::new(gtk::Orientation::Horizontal, PILL_GAP);
    crate::ui::class(&hero_actions, "welcome-hero-actions");
    hero_actions.set_halign(gtk::Align::Start);
    hero_actions.set_margin_top(12);

    let configure_btn = pill_action_button(
        "Configure",
        None,
        Some(Icon::ArrowRight),
        &["welcome-btn-primary", "welcome-next", "welcome-start"],
    );
    hero_actions.pack_start(&configure_btn, false, false, 0);

    let defaults_btn = pill_action_button(
        "Use defaults",
        Some(Icon::Check),
        None,
        &["welcome-btn-secondary", "welcome-skip"],
    );
    defaults_btn.set_tooltip_text(Some("Start with calm defaults (Esc)"));
    hero_actions.pack_start(&defaults_btn, false, false, 0);

    hero.pack_start(&hero_actions, false, false, 0);
    step0_outer.pack_start(&hero, false, false, 0);

    let bottom_spring = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom_spring.set_vexpand(true);
    step0_outer.pack_start(&bottom_spring, true, true, 0);

    let esc_hint = left_label("Press Esc anytime to start with defaults.", "welcome-foot-hint");
    esc_hint.set_margin_start(88);
    esc_hint.set_margin_end(48);
    esc_hint.set_margin_bottom(24);
    step0_outer.pack_end(&esc_hint, false, false, 0);

    stack.add_named(&step0_outer, "step-0");

    // =========================================================================
    // Setup Step 1 — Appearance & layout
    // =========================================================================
    let step1 = setup_step_container(
        1,
        "Appearance & layout",
        "Changes preview right away. Nothing is saved until you finish.",
        initial_dark,
    );

    let look_group = gtk::Box::new(gtk::Orientation::Vertical, 8);
    crate::ui::class(&look_group, "welcome-section");
    look_group.pack_start(&group_label("Appearance"), false, false, 0);

    let look_row = gtk::Box::new(gtk::Orientation::Horizontal, PILL_GAP);
    look_row.set_homogeneous(true);
    let look_buttons: Rc<RefCell<Vec<(Look, glib::WeakRef<gtk::Button>)>>> =
        Rc::new(RefCell::new(Vec::new()));
    for (look, label_text, sub_text, swatch_class) in [
        (Look::System, "System", "Match desktop", "swatch-system"),
        (Look::Light, "Light", "Warm paper", "swatch-light"),
        (Look::Dark, "Dark", "Low glare", "swatch-dark"),
    ] {
        let swatch = appearance_swatch(swatch_class);
        let btn = choice_button_with_leading(
            label_text,
            sub_text,
            Some(swatch.upcast::<gtk::Widget>()),
            draft.borrow().appearance == look,
        );
        crate::ui::class(&btn, "welcome-segment");
        crate::ui::class(&btn, "welcome-look-choice");
        look_buttons.borrow_mut().push((look.clone(), btn.downgrade()));
        let app_weak = app.weak();
        let draft_ref = draft.clone();
        let buttons_ref = look_buttons.clone();
        let canvas_weak = canvas.downgrade();
        btn.connect_clicked(move |_| {
            {
                let mut d = draft_ref.borrow_mut();
                d.appearance = look.clone();
                d.touched_appearance = true;
            }
            for (item_look, weak_b) in buttons_ref.borrow().iter() {
                if let Some(b) = weak_b.upgrade() {
                    set_chosen(&b, *item_look == look);
                }
            }
            if let Some(app) = app_weak.upgrade() {
                app.prefs.borrow_mut().appearance = look.clone();
                sync_look(&app, look.clone());
                app.apply_style();
            }
            if let Some(c) = canvas_weak.upgrade() {
                c.queue_draw();
            }
        });
        look_row.pack_start(&btn, true, true, 0);
    }
    look_group.pack_start(&look_row, false, true, 0);
    step1.pack_start(&look_group, false, true, 0);

    let layout_group = gtk::Box::new(gtk::Orientation::Vertical, 8);
    crate::ui::class(&layout_group, "welcome-section");
    layout_group.pack_start(&group_label("Tab layout"), false, false, 0);

    let layout_row = gtk::Box::new(gtk::Orientation::Horizontal, PILL_GAP);
    layout_row.set_homogeneous(true);
    let layout_buttons: Rc<RefCell<Vec<(bool, glib::WeakRef<gtk::Button>)>>> =
        Rc::new(RefCell::new(Vec::new()));
    for (use_sidebar, label_text, sub_text) in [
        (false, "Top tabs", "Compact bar at the top"),
        (true, "Sidebar", "Vertical tab column"),
    ] {
        let mark = welcome_mark(
            if use_sidebar {
                WelcomeMark::LayoutSide
            } else {
                WelcomeMark::LayoutTop
            },
            initial_dark,
        );
        let btn = choice_button_with_leading(
            label_text,
            sub_text,
            Some(mark.upcast::<gtk::Widget>()),
            draft.borrow().sidebar == use_sidebar,
        );
        crate::ui::class(&btn, "welcome-segment");
        crate::ui::class(&btn, "welcome-layout-choice");
        layout_buttons.borrow_mut().push((use_sidebar, btn.downgrade()));
        let app_weak = app.weak();
        let draft_ref = draft.clone();
        let buttons_ref = layout_buttons.clone();
        btn.connect_clicked(move |_| {
            {
                let mut d = draft_ref.borrow_mut();
                d.sidebar = use_sidebar;
                d.touched_sidebar = true;
            }
            for (flag, weak_b) in buttons_ref.borrow().iter() {
                if let Some(b) = weak_b.upgrade() {
                    set_chosen(&b, *flag == use_sidebar);
                }
            }
            if let Some(app) = app_weak.upgrade() {
                {
                    let mut prefs = app.prefs.borrow_mut();
                    prefs.sidebar = use_sidebar;
                    prefs.column_folded = false;
                }
                app.sync_tab_layout();
                app.render();
            }
        });
        layout_row.pack_start(&btn, true, true, 0);
    }
    layout_group.pack_start(&layout_row, false, true, 0);
    step1.pack_start(&layout_group, false, true, 0);

    stack.add_named(&step1, "step-1");

    // =========================================================================
    // Setup Step 2 — Default search engine
    // =========================================================================
    let step2 = setup_step_container(
        2,
        "Default search engine",
        "Used from the address bar and start page. Change anytime in Settings.",
        initial_dark,
    );

    let grid = gtk::Grid::new();
    grid.set_column_homogeneous(true);
    grid.set_row_spacing(PILL_GAP as u32);
    grid.set_column_spacing(PILL_GAP as u32);

    let engine_meta = [
        ("Startpage", "startpage.com · Default", WelcomeMark::Startpage),
        ("Ecosia", "ecosia.org", WelcomeMark::Ecosia),
        ("DuckDuckGo", "duckduckgo.com", WelcomeMark::DuckDuckGo),
        ("Google", "google.com", WelcomeMark::Google),
        ("Bing", "bing.com", WelcomeMark::Bing),
        ("Brave", "search.brave.com", WelcomeMark::Brave),
    ];
    let engine_buttons: Rc<RefCell<Vec<(String, glib::WeakRef<gtk::Button>)>>> =
        Rc::new(RefCell::new(Vec::new()));
    let current_engine = draft.borrow().search_engine.clone();
    for (idx, (name, domain, mark_kind)) in engine_meta.iter().enumerate() {
        let is_current = current_engine.eq_ignore_ascii_case(name);
        let mark = welcome_mark(*mark_kind, initial_dark);
        let btn = choice_button_with_leading(name, domain, Some(mark.upcast::<gtk::Widget>()), is_current);
        crate::ui::class(&btn, "welcome-segment");
        crate::ui::class(&btn, "welcome-engine-choice");
        engine_buttons
            .borrow_mut()
            .push(((*name).to_string(), btn.downgrade()));
        let draft_ref = draft.clone();
        let buttons_ref = engine_buttons.clone();
        let engine_name = (*name).to_string();
        btn.connect_clicked(move |_| {
            {
                let mut d = draft_ref.borrow_mut();
                d.search_engine = engine_name.clone();
                d.touched_engine = true;
            }
            for (item_name, weak_b) in buttons_ref.borrow().iter() {
                if let Some(b) = weak_b.upgrade() {
                    set_chosen(&b, item_name.eq_ignore_ascii_case(&engine_name));
                }
            }
        });
        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        grid.attach(&btn, col, row, 1, 1);
    }
    step2.pack_start(&grid, false, true, 0);

    stack.add_named(&step2, "step-2");

    // =========================================================================
    // Setup Step 3 — Privacy & starter shortcuts
    // =========================================================================
    let step3 = setup_step_container(
        3,
        "Privacy & shortcuts",
        "Control live suggestions and optional start-page shortcuts.",
        initial_dark,
    );

    // Live search suggestions row.
    let sug_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    crate::ui::class(&sug_row, "welcome-toggle-row");
    sug_row.set_valign(gtk::Align::Center);

    let sug_icon = welcome_mark(WelcomeMark::PrivacyShield, initial_dark);
    crate::ui::class(&sug_icon, "welcome-choice-icon");
    sug_icon.set_valign(gtk::Align::Center);
    sug_row.pack_start(&sug_icon, false, false, 0);

    let sug_text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let sug_title = left_label("Live search suggestions", "welcome-toggle-title");
    let sug_sub = left_label(
        "Typing is sent to your selected engine. Never in private tabs or for addresses.",
        "welcome-toggle-sub",
    );
    sug_text.pack_start(&sug_title, false, false, 0);
    sug_text.pack_start(&sug_sub, false, false, 0);
    sug_row.pack_start(&sug_text, true, true, 0);

    let sug_switch = welcome_pill_switch(draft.borrow().engine_suggestions, initial_dark);
    if let Some(access) = sug_switch.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Live search suggestions");
    }
    {
        let draft_ref = draft.clone();
        sug_switch.connect_toggled(move |sw| {
            let mut d = draft_ref.borrow_mut();
            d.engine_suggestions = sw.is_active();
            d.touched_suggestions = true;
        });
    }
    sug_row.pack_end(&sug_switch, false, false, 0);
    step3.pack_start(&sug_row, false, true, 0);

    // Optional starter shortcuts (all unselected by default; zero network on open).
    let shortcuts_group = gtk::Box::new(gtk::Orientation::Vertical, 8);
    crate::ui::class(&shortcuts_group, "welcome-section");
    shortcuts_group.pack_start(&group_label("Starter shortcuts"), false, false, 0);

    let site_grid = gtk::Grid::new();
    site_grid.set_column_homogeneous(true);
    site_grid.set_row_spacing(PILL_GAP as u32);
    site_grid.set_column_spacing(PILL_GAP as u32);

    for (idx, site) in STARTER_SITES.iter().enumerate() {
        let icon_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        icon_box.set_size_request(28, 28);
        icon_box.set_valign(gtk::Align::Center);
        icon_box.set_halign(gtk::Align::Center);
        let fallback_icon = welcome_mark(
            match idx {
                0 => WelcomeMark::Wikipedia,
                1 => WelcomeMark::GitHub,
                2 => WelcomeMark::YouTube,
                _ => WelcomeMark::Mdn,
            },
            initial_dark,
        );
        crate::ui::class(&fallback_icon, "welcome-choice-icon");
        icon_box.pack_start(&fallback_icon, false, false, 0);

        let btn = choice_button_with_leading(
            site.title,
            site.host,
            Some(icon_box.clone().upcast::<gtk::Widget>()),
            false,
        );
        crate::ui::class(&btn, "welcome-site-choice");
        if let Some(access) = btn.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::ToggleButton);
        }

        let draft_ref = draft.clone();
        let fetched_ref = fetched_icons.clone();
        let cancel_ref = active_cancel.clone();
        let icon_box_weak = icon_box.downgrade();
        let app_weak = app.weak();
        btn.connect_clicked(move |b| {
            let selected = {
                let mut d = draft_ref.borrow_mut();
                d.selected_sites[idx] = !d.selected_sites[idx];
                d.selected_sites[idx]
            };
            set_chosen(b, selected);
            if selected {
                if let Some(surface) = fetched_ref.borrow().get(site.url) {
                    if let Some(holder) = icon_box_weak.upgrade() {
                        set_site_favicon(&holder, surface);
                    }
                    return;
                }
                if let Some(app) = app_weak.upgrade() {
                    if let Some(surface) = app.favicon_for_url(site.url) {
                        if let Some(holder) = icon_box_weak.upgrade() {
                            set_site_favicon(&holder, &surface);
                        }
                        return;
                    }
                }
                let cancellable = {
                    let mut slot = cancel_ref.borrow_mut();
                    slot.get_or_insert_with(gtk::gio::Cancellable::new).clone()
                };
                let holder_weak = icon_box_weak.clone();
                let fetched_cb = fetched_ref.clone();
                let site_url = site.url;
                fetch_official_favicon(site.icon_url, Some(cancellable), move |surface| {
                    if let Some(surface) = surface {
                        if let Some(holder) = holder_weak.upgrade() {
                            set_site_favicon(&holder, &surface);
                        }
                        fetched_cb.borrow_mut().insert(site_url, surface);
                    }
                });
            }
        });

        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        site_grid.attach(&btn, col, row, 1, 1);
    }
    shortcuts_group.pack_start(&site_grid, false, true, 0);

    let site_note = left_label(
        "Optional. Official site icons load only for shortcuts you select.",
        "welcome-hint",
    );
    site_note.set_margin_start(6);
    site_note.set_margin_top(2);
    shortcuts_group.pack_start(&site_note, false, false, 0);
    step3.pack_start(&shortcuts_group, false, true, 0);

    // Optional browser bookmark import (only shown when other browsers are detected).
    let detected_browsers = crate::import::home()
        .map(|home| crate::import::installed(&home))
        .unwrap_or_default();
    if !detected_browsers.is_empty() {
        let import_row_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        crate::ui::class(&import_row_box, "welcome-import");
        let import_status = left_label("", "welcome-status");
        import_status.set_no_show_all(true);
        import_status.hide();

        let expander = gtk::Expander::new(None);
        expander.set_label_widget(Some(&pill_label(
            "Import bookmarks from another browser",
            "welcome-import-title",
        )));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        list.set_margin_top(8);
        for browser in detected_browsers {
            list.pack_start(&import_row(app, browser, &import_status), false, true, 0);
        }
        expander.add(&list);
        import_row_box.pack_start(&expander, false, true, 0);
        import_row_box.pack_start(&import_status, false, true, 0);
        step3.pack_start(&import_row_box, false, true, 0);
    }

    stack.add_named(&step3, "step-3");
    stack.set_visible_child_name("step-0");

    stage.pack_start(&stack, true, true, 0);
    scroll.add(&stage);
    shell.pack_start(&scroll, true, true, 0);

    // =========================================================================
    // Left-Aligned Pinned Step Footer (Steps 1–3 only; single compact row)
    // =========================================================================
    let footer_wrap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    crate::ui::class(&footer_wrap, "welcome-footer-wrap");
    footer_wrap.set_hexpand(true);
    footer_wrap.set_halign(gtk::Align::Fill);
    footer_wrap.set_margin_start(88);
    footer_wrap.set_margin_end(48);
    footer_wrap.set_margin_top(8);
    footer_wrap.set_margin_bottom(22);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, PILL_GAP);
    crate::ui::class(&actions, "welcome-actions");
    actions.set_halign(gtk::Align::Start);

    let back_btn = pill_action_button(
        "Back",
        Some(Icon::Back),
        None,
        &["welcome-btn-secondary", "welcome-back"],
    );
    actions.pack_start(&back_btn, false, false, 0);

    let next_btn = pill_action_button(
        "Next",
        None,
        Some(Icon::ArrowRight),
        &["welcome-btn-primary", "welcome-next", "welcome-start"],
    );
    actions.pack_start(&next_btn, false, false, 0);

    let finish_btn = pill_action_button(
        "Finish",
        Some(Icon::Check),
        None,
        &["welcome-btn-primary", "welcome-next", "welcome-start"],
    );
    actions.pack_start(&finish_btn, false, false, 0);

    footer_wrap.pack_start(&actions, false, false, 0);

    let skip_link = gtk::Button::with_label("Skip setup");
    crate::ui::class(&skip_link, "welcome-skip-link");
    crate::ui::class(&skip_link, "welcome-skip");
    skip_link.set_halign(gtk::Align::End);
    skip_link.set_valign(gtk::Align::Center);
    skip_link.set_relief(gtk::ReliefStyle::None);
    skip_link.set_tooltip_text(Some("Skip setup and revert changes (Esc)"));
    footer_wrap.pack_end(&skip_link, false, false, 0);

    shell.pack_end(&footer_wrap, false, false, 0);
    root.add_overlay(&shell);

    // Responsive asymmetric editorial rail and short-window sizing (900x560 / 800x600).
    let fitted = Rc::new(Cell::new(None::<(i32, i32, i32, i32, bool, bool)>));
    let step_cols = [step1.downgrade(), step2.downgrade(), step3.downgrade()];
    let hero_weak = hero.downgrade();
    let esc_weak = esc_hint.downgrade();
    let footer_weak = footer_wrap.downgrade();
    let root_alloc_weak = root.downgrade();
    root.connect_size_allocate(move |_, allocation| {
        let width = allocation.width().max(1);
        let height = allocation.height().max(1);
        let narrow = width < 760;
        let short = height < 580;
        let left_margin = if width >= 1160 {
            ((width as f64) * 0.075).round().clamp(72.0, 112.0) as i32
        } else if width >= 860 {
            ((width as f64) * 0.055).round().clamp(40.0, 68.0) as i32
        } else {
            22
        };
        let min_right = if narrow { 22 } else { 36 };
        let right_margin = (width - left_margin - CONTENT_MAX_WIDTH).max(min_right);
        let top_pad = if short {
            14
        } else if height < 720 {
            32
        } else {
            ((height as f64) * 0.095).round().clamp(40.0, 80.0) as i32
        };
        let bottom_pad = if short { 12 } else { 24 };
        let fit = (left_margin, right_margin, top_pad, bottom_pad, narrow, short);
        if fitted.replace(Some(fit)) == Some(fit) {
            return;
        }
        let fitted = fitted.clone();
        let step_cols = step_cols.clone();
        let hero_w = hero_weak.clone();
        let esc_w = esc_weak.clone();
        let footer_w = footer_weak.clone();
        let root_w = root_alloc_weak.clone();
        glib::idle_add_local_once(move || {
            if fitted.get() != Some(fit) {
                return;
            }
            if let Some(r) = root_w.upgrade() {
                set_class(&r, "welcome-narrow", narrow);
                set_class(&r, "welcome-short", short);
            }
            if let Some(h) = hero_w.upgrade() {
                h.set_margin_start(left_margin);
                h.set_margin_end(right_margin);
                h.set_spacing(if short { 10 } else { 14 });
            }
            if let Some(e) = esc_w.upgrade() {
                e.set_margin_start(left_margin);
                e.set_margin_end(right_margin);
                e.set_margin_bottom(if short { 14 } else { 24 });
            }
            for col_w in &step_cols {
                if let Some(col) = col_w.upgrade() {
                    col.set_margin_start(left_margin);
                    col.set_margin_end(right_margin);
                    col.set_margin_top(top_pad);
                    col.set_margin_bottom(if short { 10 } else { 18 });
                    col.set_spacing(if short { 12 } else { 18 });
                }
            }
            if let Some(foot) = footer_w.upgrade() {
                foot.set_margin_start(left_margin);
                foot.set_margin_end(right_margin);
                foot.set_margin_bottom(bottom_pad);
            }
        });
    });

    let sync_step_ui = {
        let stack = stack.downgrade();
        let scroll = scroll.downgrade();
        let footer_wrap = footer_wrap.downgrade();
        let next_btn = next_btn.downgrade();
        let finish_btn = finish_btn.downgrade();
        let configure_btn = configure_btn.downgrade();
        Rc::new(move |step: usize| {
            let child_name = step_child_name(step);
            if let Some(s) = stack.upgrade() {
                s.set_visible_child_name(child_name);
            }
            if let Some(sc) = scroll.upgrade() {
                sc.vadjustment().set_value(0.0);
            }
            let is_intro = step == 0;
            let is_last = step + 1 >= STEP_COUNT;
            if let Some(f) = footer_wrap.upgrade() {
                if is_intro {
                    f.hide();
                } else {
                    f.show();
                    if let Some(n) = next_btn.upgrade() {
                        n.set_visible(!is_last);
                    }
                    if let Some(fin) = finish_btn.upgrade() {
                        fin.set_visible(is_last);
                    }
                }
            }
            if is_intro {
                if let Some(c) = configure_btn.upgrade() {
                    c.grab_focus();
                }
            } else if is_last {
                if let Some(fin) = finish_btn.upgrade() {
                    fin.grab_focus();
                }
            } else if let Some(n) = next_btn.upgrade() {
                n.grab_focus();
            }
        })
    };

    // Screen 0: "Use defaults" (immediately finishes with defaults)
    {
        let app_weak = app.weak();
        let root_weak = root.downgrade();
        defaults_btn.connect_clicked(move |_| {
            if let (Some(app), Some(r)) = (app_weak.upgrade(), root_weak.upgrade()) {
                skip(&app, r.upcast_ref());
            }
        });
    }

    // Screen 0: "Configure" -> Step 1
    {
        let draft_ref = draft.clone();
        let sync_ui = sync_step_ui.clone();
        configure_btn.connect_clicked(move |_| {
            let step = {
                let mut d = draft_ref.borrow_mut();
                d.next_step();
                d.step
            };
            sync_ui(step);
        });
    }

    // Steps 1–3: "Skip setup"
    {
        let app_weak = app.weak();
        let root_weak = root.downgrade();
        skip_link.connect_clicked(move |_| {
            if let (Some(app), Some(r)) = (app_weak.upgrade(), root_weak.upgrade()) {
                skip(&app, r.upcast_ref());
            }
        });
    }

    // Steps 1–3: "Back"
    {
        let draft_ref = draft.clone();
        let sync_ui = sync_step_ui.clone();
        back_btn.connect_clicked(move |_| {
            let step = {
                let mut d = draft_ref.borrow_mut();
                if d.prev_step() {
                    Some(d.step)
                } else {
                    None
                }
            };
            if let Some(step) = step {
                sync_ui(step);
            }
        });
    }

    // Steps 1–2: "Next"
    {
        let draft_ref = draft.clone();
        let sync_ui = sync_step_ui.clone();
        next_btn.connect_clicked(move |_| {
            let step = {
                let mut d = draft_ref.borrow_mut();
                if d.next_step() {
                    Some(d.step)
                } else {
                    None
                }
            };
            if let Some(step) = step {
                sync_ui(step);
            }
        });
    }

    // Step 3: "Finish"
    {
        let app_weak = app.weak();
        let root_weak = root.downgrade();
        let draft_ref = draft.clone();
        let fetched_ref = fetched_icons.clone();
        finish_btn.connect_clicked(move |_| {
            if let (Some(app), Some(r)) = (app_weak.upgrade(), root_weak.upgrade()) {
                let snapshot = draft_ref.borrow().clone();
                let icons_map = fetched_ref.borrow();
                finish(&app, r.upcast_ref(), &snapshot, &icons_map);
            }
        });
    }

    app.ui.stack.add_named(&root, STACK_NAME);
    root.show_all();
    footer_wrap.hide();
    finish_btn.hide();
    app.ui.stack.set_visible_child_name(STACK_NAME);
    crate::motion::fade_in(&root, crate::motion::dur::ENTER);
    configure_btn.grab_focus();
}

fn step_child_name(step: usize) -> &'static str {
    match step {
        0 => "step-0",
        1 => "step-1",
        2 => "step-2",
        _ => "step-3",
    }
}

fn setup_step_container(
    step_idx: usize,
    heading: &str,
    subtitle: &str,
    initial_dark: bool,
) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 18);
    crate::ui::class(&col, "welcome-step-page");
    col.set_hexpand(true);
    col.set_halign(gtk::Align::Fill);
    col.set_valign(gtk::Align::Start);
    col.set_margin_start(88);
    col.set_margin_end(48);
    col.set_margin_top(48);
    col.set_margin_bottom(18);

    let header = gtk::Box::new(gtk::Orientation::Vertical, 6);
    crate::ui::class(&header, "welcome-header");

    let meta_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    meta_row.set_halign(gtk::Align::Start);
    meta_row.set_valign(gtk::Align::Center);
    meta_row.set_margin_bottom(4);
    let rail = step_progress_rail(step_idx, initial_dark);
    meta_row.pack_start(&rail, false, false, 0);
    let step_meta = pill_label(&format!("Step {step_idx} of 3"), "welcome-step-meta");
    meta_row.pack_start(&step_meta, false, false, 0);
    header.pack_start(&meta_row, false, false, 0);

    let title = left_label(heading, "welcome-step-title");
    if let Some(access) = title.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
    }
    let sub = left_label(subtitle, "welcome-step-sub");
    header.pack_start(&title, false, false, 0);
    header.pack_start(&sub, false, false, 0);
    col.pack_start(&header, false, false, 0);
    col
}

/// Minimal pure-Cairo 3-segment hairline step indicator (zero pill badge box).
fn step_progress_rail(active_step: usize, initial_dark: bool) -> gtk::DrawingArea {
    let area = welcome_drawing_area();
    area.set_size_request(58, 8);
    area.set_valign(gtk::Align::Center);
    area.connect_style_updated(|w| w.queue_draw());
    area.connect_draw(move |widget, cr| {
        let dark = crate::startpage::widget_is_dark(widget, initial_dark);
        let accent = if dark {
            (246.0 / 255.0, 146.0 / 255.0, 104.0 / 255.0)
        } else {
            (183.0 / 255.0, 69.0 / 255.0, 37.0 / 255.0)
        };
        let muted_line = if dark {
            (78.0 / 255.0, 74.0 / 255.0, 68.0 / 255.0)
        } else {
            (198.0 / 255.0, 198.0 / 255.0, 192.0 / 255.0)
        };

        let y = (widget.allocated_height().max(8) as f64) * 0.5;
        let mut x = 1.5;
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_width(3.0);
        for step in 1..=3 {
            let len = if step == active_step { 22.0 } else { 10.0 };
            if step == active_step {
                cr.set_source_rgba(accent.0, accent.1, accent.2, 0.95);
            } else if step < active_step {
                cr.set_source_rgba(accent.0, accent.1, accent.2, 0.42);
            } else {
                cr.set_source_rgba(muted_line.0, muted_line.1, muted_line.2, 0.85);
            }
            cr.new_path();
            cr.move_to(x, y);
            cr.line_to(x + len, y);
            let _ = cr.stroke();
            x += len + 7.0;
        }
        crate::gtkx::inh(false)
    });
    area
}

/// Native Rill full-tab canvas: warm paper (#F7F7F5) or warm charcoal (#1A1816)
/// ground paired with an original flowing river topographic contour motif and
/// sparse 8x8 Bayer ordered-dither current anchored along the right side.
fn welcome_canvas_drawing_area(initial_dark: bool) -> gtk::DrawingArea {
    let area = welcome_drawing_area();
    let cache: Rc<RefCell<Option<CanvasCache>>> = Rc::new(RefCell::new(None));
    area.connect_style_updated(|w| w.queue_draw());
    area.connect_draw(move |widget, cr| {
        let width = widget.allocated_width().max(1);
        let height = widget.allocated_height().max(1);
        let dark = crate::startpage::widget_is_dark(widget, initial_dark);
        let key = (width, height, dark);

        let mut slot = cache.borrow_mut();
        let needs_rebuild = slot.as_ref().is_none_or(|c| c.key != key);
        if needs_rebuild {
            if let Some(surface) = build_welcome_canvas(width, height, dark) {
                *slot = Some(CanvasCache { key, surface });
            }
        }
        if let Some(cached) = slot.as_ref() {
            let _ = cr.save();
            rounded_rect(cr, 0.0, 0.0, width as f64, height as f64, 15.0);
            cr.clip();
            let _ = cr.set_source_surface(&cached.surface, 0.0, 0.0);
            let _ = cr.paint();
            let _ = cr.restore();
        }
        crate::gtkx::inh(false)
    });
    area
}

/// Centerline `x` (normalized 0..1) and half-width of Rill's right-side flowing
/// river corridor at normalized vertical position `ny` (0..1).
fn river_geometry(ny: f64, narrow_canvas: bool) -> (f64, f64) {
    let pi = std::f64::consts::PI;
    let base_x = if narrow_canvas { 0.875 } else { 0.795 };
    let sway = if narrow_canvas { 0.036 } else { 0.056 };
    let cx = base_x
        + sway * (ny * 2.05 * pi - 0.65).sin()
        + (sway * 0.34) * (ny * 4.1 * pi + 0.35).cos();
    let half_w = if narrow_canvas {
        0.105 + 0.055 * ny
    } else {
        0.135 + 0.075 * ny
    };
    (cx, half_w)
}

fn build_welcome_canvas(width: i32, height: i32, dark: bool) -> Option<cairo::ImageSurface> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    let stride = surface.stride() as usize;
    let w = width as usize;
    let h = height as usize;
    let inv_w = 1.0 / (width.max(1) as f64);
    let inv_h = 1.0 / (height.max(1) as f64);
    let narrow_canvas = width < 980;
    let left_guard = if narrow_canvas { 0.66 } else { 0.54 };

    // Exact Rill warm paper (#F7F7F5) and warm charcoal (#1A1816) grounds.
    let ground: (f64, f64, f64) = if dark {
        (26.0, 24.0, 22.0)
    } else {
        (247.0, 247.0, 245.0)
    };
    // Subtle outer bank tone and warm terracotta-sand inner current for Bayer dither.
    let bank_dot: (f64, f64, f64) = if dark {
        (46.0, 41.0, 36.0)
    } else {
        (221.0, 217.0, 207.0)
    };
    let core_dot: (f64, f64, f64) = if dark {
        (134.0, 78.0, 56.0)
    } else {
        (216.0, 148.0, 122.0)
    };

    {
        let mut data = surface.data().ok()?;
        for y in 0..h {
            let ny = (y as f64) * inv_h;
            let (cx, half_w) = river_geometry(ny, narrow_canvas);
            let row_off = y * stride;
            let by = (y / DITHER_CELL as usize) & 7;

            for x in 0..w {
                let nx = (x as f64) * inv_w;
                let mut r = ground.0;
                let mut g = ground.1;
                let mut b = ground.2;

                // Keep left editorial column completely pure paper/charcoal.
                if nx > left_guard {
                    let guard = ((nx - left_guard) / 0.12).clamp(0.0, 1.0);
                    let dist = ((nx - cx) / half_w.max(0.01)).abs();
                    if dist < 1.38 {
                        let falloff = (1.0 - dist / 1.38).clamp(0.0, 1.0);
                        let density = falloff * falloff * guard * if narrow_canvas { 0.28 } else { 0.36 };
                        let bx = (x / DITHER_CELL as usize) & 7;
                        let threshold = (crate::startpage::BAYER_8X8[by][bx] as f64 + 0.5) / 64.0;
                        if density > threshold {
                            let core_mix = ((1.0 - dist / 0.78).clamp(0.0, 1.0)).powf(1.3);
                            let dot_r = bank_dot.0 + (core_dot.0 - bank_dot.0) * core_mix;
                            let dot_g = bank_dot.1 + (core_dot.1 - bank_dot.1) * core_mix;
                            let dot_b = bank_dot.2 + (core_dot.2 - bank_dot.2) * core_mix;
                            let blend = 0.55 + 0.35 * core_mix;
                            r += (dot_r - r) * blend;
                            g += (dot_g - g) * blend;
                            b += (dot_b - b) * blend;
                        }
                    }
                }

                let px = row_off + x * 4;
                data[px] = b.clamp(0.0, 255.0) as u8;
                data[px + 1] = g.clamp(0.0, 255.0) as u8;
                data[px + 2] = r.clamp(0.0, 255.0) as u8;
                data[px + 3] = 255;
            }
        }
    }

    // Layer crisp anti-aliased Cairo topographic river contour streamlines over
    // the right-side Bayer stream corridor.
    if let Ok(cr) = cairo::Context::new(&surface) {
        let wf = width as f64;
        let hf = height as f64;
        let pi = std::f64::consts::PI;

        let _ = cr.save();
        // Ensure streamlines never enter the left editorial column.
        let clip_x = wf * (left_guard + 0.02);
        cr.rectangle(clip_x, 0.0, (wf - clip_x).max(0.0), hf);
        cr.clip();
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);

        let accent = if dark {
            (246.0 / 255.0, 146.0 / 255.0, 104.0 / 255.0)
        } else {
            (183.0 / 255.0, 69.0 / 255.0, 37.0 / 255.0)
        };
        let stone = if dark {
            (158.0 / 255.0, 148.0 / 255.0, 136.0 / 255.0)
        } else {
            (126.0 / 255.0, 120.0 / 255.0, 110.0 / 255.0)
        };

        let contours: [(f64, f64, bool, f64, f64); 9] = [
            (-1.02, 0.20, false, 0.08, 0.95),
            (-0.72, 0.65, false, 0.13, 1.00),
            (-0.44, 1.15, false, 0.17, 1.05),
            (-0.20, 1.55, true, 0.22, 1.20),
            (0.00, 0.00, true, 0.34, 1.55),
            (0.21, 2.10, true, 0.22, 1.20),
            (0.46, 2.55, false, 0.17, 1.05),
            (0.75, 3.05, false, 0.12, 1.00),
            (1.06, 3.55, false, 0.08, 0.95),
        ];

        let steps = 64usize;
        let alpha_scale = if narrow_canvas { 0.72 } else { 1.0 };
        for (offset, phase, is_accent, alpha, stroke_w) in contours {
            let (r, g, b) = if is_accent { accent } else { stone };
            cr.set_source_rgba(r, g, b, alpha * alpha_scale);
            cr.set_line_width(stroke_w);
            cr.new_path();
            for i in 0..=steps {
                let ny = -0.04 + 1.08 * (i as f64) / (steps as f64);
                let (cx, half_w) = river_geometry(ny.clamp(0.0, 1.0), narrow_canvas);
                // Organic contour pinch/swell along the stream length.
                let ripple = 1.0 + 0.12 * (ny * 3.0 * pi + phase).sin();
                let x = (cx + offset * half_w * ripple) * wf;
                let y = ny * hf;
                if i == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
            }
            let _ = cr.stroke();
        }
        let _ = cr.restore();
    }

    Some(surface)
}

fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -pi * 0.5, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, pi * 0.5);
    cr.arc(x + r, y + h - r, r, pi * 0.5, pi);
    cr.arc(x + r, y + r, r, pi, pi * 1.5);
    cr.close_path();
}

/// Understated geometric Rill mark: three calm parallel stream waves painted in
/// Rill's terracotta accent, reacting live to Light/Dark toggles.
fn rill_wave_mark(initial_dark: bool) -> gtk::DrawingArea {
    let area = welcome_drawing_area();
    area.set_size_request(42, 28);
    area.connect_style_updated(|w| w.queue_draw());
    area.connect_draw(move |widget, cr| {
        let dark = crate::startpage::widget_is_dark(widget, initial_dark);
        let h = widget.allocated_height().max(28) as f64;
        let cy = h * 0.5;
        let span = 34.0;
        let x0 = 2.0;
        let amp = 3.2;

        let _ = cr.save();
        if dark {
            cr.set_source_rgb(246.0 / 255.0, 146.0 / 255.0, 104.0 / 255.0);
        } else {
            cr.set_source_rgb(183.0 / 255.0, 69.0 / 255.0, 37.0 / 255.0);
        }
        cr.set_line_width(2.2);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);

        for (idx, dy) in [-7.5, 0.0, 7.5].iter().enumerate() {
            let y = cy + dy;
            let row_span = if idx == 2 { span * 0.80 } else { span };
            cr.new_path();
            cr.move_to(x0, y + amp * 0.35);
            cr.curve_to(
                x0 + row_span * 0.28,
                y - amp * 1.15,
                x0 + row_span * 0.48,
                y - amp * 0.35,
                x0 + row_span * 0.52,
                y,
            );
            cr.curve_to(
                x0 + row_span * 0.68,
                y + amp * 0.95,
                x0 + row_span * 0.84,
                y + amp * 0.55,
                x0 + row_span,
                y - amp * 0.35,
            );
            let _ = cr.stroke();
        }
        let _ = cr.restore();
        crate::gtkx::inh(false)
    });
    area
}

fn left_label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    crate::ui::class(&label, class);
    label
}

fn pill_label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_line_wrap(false);
    crate::ui::class(&label, class);
    label
}

fn group_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_halign(gtk::Align::Start);
    label.set_margin_start(6);
    crate::ui::class(&label, "welcome-group-label");
    label
}

/// Pure-Cairo pill toggle styled in Rill's terracotta/paper/charcoal palette.
/// Uses `GtkToggleButton` + persistent `DrawingArea` instead of `GtkSwitch` so
/// GTK3 never loads themed SVG assets via `glycin-image-rs` / `bwrap` helpers.
fn welcome_pill_switch(value: bool, initial_dark: bool) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::new();
    crate::ui::class(&button, "mark");
    crate::ui::class(&button, "pill-switch");
    crate::ui::class(&button, "welcome-switch");
    button.set_relief(gtk::ReliefStyle::None);
    button.set_can_focus(true);
    button.set_valign(gtk::Align::Center);
    button.set_active(value);

    let canvas = welcome_drawing_area();
    canvas.set_size_request(44, 24);
    canvas.connect_style_updated(|w| w.queue_draw());
    let btn_weak = button.downgrade();
    canvas.connect_draw(move |widget, cr| {
        let active = btn_weak.upgrade().is_some_and(|b| b.is_active());
        let dark = crate::startpage::widget_is_dark(widget, initial_dark);
        let w = f64::from(widget.allocated_width()).max(44.0);
        let h = f64::from(widget.allocated_height()).max(24.0);
        let x0 = ((w - 44.0) * 0.5).max(0.0);
        let y0 = ((h - 24.0) * 0.5).max(0.0);
        let r = 12.0;

        // Pill track.
        cr.new_sub_path();
        cr.arc(
            x0 + 44.0 - r,
            y0 + r,
            r,
            -std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_2,
        );
        cr.arc(
            x0 + r,
            y0 + r,
            r,
            std::f64::consts::FRAC_PI_2,
            3.0 * std::f64::consts::FRAC_PI_2,
        );
        cr.close_path();
        if active {
            if dark {
                cr.set_source_rgb(246.0 / 255.0, 146.0 / 255.0, 104.0 / 255.0);
            } else {
                cr.set_source_rgb(183.0 / 255.0, 69.0 / 255.0, 37.0 / 255.0);
            }
            let _ = cr.fill();
        } else {
            if dark {
                cr.set_source_rgb(110.0 / 255.0, 105.0 / 255.0, 98.0 / 255.0);
            } else {
                cr.set_source_rgb(129.0 / 255.0, 129.0 / 255.0, 119.0 / 255.0);
            }
            let _ = cr.fill();
        }

        // Circular knob.
        let knob_x = if active { x0 + 32.0 } else { x0 + 12.0 };
        let knob_y = y0 + 12.0;
        cr.arc(knob_x, knob_y + 0.8, 8.5, 0.0, 2.0 * std::f64::consts::PI);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.16);
        let _ = cr.fill();

        cr.arc(knob_x, knob_y, 8.5, 0.0, 2.0 * std::f64::consts::PI);
        if active && dark {
            cr.set_source_rgb(38.0 / 255.0, 29.0 / 255.0, 22.0 / 255.0);
        } else if dark {
            cr.set_source_rgb(239.0 / 255.0, 236.0 / 255.0, 230.0 / 255.0);
        } else {
            cr.set_source_rgb(1.0, 1.0, 1.0);
        }
        let _ = cr.fill();
        crate::gtkx::inh(false)
    });

    let canvas_weak = canvas.downgrade();
    button.connect_toggled(move |_| {
        if let Some(c) = canvas_weak.upgrade() {
            c.queue_draw();
        }
    });

    button.add(&canvas);
    button
}

fn pill_action_button(
    text: &str,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    classes: &[&str],
) -> gtk::Button {
    let btn = gtk::Button::new();
    for c in classes {
        crate::ui::class(&btn, c);
    }
    btn.set_relief(gtk::ReliefStyle::None);
    let inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    inner.set_halign(gtk::Align::Center);
    inner.set_valign(gtk::Align::Center);

    let fallback_color = if classes.contains(&"welcome-btn-primary") {
        Rgb(255, 255, 255)
    } else {
        Rgb(39, 39, 36)
    };
    if let Some(icon_kind) = leading {
        let ic = crate::ui::passthrough_icon(icon_kind, 14, fallback_color);
        crate::ui::class(&ic, "welcome-btn-icon");
        ic.set_valign(gtk::Align::Center);
        inner.pack_start(&ic, false, false, 0);
    }
    let lbl = pill_label(text, "welcome-btn-label");
    lbl.set_valign(gtk::Align::Center);
    inner.pack_start(&lbl, false, false, 0);
    if let Some(icon_kind) = trailing {
        let ic = crate::ui::passthrough_icon(icon_kind, 14, fallback_color);
        crate::ui::class(&ic, "welcome-btn-icon");
        ic.set_valign(gtk::Align::Center);
        inner.pack_start(&ic, false, false, 0);
    }
    btn.add(&inner);
    if let Some(access) = btn.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, text);
    }
    btn
}

fn appearance_swatch(swatch_class: &str) -> gtk::Box {
    let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    holder.set_size_request(28, 28);
    holder.set_valign(gtk::Align::Center);
    holder.set_halign(gtk::Align::Center);

    let kind = match swatch_class {
        "swatch-dark" => WelcomeMark::AppearanceDark,
        "swatch-system" => WelcomeMark::AppearanceSystem,
        _ => WelcomeMark::AppearanceLight,
    };
    let mark = welcome_mark(kind, false);
    holder.pack_start(&mark, true, true, 0);
    holder
}

fn choice_button_with_leading(
    title: &str,
    subtitle: &str,
    leading: Option<gtk::Widget>,
    chosen: bool,
) -> gtk::Button {
    let btn = gtk::Button::new();
    crate::ui::class(&btn, "welcome-choice");
    btn.set_relief(gtk::ReliefStyle::None);
    set_chosen(&btn, chosen);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 11);
    row.set_valign(gtk::Align::Center);
    if let Some(widget) = leading {
        row.pack_start(&widget, false, false, 0);
    }
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    let t = gtk::Label::new(Some(title));
    t.set_xalign(0.0);
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    crate::ui::class(&t, "welcome-choice-title");
    text.pack_start(&t, false, false, 0);
    if !subtitle.is_empty() {
        let s = gtk::Label::new(Some(subtitle));
        s.set_xalign(0.0);
        s.set_ellipsize(gtk::pango::EllipsizeMode::End);
        crate::ui::class(&s, "welcome-choice-sub");
        text.pack_start(&s, false, false, 0);
    }
    row.pack_start(&text, true, true, 0);
    btn.add(&row);

    if let Some(access) = btn.accessible() {
        let full = if subtitle.is_empty() {
            title.to_string()
        } else {
            format!("{title} — {subtitle}")
        };
        gtk::atk::prelude::AtkObjectExt::set_name(&access, &full);
    }
    btn
}

fn set_chosen(btn: &gtk::Button, chosen: bool) {
    set_class(btn, "chosen", chosen);
}

#[derive(Clone, Copy)]
enum WelcomeMark {
    AppearanceLight,
    AppearanceDark,
    AppearanceSystem,
    PrivacyShield,
    LayoutTop,
    LayoutSide,
    Startpage,
    Ecosia,
    DuckDuckGo,
    Google,
    Bing,
    Brave,
    Wikipedia,
    GitHub,
    YouTube,
    Mdn,
}

fn welcome_drawing_area() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.connect_realize(|widget| {
        if let Some(window) = widget.window() {
            window.set_pass_through(true);
        }
    });
    area
}

fn welcome_mark(kind: WelcomeMark, dark: bool) -> gtk::DrawingArea {
    let area = welcome_drawing_area();
    area.set_size_request(28, 28);
    area.set_valign(gtk::Align::Center);
    area.set_halign(gtk::Align::Center);
    area.connect_style_updated(|w| w.queue_draw());
    area.connect_draw(move |widget, cr| {
        let dark = crate::startpage::widget_is_dark(widget, dark);
        draw_welcome_mark(cr, kind, dark);
        crate::gtkx::inh(false)
    });
    area
}

fn draw_welcome_mark(cr: &cairo::Context, kind: WelcomeMark, dark: bool) {
    let w = 28.0;
    let h = 28.0;
    let ink = if dark { 0.94 } else { 0.15 };
    let accent = if dark {
        (0.97, 0.57, 0.41)
    } else {
        (0.72, 0.27, 0.15)
    };

    match kind {
        WelcomeMark::AppearanceLight => {
            cr.set_source_rgb(0.99, 0.98, 0.94);
            cr.rectangle(3.5, 4.5, 21.0, 19.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.72, 0.58, 0.18);
            cr.arc(19.0, 10.0, 3.0, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.90, 0.89, 0.85);
            cr.rectangle(3.5, 17.0, 21.0, 6.5);
            let _ = cr.fill();
            cr.set_source_rgb(0.72, 0.72, 0.68);
            cr.set_line_width(1.0);
            cr.rectangle(3.5, 4.5, 21.0, 19.0);
            let _ = cr.stroke();
        }
        WelcomeMark::AppearanceDark => {
            cr.set_source_rgb(0.12, 0.12, 0.15);
            rounded_rect(cr, 3.5, 4.5, 21.0, 19.0, 3.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.91, 0.91, 0.86);
            cr.arc(10.0, 10.0, 2.6, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.22, 0.26, 0.35);
            rounded_rect(cr, 13.0, 13.0, 8.0, 6.0, 1.5);
            let _ = cr.fill();
        }
        WelcomeMark::AppearanceSystem => {
            cr.set_source_rgb(0.98, 0.97, 0.93);
            cr.rectangle(3.5, 4.5, 10.5, 19.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.16, 0.16, 0.19);
            cr.rectangle(14.0, 4.5, 10.5, 19.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.90, 0.72, 0.26);
            cr.arc(9.0, 10.0, 2.6, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.93, 0.93, 0.88);
            cr.arc(19.0, 10.0, 2.6, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
        WelcomeMark::PrivacyShield => {
            cr.set_source_rgb(0.16, 0.52, 0.40);
            cr.move_to(14.0, 3.0);
            cr.curve_to(17.0, 5.0, 20.0, 6.0, 23.0, 6.5);
            cr.line_to(22.0, 15.0);
            cr.curve_to(21.5, 19.0, 17.0, 23.0, 14.0, 25.0);
            cr.curve_to(11.0, 23.0, 6.5, 19.0, 6.0, 15.0);
            cr.line_to(5.0, 6.5);
            cr.curve_to(8.0, 6.0, 11.0, 5.0, 14.0, 3.0);
            cr.close_path();
            let _ = cr.fill();
            cr.set_source_rgb(0.98, 0.99, 0.95);
            cr.set_line_width(2.0);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.move_to(9.5, 13.5);
            cr.line_to(12.5, 16.5);
            cr.line_to(18.5, 10.0);
            let _ = cr.stroke();
        }
        WelcomeMark::LayoutTop => {
            cr.set_source_rgb(0.74, 0.76, 0.79);
            rounded_rect(cr, 3.0, 5.0, 22.0, 18.0, 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.96, 0.96, 0.93);
            cr.rectangle(4.0, 7.0, 8.0, 4.0);
            let _ = cr.fill();
            cr.set_source_rgb(accent.0, accent.1, accent.2);
            cr.rectangle(12.5, 7.0, 8.0, 4.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.96, 0.96, 0.93);
            cr.rectangle(4.0, 12.5, 20.0, 9.0);
            let _ = cr.fill();
        }
        WelcomeMark::LayoutSide => {
            cr.set_source_rgb(0.74, 0.76, 0.79);
            rounded_rect(cr, 3.0, 5.0, 22.0, 18.0, 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.96, 0.96, 0.93);
            cr.rectangle(4.0, 7.0, 5.5, 14.0);
            let _ = cr.fill();
            cr.set_source_rgb(accent.0, accent.1, accent.2);
            cr.rectangle(4.5, 8.0, 4.5, 3.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.96, 0.96, 0.93);
            cr.rectangle(10.5, 7.0, 13.5, 14.0);
            let _ = cr.fill();
        }
        WelcomeMark::Startpage => {
            cr.set_source_rgb(0.08, 0.45, 0.62);
            rounded_rect(cr, 3.5, 4.5, 21.0, 19.0, 5.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.96, 0.98, 0.97);
            cr.set_line_width(2.0);
            cr.arc(14.0, 14.0, 5.5, 0.35, std::f64::consts::PI * 1.75);
            let _ = cr.stroke();
            cr.set_source_rgb(0.45, 0.86, 0.77);
            cr.arc(14.0, 14.0, 2.6, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
        WelcomeMark::Ecosia => {
            cr.set_source_rgb(0.17, 0.50, 0.24);
            rounded_rect(cr, 3.5, 4.5, 21.0, 19.0, 5.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.98, 0.88, 0.33);
            cr.arc(11.5, 11.0, 4.0, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.46, 0.76, 0.36);
            cr.arc(17.5, 15.0, 4.5, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
        WelcomeMark::DuckDuckGo => {
            cr.set_source_rgb(0.98, 0.59, 0.17);
            cr.arc(13.0, 12.0, 7.0, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.98, 0.59, 0.17);
            cr.arc(15.0, 18.0, 5.0, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 1.0, 0.98);
            cr.arc(13.0, 9.5, 2.1, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.18, 0.18, 0.20);
            cr.arc(13.7, 9.5, 0.9, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.94, 0.29, 0.20);
            cr.move_to(18.0, 12.5);
            cr.line_to(25.0, 15.0);
            cr.line_to(18.0, 17.0);
            cr.close_path();
            let _ = cr.fill();
        }
        WelcomeMark::Google => {
            cr.set_line_width(3.6);
            cr.set_line_cap(cairo::LineCap::Butt);
            cr.set_source_rgb(0.18, 0.65, 0.33);
            cr.arc(14.0, 14.0, 8.0, 1.75, 3.15);
            let _ = cr.stroke();
            cr.set_source_rgb(0.98, 0.75, 0.02);
            cr.arc(14.0, 14.0, 8.0, 3.15, 4.70);
            let _ = cr.stroke();
            cr.set_source_rgb(0.93, 0.26, 0.21);
            cr.arc(14.0, 14.0, 8.0, 4.70, 6.18);
            let _ = cr.stroke();
            cr.set_source_rgb(0.24, 0.53, 0.94);
            cr.arc(14.0, 14.0, 8.0, 6.18, 7.85);
            let _ = cr.stroke();
            cr.set_source_rgb(0.24, 0.53, 0.94);
            cr.rectangle(13.5, 12.4, 9.0, 3.4);
            let _ = cr.fill();
        }
        WelcomeMark::Bing => {
            cr.set_source_rgb(0.04, 0.58, 0.66);
            cr.move_to(8.0, 4.5);
            cr.line_to(17.5, 8.0);
            cr.line_to(13.5, 11.0);
            cr.line_to(18.5, 23.5);
            cr.line_to(13.0, 22.0);
            cr.line_to(8.0, 9.5);
            cr.close_path();
            let _ = cr.fill();
            cr.set_source_rgb(0.33, 0.86, 0.85);
            cr.move_to(18.5, 5.5);
            cr.line_to(22.5, 13.0);
            cr.line_to(17.5, 12.5);
            cr.line_to(14.0, 8.0);
            cr.close_path();
            let _ = cr.fill();
        }
        WelcomeMark::Brave => {
            cr.set_source_rgb(0.97, 0.36, 0.18);
            cr.move_to(14.0, 3.5);
            cr.line_to(23.0, 8.0);
            cr.line_to(20.5, 20.0);
            cr.line_to(14.0, 24.5);
            cr.line_to(7.5, 20.0);
            cr.line_to(5.0, 8.0);
            cr.close_path();
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 0.95, 0.89);
            cr.move_to(9.5, 11.5);
            cr.line_to(14.0, 15.0);
            cr.line_to(18.5, 11.5);
            cr.line_to(17.0, 19.0);
            cr.line_to(14.0, 21.0);
            cr.line_to(11.0, 19.0);
            cr.close_path();
            let _ = cr.fill();
        }
        WelcomeMark::Wikipedia => {
            cr.set_source_rgb(0.95, 0.95, 0.93);
            rounded_rect(cr, 4.0, 5.0, 20.0, 18.0, 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.13, 0.13, 0.15);
            cr.set_font_size(11.0);
            cr.move_to(8.0, 18.5);
            let _ = cr.show_text("W");
            cr.set_source_rgb(0.72, 0.27, 0.15);
            cr.set_line_width(1.5);
            cr.move_to(8.0, 20.5);
            cr.line_to(20.0, 20.5);
            let _ = cr.stroke();
        }
        WelcomeMark::GitHub => {
            cr.set_source_rgb(ink, ink, ink);
            cr.arc(14.0, 15.0, 8.0, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.move_to(7.5, 11.0);
            cr.line_to(8.0, 4.5);
            cr.line_to(13.0, 8.0);
            cr.line_to(15.0, 8.0);
            cr.line_to(20.0, 4.5);
            cr.line_to(20.5, 11.0);
            cr.close_path();
            let _ = cr.fill();
            let detail = if dark { 0.12 } else { 0.98 };
            cr.set_source_rgb(detail, detail, detail);
            cr.arc(11.0, 14.0, 1.0, 0.0, std::f64::consts::PI * 2.0);
            cr.arc(17.0, 14.0, 1.0, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
            cr.set_line_width(1.8);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.move_to(14.0, 18.0);
            cr.line_to(14.0, 22.0);
            let _ = cr.stroke();
        }
        WelcomeMark::YouTube => {
            cr.set_source_rgb(0.91, 0.12, 0.13);
            rounded_rect(cr, 3.0, 6.0, 22.0, 16.0, 5.0);
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(12.0, 10.0);
            cr.line_to(12.0, 18.0);
            cr.line_to(19.0, 14.0);
            cr.close_path();
            let _ = cr.fill();
        }
        WelcomeMark::Mdn => {
            cr.set_source_rgb(0.08, 0.10, 0.16);
            rounded_rect(cr, 3.0, 6.0, 22.0, 16.0, 2.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.96, 0.48, 0.20);
            cr.move_to(6.0, 18.5);
            cr.line_to(6.0, 10.0);
            cr.line_to(10.0, 14.0);
            cr.line_to(14.0, 10.0);
            cr.line_to(14.0, 18.5);
            cr.line_to(11.8, 18.5);
            cr.line_to(11.8, 14.8);
            cr.line_to(10.0, 16.8);
            cr.line_to(8.2, 14.8);
            cr.line_to(8.2, 18.5);
            cr.close_path();
            let _ = cr.fill();
            cr.set_source_rgb(0.35, 0.73, 0.98);
            cr.set_line_width(1.8);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.move_to(17.0, 10.5);
            cr.curve_to(14.5, 11.0, 14.5, 17.5, 17.0, 18.0);
            cr.move_to(17.0, 10.5);
            cr.line_to(21.0, 10.5);
            cr.line_to(21.0, 18.0);
            cr.line_to(17.0, 18.0);
            cr.curve_to(14.5, 17.5, 14.5, 11.0, 17.0, 10.5);
            let _ = cr.stroke();
        }
    }

    if matches!(
        kind,
        WelcomeMark::AppearanceLight
            | WelcomeMark::AppearanceDark
            | WelcomeMark::AppearanceSystem
            | WelcomeMark::LayoutTop
            | WelcomeMark::LayoutSide
    ) {
        cr.set_source_rgba(ink, ink, ink, 0.10);
        cr.set_line_width(1.0);
        rounded_rect(cr, 2.5, 4.0, w - 5.0, h - 8.0, 3.0);
        let _ = cr.stroke();
    }
}

fn set_class(widget: &impl IsA<gtk::Widget>, name: &str, active: bool) {
    let ctx = widget.style_context();
    if active {
        ctx.add_class(name);
    } else {
        ctx.remove_class(name);
    }
}

/// Swap only the non-focusable icon container so focus on the shortcut button
/// is never disturbed when an opt-in official favicon finishes downloading.
fn set_site_favicon(holder: &gtk::Box, surface: &cairo::Surface) {
    for child in holder.children() {
        holder.remove(&child);
    }
    let img = crate::ui::favicon(surface, 16);
    crate::ui::class(&img, "favicon");
    img.set_valign(gtk::Align::Center);
    img.set_halign(gtk::Align::Center);
    holder.pack_start(&img, false, false, 0);
    img.show();
}

fn import_row(app: &AppRef, browser: crate::import::Browser, status: &gtk::Label) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    crate::ui::class(&row, "welcome-browser");
    let icon = crate::ui::passthrough_icon(Icon::Download, 14, Rgb(89, 90, 82));
    crate::ui::class(&icon, "welcome-browser-icon");
    row.pack_start(&icon, false, false, 0);
    let label = gtk::Label::new(Some(browser.name()));
    label.set_xalign(0.0);
    crate::ui::class(&label, "welcome-browser-name");
    row.pack_start(&label, true, true, 0);

    let button = gtk::Button::with_label("Import");
    crate::ui::class(&button, "welcome-btn-secondary");
    crate::ui::class(&button, "welcome-import-btn");
    let app_weak = app.weak();
    let status_weak = status.downgrade();
    let btn_weak = button.downgrade();
    button.connect_clicked(move |_| {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        let Some(status) = status_weak.upgrade() else {
            return;
        };
        let Some(button) = btn_weak.upgrade() else {
            return;
        };
        button.set_sensitive(false);
        status.set_text("Reading bookmarks and history…");
        status.show();
        let status_weak = status.downgrade();
        let button_weak = button.downgrade();
        crate::import::bring_later_checked(&app, vec![browser], move |ok, message| {
            if let Some(status) = status_weak.upgrade() {
                status.set_text(&message);
                status.show();
            }
            if let Some(button) = button_weak.upgrade() {
                if ok {
                    button.set_label("Imported");
                } else {
                    button.set_sensitive(true);
                }
            }
        });
    });
    row.pack_end(&button, false, false, 0);
    row
}

/// Strictly opt-in official favicon fetcher. Only ever invoked after the user
/// clicks a starter shortcut; rejects non-HTTPS URLs, redirects, oversized
/// payloads, and non-PNG/ICO image bytes.
fn fetch_official_favicon<F>(
    url: &'static str,
    cancellable: Option<gtk::gio::Cancellable>,
    on_done: F,
) where
    F: FnOnce(Option<cairo::ImageSurface>) + 'static,
{
    if !url.starts_with("https://") {
        on_done(None);
        return;
    }
    let Ok(msg) = soup::Message::new("GET", url) else {
        on_done(None);
        return;
    };
    msg.set_flags(soup::MessageFlags::NO_REDIRECT);
    let session = FAVICON_SESSION.with(|s| s.clone());
    session.set_timeout(FAVICON_TIMEOUT_SECS);
    session.send_and_read_async(
        &msg,
        glib::Priority::LOW,
        cancellable.as_ref(),
        move |res| {
            let surface = res.ok().and_then(|bytes| {
                let slice = bytes.as_ref();
                if slice.is_empty() || slice.len() > FAVICON_MAX_BYTES {
                    return None;
                }
                decode_png_surface(slice)
            });
            on_done(surface);
        },
    );
}

fn decode_png_surface(bytes: &[u8]) -> Option<cairo::ImageSurface> {
    const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.starts_with(&PNG_SIG) {
        let mut cursor = std::io::Cursor::new(bytes);
        let surf = cairo::ImageSurface::create_from_png(&mut cursor).ok()?;
        if surf.width() > 0 && surf.height() > 0 && surf.width() <= 512 && surf.height() <= 512 {
            return Some(surf);
        }
        return None;
    }
    if bytes.len() >= 22 && bytes[0..4] == [0, 0, 1, 0] {
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        for i in 0..count.min(16) {
            let off = 6 + i * 16;
            if off + 16 > bytes.len() {
                break;
            }
            let size = u32::from_le_bytes([
                bytes[off + 8],
                bytes[off + 9],
                bytes[off + 10],
                bytes[off + 11],
            ]) as usize;
            let data_off = u32::from_le_bytes([
                bytes[off + 12],
                bytes[off + 13],
                bytes[off + 14],
                bytes[off + 15],
            ]) as usize;
            if data_off + size <= bytes.len()
                && size >= 8
                && bytes[data_off..data_off + 8] == PNG_SIG
            {
                let mut cursor = std::io::Cursor::new(&bytes[data_off..data_off + size]);
                if let Ok(surf) = cairo::ImageSurface::create_from_png(&mut cursor) {
                    if surf.width() > 0 && surf.height() > 0 {
                        return Some(surf);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> PrefsSnapshot {
        PrefsSnapshot {
            appearance: Look::System,
            sidebar: false,
            column_folded: false,
            search_engine: "Startpage".to_string(),
            engine_suggestions: true,
        }
    }

    #[test]
    fn step_navigation_and_bounds_work() {
        let mut draft = WizardDraft::from_snapshot(sample_snapshot());
        assert_eq!(draft.step, 0);
        assert_eq!(step_child_name(draft.step), "step-0");
        assert!(!draft.prev_step());
        assert!(draft.next_step());
        assert_eq!(draft.step, 1);
        assert!(draft.next_step());
        assert_eq!(draft.step, 2);
        assert!(draft.next_step());
        assert_eq!(draft.step, 3);
        assert!(!draft.next_step());
        assert!(draft.prev_step());
        assert_eq!(draft.step, 2);
        draft.set_step(99);
        assert_eq!(draft.step, STEP_COUNT - 1);
    }

    #[test]
    fn starter_shortcuts_default_unselected_and_https_only() {
        let draft = WizardDraft::from_snapshot(sample_snapshot());
        assert_eq!(draft.selected_sites, [false, false, false, false]);
        for site in STARTER_SITES {
            assert!(site.url.starts_with("https://"));
            assert!(site.icon_url.starts_with("https://"));
        }
    }

    #[test]
    fn draft_preserves_initial_snapshot_for_revert() {
        let mut draft = WizardDraft::from_snapshot(sample_snapshot());
        draft.appearance = Look::Dark;
        draft.touched_appearance = true;
        draft.sidebar = true;
        draft.touched_sidebar = true;
        draft.search_engine = "DuckDuckGo".to_string();
        draft.touched_engine = true;
        draft.engine_suggestions = false;
        draft.touched_suggestions = true;
        assert_eq!(draft.initial.appearance, Look::System);
        assert!(!draft.initial.sidebar);
        assert_eq!(draft.initial.search_engine, "Startpage");
        assert!(draft.initial.engine_suggestions);
    }

    #[test]
    fn decode_png_surface_accepts_valid_png_and_rejects_junk() {
        assert!(decode_png_surface(b"not an image").is_none());
        assert!(decode_png_surface(&[]).is_none());
        let src = cairo::ImageSurface::create(cairo::Format::ARgb32, 16, 16).unwrap();
        let mut buf = Vec::new();
        src.write_to_png(&mut buf).unwrap();
        let decoded = decode_png_surface(&buf).expect("valid PNG decodes cleanly");
        assert_eq!(decoded.width(), 16);
        assert_eq!(decoded.height(), 16);
    }

    #[test]
    fn welcome_canvas_builds_cleanly_in_light_and_dark() {
        for dark in [false, true] {
            let surf = build_welcome_canvas(900, 560, dark).expect("canvas surface builds");
            assert_eq!(surf.width(), 900);
            assert_eq!(surf.height(), 560);
        }
    }
}

// Peek: shift-click a link and read it in a card over the page. Keep turns it
// into a tab; Close, Escape or leaving the tab drops it. One peek at a time.
//
// A peek is not a tab yet, so its view is built without the tab's callbacks
// (attach binds them to a tab id). It can load and show a page, and it
// refuses what a page may not do unasked: permissions, sign-in, dialogs,
// windows and schemes the browser does not handle. Keep moves the peeked view
// itself into a new tab: its refusals are disconnected, the tab adopts it and
// the tab's callbacks are attached, so nothing loads twice and what was typed
// or scrolled in the card stays.
use crate::app::{App, AppRef, TabId};
use crate::web;
use gtk::glib::SignalHandlerId;
use gtk::prelude::*;
use gtk::gdk::keys::constants::Escape;
use std::cell::RefCell;
use webkit2gtk::{
    AuthenticationRequestExt, ContextMenuAction, ContextMenuExt, ContextMenuItemExt,
    NavigationPolicyDecisionExt, PermissionRequestExt, PolicyDecisionExt, URIRequestExt, WebView,
    WebViewExt,
};

const PEEK_VIEW: &str = "peek-view";
const PEEK_CARD: &str = "peek-card";
const PRIMARY_BUTTON: u32 = 1;

/// Shift is held in a navigation's modifier bits (GdkModifierType::SHIFT_MASK).
pub fn shift_held(modifiers: u32) -> bool {
    modifiers & gtk::gdk::ModifierType::SHIFT_MASK.bits() != 0
}

/// A click that peeks: shift, the primary button, a link, and not from inside
/// a peek (a peek's own links navigate its own card as usual).
pub fn is_peek_click(shift: bool, button: u32, link: bool, in_peek: bool) -> bool {
    shift && button == PRIMARY_BUTTON && link && !in_peek
}

pub fn is_peek_view(view: &WebView) -> bool {
    view.widget_name() == PEEK_VIEW
}

/// Keep makes a tab with the flag of the tab the peek came from. A tab that
/// cannot be found counts as private, the same safe default the rest uses.
pub fn keep_private(source: Option<bool>) -> bool {
    source.unwrap_or(true)
}

/// Escape closes a peek only when neither the field, the find bar, nor a panel took it.
pub fn escape_closes(field_open: bool, find_open: bool, panel_open: bool, showing: bool) -> bool {
    crate::keys::escape_reaches_cards(field_open, find_open, panel_open) && showing
}

/// The card is about 80% of the page, with a floor so it stays usable.
pub fn card_size(width: i32, height: i32) -> (i32, i32) {
    ((width * 4 / 5).max(240), (height * 4 / 5).max(160))
}

/// The room left around the card, as (side, top, bottom) margins.
///
/// A size request is only a minimum: the page view inside the card expands,
/// so a centred card with a size request grew to the whole overlay, its top
/// landed under the window's band and Keep and Close were cut in half.
/// Margins hold it in place instead. The top margin starts below the band.
/// `page_left` is where the page begins in the overlay: the tab column's
/// width, the folded strip, or 0 in fullscreen. Centred on the whole window,
/// round 43's card started at x=150 and covered the column's tab titles.
/// Returns (start, end, top, bottom).
pub fn card_margins(width: i32, height: i32, band: i32, page_left: i32) -> (i32, i32, i32, i32) {
    let band = band.max(0);
    let page_left = page_left.clamp(0, width.max(0));
    let page_width = (width - page_left).max(0);
    let (card_width, card_height) = card_size(page_width, (height - band).max(0));
    let side = ((page_width - card_width) / 2).max(0);
    let spare = ((height - band) - card_height).max(0);
    let top = band + spare / 2;
    let bottom = spare - spare / 2;
    (page_left + side, side, top, bottom)
}

struct Peek {
    tab: TabId,
    private: bool,
    card: gtk::Box,
    scrim: gtk::DrawingArea,
    view: WebView,
    /// The peek's own refusal handlers, disconnected when Keep makes it a tab.
    handlers: Vec<SignalHandlerId>,
}

thread_local! {
    static PEEK: RefCell<Option<Peek>> = const { RefCell::new(None) };
}

fn take() -> Option<Peek> {
    PEEK.with(|p| p.borrow_mut().take())
}

pub(crate) fn showing() -> bool {
    PEEK.with(|p| p.borrow().as_ref().is_some_and(|peek| peek.card.is_visible()))
}

fn page_left(app: &App) -> i32 {
    if app.ui.side.is_visible() && app.ui.side.is_child_visible() {
        app.ui.side.allocated_width().max(0)
    } else {
        0
    }
}

pub fn relayout(app: &App) {
    PEEK.with(|p| {
        let borrow = p.borrow();
        let Some(peek) = borrow.as_ref() else { return };
        // The band is where the page starts under the toolbar (the same one
        // Find uses). With 0 the card was centred on the whole overlay, so on a
        // short window its top could slide up under the toolbar (Gemini R59
        // fix 6). toolbar_band is 0 in fullscreen, so the card still uses the
        // full height there.
        let (start, end, top, bottom) = card_margins(
            app.ui.overlay.allocated_width(),
            app.ui.overlay.allocated_height(),
            crate::find::toolbar_band(&app.ui),
            page_left(app),
        );
        peek.card.set_margin_start(start);
        peek.card.set_margin_end(end);
        peek.card.set_margin_top(top);
        peek.card.set_margin_bottom(bottom);
    });
}

/// Takes the peek down: the card leaves the page and its view is destroyed.
fn remove(app: &App, peek: Peek) -> String {
    crate::motion::cancel(&peek.card);
    app.ui.overlay.remove(&peek.card);
    app.ui.overlay.remove(&peek.scrim);
    let uri = peek.view.uri().map(|u| u.to_string()).unwrap_or_default();
    // SAFETY: the view has just left its container and nothing else holds it
    // beyond this scope. close_tab destroys its views the same way.
    unsafe { peek.view.destroy() };
    uri
}

/// Drops the peek of one tab, when that tab closes.
pub fn clear_for(app: &App, tab: TabId) {
    if PEEK.with(|p| p.borrow().as_ref().map(|peek| peek.tab) == Some(tab)) {
        if let Some(peek) = take() {
            remove(app, peek);
        }
    }
}

/// Drops whatever peek is open, on a tab switch.
pub fn clear(app: &App) {
    if let Some(peek) = take() {
        remove(app, peek);
    }
}

pub fn open(app: &App, tab: TabId, opener: &WebView, url: &str) {
    if app.active_id() != tab {
        return;
    }
    if !web::scheme_is_allowed(url, false) {
        app.flash_toast("This link asks for an address the browser cannot open.");
        return;
    }
    clear(app);
    let private = keep_private(app.state.borrow().tab(tab).map(|t| t.private));
    let mut builder = WebView::builder().settings(&web::settings());
    if let Some(context) = opener.context() {
        builder = builder.web_context(&context);
    }
    if let Some(manager) = opener.user_content_manager() {
        builder = builder.user_content_manager(&manager);
    }
    let view = builder.build();
    view.set_widget_name(PEEK_VIEW);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_margin_start(1);
    view.set_margin_end(1);
    view.set_margin_bottom(4);
    let page = if app.prefs.borrow().darken_pages { (0.043, 0.043, 0.043) } else { (1.0, 1.0, 1.0) };
    view.set_background_color(&gtk::gdk::RGBA::new(page.0, page.1, page.2, 1.0));
    app.tone_for(&view);
    let handlers = connect_refusals(&view);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.set_widget_name(PEEK_CARD);
    crate::ui::class(&card, "dlbar");
    if let Some(access) = card.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Peek preview");
    }
    card.set_halign(gtk::Align::Fill);
    card.set_valign(gtk::Align::Fill);
    // Where the opener's page starts in the overlay, so the card sits over
    // the page and never over the tab column (or the folded strip).
    let opener_left = opener
        .translate_coordinates(&app.ui.overlay, 0, 0)
        .map(|(x, _)| x)
        .unwrap_or_else(|| page_left(app));
    // Start below the toolbar, as relayout does, so the first frame and every
    // later relayout agree and the card does not jump when the window resizes.
    let (start, end, top, bottom) = card_margins(
        app.ui.overlay.allocated_width(),
        app.ui.overlay.allocated_height(),
        crate::find::toolbar_band(&app.ui),
        opener_left,
    );
    card.set_margin_start(start);
    card.set_margin_end(end);
    card.set_margin_top(top);
    card.set_margin_bottom(bottom);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.set_margin_start(12);
    header.set_margin_end(8);
    header.set_margin_top(6);
    header.set_margin_bottom(6);
    let host = gtk::Label::new(Some(&crate::shield::host_of(url)));
    host.set_xalign(0.0);
    host.set_hexpand(true);
    host.set_ellipsize(gtk::pango::EllipsizeMode::End);
    crate::ui::pack(&header, &host, true);
    let keep = gtk::Button::with_label("Keep");
    crate::ui::class(&keep, "act");
    crate::ui::pack(&header, &keep, false);
    let close = gtk::Button::with_label("Close");
    crate::ui::class(&close, "act");
    crate::ui::pack(&header, &close, false);
    crate::ui::pack(&card, &header, false);
    crate::ui::pack(&card, &view, true);

    let weak = app.weak();
    keep.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            keep_peek(&app);
        }
    });
    let weak = app.weak();
    close.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            dismiss(&app);
        }
    });

    // A peek is a preview over the page, not another floating page. A dimmed
    // backdrop separates the card from the article and gives Close a target.
    let scrim = crate::ui::scrim(&app.palette.borrow());
    scrim.set_widget_name("peek-scrim");
    app.ui.overlay.add_overlay(&scrim);
    scrim.set_hexpand(true);
    scrim.set_vexpand(true);
    app.ui.overlay.add_overlay(&card);
    card.show_all();
    scrim.show_all();
    // The peek card arrives on the shared entrance role (180ms) and travels
    // the shared 8px, the same as every other card, instead of its own
    // 18px and 190ms.
    crate::motion::pop_card_in(&card, top, crate::motion::dur::TRAVEL, crate::motion::dur::ENTER);
    keep.grab_focus();
    view.load_uri(url);
    PEEK.with(|p| {
        *p.borrow_mut() = Some(Peek { tab, private, card, scrim, view, handlers });
    });
}

/// Refusals the peek makes on its own: no permission, no sign-in, no dialog,
/// no window, and no scheme the browser does not open.
fn connect_refusals(view: &WebView) -> Vec<SignalHandlerId> {
    let mut handlers = Vec::new();
    handlers.push(view.connect_permission_request(|_, request| {
        request.deny();
        true
    }));
    handlers.push(view.connect_authenticate(|_, request| {
        request.cancel();
        true
    }));
    handlers.push(view.connect_script_dialog(|_, dialog| {
        dialog.confirm_set_confirmed(false);
        dialog.close();
        true
    }));
    handlers.push(view.connect_create(|_, _| None));
    handlers.push(view.connect_decide_policy(|_, decision, kind| {
        if kind != webkit2gtk::PolicyDecisionType::NavigationAction {
            return false;
        }
        let Some(policy) = decision.downcast_ref::<webkit2gtk::NavigationPolicyDecision>() else {
            return false;
        };
        let uri = policy.navigation_action().and_then(|action| action.request()).and_then(|r| r.uri());
        match uri {
            Some(uri) if web::scheme_is_allowed(&uri, false) => false,
            _ => {
                decision.ignore();
                true
            }
        }
    }));
    handlers.push(view.connect_context_menu(|_, menu, _, _| {
        for item in menu.items() {
            if item.stock_action() == ContextMenuAction::OpenLinkInNewWindow {
                menu.remove(&item);
            }
        }
        false
    }));
    handlers
}

fn dismiss(app: &App) {
    let Some(peek) = take() else { return };
    crate::motion::cancel(&peek.card);
    crate::permissions::focus_page(app);
    let overlay = app.ui.overlay.clone();
    let Peek { card, scrim, view, .. } = peek;
    // Retire the page now, not after the fade. A dismissed peek must not keep
    // loading, running script or holding its WebView for the EXIT fade. The
    // card keeps its current size, so the header and frame fade out without
    // collapsing around the missing page.
    view.stop_loading();
    card.set_size_request(card.allocated_width(), card.allocated_height());
    card.remove(&view);
    // SAFETY: the view has just left its container and nothing else holds it
    // beyond this scope. close_tab destroys its views the same way.
    unsafe { view.destroy() };
    drop(view);
    // The completion holds only weak handles, so a cancelled fade or another
    // teardown does not keep the card or the overlay alive.
    let card_weak = card.downgrade();
    let overlay_weak = overlay.downgrade();
    let scrim_weak = scrim.downgrade();
    crate::motion::fade_out_then(&card, crate::motion::dur::EXIT, move || {
        if let (Some(overlay), Some(card), Some(scrim)) = (
            overlay_weak.upgrade(),
            card_weak.upgrade(),
            scrim_weak.upgrade(),
        ) {
            overlay.remove(&card);
            overlay.remove(&scrim);
        }
    });
}

/// Keep moves the peeked view into a new tab with the flag the peek was made
/// with. The view leaves the card, drops the peek's refusals, is adopted (which
/// also renames it, so it is no longer a peek view) and then gets the tab's
/// callbacks. The page is not loaded again.
fn keep_peek(app: &AppRef) {
    let Some(peek) = take() else { return };
    let Peek { private, card, scrim, view, handlers, .. } = peek;
    crate::motion::cancel(&card);
    let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
    card.remove(&view);
    view.set_margin_start(0);
    view.set_margin_end(0);
    view.set_margin_bottom(0);
    app.ui.overlay.remove(&card);
    app.ui.overlay.remove(&scrim);
    for id in handlers {
        view.disconnect(id);
    }
    let adopted = if url.is_empty() { None } else { app.adopt_tab(&url, private, &view) };
    let Some(id) = adopted else {
        // SAFETY: the view has left the card and no tab took it, so nothing
        // else holds it. close_tab destroys its views the same way.
        unsafe { view.destroy() };
        crate::permissions::focus_page(app);
        return;
    };
    web::attach(app, id, &view);
    if let Some(title) = view.title() {
        app.set_title(id, &title);
    }
    // The tab callbacks are attached after the peek has already been loading,
    // so they miss any signals that fired before. Copy the view's current
    // state into the new tab: back/forward, loading and progress. If the page
    // had already finished, the Finished signal will never come again, so the
    // visit is recorded here. Otherwise a kept page would never reach history.
    let loading = view.is_loading();
    app.sync_nav(id, &view);
    app.set_loading(id, loading);
    if loading {
        app.set_load_progress(id, view.estimated_load_progress());
    } else {
        app.set_load_progress(id, 1.0);
        app.record_visit(id);
    }
    app.select(id);
}

/// Presses Keep on the open peek, the same as clicking the card's Keep button.
/// The bench socket cannot send a pointer click, so the stress suite reaches
/// Keep through this. It runs the same keep_peek the button runs. Returns false
/// when no peek is open, so a test cannot pass by pressing a button that is
/// not there.
pub fn keep(app: &AppRef) -> bool {
    if !PEEK.with(|p| p.borrow().is_some()) {
        return false;
    }
    keep_peek(app);
    true
}

/// Escape closes the peek when it is up and the field, find bar, and panels are not.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    if event.keyval() != Escape {
        return false;
    }
    let field_open = app.omni.visible.get();
    let find_open = app.find.borrow().visible;
    let panel_open = app.panels.borrow().open.is_some();
    if !escape_closes(field_open, find_open, panel_open, showing()) {
        return false;
    }
    dismiss(app);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_is_read_from_the_modifier_bits() {
        assert!(shift_held(gtk::gdk::ModifierType::SHIFT_MASK.bits()));
        assert!(shift_held(gtk::gdk::ModifierType::SHIFT_MASK.bits() | gtk::gdk::ModifierType::CONTROL_MASK.bits()));
        assert!(!shift_held(0));
        assert!(!shift_held(gtk::gdk::ModifierType::CONTROL_MASK.bits()));
    }

    #[test]
    fn only_shift_primary_link_outside_a_peek_is_a_peek() {
        assert!(is_peek_click(true, 1, true, false));
        assert!(!is_peek_click(false, 1, true, false));
        assert!(!is_peek_click(true, 2, true, false));
        assert!(!is_peek_click(true, 0, true, false));
        assert!(!is_peek_click(true, 1, false, false));
        assert!(!is_peek_click(true, 1, true, true));
    }

    #[test]
    fn keep_takes_the_flag_of_its_tab_and_a_lost_tab_is_private() {
        assert!(keep_private(Some(true)));
        assert!(!keep_private(Some(false)));
        assert!(keep_private(None));
    }

    #[test]
    fn escape_closes_a_peek_only_when_field_find_and_panels_are_closed() {
        assert!(escape_closes(false, false, false, true));
        assert!(!escape_closes(true, false, false, true));
        assert!(!escape_closes(false, true, false, true));
        assert!(!escape_closes(false, false, true, true));
        assert!(!escape_closes(false, false, false, false));
    }

    #[test]
    fn the_card_is_four_fifths_of_the_page_with_a_floor() {
        assert_eq!(card_size(1000, 800), (800, 640));
        assert_eq!(card_size(0, 0), (240, 160));
    }

    #[test]
    fn the_card_starts_below_the_band_and_keeps_its_margins() {
        // 1500x917 window with the 23px band: the page below it is 894 tall.
        // No column (fullscreen): centred on the window.
        let (start, end, top, bottom) = card_margins(1500, 917, 23, 0);
        assert_eq!((start, end), (150, 150));
        assert!(top >= 23, "the card's top {top} is inside the band");
        assert_eq!(917 - top - bottom, 894 * 4 / 5);
        // A 232px tab column: the card stays on the page, centred on it.
        let (start, end, _, _) = card_margins(1500, 917, 23, 232);
        assert!(start >= 232, "the card's left edge {start} covers the tab column");
        assert_eq!(start - 232, end);
        assert_eq!(1500 - start - end, 1268 * 4 / 5);
        // A window too small for the floor never gets negative margins.
        let (start, end, top, bottom) = card_margins(100, 100, 23, 40);
        assert!(start >= 40 && end >= 0 && bottom >= 0 && top >= 23);
    }
}

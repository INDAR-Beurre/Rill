// The right-click menu. WebKit builds its own; this edits it. Links get the
// browser's ways of opening them, a selection can be searched, and a page can
// be kept. Everything WebKit has that the browser does not replace stays.
use crate::app::{AppRef, TabId};
use crate::js;
use crate::web;
use gtk::gio::SimpleAction;
use webkit2gtk::{
    ContextMenuAction, ContextMenuExt, ContextMenuItem, ContextMenuItemExt, HitTestResultExt,
    WebView, WebViewExt,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Pick {
    LinkTab(String),
    LinkPrivate(String),
    CopyLink(String),
    Search,
    Keep,
}

/// What the menu offers for a click, in the order it is shown. A link in a
/// private tab is only ever offered as a private tab: the ordinary way is not
/// in the list at all, so nothing can open it from there.
pub fn choose(private: bool, link: Option<&str>, has_selection: bool, page: &str) -> Vec<Pick> {
    let mut picks = Vec::new();
    if let Some(link) = link.filter(|l| !l.trim().is_empty()) {
        if web::scheme_is_allowed(link, false) {
            if !private {
                picks.push(Pick::LinkTab(link.to_string()));
            }
            picks.push(Pick::LinkPrivate(link.to_string()));
        }
        picks.push(Pick::CopyLink(link.to_string()));
    }
    if has_selection {
        picks.push(Pick::Search);
    }
    if is_web_page(page) {
        picks.push(Pick::Keep);
    }
    picks
}

fn is_web_page(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// `kept` says whether the page is already among the bookmarks. The Keep item
/// runs the same toggle as Ctrl D, so on a page that is already kept it takes
/// the bookmark away, and its label has to say that instead of promising to keep.
pub fn label(pick: &Pick, engine: &str, kept: bool) -> String {
    match pick {
        Pick::LinkTab(_) => "Open link in new tab".into(),
        Pick::LinkPrivate(_) => "Open link in private tab".into(),
        Pick::CopyLink(_) => "Copy link address".into(),
        Pick::Search => format!("Search {engine} for the selection"),
        Pick::Keep if kept => "Stop keeping this page".into(),
        Pick::Keep => "Keep this page".into(),
    }
}

/// The text to search for, or nothing when the selection is only spaces.
pub fn search_text(selected: &str) -> Option<String> {
    let text = selected.trim();
    (!text.is_empty()).then(|| text.to_string())
}

pub fn attach(app: &AppRef, tab: TabId, view: &WebView) {
    let weak = app.weak();
    view.connect_context_menu(move |_, menu, _event, hit| {
        let Some(app) = weak.upgrade() else { return false };
        let (private, page) = {
            let state = app.state.borrow();
            let slot = state.tab(tab);
            (
                slot.is_some_and(|t| t.private),
                slot.map(|t| t.url.clone()).unwrap_or_default(),
            )
        };
        let link = if hit.context_is_link() {
            hit.link_uri().map(|u| u.to_string())
        } else {
            None
        };
        let picks = choose(private, link.as_deref(), hit.context_is_selection(), &page);

        // WebKit's own link items are replaced by ours, so they come out.
        for item in menu.items() {
            if matches!(
                item.stock_action(),
                ContextMenuAction::OpenLinkInNewWindow | ContextMenuAction::CopyLinkToClipboard
            ) {
                menu.remove(&item);
            }
        }

        let engine = app.prefs.borrow().search_engine.clone();
        let kept = app.state.borrow().bookmarks.iter().any(|b| b.url == page);
        let count = picks.len();
        for (i, pick) in picks.into_iter().enumerate() {
            // The action does not need to be in any group of the browser's:
            // WebKit makes its own `context-menu` action group for each menu
            // (the library imports g_simple_action_group_new,
            // g_action_map_add_action and gtk_widget_insert_action_group), adds
            // every item's GAction to it and inserts it on the menu widget, so
            // choosing the item activates this action. The `browser-` prefix
            // keeps these names apart from WebKit's own items in that group.
            // Read from the installed libwebkit2gtk-4.1 2.54.2, not yet clicked
            // through on a display.
            let action = SimpleAction::new(&format!("browser-{i}"), None);
            let weak = weak.clone();
            let chosen = pick.clone();
            action.connect_activate(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    run(&app, tab, &chosen);
                }
            });
            let item = ContextMenuItem::from_gaction(&action, &label(&pick, &engine, kept), None);
            menu.insert(&item, i as i32);
        }
        if count > 0 {
            menu.insert(&ContextMenuItem::new_separator(), count as i32);
        }
        // false: WebKit still shows the menu, now with the items above in it.
        false
    });
}

fn run(app: &AppRef, tab: TabId, pick: &Pick) {
    match pick {
        Pick::LinkTab(link) => {
            app.new_tab(Some(link), false, false);
        }
        Pick::LinkPrivate(link) => {
            app.new_tab(Some(link), true, false);
        }
        Pick::CopyLink(link) => {
            let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
            clipboard.set_text(link);
            clipboard.store();
            app.flash_toast("Link address copied");
        }
        Pick::Search => {
            // The selection is read from the page when the item is chosen, not
            // when the menu opens: the menu is built synchronously and the
            // engine answers only through a callback.
            let Some(view) = app.view(tab) else { return };
            let weak = app.weak();
            js::run(&view, "window.getSelection().toString()", move |result| {
                let Some(app) = weak.upgrade() else { return };
                match result {
                    Ok(text) => match search_text(&text) {
                        Some(query) => {
                            let url = app.prefs.borrow().search_url(&query);
                            let private = app.state.borrow().tab(tab).is_some_and(|t| t.private);
                            app.new_tab(Some(&url), private, false);
                        }
                        None => app.flash_toast("Nothing is selected"),
                    },
                    Err(error) => app.flash_toast(&format!("No selection: {error}")),
                }
            });
        }
        Pick::Keep => app.add_bookmark(),
    }
}

#[cfg(test)]
mod tests {
    use super::{choose, label, search_text, Pick};

    #[test]
    fn a_link_in_a_private_tab_only_ever_opens_private() {
        let picks = choose(true, Some("https://a.test/"), false, "https://x.test/");
        assert!(!picks.iter().any(|p| matches!(p, Pick::LinkTab(_))));
        assert!(picks.iter().any(|p| matches!(p, Pick::LinkPrivate(_))));
    }

    #[test]
    fn an_ordinary_tab_offers_both_ways_to_open_a_link() {
        let picks = choose(false, Some("https://a.test/"), false, "https://x.test/");
        assert!(picks.iter().any(|p| matches!(p, Pick::LinkTab(l) if l == "https://a.test/")));
        assert!(picks.iter().any(|p| matches!(p, Pick::LinkPrivate(_))));
    }

    #[test]
    fn a_link_the_browser_refuses_is_not_offered_to_open() {
        let picks = choose(false, Some("javascript:alert(1)"), false, "https://x.test/");
        assert!(!picks.iter().any(|p| matches!(p, Pick::LinkTab(_) | Pick::LinkPrivate(_))));
        assert!(picks.iter().any(|p| matches!(p, Pick::CopyLink(_))));
    }

    #[test]
    fn a_selection_is_searched_only_when_it_has_words() {
        assert!(choose(false, None, true, "https://x.test/").contains(&Pick::Search));
        assert!(!choose(false, None, false, "https://x.test/").contains(&Pick::Search));
        assert_eq!(search_text("  rust gtk \n"), Some("rust gtk".into()));
        assert_eq!(search_text(" \n\t "), None);
    }

    #[test]
    fn keep_is_offered_on_a_web_page_and_not_on_a_blank_one() {
        assert!(choose(false, None, false, "https://x.test/").contains(&Pick::Keep));
        assert!(!choose(false, None, false, "about:blank").contains(&Pick::Keep));
        assert!(choose(false, None, false, "about:blank").is_empty());
    }

    #[test]
    fn the_search_item_names_the_engine_in_plain_words() {
        assert_eq!(label(&Pick::Search, "DuckDuckGo", false), "Search DuckDuckGo for the selection");
        assert_eq!(label(&Pick::Keep, "DuckDuckGo", false), "Keep this page");
    }

    /// Keep is a toggle, like Ctrl D: on a page already kept it removes the
    /// bookmark, so the item must say so rather than offer to keep it again.
    #[test]
    fn keep_on_a_kept_page_says_it_will_stop_keeping() {
        assert_eq!(label(&Pick::Keep, "DuckDuckGo", true), "Stop keeping this page");
        assert_ne!(label(&Pick::Keep, "DuckDuckGo", true), label(&Pick::Keep, "DuckDuckGo", false));
    }
}

// The address of the link under the pointer, in a small line at the bottom
// left of the page. The address comes from the engine's hit test, never from
// the page's own title or text, so a page cannot make it say something else.
use crate::app::App;
use gtk::prelude::*;
use std::cell::RefCell;

const MAX_CHARS: usize = 96;

/// Keeps the start and the end of an address and cuts the middle, since the
/// host is at the start and the path's file name is at the end.
pub fn elide_middle(url: &str, max: usize) -> String {
    let chars: Vec<char> = url.chars().collect();
    if chars.len() <= max {
        return url.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let keep = max - 1;
    let head = keep * 3 / 5;
    let tail = keep - head;
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

thread_local! {
    static LABEL: RefCell<Option<gtk::Label>> = const { RefCell::new(None) };
    static HIDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn page_left(app: &App) -> i32 {
    if app.ui.side.is_visible() && app.ui.side.is_child_visible() {
        app.ui.side.allocated_width().max(0)
    } else {
        0
    }
}

fn label(app: &App) -> gtk::Label {
    LABEL.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| {
                let label = gtk::Label::new(None);
                crate::ui::class(&label, "dlbar");
                crate::ui::class(&label, "statuspill");
                label.set_halign(gtk::Align::Start);
                label.set_valign(gtk::Align::End);
                label.set_margin_start(page_left(app) + 12);
                label.set_margin_bottom(8);
                label.set_no_show_all(true);
                app.ui.overlay.add_overlay(&label);
                label
            })
            .clone()
    })
}

pub fn relayout(app: &App) {
    LABEL.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.set_margin_start(page_left(app) + 12);
        }
    });
}

pub fn show(app: &App, url: &str) {
    let label = label(app);
    label.set_margin_start(page_left(app) + 12);
    label.set_text(&elide_middle(url, MAX_CHARS));
    label.set_tooltip_text(Some(url));
    if !label.is_visible() {
        HIDING.with(|h| h.set(false));
        crate::motion::cancel(&label);
        label.set_opacity(1.0);
        label.show();
        crate::motion::fade_in(&label, 100);
    } else if HIDING.with(|h| h.replace(false)) {
        crate::motion::cancel(&label);
        label.set_opacity(1.0);
    }
}

pub fn hide() {
    LABEL.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            if label.is_visible() && !HIDING.with(|h| h.get()) {
                HIDING.with(|h| h.set(true));
                crate::motion::cancel(label);
                let label_done = label.clone();
                crate::motion::fade_out_then(label, 110, move || {
                    if HIDING.with(|h| h.replace(false)) {
                        label_done.hide();
                        label_done.set_opacity(1.0);
                    }
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_address_is_left_whole() {
        assert_eq!(elide_middle("https://example.org/a", 40), "https://example.org/a");
    }

    #[test]
    fn a_long_address_keeps_its_host_and_its_end() {
        let url = "https://example.org/a/very/long/path/that/goes/on/and/on/report-final.pdf";
        let short = elide_middle(url, 30);
        assert_eq!(short.chars().count(), 30);
        assert!(short.starts_with("https://"));
        assert!(short.ends_with("final.pdf"));
        assert!(short.contains('…'));
    }

    #[test]
    fn elision_counts_characters_not_bytes() {
        let url = "https://example.org/ééééééééééééééééééééééééééééééééé";
        let short = elide_middle(url, 20);
        assert_eq!(short.chars().count(), 20);
    }

    #[test]
    fn elision_with_no_room_is_empty() {
        assert_eq!(elide_middle("https://example.org/a", 0), "");
        assert_eq!(elide_middle("", 0), "");
    }
}

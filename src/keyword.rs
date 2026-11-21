//! Keyword shortcuts in the address field. A word typed before the search
//! words, like "yt cats", goes straight to that site's search, whatever the
//! default engine is. Firefox calls these keyword bookmarks. Here there is no
//! bookmark, only the word and the address it sends the words to.
//!
//! The rules follow the reference browser's Keyword.swift, which was read and
//! not run:
//! - The keyword has to be followed by a space and at least one word.
//! - The keyword is matched without regard to case.
//! - A template has to be http or https, name one site whatever is searched
//!   for, and hold a single %s in the path, query or fragment. A %s in the
//!   host, or before an @, would let the typed words decide where you end up
//!   instead of what you look for there, so such a template is never followed.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Keyword {
    pub keyword: String,
    pub template: String,
}

impl Keyword {
    pub fn new(keyword: &str, template: &str) -> Keyword {
        Keyword { keyword: keyword.to_string(), template: template.to_string() }
    }

    /// What a Settings row calls this keyword: the site's host, without www.
    pub fn name(&self) -> String {
        host_of_template(&self.template).unwrap_or_else(|| self.template.trim().to_string())
    }

    /// The words to search with, if `typed` starts with this keyword, a space
    /// and something after it. None if the keyword is empty, there is no
    /// space, the first word differs, or nothing follows it.
    fn matches<'a>(&self, typed: &'a str) -> Option<&'a str> {
        let word = self.keyword.trim();
        if word.is_empty() {
            return None;
        }
        let (first, rest) = typed.trim_start().split_once(char::is_whitespace)?;
        if !first.eq_ignore_ascii_case(word) {
            return None;
        }
        let rest = rest.trim();
        if rest.is_empty() {
            None
        } else {
            Some(rest)
        }
    }

    /// The address for these words: the template with %s replaced by the
    /// words, encoded the same way the default engine's search is.
    pub fn url_for(&self, words: &str) -> String {
        self.template.trim().replacen("%s", &crate::store::urlencode(words), 1)
    }
}

/// The keywords a fresh profile starts with. Each one can be removed or
/// changed in Settings.
pub fn defaults() -> Vec<Keyword> {
    vec![
        Keyword::new("yt", "https://www.youtube.com/results?search_query=%s"),
        Keyword::new("w", "https://en.wikipedia.org/w/index.php?search=%s"),
        Keyword::new("gh", "https://github.com/search?q=%s"),
        Keyword::new("map", "https://www.openstreetmap.org/search?query=%s"),
    ]
}

/// The address `typed` asks for through the first keyword it names, or None
/// when it names none. A keyword whose template would not be accepted today
/// is passed over instead of followed.
pub fn find(typed: &str, keywords: &[Keyword]) -> Option<String> {
    keywords
        .iter()
        .filter(|k| accepts(&k.template))
        .find_map(|k| k.matches(typed).map(|words| k.url_for(words)))
}

/// Whether a template may receive typed words: http or https, a host with no
/// %s, no @ and no spaces in it, and exactly one %s after the host.
pub fn accepts(template: &str) -> bool {
    let template = template.trim();
    let Some(rest) = template
        .strip_prefix("https://")
        .or_else(|| template.strip_prefix("http://"))
    else {
        return false;
    };
    if template.matches("%s").count() != 1 {
        return false;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty()
        || authority.contains("%s")
        || authority.contains('@')
        || authority.contains(char::is_whitespace)
    {
        return false;
    }
    rest[end..].contains("%s")
}

/// The same as `find`, plus what the address field's row shows: the site's
/// name and the words that will be searched there.
pub fn find_named(typed: &str, keywords: &[Keyword]) -> Option<(String, String, String)> {
    keywords
        .iter()
        .filter(|k| accepts(&k.template))
        .find_map(|k| k.matches(typed).map(|words| (k.url_for(words), k.name(), words.to_string())))
}

/// Why `word` and `template` cannot be saved next to `keywords` yet, as a
/// line Settings can show, or None when they can.
pub fn problem(word: &str, template: &str, keywords: &[Keyword]) -> Option<String> {
    let word = word.trim();
    if word.is_empty() {
        return Some("A word to type first, like yt".to_string());
    }
    if word.contains(char::is_whitespace) {
        return Some("One word, with no spaces in it".to_string());
    }
    if let Some(taken) = keywords.iter().find(|k| k.keyword.trim().eq_ignore_ascii_case(word)) {
        return Some(format!("{} already goes to {}", taken.keyword, taken.name()));
    }
    if !accepts(template) {
        return Some("An http or https address with %s once, where the words go".to_string());
    }
    None
}

/// The host a template names, lowercased, without www. and without a port.
fn host_of_template(template: &str) -> Option<String> {
    let template = template.trim();
    let rest = template
        .strip_prefix("https://")
        .or_else(|| template.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_lowercase();
    let host = host.strip_prefix("www.").map(str::to_string).unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// Settings › KEYWORDS: each saved shortcut with a Remove button, then a row
/// to add one. A shortcut that cannot be saved stays in the fields, and the
/// reason is shown under them. Nothing is lost when the reason is fixed.
pub fn settings_section(app: &crate::app::AppRef) -> gtk::Box {
    use gtk::prelude::*;

    let section = gtk::Box::new(gtk::Orientation::Vertical, 6);
    section.set_margin_top(14);
    let head = gtk::Label::new(Some("KEYWORDS"));
    crate::ui::class(&head, "group");
    head.set_xalign(0.0);
    section.add(&head);

    let note = gtk::Label::new(Some("Type a word, a space and what to look for: yt cats searches YouTube."));
    crate::ui::class(&note, "settingdesc");
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    section.add(&note);

    let keywords = app.prefs.borrow().keywords.clone();
    for keyword in keywords {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        let word = gtk::Label::new(Some(&keyword.keyword));
        crate::ui::class(&word, "setting");
        word.set_xalign(0.0);
        let site = gtk::Label::new(Some(&keyword.name()));
        crate::ui::class(&site, "settingdesc");
        site.set_xalign(0.0);
        site.set_ellipsize(gtk::pango::EllipsizeMode::End);
        crate::ui::pack(&texts, &word, false);
        crate::ui::pack(&texts, &site, false);
        crate::ui::pack(&row, &texts, true);

        let remove = gtk::Button::with_label("Remove");
        crate::ui::class(&remove, "act");
        remove.set_valign(gtk::Align::Center);
        remove.set_tooltip_text(Some("Stop using this keyword"));
        let weak = app.weak();
        let gone = keyword.clone();
        remove.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.prefs.borrow_mut().keywords.retain(|k| *k != gone);
                app.prefs.borrow().save();
                app.open_panel(crate::panels::PanelKind::Settings);
            }
        });
        crate::ui::pack(&row, &remove, false);
        section.add(&row);
    }

    let add_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    add_row.set_margin_top(6);
    let word_entry = gtk::Entry::new();
    word_entry.set_placeholder_text(Some("yt"));
    word_entry.set_width_chars(6);
    if let Some(access) = word_entry.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Shortcut keyword");
    }
    let template_entry = gtk::Entry::new();
    template_entry.set_placeholder_text(Some("https://www.youtube.com/results?search_query=%s"));
    template_entry.set_hexpand(true);
    if let Some(access) = template_entry.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Search URL template");
    }
    let add = gtk::Button::with_label("Add");
    crate::ui::class(&add, "act");
    crate::ui::pack(&add_row, &word_entry, false);
    crate::ui::pack(&add_row, &template_entry, true);
    crate::ui::pack(&add_row, &add, false);
    section.add(&add_row);

    let why = gtk::Label::new(None);
    crate::ui::class(&why, "settingdesc");
    why.set_xalign(0.0);
    why.set_line_wrap(true);
    why.set_no_show_all(true);
    section.add(&why);

    let weak = app.weak();
    add.connect_clicked(move |_| {
        let Some(app) = weak.upgrade() else { return };
        let word = word_entry.text().to_string();
        let template = template_entry.text().to_string();
        let existing = app.prefs.borrow().keywords.clone();
        if let Some(reason) = problem(&word, &template, &existing) {
            why.set_text(&reason);
            why.show();
            return;
        }
        app.prefs.borrow_mut().keywords.push(Keyword::new(word.trim(), template.trim()));
        app.prefs.borrow().save();
        app.open_panel(crate::panels::PanelKind::Settings);
    });

    section
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keyword_and_words_go_to_that_site() {
        let keywords = defaults();
        assert_eq!(
            find("yt cats", &keywords).as_deref(),
            Some("https://www.youtube.com/results?search_query=cats")
        );
        assert_eq!(
            find("YT funny  cats ", &keywords).as_deref(),
            Some("https://www.youtube.com/results?search_query=funny++cats")
        );
    }

    #[test]
    fn a_keyword_alone_or_inside_a_word_is_not_a_shortcut() {
        let keywords = defaults();
        assert_eq!(find("yt", &keywords), None);
        assert_eq!(find("yt   ", &keywords), None);
        assert_eq!(find("ytcats", &keywords), None);
        assert_eq!(find("cats yt", &keywords), None);
    }

    #[test]
    fn the_words_cannot_change_the_site() {
        assert!(accepts("https://example.com/search?q=%s"));
        assert!(accepts("http://example.com/#%s"));
        assert!(!accepts("https://%s.example.com/"));
        assert!(!accepts("https://user%s@example.com/"));
        assert!(!accepts("https://example.com/?a=%s&b=%s"));
        assert!(!accepts("https://example.com/"));
        assert!(!accepts("javascript:alert(%s)"));
        assert!(!accepts("file:///home/%s"));
    }

    #[test]
    fn a_template_that_is_not_accepted_is_passed_over() {
        let keywords = vec![Keyword::new("x", "https://%s.evil.test/"), Keyword::new("x", "https://good.test/?q=%s")];
        assert_eq!(find("x hi", &keywords).as_deref(), Some("https://good.test/?q=hi"));
    }

    #[test]
    fn settings_explain_what_cannot_be_saved() {
        let keywords = defaults();
        assert!(problem("", "https://a.test/?q=%s", &keywords).is_some());
        assert!(problem("two words", "https://a.test/?q=%s", &keywords).is_some());
        assert_eq!(
            problem("YT", "https://a.test/?q=%s", &keywords).as_deref(),
            Some("yt already goes to youtube.com")
        );
        assert!(problem("a", "https://a.test/", &keywords).is_some());
        assert_eq!(problem("a", "https://a.test/?q=%s", &keywords), None);
    }
}

// Persistent tab-group Spaces.
//
// Spaces organize open tabs into named groups inside the single shared browser
// profile (same cookies, logins, history, and bookmarks). They are NOT isolated
// security or privacy containers — Private Tabs serve temporary separate sessions.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::LazyLock;

pub const DEFAULT_SPACE_ID: &str = "default";
pub const DEFAULT_SPACE_NAME: &str = "Default";
pub const MAX_SPACE_NAME_LEN: usize = 32;
pub const MAX_SPACES: usize = 16;

static FALLBACK_SPACE: LazyLock<Space> = LazyLock::new(|| Space {
    id: DEFAULT_SPACE_ID.to_string(),
    name: DEFAULT_SPACE_NAME.to_string(),
});

pub fn default_space_id() -> String {
    DEFAULT_SPACE_ID.to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Space {
    pub id: String,
    pub name: String,
}

impl Space {
    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_SPACE_ID
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct SpaceStore {
    pub spaces: Vec<Space>,
    pub active_space: String,
    pub next_space_seq: u64,
}

impl Default for SpaceStore {
    fn default() -> Self {
        Self {
            spaces: vec![Space {
                id: DEFAULT_SPACE_ID.to_string(),
                name: DEFAULT_SPACE_NAME.to_string(),
            }],
            active_space: DEFAULT_SPACE_ID.to_string(),
            next_space_seq: 1,
        }
    }
}

/// Normalize a user-supplied space name: trim outer whitespace, collapse inner
/// whitespace/control breaks to single spaces, reject empty names, and bound
/// to `MAX_SPACE_NAME_LEN` Unicode scalar values.
pub fn sanitize_space_name(raw: &str) -> Result<String, &'static str> {
    let collapsed: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let bounded: String = collapsed.chars().take(MAX_SPACE_NAME_LEN).collect();
    let trimmed = bounded.trim();
    if trimmed.is_empty() {
        return Err("Enter a space name");
    }
    Ok(trimmed.to_string())
}

fn suffix_number(id: &str) -> Option<u64> {
    id.strip_prefix("space-")?.parse::<u64>().ok()
}

impl SpaceStore {
    /// Enforce invariants on any loaded store:
    /// - `default` always exists at index 0 with a non-empty bounded name.
    /// - IDs are unique and non-empty; names are bounded, trimmed, and unique
    ///   (case-insensitively).
    /// - `active_space` always points to an existing space.
    /// - `next_space_seq` stays strictly ahead of all `space-N` IDs.
    pub fn sanitised(mut self) -> Self {
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut seen_names: HashSet<String> = HashSet::new();
        let mut default_name = DEFAULT_SPACE_NAME.to_string();
        let mut custom: Vec<Space> = Vec::new();
        let mut max_seq: u64 = self.next_space_seq.saturating_sub(1);

        for space in self.spaces.drain(..) {
            let id = space.id.trim().to_string();
            if id.is_empty() || !seen_ids.insert(id.clone()) {
                continue;
            }
            if let Some(n) = suffix_number(&id) {
                max_seq = max_seq.max(n);
            }
            if id == DEFAULT_SPACE_ID {
                if let Ok(clean) = sanitize_space_name(&space.name) {
                    default_name = clean;
                }
                continue;
            }
            if custom.len() + 1 >= MAX_SPACES {
                continue;
            }
            let Ok(clean) = sanitize_space_name(&space.name) else {
                continue;
            };
            let key = clean.to_lowercase();
            if !seen_names.insert(key) {
                continue;
            }
            custom.push(Space { id, name: clean });
        }

        // Prevent a custom space from colliding case-insensitively with Default's name.
        let default_key = default_name.to_lowercase();
        custom.retain(|s| s.name.to_lowercase() != default_key);

        let mut spaces = Vec::with_capacity(1 + custom.len());
        spaces.push(Space {
            id: DEFAULT_SPACE_ID.to_string(),
            name: default_name,
        });
        spaces.extend(custom);

        let active_space = if spaces.iter().any(|s| s.id == self.active_space) {
            self.active_space
        } else {
            DEFAULT_SPACE_ID.to_string()
        };

        Self {
            spaces,
            active_space,
            next_space_seq: max_seq.saturating_add(1).max(1),
        }
    }

    pub fn space(&self, id: &str) -> Option<&Space> {
        self.spaces.iter().find(|s| s.id == id)
    }

    pub fn active(&self) -> &Space {
        self.space(&self.active_space)
            .or_else(|| self.spaces.first())
            .unwrap_or(&FALLBACK_SPACE)
    }

    pub fn default_name(&self) -> &str {
        self.space(DEFAULT_SPACE_ID)
            .map(|s| s.name.as_str())
            .unwrap_or(DEFAULT_SPACE_NAME)
    }

    pub fn resolve_space_id(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && self.spaces.iter().any(|s| s.id == trimmed) {
            trimmed.to_string()
        } else {
            DEFAULT_SPACE_ID.to_string()
        }
    }

    pub fn set_active(&mut self, id: &str) -> bool {
        if self.active_space == id || !self.spaces.iter().any(|s| s.id == id) {
            return false;
        }
        self.active_space = id.to_string();
        true
    }

    pub fn create(&mut self, raw_name: &str) -> Result<Space, &'static str> {
        if self.spaces.len() >= MAX_SPACES {
            return Err("Space limit reached");
        }
        let name = sanitize_space_name(raw_name)?;
        if self.spaces.iter().any(|s| s.name.to_lowercase() == name.to_lowercase()) {
            return Err("A space with that name already exists");
        }
        let mut seq = self.next_space_seq.max(1);
        let id = loop {
            let candidate = format!("space-{seq}");
            seq = seq.checked_add(1).unwrap_or(1);
            if !self.spaces.iter().any(|s| s.id == candidate) {
                break candidate;
            }
        };
        self.next_space_seq = seq;
        let space = Space { id, name };
        self.spaces.push(space.clone());
        Ok(space)
    }

    pub fn rename(&mut self, id: &str, raw_name: &str) -> Result<String, &'static str> {
        let name = sanitize_space_name(raw_name)?;
        if !self.spaces.iter().any(|s| s.id == id) {
            return Err("Space not found");
        }
        if self
            .spaces
            .iter()
            .any(|s| s.id != id && s.name.to_lowercase() == name.to_lowercase())
        {
            return Err("A space with that name already exists");
        }
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == id) {
            space.name = name.clone();
        }
        Ok(name)
    }

    /// Delete a non-default space. Callers are responsible for migrating its
    /// tabs into `DEFAULT_SPACE_ID`.
    pub fn delete(&mut self, id: &str) -> Result<Space, &'static str> {
        if id == DEFAULT_SPACE_ID {
            return Err("Default space cannot be deleted");
        }
        let Some(pos) = self.spaces.iter().position(|s| s.id == id) else {
            return Err("Space not found");
        };
        let removed = self.spaces.remove(pos);
        if self.active_space == id {
            self.active_space = DEFAULT_SPACE_ID.to_string();
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_store_starts_with_default_only() {
        let store = SpaceStore::default();
        assert_eq!(store.spaces.len(), 1);
        assert_eq!(store.spaces[0].id, DEFAULT_SPACE_ID);
        assert_eq!(store.spaces[0].name, DEFAULT_SPACE_NAME);
        assert_eq!(store.active_space, DEFAULT_SPACE_ID);
    }

    #[test]
    fn space_names_are_trimmed_collapsed_and_bounded() {
        assert_eq!(sanitize_space_name("   Work \t\n Bench  ").unwrap(), "Work Bench");
        assert!(sanitize_space_name("   \n\t ").is_err());
        let long = "a".repeat(60);
        let bounded = sanitize_space_name(&long).unwrap();
        assert_eq!(bounded.chars().count(), MAX_SPACE_NAME_LEN);
    }

    #[test]
    fn create_rename_delete_maintain_stable_ids_and_protect_default() {
        let mut store = SpaceStore::default();
        let work = store.create("  Work  ").unwrap();
        assert_eq!(work.id, "space-1");
        assert_eq!(work.name, "Work");

        // Case-insensitive duplicate rejected.
        assert!(store.create("work").is_err());

        let research = store.create("Research").unwrap();
        assert_eq!(research.id, "space-2");

        // Rename keeps the same stable ID.
        assert_eq!(store.rename(&work.id, "  Deep Work ").unwrap(), "Deep Work");
        assert_eq!(store.space(&work.id).unwrap().name, "Deep Work");
        assert!(store.rename(&research.id, "deep work").is_err());

        // Default cannot be deleted.
        assert!(store.delete(DEFAULT_SPACE_ID).is_err());

        // Deleting active space resets active_space to default, and next ID never reuses deleted ID.
        assert!(store.set_active(&work.id));
        assert_eq!(store.delete(&work.id).unwrap().id, "space-1");
        assert_eq!(store.active_space, DEFAULT_SPACE_ID);
        let next = store.create("Reading").unwrap();
        assert_eq!(next.id, "space-3");
    }

    #[test]
    fn sanitised_recovers_missing_default_and_unknown_active_space() {
        let broken = SpaceStore {
            spaces: vec![
                Space { id: "space-4".into(), name: "  Alpha  ".into() },
                Space { id: "space-4".into(), name: "Duplicate ID".into() },
                Space { id: "".into(), name: "Empty ID".into() },
            ],
            active_space: "ghost".into(),
            next_space_seq: 1,
        }
        .sanitised();

        assert_eq!(broken.spaces.len(), 2);
        assert_eq!(broken.spaces[0].id, DEFAULT_SPACE_ID);
        assert_eq!(broken.spaces[1].id, "space-4");
        assert_eq!(broken.spaces[1].name, "Alpha");
        assert_eq!(broken.active_space, DEFAULT_SPACE_ID);
        assert_eq!(broken.next_space_seq, 5);
        assert_eq!(broken.resolve_space_id("ghost"), DEFAULT_SPACE_ID);
        assert_eq!(broken.resolve_space_id("space-4"), "space-4");
    }

    #[test]
    fn corrupt_empty_store_has_a_safe_active_fallback() {
        let store = SpaceStore {
            spaces: Vec::new(),
            active_space: "missing".into(),
            next_space_seq: 1,
        };
        assert_eq!(store.active().id, DEFAULT_SPACE_ID);
    }

    #[test]
    fn exhausted_sequence_recovers_without_reusing_an_existing_id() {
        let mut store = SpaceStore::default();
        store.spaces.push(Space {
            id: format!("space-{}", u64::MAX),
            name: "Existing".into(),
        });
        store.next_space_seq = u64::MAX;
        let created = store.create("Work").unwrap();
        assert_eq!(created.id, "space-1");
        assert_eq!(store.next_space_seq, 2);
    }

    #[test]
    fn unicode_name_duplicates_are_rejected_before_and_after_reload() {
        let mut store = SpaceStore::default();
        let first = store.create("Équipe").unwrap();
        assert!(store.create("équipe").is_err());
        let second = store.create("Research").unwrap();
        assert!(store.rename(&second.id, "ÉQUIPE").is_err());
        assert_eq!(store.sanitised().space(&first.id).unwrap().name, "Équipe");
    }
}

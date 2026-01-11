use std::collections::BTreeSet;

use anyhow::Result;
use tdcore::profile::{DangerLevel, Profile, ProfileFilters, ProfileStore, ProfileType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

pub struct AppState {
    store: ProfileStore,
    filters: ProfileFilters,
    filtered: Vec<Profile>,
    groups: Vec<String>,
    tags: Vec<String>,
    tag_cursor: usize,
    mode: InputMode,
    search_input: String,
}

impl AppState {
    pub fn new(store: ProfileStore) -> Result<Self> {
        let profiles = store.list()?;
        let groups = collect_groups(&profiles);
        let tags = collect_tags(&profiles);
        let filters = ProfileFilters::default();
        let filtered = store.list_filtered(&filters)?;
        Ok(Self {
            store,
            filters,
            filtered,
            groups,
            tags,
            tag_cursor: 0,
            mode: InputMode::Normal,
            search_input: String::new(),
        })
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn filters(&self) -> &ProfileFilters {
        &self.filters
    }

    pub fn filtered(&self) -> &[Profile] {
        &self.filtered
    }

    pub fn groups(&self) -> &[String] {
        &self.groups
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn tag_cursor(&self) -> Option<&str> {
        self.tags.get(self.tag_cursor).map(String::as_str)
    }

    pub fn search_input(&self) -> &str {
        &self.search_input
    }

    pub fn enter_search(&mut self) {
        self.mode = InputMode::Search;
        self.search_input = self.filters.query.clone().unwrap_or_default();
    }

    pub fn exit_search(&mut self) -> Result<()> {
        self.mode = InputMode::Normal;
        self.update_query()
    }

    pub fn push_search_char(&mut self, ch: char) -> Result<()> {
        self.search_input.push(ch);
        self.update_query()
    }

    pub fn pop_search_char(&mut self) -> Result<()> {
        self.search_input.pop();
        self.update_query()
    }

    pub fn clear_filters(&mut self) -> Result<()> {
        self.filters = ProfileFilters::default();
        self.search_input.clear();
        self.refresh()
    }

    pub fn cycle_profile_type(&mut self) -> Result<()> {
        self.filters.profile_type = match self.filters.profile_type {
            None => Some(ProfileType::Ssh),
            Some(ProfileType::Ssh) => Some(ProfileType::Telnet),
            Some(ProfileType::Telnet) => Some(ProfileType::Serial),
            Some(ProfileType::Serial) => None,
        };
        self.refresh()
    }

    pub fn cycle_danger(&mut self) -> Result<()> {
        self.filters.danger = match self.filters.danger {
            None => Some(DangerLevel::Normal),
            Some(DangerLevel::Normal) => Some(DangerLevel::High),
            Some(DangerLevel::High) => Some(DangerLevel::Critical),
            Some(DangerLevel::Critical) => None,
        };
        self.refresh()
    }

    pub fn cycle_group(&mut self) -> Result<()> {
        if self.groups.is_empty() {
            self.filters.group = None;
            return self.refresh();
        }
        let next = match &self.filters.group {
            None => Some(self.groups[0].clone()),
            Some(current) => match self
                .groups
                .iter()
                .position(|g| g.eq_ignore_ascii_case(current))
            {
                Some(idx) if idx + 1 < self.groups.len() => Some(self.groups[idx + 1].clone()),
                _ => None,
            },
        };
        self.filters.group = next;
        self.refresh()
    }

    pub fn tag_cursor_next(&mut self) {
        if self.tags.is_empty() {
            return;
        }
        self.tag_cursor = (self.tag_cursor + 1) % self.tags.len();
    }

    pub fn tag_cursor_prev(&mut self) {
        if self.tags.is_empty() {
            return;
        }
        if self.tag_cursor == 0 {
            self.tag_cursor = self.tags.len() - 1;
        } else {
            self.tag_cursor -= 1;
        }
    }

    pub fn toggle_tag(&mut self) -> Result<()> {
        if self.tags.is_empty() {
            return Ok(());
        }
        let tag = &self.tags[self.tag_cursor];
        if let Some(pos) = self
            .filters
            .tags
            .iter()
            .position(|t| t.eq_ignore_ascii_case(tag))
        {
            self.filters.tags.remove(pos);
        } else {
            self.filters.tags.push(tag.clone());
        }
        self.refresh()
    }

    fn update_query(&mut self) -> Result<()> {
        let trimmed = self.search_input.trim();
        self.filters.query = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        self.refresh()
    }

    fn refresh(&mut self) -> Result<()> {
        self.filtered = self.store.list_filtered(&self.filters)?;
        Ok(())
    }
}

fn collect_groups(profiles: &[Profile]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for profile in profiles {
        if let Some(group) = &profile.group {
            set.insert(group.to_string());
        }
    }
    set.into_iter().collect()
}

fn collect_tags(profiles: &[Profile]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for profile in profiles {
        for tag in &profile.tags {
            set.insert(tag.to_string());
        }
    }
    set.into_iter().collect()
}

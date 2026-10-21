// Dashboard: navigation, state and rendering.

pub mod nav;
pub mod view;

// Dashboard state.
//
// Owns the selected route and the data each page needs. Data is loaded from the
// database on demand and kept here so a re-render never re-queries by accident.

use crate::features::dashboard::nav::Route;
use crate::features::db::conversations::{self, ConversationRecord};
use crate::features::db::library::{self, FileRecord};
use crate::features::db::meetings::{self, MeetingRecord};
use crate::features::db::settings::{self, PromptRecord};
use crate::features::db::{Database, DbError};

/// A loaded page payload.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum PageData {
    #[default]
    Empty,
    Conversations(Vec<ConversationRecord>),
    Files(Vec<FileRecord>),
    Prompts(Vec<PromptRecord>),
    Meetings(Vec<MeetingRecord>),
}

/// Errors shown inline when a page cannot load.
pub type LoadError = String;

/// Dashboard view state.
pub struct Dashboard {
    route: Route,
    data: PageData,
    error: Option<LoadError>,
    search: String,
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            route: Route::Home,
            data: PageData::Empty,
            error: None,
            search: String::new(),
        }
    }

    pub fn route(&self) -> Route {
        self.route
    }

    pub fn data(&self) -> &PageData {
        &self.data
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn set_search(&mut self, value: impl Into<String>) {
        self.search = value.into();
    }

    /// Navigate and load the target page's data.
    pub fn navigate(&mut self, route: Route, database: Option<&Database>) {
        self.route = route;
        self.error = None;
        self.data = match route {
            Route::Conversations => match Self::load_conversations(database) {
                Ok(records) => PageData::Conversations(records),
                Err(error) => {
                    self.error = Some(error);
                    PageData::Empty
                }
            },
            Route::Files => match Self::load_files(database) {
                Ok(records) => PageData::Files(records),
                Err(error) => {
                    self.error = Some(error);
                    PageData::Empty
                }
            },
            Route::Meetings | Route::MeetingDetail => match Self::load_meetings(database) {
                Ok(records) => PageData::Meetings(records),
                Err(error) => {
                    self.error = Some(error);
                    PageData::Empty
                }
            },
            Route::Prompts => match Self::load_prompts(database) {
                Ok(records) => PageData::Prompts(records),
                Err(error) => {
                    self.error = Some(error);
                    PageData::Empty
                }
            },
            _ => PageData::Empty,
        };
    }

    fn load_conversations(database: Option<&Database>) -> Result<Vec<ConversationRecord>, String> {
        let database = database.ok_or_else(not_ready)?;
        conversations::list(database.connection(), 200).map_err(describe)
    }

    fn load_meetings(database: Option<&Database>) -> Result<Vec<MeetingRecord>, String> {
        let database = database.ok_or_else(not_ready)?;
        meetings::list(database.connection(), 100).map_err(describe)
    }

    fn load_files(database: Option<&Database>) -> Result<Vec<FileRecord>, String> {
        let database = database.ok_or_else(not_ready)?;
        library::list(database.connection()).map_err(describe)
    }

    fn load_prompts(database: Option<&Database>) -> Result<Vec<PromptRecord>, String> {
        let database = database.ok_or_else(not_ready)?;
        settings::prompts(database.connection()).map_err(describe)
    }

    /// Conversations matching the current search, by title.
    pub fn filtered_conversations(&self) -> Vec<&ConversationRecord> {
        let PageData::Conversations(records) = &self.data else {
            return Vec::new();
        };
        let needle = self.search.trim().to_lowercase();
        records
            .iter()
            .filter(|record| needle.is_empty() || record.title.to_lowercase().contains(&needle))
            .collect()
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

fn not_ready() -> String {
    "The local database is not ready yet.".to_owned()
}

fn describe(error: DbError) -> String {
    format!("Could not load this page: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::db::conversations::create;

    #[test]
    fn navigation_without_database_reports_a_clear_error() {
        let mut dashboard = Dashboard::new();
        dashboard.navigate(Route::Conversations, None);
        assert_eq!(dashboard.route(), Route::Conversations);
        assert!(dashboard.error().unwrap().contains("not ready"));
        assert_eq!(dashboard.data(), &PageData::Empty);
    }

    #[test]
    fn conversations_load_from_the_database() {
        let database = Database::open_in_memory().unwrap();
        create(database.connection(), "First chat", 10).unwrap();
        create(database.connection(), "Second chat", 20).unwrap();

        let mut dashboard = Dashboard::new();
        dashboard.navigate(Route::Conversations, Some(&database));
        assert!(dashboard.error().is_none());
        let PageData::Conversations(records) = dashboard.data() else {
            panic!("expected conversations, got {:?}", dashboard.data());
        };
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].title, "Second chat");
    }

    #[test]
    fn search_filters_by_title_case_insensitively() {
        let database = Database::open_in_memory().unwrap();
        create(database.connection(), "Alpha", 1).unwrap();
        create(database.connection(), "Beta", 2).unwrap();

        let mut dashboard = Dashboard::new();
        dashboard.navigate(Route::Conversations, Some(&database));
        dashboard.set_search("alp");
        let filtered = dashboard.filtered_conversations();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Alpha");

        dashboard.set_search("");
        assert_eq!(dashboard.filtered_conversations().len(), 2);
    }

    #[test]
    fn prompts_page_lists_seeded_modes() {
        let database = Database::open_in_memory().unwrap();
        let mut dashboard = Dashboard::new();
        dashboard.navigate(Route::Prompts, Some(&database));
        let PageData::Prompts(prompts) = dashboard.data() else {
            panic!("expected prompts");
        };
        assert_eq!(prompts.len(), 5);
    }

    #[test]
    fn navigating_clears_previous_data() {
        let database = Database::open_in_memory().unwrap();
        let mut dashboard = Dashboard::new();
        dashboard.navigate(Route::Prompts, Some(&database));
        dashboard.navigate(Route::Settings, Some(&database));
        assert_eq!(dashboard.data(), &PageData::Empty);
    }
}

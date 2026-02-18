//! Defines structs and code for the settings page of the radio

/// Defines the subtabs for the settings page
#[derive(Default, PartialEq)]
pub enum Subsetting {
    #[default]
    /// The general settings tab
    General,
    /// The video configuration tab
    Video,
    /// The software update tab
    Update,
}

/// The settings for the settings page
#[derive(Default)]
pub struct Settings {
    /// The video stream selected
    pub selected_video: u8,
    /// The currently selected subtab
    pub tab: Subsetting,
    /// The list of files on the update server
    pub list: Vec<String>,
    /// The status of the download
    pub download_status: Option<bool>,
}
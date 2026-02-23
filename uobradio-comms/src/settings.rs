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

/// The status of the firmware update process
#[derive(Default)]
pub enum UpdateStatus {
    /// Doing nothing
    #[default]
    Idle,
    /// Started download
    DownloadStarted,
    /// The download is in process
    Downloading(f32),
    /// Completed with a status
    Completed(bool),
    /// The update process has been started
    UpdateStarted,
    /// The progress and step for updating
    UpdateProgress(u8, u8),
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
    pub download_status: UpdateStatus,
    /// update progress pending a reply
    pub update_status_pending: bool,
}

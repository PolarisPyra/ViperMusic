pub(crate) struct PreferencesState {
    pub(crate) category: Option<super::screen::Category>,
    /// Preview textures for the app logo picker, filled the first time it is shown.
    pub(crate) logo_thumbnails: Vec<eframe::egui::TextureHandle>,
    pub(crate) discord_application_id_draft: String,
    pub(crate) lastfm_api_key_draft: String,
    pub(crate) lastfm_api_secret_draft: String,
    #[cfg(target_os = "linux")]
    pub(crate) smb_url_draft: String,
    #[cfg(target_os = "linux")]
    pub(crate) smb_username: String,
    #[cfg(target_os = "linux")]
    pub(crate) smb_password: String,
    #[cfg(target_os = "linux")]
    pub(crate) smb_workgroup: String,
}

impl PreferencesState {
    pub(crate) fn new(
        discord_application_id_draft: String,
        lastfm_api_key_draft: String,
        lastfm_api_secret_draft: String,
    ) -> Self {
        Self {
            category: None,
            logo_thumbnails: Vec::new(),
            discord_application_id_draft,
            lastfm_api_key_draft,
            lastfm_api_secret_draft,
            #[cfg(target_os = "linux")]
            smb_url_draft: String::new(),
            #[cfg(target_os = "linux")]
            smb_username: String::new(),
            #[cfg(target_os = "linux")]
            smb_password: String::new(),
            #[cfg(target_os = "linux")]
            smb_workgroup: String::new(),
        }
    }
}

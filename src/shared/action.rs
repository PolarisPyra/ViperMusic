/// User requests raised by views and handled by the application after the frame is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppAction {
    /// Pick a local music folder and rescan the library.
    ChooseMusicFolder,
    /// Persist application preferences.
    SaveSettings,
    /// Persist the Discord application ID and application preferences.
    SaveDiscordApplicationId,
    /// Save SMB connection details and scan the share.
    ConnectSmbShare,
    /// Persist Last.fm API credentials from the preferences drafts.
    SaveLastFmSettings,
    /// Begin Last.fm browser authorization.
    StartLastFmAuth,
    /// Exchange the approved Last.fm token for a session.
    FinishLastFmAuth,
    /// Forget the Last.fm session.
    DisconnectLastFm,
    /// Check GitHub for a newer release now.
    CheckForUpdates,
    /// Quit and start the installed update.
    RestartToUpdate,
}

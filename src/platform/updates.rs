//! Keeps Viper Music up to date from its GitHub releases. Shortly after launch and then every hour
//! it checks the latest release, and an AppImage downloads the new AppImage, checks it against the
//! digest GitHub publishes, and moves it into place. The new version runs from the next start; the
//! update card and Preferences offer a restart. A copy that is not a release AppImage (a dev or
//! source build) cannot replace itself, so it only reports the version.

use anyhow::{ensure, Context};
use eframe::egui;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, PoisonError,
    },
    thread,
    time::{Duration, SystemTime},
};

/// Release page opened by Download, for copies that cannot install updates.
pub(crate) const RELEASES_URL: &str = "https://github.com/PolarisPyra/ViperMusic/releases/latest";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/PolarisPyra/ViperMusic/releases/latest";
/// How long after launch the first check waits, so it does not slow the start.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// How often to check. Measured on the wall clock, so time spent asleep counts.
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// How often the timer looks at the clock.
const TICK: Duration = Duration::from_secs(5 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// A download that has not finished by then has stalled; the next check starts it again.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// AppImage the app starts again into once it has quit, set by [`Updater::restart`].
static RESTART_INTO: OnceLock<PathBuf> = OnceLock::new();

/// Where updating stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateState {
    /// No check has finished yet.
    Idle,
    /// A check the user asked for is running.
    Checking,
    /// The latest release is the running version.
    UpToDate,
    /// A newer release this copy cannot install itself.
    Available { version: String },
    /// The newer release is downloading; `total` is unknown when the server does not say.
    Downloading {
        version: String,
        downloaded: u64,
        total: Option<u64>,
    },
    /// Installed; it runs once Viper Music restarts.
    Ready { version: String },
    /// The last check the user asked for, or the last download, failed with this message.
    Failed { message: String },
}

/// What the user asked of the update card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateAction {
    /// Quit and start the installed update.
    Restart,
    /// Open the release page.
    Download,
}

#[derive(Default)]
struct Shared {
    state: Mutex<Option<UpdateState>>,
    /// One check or download at a time.
    busy: AtomicBool,
    last_check: Mutex<Option<SystemTime>>,
    /// Wakes the window when the state changes; set by the first frame.
    ctx: OnceLock<egui::Context>,
}

/// Checks for and installs updates on background threads; the UI reads its state each frame.
#[derive(Default)]
pub(crate) struct Updater {
    shared: Arc<Shared>,
    /// Version whose card the user closed this session.
    dismissed: Option<String>,
}

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// `sha256:<hex>`, published by GitHub for each asset.
    digest: Option<String>,
}

/// The AppImage this copy runs from, when it is a release AppImage that can replace itself.
fn installed_appimage() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return None;
    }
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

impl Updater {
    /// Start the background checks.
    ///
    /// # Returns
    /// The updater whose state the UI shows.
    pub(crate) fn start() -> Self {
        let updater = Self::default();
        let shared = Arc::clone(&updater.shared);
        let spawned = thread::Builder::new()
            .name("updates".to_owned())
            .spawn(move || {
                thread::sleep(FIRST_CHECK);
                loop {
                    if shared.due() {
                        shared.check(false);
                    }
                    thread::sleep(TICK);
                }
            });
        if let Err(error) = spawned {
            eprintln!("Update checks could not start: {error}");
        }
        updater
    }

    /// Where updating stands now.
    ///
    /// # Arguments
    /// * `ctx` - egui context, kept so background progress can repaint the window.
    ///
    /// # Returns
    /// The current state.
    pub(crate) fn state(&self, ctx: &egui::Context) -> UpdateState {
        let _ = self.shared.ctx.set(ctx.clone());
        self.shared.state().unwrap_or(UpdateState::Idle)
    }

    /// Check for a newer release on a background thread, showing its progress and errors.
    pub(crate) fn check_now(&self) {
        let shared = Arc::clone(&self.shared);
        if let Err(error) = thread::Builder::new()
            .name("update-check".to_owned())
            .spawn(move || shared.check(true))
        {
            self.shared.set(UpdateState::Failed {
                message: format!("Could not check for updates: {error}"),
            });
        }
    }

    /// Remember the installed update so `main` starts it once the app has quit.
    ///
    /// # Returns
    /// Whether an update is installed and the app should now quit.
    pub(crate) fn restart(&self) -> bool {
        if !matches!(self.shared.state(), Some(UpdateState::Ready { .. })) {
            return false;
        }
        installed_appimage().is_some_and(|path| {
            let _ = RESTART_INTO.set(path);
            true
        })
    }

    /// Draw the card in the top right once an update is installed, or, for a copy that cannot
    /// install it, released.
    ///
    /// # Arguments
    /// * `ctx` - egui context for the frame.
    ///
    /// # Returns
    /// The action the user picked on the card, if any.
    pub(crate) fn show_card(&mut self, ctx: &egui::Context) -> Option<UpdateAction> {
        let (version, action) = match self.state(ctx) {
            UpdateState::Ready { version } => (version, UpdateAction::Restart),
            UpdateState::Available { version } => (version, UpdateAction::Download),
            _ => return None,
        };
        if self.dismissed.as_deref() == Some(version.as_str()) {
            return None;
        }
        let ready = action == UpdateAction::Restart;
        let colors = crate::shared::ui::theme::colors(ctx);
        let mut picked = None;
        let mut dismissed = false;
        egui::Area::new(egui::Id::new("update-card"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 46.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(colors.surface)
                    .stroke(egui::Stroke::new(1.0_f32, colors.accent))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::ARROW_CIRCLE_UP)
                                    .font(crate::shared::ui::theme::icon_font(18.0))
                                    .color(colors.accent),
                            );
                            let status = if ready { "is ready" } else { "is out" };
                            ui.label(
                                egui::RichText::new(format!("Viper Music {version} {status}"))
                                    .size(13.0)
                                    .strong()
                                    .color(colors.text),
                            );
                            let label = if ready { "Restart" } else { "Download" };
                            if crate::shared::ui::controls::action_button(ui, label, true).clicked() {
                                picked = Some(action);
                            }
                            dismissed = ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(egui_phosphor::regular::X)
                                            .font(crate::shared::ui::theme::icon_font(14.0))
                                            .color(colors.muted),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("Dismiss")
                                .clicked();
                        });
                    });
            });
        if dismissed {
            self.dismissed = Some(version);
        }
        picked
    }
}

impl Shared {
    fn state(&self) -> Option<UpdateState> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, state: UpdateState) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = Some(state);
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }

    /// Whether an hour has passed since the last check.
    fn due(&self) -> bool {
        self.last_check
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none_or(|last| last.elapsed().map_or(true, |since| since >= CHECK_EVERY))
    }

    /// Check for a newer release, then download and install it. A check the user asked for
    /// (`watched`) shows its progress and its errors; a background one changes the state only
    /// when it finds something, so going offline does not hide an update already found.
    fn check(&self, watched: bool) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        *self
            .last_check
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(SystemTime::now());
        // An installed update waits for the restart; checking again would download it again.
        if !matches!(self.state(), Some(UpdateState::Ready { .. })) {
            if watched {
                self.set(UpdateState::Checking);
            }
            if let Err(error) = self.update(watched) {
                eprintln!("Update failed: {error:#}");
                let shown = self.state();
                if watched || matches!(shown, Some(UpdateState::Downloading { .. })) {
                    self.set(UpdateState::Failed {
                        message: format!("{error:#}"),
                    });
                } else if shown == Some(UpdateState::Checking) {
                    self.set(UpdateState::Idle);
                }
            }
        }
        self.busy.store(false, Ordering::SeqCst);
    }

    fn update(&self, watched: bool) -> anyhow::Result<()> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("ViperMusic/", env!("CARGO_PKG_VERSION")))
            .timeout(CHECK_TIMEOUT)
            .build()?;
        let release: Release = client
            .get(LATEST_RELEASE_API)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .context("Could not reach GitHub")?
            .json()
            .context("GitHub sent an unreadable release")?;
        let version = release.tag_name.trim_start_matches('v');
        let latest = semver::Version::parse(version)
            .with_context(|| format!("Unrecognized release tag {}", release.tag_name))?;
        let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        if latest <= current {
            if watched || matches!(self.state(), None | Some(UpdateState::Failed { .. })) {
                self.set(UpdateState::UpToDate);
            }
            return Ok(());
        }
        let version = latest.to_string();
        let Some(target) = installed_appimage() else {
            self.set(UpdateState::Available { version });
            return Ok(());
        };
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with(".AppImage"))
            .context("The release has no AppImage")?;
        let digest = asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .context("The release does not publish the AppImage's checksum")?;
        self.download(&asset.browser_download_url, digest, &target, &version)?;
        self.set(UpdateState::Ready { version });
        Ok(())
    }

    /// Download the AppImage beside `target`, check its SHA-256, and move it over `target`. The
    /// running copy keeps its open file, so it plays on until the restart.
    fn download(&self, url: &str, sha256: &str, target: &Path, version: &str) -> anyhow::Result<()> {
        let mut response = reqwest::blocking::Client::builder()
            .user_agent(concat!("ViperMusic/", env!("CARGO_PKG_VERSION")))
            .timeout(DOWNLOAD_TIMEOUT)
            .build()?
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .context("The download did not start")?;
        let total = response.content_length();
        let partial = target.with_extension("download");
        let result = (|| {
            let mut file = fs::File::create(&partial)
                .with_context(|| format!("Could not write {}", partial.display()))?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0_u8; 1 << 16];
            let mut downloaded = 0_u64;
            // Each whole percent, or each MiB when the size is unknown.
            let step = total.map_or(1 << 20, |total| (total / 100).max(1));
            self.set(UpdateState::Downloading {
                version: version.to_owned(),
                downloaded,
                total,
            });
            loop {
                let read = response.read(&mut buf).context("The download stopped")?;
                if read == 0 {
                    break;
                }
                hasher.update(&buf[..read]);
                file.write_all(&buf[..read])?;
                let before = downloaded;
                downloaded += read as u64;
                if downloaded / step != before / step {
                    self.set(UpdateState::Downloading {
                        version: version.to_owned(),
                        downloaded,
                        total,
                    });
                }
            }
            file.sync_all()?;
            let got = hasher
                .finalize()
                .iter()
                .fold(String::with_capacity(64), |mut hex, byte| {
                    use std::fmt::Write as _;
                    let _ = write!(hex, "{byte:02x}");
                    hex
                });
            ensure!(
                got.eq_ignore_ascii_case(sha256),
                "The downloaded AppImage does not match its checksum"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&partial, fs::Permissions::from_mode(0o755))?;
            }
            fs::rename(&partial, target)
                .with_context(|| format!("Could not replace {}", target.display()))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&partial);
        }
        result
    }
}

/// The installed update to start once the app has quit, if the user asked to restart into it.
///
/// # Returns
/// The AppImage to run, or `None` for a plain quit.
pub fn restart_target() -> Option<&'static Path> {
    RESTART_INTO.get().map(PathBuf::as_path)
}

use eframe::egui;
use image::DynamicImage;
use serde::{Deserialize, Serialize};

/// Cassette color ways the app logo can use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LogoColorway {
    Original,
    Ocean,
    Berry,
    Citrus,
    Electric,
    Tropic,
    Popsicle,
    Arcade,
    Zest,
    CoralReef,
    /// Default, because its frame stands out on both light and dark docks.
    #[default]
    Flare,
    Rave,
    Bubblegum,
    GrapeSoda,
    Mango,
    Lagoon,
    Ultraviolet,
    Sorbet,
    DeepSea,
    LimePop,
    NeonWindow,
    CandySpools,
    MidnightPop,
    Snow,
}

impl LogoColorway {
    /// Every color way, in the order the preferences grid shows them.
    pub(crate) const ALL: [Self; 24] = [
        Self::Original,
        Self::Ocean,
        Self::Berry,
        Self::Citrus,
        Self::Electric,
        Self::Tropic,
        Self::Popsicle,
        Self::Arcade,
        Self::Zest,
        Self::CoralReef,
        Self::Flare,
        Self::Rave,
        Self::Bubblegum,
        Self::GrapeSoda,
        Self::Mango,
        Self::Lagoon,
        Self::Ultraviolet,
        Self::Sorbet,
        Self::DeepSea,
        Self::LimePop,
        Self::NeonWindow,
        Self::CandySpools,
        Self::MidnightPop,
        Self::Snow,
    ];

    /// Human-readable name of the color way.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Ocean => "Ocean",
            Self::Berry => "Berry",
            Self::Citrus => "Citrus",
            Self::Electric => "Electric",
            Self::Tropic => "Tropic",
            Self::Popsicle => "Popsicle",
            Self::Arcade => "Arcade",
            Self::Zest => "Zest",
            Self::CoralReef => "Coral Reef",
            Self::Flare => "Flare",
            Self::Rave => "Rave",
            Self::Bubblegum => "Bubblegum",
            Self::GrapeSoda => "Grape Soda",
            Self::Mango => "Mango",
            Self::Lagoon => "Lagoon",
            Self::Ultraviolet => "Ultraviolet",
            Self::Sorbet => "Sorbet",
            Self::DeepSea => "Deep Sea",
            Self::LimePop => "Lime Pop",
            Self::NeonWindow => "Neon Window",
            Self::CandySpools => "Candy Spools",
            Self::MidnightPop => "Midnight Pop",
            Self::Snow => "Snow",
        }
    }

    /// Bundled 512 pixel PNG of the color way.
    fn png(self) -> &'static [u8] {
        match self {
            Self::Original => include_bytes!("../../assets/logos/original.png"),
            Self::Ocean => include_bytes!("../../assets/logos/ocean.png"),
            Self::Berry => include_bytes!("../../assets/logos/berry.png"),
            Self::Citrus => include_bytes!("../../assets/logos/citrus.png"),
            Self::Electric => include_bytes!("../../assets/logos/electric.png"),
            Self::Tropic => include_bytes!("../../assets/logos/tropic.png"),
            Self::Popsicle => include_bytes!("../../assets/logos/popsicle.png"),
            Self::Arcade => include_bytes!("../../assets/logos/arcade.png"),
            Self::Zest => include_bytes!("../../assets/logos/zest.png"),
            Self::CoralReef => include_bytes!("../../assets/logos/coral-reef.png"),
            Self::Flare => include_bytes!("../../assets/logos/flare.png"),
            Self::Rave => include_bytes!("../../assets/logos/rave.png"),
            Self::Bubblegum => include_bytes!("../../assets/logos/bubblegum.png"),
            Self::GrapeSoda => include_bytes!("../../assets/logos/grape-soda.png"),
            Self::Mango => include_bytes!("../../assets/logos/mango.png"),
            Self::Lagoon => include_bytes!("../../assets/logos/lagoon.png"),
            Self::Ultraviolet => include_bytes!("../../assets/logos/ultraviolet.png"),
            Self::Sorbet => include_bytes!("../../assets/logos/sorbet.png"),
            Self::DeepSea => include_bytes!("../../assets/logos/deep-sea.png"),
            Self::LimePop => include_bytes!("../../assets/logos/lime-pop.png"),
            Self::NeonWindow => include_bytes!("../../assets/logos/neon-window.png"),
            Self::CandySpools => include_bytes!("../../assets/logos/candy-spools.png"),
            Self::MidnightPop => include_bytes!("../../assets/logos/midnight-pop.png"),
            Self::Snow => include_bytes!("../../assets/logos/snow.png"),
        }
    }

    /// Decode the bundled image of the color way.
    ///
    /// # Returns
    /// The full-size logo image.
    pub(crate) fn decode(self) -> DynamicImage {
        image::load_from_memory(self.png()).expect("bundled logo PNGs are valid")
    }
}

/// Convert a logo into the window icon data egui sends to the window manager.
///
/// # Arguments
/// * `logo` - Source image, scaled to 256 pixels per side.
pub(crate) fn window_icon(logo: &DynamicImage) -> egui::IconData {
    let icon = logo
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = icon.dimensions();
    egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    }
}

/// Point the installed launcher at an icon of `colorway`, so the dock, app menu, and title bar show it.
///
/// HACK: winit cannot set a Wayland window's icon yet (the `xdg-toplevel-icon` protocol), so the
/// desktop only learns the icon from the launcher. Once winit supports that protocol, set the icon on
/// the window instead and remove this function, the window reopen in `AppWindow::logic`, and the
/// icon handling in `scripts/build.sh`.
///
/// Every change installs the icon under a name never used before, because the window manager and
/// panel keep icons, and failed lookups, cached by name. Reusing a name, such as when switching back
/// to an earlier pick, would show whatever they cached for it. Only the installer creates the
/// launcher, so runs without one, such as the AppImage, leave the desktop files alone.
///
/// # Arguments
/// * `colorway` - Color way to install.
///
/// # Returns
/// Whether the launcher now names a different icon, which open windows pick up only when reopened.
///
/// # Errors
/// Returns an I/O error if the launcher or icons cannot be read, written, or removed.
#[cfg(target_os = "linux")]
pub(crate) fn install_launcher_icon(colorway: LogoColorway) -> std::io::Result<bool> {
    use std::{
        env, fs, io,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    let Some(data_home) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    else {
        return Ok(false);
    };
    let launcher = data_home.join("applications/viper.desktop");
    let entry = match fs::read_to_string(&launcher) {
        Ok(entry) => entry,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let theme = data_home.join("icons/hicolor");
    let apps = theme.join("512x512/apps");
    let id = serde_json::to_value(colorway)?;
    let prefix = format!("viper-logo-{}-", id.as_str().unwrap_or_default());
    let current = entry.lines().find_map(|line| line.strip_prefix("Icon="));
    if let Some(current) = current.filter(|current| current.starts_with(&prefix)) {
        if fs::read(apps.join(format!("{current}.png")))
            .ok()
            .as_deref()
            == Some(colorway.png())
        {
            return Ok(false);
        }
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let name = format!("{prefix}{stamp}");
    let icon = apps.join(format!("{name}.png"));
    fs::create_dir_all(&apps)?;
    let staged = icon.with_extension("png.tmp");
    fs::write(&staged, colorway.png())?;
    fs::rename(&staged, &icon)?;
    // Icon caches compare the theme directory's modification time to notice new icons.
    fs::File::open(&theme)?.set_modified(SystemTime::now())?;

    let icon_line = format!("Icon={name}");
    let updated: String = entry
        .lines()
        .map(|line| {
            if line.starts_with("Icon=") {
                icon_line.as_str()
            } else {
                line
            }
        })
        .flat_map(|line| [line, "\n"])
        .collect();
    let staged = launcher.with_extension("desktop.tmp");
    fs::write(&staged, updated)?;
    fs::rename(&staged, &launcher)?;

    // Earlier picks are unused once the launcher names this one.
    let installed = format!("{name}.png");
    for file in fs::read_dir(&apps)? {
        let file_name = file?.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with("viper-logo-") && file_name != installed {
            match fs::remove_file(apps.join(&*file_name)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
    }
    Ok(true)
}

/// Rebuild KDE's application cache so the taskbar reads the launcher's new icon.
///
/// HACK: see [`install_launcher_icon`]. The taskbar looks up a window's icon in this cache when the
/// window appears, and the desktop otherwise rebuilds it on its own schedule. Blocks until the
/// rebuild finishes; desktops without the tool skip it.
#[cfg(target_os = "linux")]
pub(crate) fn refresh_launcher_cache() {
    let _ = std::process::Command::new("kbuildsycoca6").output();
}

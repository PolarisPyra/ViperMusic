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

/// Replace the installed launcher icon with `colorway`, so the dock and app menu show it.
///
/// Only the installer creates the launcher icon, so runs without one, such as the AppImage, leave
/// the icon theme alone. Desktops may show the new icon only after the app next starts.
///
/// # Arguments
/// * `colorway` - Color way to install.
///
/// # Errors
/// Returns an I/O error if the installed icon cannot be read or replaced.
#[cfg(target_os = "linux")]
pub(crate) fn install_launcher_icon(colorway: LogoColorway) -> std::io::Result<()> {
    use std::{env, fs, io, path::PathBuf, time::SystemTime};

    let Some(data_home) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    else {
        return Ok(());
    };
    let theme = data_home.join("icons/hicolor");
    let path = theme.join("512x512/apps/viper.png");
    let png = colorway.png();
    match fs::read(&path) {
        Ok(current) if current == png => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    let staged = path.with_extension("png.tmp");
    fs::write(&staged, png)?;
    fs::rename(&staged, &path)?;
    // Icon caches compare the theme directory's modification time to notice changes.
    fs::File::open(&theme)?.set_modified(SystemTime::now())
}

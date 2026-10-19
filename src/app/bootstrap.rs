use crate::{app::ViperApp, platform::persistence::settings::Settings};

const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/logo.png");

fn load_app_fonts() -> eframe::egui::FontDefinitions {
    use eframe::egui::{FontData, FontDefinitions, FontFamily};
    use font_kit::{family_name::FamilyName, properties::Properties, source::SystemSource};
    use std::sync::Arc;

    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        "inter".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/InterVariable.ttf"
        ))),
    );
    definitions
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    egui_phosphor::add_to_fonts(&mut definitions, egui_phosphor::Variant::Regular);
    definitions.families.insert(
        FontFamily::Name("phosphor".into()),
        vec!["phosphor".to_owned()],
    );
    definitions.font_data.insert(
        "phosphor-fill".to_owned(),
        Arc::new(egui_phosphor::Variant::Fill.font_data()),
    );
    definitions.families.insert(
        FontFamily::Name("phosphor-fill".into()),
        vec!["phosphor-fill".to_owned()],
    );

    let families = [
        "Noto Sans CJK JP",
        "Noto Sans JP",
        "Yu Gothic",
        "Yu Gothic UI",
        "Meiryo",
        "Hiragino Sans",
        "Hiragino Kaku Gothic ProN",
    ]
    .iter()
    .map(|family| FamilyName::Title((*family).to_owned()))
    .collect::<Vec<_>>();
    let Ok(handle) = SystemSource::new().select_best_match(&families, &Properties::new()) else {
        return definitions;
    };
    let Ok(font) = handle.load() else {
        return definitions;
    };
    let Some(bytes) = font.copy_font_data() else {
        return definitions;
    };
    drop(font);
    let bytes = Arc::try_unwrap(bytes).unwrap_or_else(|shared| shared.as_ref().clone());
    definitions.font_data.insert(
        "japanese-fallback".to_owned(),
        Arc::new(FontData::from_owned(bytes)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .push("japanese-fallback".to_owned());
    }
    definitions
}

/// Launch the native Viper Music application window.
///
/// # Returns
/// The result from the eframe event loop.
///
/// # Errors
/// Returns an eframe error if native application startup fails.
pub fn run() -> eframe::Result<()> {
    let settings = Settings::load();
    let window_size = settings
        .as_ref()
        .ok()
        .and_then(|settings| settings.window_size)
        .filter(|[width, height]| width.is_finite() && height.is_finite())
        .map(|[width, height]| [width.max(760.0), height.max(520.0)])
        .unwrap_or([1440.0, 900.0]);
    let icon = image::load_from_memory(APP_ICON_PNG)
        .expect("failed to load assets/logo.png")
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let options = eframe::NativeOptions {
        persist_window: false,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id("viper")
            .with_inner_size(window_size)
            .with_min_inner_size([760.0, 520.0])
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "Viper Music",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_fonts(load_app_fonts());
            cc.egui_ctx.style_mut(|style| {
                style.spacing.scroll.dormant_background_opacity = 0.0;
                style.spacing.scroll.active_background_opacity = 0.0;
                style.spacing.scroll.interact_background_opacity = 0.0;
                style.interaction.selectable_labels = false;
            });
            Ok(Box::new(ViperApp::with_settings_result(settings)))
        }),
    )
}

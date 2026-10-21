use crate::{
    app::ViperApp,
    platform::{
        persistence::settings::Settings,
        tray::{self, TrayEvent},
    },
};
use eframe::egui;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc, Mutex, PoisonError,
    },
    time::Duration,
};

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

/// Delegates to the shared app so its state survives the window closing to the tray.
struct AppWindow {
    app: Rc<RefCell<ViperApp>>,
    tray_events: Rc<Receiver<TrayEvent>>,
    reopen: Rc<Cell<bool>>,
}

impl eframe::App for AppWindow {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut app = self.app.borrow_mut();
        while let Ok(event) = self.tray_events.try_recv() {
            match event {
                // Wayland ignores unminimize and focus requests, so bring the window forward by
                // recreating it.
                TrayEvent::Show => {
                    if !ctx.input(|input| input.viewport().focused.unwrap_or(false)) {
                        self.reopen.set(true);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                TrayEvent::Play => app.tray_play(),
                TrayEvent::Pause => app.tray_pause(),
                TrayEvent::Quit => {
                    app.workbench.quit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        app.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.borrow_mut().ui(ui, frame);
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.borrow_mut().on_exit(gl);
    }
}

/// Launch the native Viper Music application window.
///
/// Closing the window hides Viper Music to the system tray when one is available; playback keeps
/// running and the tray menu reopens the window or quits.
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
    let logo = image::load_from_memory(APP_ICON_PNG).expect("failed to load assets/logo.png");
    let icon = logo
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let options = eframe::NativeOptions {
        persist_window: false,
        viewport: egui::ViewportBuilder::default()
            .with_app_id("viper")
            .with_inner_size(window_size)
            .with_min_inner_size([760.0, 520.0])
            .with_icon(egui::IconData {
                rgba: icon.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        ..Default::default()
    };
    let (tray_sender, tray_receiver) = mpsc::channel();
    // The open window's context, so tray clicks can wake its event loop.
    let window_ctx: Arc<Mutex<Option<egui::Context>>> = Arc::default();
    let tray = tray::create(&logo, tray_sender, {
        let window_ctx = Arc::clone(&window_ctx);
        move || {
            if let Some(ctx) = window_ctx
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
            {
                ctx.request_repaint();
            }
        }
    })
    .inspect_err(|error| eprintln!("System tray unavailable: {error}"))
    .ok();
    let tray_events = Rc::new(tray_receiver);
    let app = Rc::new(RefCell::new(ViperApp::with_settings_result(settings)));
    let reopen = Rc::new(Cell::new(false));
    loop {
        eframe::run_native(
            "Viper Music",
            options.clone(),
            Box::new({
                let app = Rc::clone(&app);
                let tray_events = Rc::clone(&tray_events);
                let reopen = Rc::clone(&reopen);
                let window_ctx = Arc::clone(&window_ctx);
                move |cc| {
                    *window_ctx.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some(cc.egui_ctx.clone());
                    cc.egui_ctx.set_fonts(load_app_fonts());
                    cc.egui_ctx.global_style_mut(|style| {
                        style.spacing.scroll.dormant_background_opacity = 0.0;
                        style.spacing.scroll.active_background_opacity = 0.0;
                        style.spacing.scroll.interact_background_opacity = 0.0;
                        style.interaction.selectable_labels = false;
                    });
                    let mut viper = app.borrow_mut();
                    cc.egui_ctx.set_zoom_factor(viper.settings.ui_zoom);
                    // Textures belong to the previous window's context.
                    viper.artwork_cache.clear();
                    drop(viper);
                    Ok(Box::new(AppWindow {
                        app,
                        tray_events,
                        reopen,
                    }))
                }
            }),
        )?;
        if reopen.replace(false) {
            continue;
        }
        if tray.is_none() || app.borrow().workbench.quit_requested {
            return Ok(());
        }
        // Wayland cannot remap a hidden window, so the window is destroyed and recreated on Show.
        let idle_ctx = egui::Context::default();
        loop {
            match tray_events.recv_timeout(Duration::from_millis(50)) {
                Ok(TrayEvent::Show) => break,
                Ok(TrayEvent::Play) => app.borrow().tray_play(),
                Ok(TrayEvent::Pause) => app.borrow().tray_pause(),
                Ok(TrayEvent::Quit) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
                Err(RecvTimeoutError::Timeout) => app.borrow_mut().tick_in_background(&idle_ctx),
            }
        }
    }
}

use crate::{
    app::ViperApp,
    platform::{instance, persistence::settings::Settings, session::DisplaySocket, tray},
    shared::{repaint::Repaint, ui::icons::Assets},
};
use gpui::{AppContext, QuitMode};
use std::{borrow::Cow, sync::mpsc};

/// Interface font, also used by the component library.
const INTER: &[u8] = include_bytes!("../../assets/fonts/InterVariable.ttf");

/// Launch the native Viper Music application window.
///
/// Closing the window hides Viper Music to the system tray when one is available; playback keeps
/// running and the tray menu reopens the window or quits.
pub fn run() {
    let display = DisplaySocket::current();
    let settings = Settings::load();
    let colorway = settings
        .as_ref()
        .ok()
        .map(|settings| settings.logo)
        .unwrap_or_default();
    // The window manager reads the launcher icon when the window opens, so update it first.
    #[cfg(target_os = "linux")]
    match crate::shared::logo::install_launcher_icon(colorway) {
        Ok(true) => crate::shared::logo::refresh_launcher_cache(),
        Ok(false) => {}
        Err(error) => eprintln!("Launcher icon could not change: {error}"),
    }
    let logo = colorway.decode();
    let (repaint, wakes) = Repaint::new();
    let (tray_sender, tray_receiver) = mpsc::channel();
    let tray = tray::create(&logo, tray_sender.clone(), {
        let repaint = repaint.clone();
        move || repaint.request_repaint()
    })
    .inspect_err(|error| eprintln!("System tray unavailable: {error}"))
    .ok();
    // Launching Viper Music again, such as from the app menu, shows the running window.
    instance::listen({
        let repaint = repaint.clone();
        move || {
            let _ = tray_sender.send(tray::TrayEvent::Show);
            repaint.request_repaint();
        }
    });
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            if let Err(error) = cx.text_system().add_fonts(vec![Cow::Borrowed(INTER)]) {
                eprintln!("Interface font could not load: {error}");
            }
            // Without a tray, closing the window is the only way to quit.
            cx.set_quit_mode(if tray.is_some() {
                QuitMode::Explicit
            } else {
                QuitMode::LastWindowClosed
            });
            let app = cx.new(|cx| {
                let mut app =
                    ViperApp::with_settings_result(settings, repaint, tray_receiver, display, cx);
                app.tray = tray;
                app.start_polling(wakes, cx);
                app
            });
            ViperApp::open_window(&app, cx);
        });
}

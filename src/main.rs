use std::{
    thread,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = single_instance::SingleInstance::new("viper-music")?;
    if !instance.is_single() {
        if let Err(error) = viper::show_running_instance() {
            eprintln!("Viper Music is already running but did not respond: {error}");
        }
        // Take over if the running instance exits instead of showing its window, such as one left
        // over from an ended session.
        let deadline = Instant::now() + Duration::from_secs(3);
        while !instance.is_single() {
            if Instant::now() >= deadline {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
            instance = single_instance::SingleInstance::new("viper-music")?;
        }
    }
    viper::run()?;
    // Restarting into an installed update: release the instance lock, then become the new copy.
    if let Some(appimage) = viper::restart_target() {
        drop(instance);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            return Err(std::process::Command::new(appimage).exec().into());
        }
    }
    Ok(())
}

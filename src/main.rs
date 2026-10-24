fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instance = single_instance::SingleInstance::new("viper-music")?;
    if !instance.is_single() {
        if let Err(error) = viper::show_running_instance() {
            eprintln!("Viper Music is already running but did not respond: {error}");
        }
        return Ok(());
    }
    viper::run()?;
    Ok(())
}

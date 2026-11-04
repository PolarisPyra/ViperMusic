//! Compiles the Discord application ID into the app, from the environment or the gitignored `.env`
//! at the repository root. Release builds get it from the `DISCORD_APPLICATION_ID` repository
//! secret; without one, Discord Rich Presence is left out.
use std::path::Path;

const APPLICATION_ID: &str = "DISCORD_APPLICATION_ID";

fn main() {
    let env_file = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    println!("cargo:rerun-if-changed={}", env_file.display());
    println!("cargo:rerun-if-env-changed={APPLICATION_ID}");
    // Values already in the environment win over the file.
    if env_file.exists() {
        dotenvy::from_path(&env_file).expect("could not read .env");
    }
    if let Some(id) = std::env::var(APPLICATION_ID)
        .ok()
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
    {
        println!("cargo:rustc-env={APPLICATION_ID}={id}");
    }
}

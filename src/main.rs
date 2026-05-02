use miniscript::bitcoin;

mod main_cli;
#[cfg(feature = "gui")]
mod main_gui;
#[cfg(feature = "gui")]
mod styles;
mod util;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    main_cli::run(bitcoin::Network::Bitcoin)?;
    Ok(())
}

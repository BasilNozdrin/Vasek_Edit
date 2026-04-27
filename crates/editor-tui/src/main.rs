//! Entry point for the vasek-edit terminal editor.

mod logging;

fn main() -> anyhow::Result<()> {
    logging::init()?;
    tracing::info!("vasek-edit starting");
    Ok(())
}

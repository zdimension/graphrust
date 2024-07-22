use anyhow::Result;
use vergen_gitcl::{BuildBuilder, Emitter, GitclBuilder};

pub fn main() -> Result<()> {
    Emitter::default()
        .add_instructions(&BuildBuilder::default().build_date(true).build()?)?
        .add_instructions(&GitclBuilder::default().sha(true).build()?)?
        .emit()?;
    Ok(())
}

use anyhow::Result;
use vergen::EmitBuilder;

pub fn main() -> Result<()> {
    EmitBuilder::builder().build_date().git_sha(true).emit()?;
    Ok(())
}

use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let descriptor_path = PathBuf::from(env::var("OUT_DIR")?).join("flash-cat.bin");

    tonic_prost_build::configure().file_descriptor_set_path(descriptor_path).bytes(".").compile_protos(&["proto/relay.proto"], &["proto/"])?;

    Ok(())
}

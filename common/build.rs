use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let descriptor_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("flash-cat.bin");
    tonic_build::configure()
        .file_descriptor_set_path(descriptor_path)
        .bytes(["."])
        .compile_protos(&["proto/relay.proto"], &["proto/"])?;
    Ok(())
}

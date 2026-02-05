fn main() {
    built::write_built_file().expect("Failed to acquire build-time information");

    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("windows") {
        let mut res = winres::WindowsResource::new();

        res.set_icon("assets/logos/icons/icon.ico");

        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows resources: {}", e);
            std::process::exit(1);
        }
    }
}

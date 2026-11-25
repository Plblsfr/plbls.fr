fn main() {
    println!("cargo:rerun-if-env-changed=ESTOMPE_MPV_DIR");
    if let Some(dossier) = std::env::var_os("ESTOMPE_MPV_DIR") {
        println!(
            "cargo:rustc-link-search=native={}",
            std::path::Path::new(&dossier).display()
        );
    }
}

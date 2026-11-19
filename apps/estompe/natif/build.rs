const PILE_DU_COMPILATEUR: usize = 256 * 1024 * 1024;

fn main() {
    std::thread::Builder::new()
        .stack_size(PILE_DU_COMPILATEUR)
        .spawn(|| {
            let config = slint_build::CompilerConfiguration::new()
                .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
                .with_style("fluent".into());
            slint_build::compile_with_config("ui/app.slint", config)
                .expect("compilation de l'interface Slint");
        })
        .expect("fil du compilateur Slint")
        .join()
        .expect("compilation de l'interface Slint");
}

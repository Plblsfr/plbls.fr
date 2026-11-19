fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
        .with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("compilation de l'interface Slint");
}

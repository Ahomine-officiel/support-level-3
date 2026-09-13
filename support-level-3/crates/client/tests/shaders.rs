//! Validation des shaders WGSL avec naga (même version que wgpu) :
//! attrape les erreurs de syntaxe / typage / limites sans GPU.

use std::path::PathBuf;

const SHADERS: [&str; 4] = ["world.wgsl", "post.wgsl", "ui.wgsl", "rt.wgsl"];

#[test]
fn validate_all_wgsl() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join("shaders");
    for file in SHADERS {
        let path = dir.join(file);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("lecture {}: {}", path.display(), e));
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{} : erreur de parse WGSL\n{:?}", file, e));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{} : validation naga échouée\n{:?}", file, e));
        assert!(
            !module.entry_points.is_empty(),
            "{} : aucun point d'entrée",
            file
        );
        let _ = info; // validate() = type-check complet du module
    }
}

#[test]
fn world_shader_binds_rt_textures() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("shaders")
            .join("world.wgsl"),
    )
    .unwrap();
    assert!(src.contains("rt0_tex"), "world.wgsl doit échantillonner rt0");
    assert!(src.contains("rt1_tex"), "world.wgsl doit échantillonner rt1");
    // Neutralité hors RT garantie par la texture 1x1 (1,1,0).
    assert!(src.contains("mix(1.0, ao"), "AO appliquée avec mix identité");
}

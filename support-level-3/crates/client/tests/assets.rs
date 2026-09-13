//! Validation des assets : chaque GLTF doit charger, référencer des matériaux
//! connus du moteur, et rester dans un budget de triangles raisonnable.

use std::path::PathBuf;

/// Liste officielle des matériaux chargés par le renderer (gpu/mod.rs).
const MATERIALS: &[&str] = &[
    "concrete", "concrete_dark", "floor_hall", "floor_corridor", "floor_office",
    "floor_server", "floor_arch", "floor_elec", "ceiling", "metal_dark",
    "rack_front", "server_front", "led_strip", "screen_off", "screen_on",
    "terminal_screen", "desk_wood", "terminal_body", "chair_fabric", "door_metal",
    "door_frame", "shelf_metal", "cardboard", "entity_cloth", "entity_mask",
    "eyes", "tech_vest", "tech_pants", "tech_head", "receipt_paper",
    "breaker_panel", "exit_sign", "battery", "extinguisher", "duct",
    "light_panel", "hazard", "poster_a", "poster_b", "poster_c", "poster_d",
];

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models")
}

#[test]
fn models_load_and_materials_exist() {
    let dir = models_dir();
    let mut count = 0usize;
    let entries = std::fs::read_dir(&dir).expect("dossier assets/models");
    for entry in entries {
        let p = entry.expect("entrée lisible").path();
        if p.extension().map(|e| e == "gltf").unwrap_or(false) {
            let (doc, buffers, _images) =
                gltf::import(&p).unwrap_or_else(|e| panic!("GLTF invalide {:?}: {}", p, e));
            let mut tris = 0usize;
            let mut prim_count = 0usize;
            for node in doc.nodes() {
                let mesh = match node.mesh() {
                    Some(m) => m,
                    None => continue,
                };
                for prim in mesh.primitives() {
                    prim_count += 1;
                    let mat = prim
                        .material()
                        .name()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    assert!(
                        MATERIALS.contains(&mat.as_str()),
                        "matériau inconnu « {mat} » dans {:?} (ajouter à gpu/mod.rs ?)",
                        p
                    );
                    let reader = prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
                    let positions: Vec<_> = reader
                        .read_positions()
                        .map(|it| it.collect())
                        .unwrap_or_default();
                    let indices: Vec<_> = reader
                        .read_indices()
                        .map(|it| it.into_u32().collect())
                        .unwrap_or_else(|| (0..positions.len() as u32).collect());
                    assert!(!positions.is_empty(), "primitive sans sommets dans {:?}", p);
                    assert!(
                        indices.iter().max().copied().unwrap_or(0) < positions.len() as u32,
                        "indice hors bornes dans {:?}",
                        p
                    );
                    tris += indices.len() / 3;
                }
            }
            assert!(prim_count > 0, "aucune primitive dans {:?}", p);
            assert!(
                tris <= 3000,
                "{:?} trop lourd : {} tris (budget 3000)",
                p,
                tris
            );
            count += 1;
        }
    }
    assert!(count >= 28, "attendu au moins 28 modèles, trouvé {count}");
}

//! Validation des assets : le bundle embarqué doit être complet et cohérent
//! avec `assets/` sur disque, chaque GLTF doit charger via le même chemin
//! mémoire que le jeu, référencer des matériaux connus du moteur, et rester
//! dans un budget de triangles raisonnable.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn walk(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("lire le dossier assets") {
        let p = entry.expect("entrée lisible").path();
        if p.is_dir() {
            walk(&p, root, out);
        } else {
            let rel = p
                .strip_prefix(root)
                .expect("sous-chemin")
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel);
        }
    }
}

/// Le bundle embarqué (ce qui tourne réellement dans le jeu) doit refléter
/// exactement assets/ — sinon rappeler `python3 tools/gen_bundle.py`.
#[test]
fn bundle_covers_disk_exactly() {
    let root = assets_dir();
    let mut disk = BTreeSet::new();
    walk(&root, &root, &mut disk);
    assert!(disk.len() >= 100, "assets/ anormalement vide : {} fichiers", disk.len());

    let bundled: BTreeSet<String> = sl3_client::assets_bundle::BUNDLE
        .iter()
        .map(|(n, _)| (*n).to_string())
        .collect();

    let missing: Vec<&String> = disk.difference(&bundled).collect();
    assert!(
        missing.is_empty(),
        "fichiers présents sur disque mais NON embarqués (relancer tools/gen_bundle.py) : {missing:?}"
    );
    let extra: Vec<&String> = bundled.difference(&disk).collect();
    assert!(
        extra.is_empty(),
        "entrées de bundle sans fichier sur disque (relancer tools/gen_bundle.py) : {extra:?}"
    );
}

/// Chaque GLTF embarqué doit se charger en mémoire (comme le fait le jeu),
/// référencer des matériaux connus et respecter le budget de triangles.
#[test]
fn bundle_models_load_and_materials_exist() {
    let names = sl3_client::assets::stems_with_ext("models", "gltf");
    assert!(
        names.len() >= 28,
        "attendu au moins 28 modèles embarqués, trouvé {}",
        names.len()
    );

    for name in &names {
        let json = sl3_client::assets::read_expect(&format!("models/{name}.gltf"));
        let doc = gltf::Gltf::from_slice(json)
            .unwrap_or_else(|e| panic!("GLTF invalide {name}: {e}"));
        let buffers: Vec<gltf::buffer::Data> = doc
            .buffers()
            .map(|b| match b.source() {
                gltf::buffer::Source::Bin => {
                    gltf::buffer::Data(doc.blob.clone().expect("GLB sans section BIN"))
                }
                gltf::buffer::Source::Uri(uri) => gltf::buffer::Data(
                    sl3_client::assets::read_expect(&format!("models/{uri}")).to_vec(),
                ),
            })
            .collect();

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
                    "matériau inconnu « {mat} » dans {name} (ajouter à gpu/mod.rs ?)"
                );
                let reader =
                    prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
                let positions: Vec<_> = reader
                    .read_positions()
                    .map(|it| it.collect())
                    .unwrap_or_default();
                let indices: Vec<_> = reader
                    .read_indices()
                    .map(|it| it.into_u32().collect())
                    .unwrap_or_else(|| (0..positions.len() as u32).collect());
                assert!(!positions.is_empty(), "primitive sans sommets dans {name}");
                assert!(
                    indices.iter().max().copied().unwrap_or(0) < positions.len() as u32,
                    "indice hors bornes dans {name}"
                );
                tris += indices.len() / 3;
            }
        }
        assert!(prim_count > 0, "aucune primitive dans {name}");
        assert!(
            tris <= 3000,
            "{name} trop lourd : {tris} tris (budget 3000)"
        );
    }
}

//! Accès aux ressources embarquées dans le binaire (voir `assets_bundle.rs`,
//! généré par `tools/gen_bundle.py`). L'exécutable est autoportant : plus aucun
//! accès disque aux assets, il se lance depuis n'importe quel dossier courant.

/// Lit une ressource par son chemin relatif à `assets/`
/// (ex. `("models/rack.gltf")`, `("textures/concrete.png")`).
pub fn read(name: &str) -> Option<&'static [u8]> {
    crate::assets_bundle::BUNDLE
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, d)| *d)
}

/// Comme [`read`], mais panique avec un message clair si la ressource manque.
pub fn read_expect(name: &str) -> &'static [u8] {
    read(name).unwrap_or_else(|| panic!("ressource embarquée manquante : {name}"))
}

/// Noms de base (sans extension) des fichiers `*.{ext}` du dossier embarqué
/// `dir`, triés par ordre alphabétique.
pub fn stems_with_ext(dir: &str, ext: &str) -> Vec<&'static str> {
    let prefix = format!("{dir}/");
    let suffix = format!(".{ext}");
    let mut out: Vec<&'static str> = crate::assets_bundle::BUNDLE
        .iter()
        .filter(|(n, _)| n.starts_with(&prefix) && n.ends_with(&suffix))
        .map(|(n, _)| &n[prefix.len()..n.len() - suffix.len()])
        .collect();
    out.sort_unstable();
    out
}

//! Diagnostic RT plein-fidélité : scène statique + dynamiques avec les VRAIES
//! bounds GLTF, rejoue shadow_ray pour des points de sol sous chaque néon
//! (et au-dessus/latéral), nomme la boîte qui bloque.

use glam::{Mat4, Vec3};
use sl3_client::gpu::rtscene::{aabb_of_instance, build_static_boxes, GpuAabb, MAX_RT_DYN_BOXES};
use sl3_shared::map::MapData;

/// Bounds d'un modèle embarqué, sans GPU (même sémantique que load_model).
fn model_bounds(name: &str) -> (Vec3, Vec3) {
    let json = sl3_client::assets::read_expect(&format!("models/{name}.gltf"));
    let doc = gltf::Gltf::from_slice(json).expect("GLTF invalide");
    let buffers: Vec<gltf::buffer::Data> = doc
        .buffers()
        .map(|b| match b.source() {
            gltf::buffer::Source::Bin => {
                gltf::buffer::Data(doc.blob.clone().expect("GLB sans BIN"))
            }
            gltf::buffer::Source::Uri(uri) => {
                gltf::buffer::Data(sl3_client::assets::read_expect(&format!("models/{uri}")).to_vec())
            }
        })
        .collect();
    let (mut bmin, mut bmax) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for node in doc.nodes() {
        let mesh = match node.mesh() {
            Some(m) => m,
            None => continue,
        };
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            for p in reader.read_positions().map(|it| it.collect::<Vec<_>>()).unwrap_or_default() {
                bmin = bmin.min(Vec3::from(p));
                bmax = bmax.max(Vec3::from(p));
            }
        }
    }
    (bmin, bmax)
}

#[test]
fn diag_rt_shadow_blockers_full() {
    let map = MapData::parse();

    // Instances statiques = props de la carte (comme Game::build_static_list).
    let insts: Vec<(String, sl3_client::gpu::InstanceData)> = map
        .props
        .iter()
        .map(|(name, pos, yaw)| {
            (
                name.clone(),
                sl3_client::gpu::InstanceData::new(
                    Mat4::from_translation(*pos) * Mat4::from_rotation_y(*yaw),
                ),
            )
        })
        .collect();

    // Bounds de chaque modèle utilisé (props + dynamiques).
    let mut bounds: std::collections::HashMap<String, (Vec3, Vec3)> = Default::default();
    let mut need: Vec<String> = map.props.iter().map(|(n, _, _)| n.clone()).collect();
    for d in ["door_panel", "server_bay", "tech", "entity"] {
        need.push(d.into());
    }
    for n in need.drain(..) {
        let entry = bounds.entry(n.clone());
        if let std::collections::hash_map::Entry::Vacant(e) = entry {
            let _ = e.insert(model_bounds(&n));
        }
    }

    let statics = build_static_boxes(&map, &insts, |name| bounds.get(name).copied());
    println!("scène : {} boîtes statiques", statics.len());

    // Dynamiques : portes fermées + baies + techs factices hors carte + entité.
    let mut dyns: Vec<GpuAabb> = Vec::new();
    for d in &map.doors {
        let yaw = if d.vertical_passage { 0.0 } else { std::f32::consts::FRAC_PI_2 };
        let m = Mat4::from_translation(d.pos) * Mat4::from_rotation_y(yaw);
        if let Some((lo, hi)) = bounds.get("door_panel") {
            if dyns.len() < MAX_RT_DYN_BOXES {
                dyns.push(aabb_of_instance(&m, *lo, *hi));
            }
        }
    }
    for s in &map.servers {
        let m = Mat4::from_translation(s.pos) * Mat4::from_rotation_y(s.yaw);
        if let Some((lo, hi)) = bounds.get("server_bay") {
            if dyns.len() < MAX_RT_DYN_BOXES {
                dyns.push(aabb_of_instance(&m, *lo, *hi));
            }
        }
    }
    println!("dynamiques : {} boîtes (cap 32)", dyns.len());

    let all: Vec<&GpuAabb> = statics.iter().chain(dyns.iter()).collect();

    let mut blocked = 0usize;
    let mut total = 0usize;
    for (li, l) in map.lights.iter().enumerate().take(24) {
        let lp = Vec3::new(l.pos.x, l.pos.y, l.pos.z);
        for (dx, dz) in [(0.0f32, 0.0f32), (0.6, 0.0), (-0.6, 0.0), (0.0, 0.6), (0.0, -0.6)] {
            let p = Vec3::new(lp.x + dx, 0.025, lp.z + dz);
            let target = lp + Vec3::new(0.0, 0.18, 0.0);
            let to = target - p;
            let dist = to.length() - 0.35;
            let rd = to.normalize();
            total += 1;
            let mut hit: Option<(usize, f32, bool)> = None;
            for (i, b) in all.iter().enumerate() {
                if let Some(t) = ray_aabb(p, rd, **b, dist) {
                    hit = Some((i, t, i >= statics.len()));
                    break;
                }
            }
            if let Some((i, t, is_dyn)) = hit {
                blocked += 1;
                if dx == 0.0 && dz == 0.0 {
                    let lo = all[i].lo;
                    let hi = all[i].hi;
                    println!(
                        "NEON {li} ({:.1},{:.1},{:.1}) BLOQUÉ par boîte #{i} {} t={t:.2} lo({:.2},{:.2},{:.2}) hi({:.2},{:.2},{:.2})",
                        lp.x, lp.y, lp.z,
                        if is_dyn { "DYNAMIQUE" } else { "statique" },
                        lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
                    );
                }
            }
        }
    }
    println!("résultat : {blocked}/{total} rayons bloqués");
}

fn ray_aabb(ro: Vec3, rd: Vec3, b: GpuAabb, tmax: f32) -> Option<f32> {
    let lo = Vec3::new(b.lo[0], b.lo[1], b.lo[2]);
    let hi = Vec3::new(b.hi[0], b.hi[1], b.hi[2]);
    if hi.x <= lo.x || hi.y <= lo.y || hi.z <= lo.z {
        return None;
    }
    let inv = 1.0 / rd;
    let t0 = (lo - ro) * inv;
    let t1 = (hi - ro) * inv;
    let tmin3 = t0.min(t1);
    let tmax3 = t0.max(t1);
    let tmin = tmin3.x.max(tmin3.y).max(tmin3.z.max(0.0));
    let tm = tmax3.x.min(tmax3.y).min(tmax3.z);
    if tm < tmin || tmin > tmax {
        None
    } else {
        Some(tmin)
    }
}

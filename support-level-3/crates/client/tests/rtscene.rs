//! Scène de ray tracing : fusion des murs, budget de boîtes, couverture.

use glam::{Mat4, Vec3};
use sl3_client::gpu::rtscene::{
    aabb_of_instance, build_static_boxes, merge_wall_rects, MAX_RT_DYN_BOXES,
    MAX_RT_STATIC_BOXES,
};
use sl3_client::gpu::InstanceData;
use sl3_shared::map::{MapData, CELL, WALL_H};

fn fake_bounds(_name: &str) -> Option<(Vec3, Vec3)> {
    Some((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5)))
}

#[test]
fn wall_merge_covers_every_wall_cell() {
    let rects = merge_wall_rects();
    assert!(!rects.is_empty(), "aucun rectangle de mur");
    assert!(
        rects.len() < 120,
        "trop de rectangles de murs ({}) : la fusion est inefficace",
        rects.len()
    );

    // Chaque cellule '#' de la carte doit être couverte par un rectangle.
    let h = sl3_shared::map::MAP.len();
    let w = sl3_shared::map::MAP[0].len();
    for r in 0..h {
        for c in 0..w {
            if sl3_shared::map::MAP[r].chars().nth(c) != Some('#') {
                continue;
            }
            let center = Vec3::new(
                (c as f32 + 0.5) * CELL,
                WALL_H * 0.5,
                (r as f32 + 0.5) * CELL,
            );
            let covered = rects.iter().any(|&(c0, r0, c1, r1)| {
                c >= c0 && c <= c1 && r >= r0 && r <= r1
            });
            // Les cellules hors rectangle = bug de fusion.
            assert!(covered, "cellule mur ({c},{r}) non couverte par la fusion");
            let _ = center;
        }
    }
}

#[test]
fn static_scene_budget_and_coverage() {
    let map = MapData::parse();
    let insts: Vec<(String, InstanceData)> = map
        .props
        .iter()
        .map(|(name, pos, yaw)| {
            (
                name.clone(),
                InstanceData::new(
                    Mat4::from_translation(*pos) * Mat4::from_rotation_y(*yaw),
                ),
            )
        })
        .collect();
    let boxes = build_static_boxes(&map, &insts, fake_bounds);

    assert!(boxes.len() >= 10, "scène RT trop vide : {}", boxes.len());
    assert!(
        boxes.len() <= MAX_RT_STATIC_BOXES,
        "scène RT hors budget : {} > {}",
        boxes.len(),
        MAX_RT_STATIC_BOXES
    );

    // Sol + plafond présents (y=0 et y=WALL_H).
    let has_floor = boxes.iter().any(|b| b.lo[1] <= -0.1 && b.hi[1] >= 0.0);
    let has_ceil = boxes
        .iter()
        .any(|b| b.lo[1] <= WALL_H && b.hi[1] >= WALL_H + 0.1);
    assert!(has_floor, "boîte de sol manquante");
    assert!(has_ceil, "boîte de plafond manquante");

    // Chaque cellule mur jouxtant un sol doit être dans une boîte (x/z au
    // centre de la cellule, y à mi-hauteur).
    let h = map.h;
    let w = map.w;
    for r in 0..h {
        for c in 0..w {
            let ch = sl3_shared::map::MAP[r].chars().nth(c).unwrap();
            if ch != '#' {
                continue;
            }
            let p = Vec3::new((c as f32 + 0.5) * CELL, 1.5, (r as f32 + 0.5) * CELL);
            assert!(
                boxes.iter().any(|b| b.contains(p)),
                "cellule mur ({c},{r}) absente de la scène RT"
            );
        }
    }
}

#[test]
fn instance_aabb_respects_yaw() {
    // Boîte locale 1x2x1 tournée de 45° -> AABB élargie d'environ sqrt(2).
    let m = Mat4::from_translation(Vec3::new(10.0, 0.0, 10.0))
        * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let b = aabb_of_instance(&m, Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5));
    let ext_x = b.hi[0] - b.lo[0];
    assert!(
        (ext_x - 2.0_f32.sqrt()).abs() < 0.01,
        "AABB tournée incorrecte : {ext_x}"
    );
    assert!((b.lo[1] - 0.0).abs() < 1e-4 && (b.hi[1] - 2.0).abs() < 1e-4);
}

#[test]
fn degenerate_boxes_never_occlude() {
    // Les boîtes de padding (ZERO) ont une AABB vide : le shader les ignore.
    // Côté Rust, on vérifie juste la cohérence lo<hi des boîtes construites.
    let map = MapData::parse();
    let boxes = build_static_boxes(&map, &[], fake_bounds);
    for (i, b) in boxes.iter().enumerate() {
        assert!(
            b.hi[0] > b.lo[0] && b.hi[1] > b.lo[1] && b.hi[2] > b.lo[2],
            "boîte statique {} dégénérée",
            i
        );
    }
    let _ = MAX_RT_DYN_BOXES;
}

//! Scène de ray tracing (côté CPU, sans device) : liste d'AABB décrivant
//! l'occlusion de l'étage — sol/plafond fusionnés, murs fusionnés en
//! rectangles maximaux, mobilier important. Les rayons GPU (rt.wgsl)
//! testent ces boîtes : ombres douces, occlusion ambiante, rebond (GI).
//!
//! Budget volontairement serré (i7 7e gén. friendly quand RT désactivé,
//! RTX 2060 friendly quand activé) : ~80 boîtes pour cette carte.

use glam::Vec3;
use sl3_shared::map::{MapData, CELL, MAP, WALL_H};

use super::InstanceData;

/// Nombre max de boîtes statiques uploadées au GPU.
pub const MAX_RT_STATIC_BOXES: usize = 384;
/// Nombre max de boîtes dynamiques (portes, baies, personnages) par frame.
pub const MAX_RT_DYN_BOXES: usize = 32;

/// AABB packée GPU (32 octets, deux vec4).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAabb {
    pub lo: [f32; 4],
    pub hi: [f32; 4],
}

impl GpuAabb {
    pub const ZERO: GpuAabb = GpuAabb { lo: [0.0; 4], hi: [0.0; 4] };

    pub fn from_min_max(lo: Vec3, hi: Vec3) -> GpuAabb {
        GpuAabb {
            lo: [lo.x.min(hi.x), lo.y.min(hi.y), lo.z.min(hi.z), 0.0],
            hi: [lo.x.max(hi.x), lo.y.max(hi.y), lo.z.max(hi.z), 0.0],
        }
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.x >= self.lo[0] && p.x <= self.hi[0] && p.y >= self.lo[1] && p.y <= self.hi[1] && p.z >= self.lo[2] && p.z <= self.hi[2]
    }
}

fn is_wall_cell(c: usize, r: usize) -> bool {
    r < MAP.len() && c < MAP[r].len() && MAP[r].chars().nth(c) == Some('#')
}

/// Fusion gourmande des cellules `#` de la carte en rectangles maximaux
/// (c0, r0, c1, r1) inclusifs — ~20-40 boîtes au lieu de ~700 cellules.
pub fn merge_wall_rects() -> Vec<(usize, usize, usize, usize)> {
    let h = MAP.len();
    let w = MAP.first().map(|r| r.len()).unwrap_or(0);
    let mut visited = vec![false; w * h];
    let mut out = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if visited[r * w + c] || !is_wall_cell(c, r) {
                continue;
            }
            // Étendre en largeur sur la rangée courante.
            let mut c1 = c;
            while c1 + 1 < w && is_wall_cell(c1 + 1, r) && !visited[r * w + c1 + 1] {
                c1 += 1;
            }
            // Étendre en hauteur tant que tout le segment est mur non visité.
            let mut r1 = r;
            'rows: while r1 + 1 < h {
                for cc in c..=c1 {
                    if !is_wall_cell(cc, r1 + 1) || visited[(r1 + 1) * w + cc] {
                        break 'rows;
                    }
                }
                r1 += 1;
            }
            for rr in r..=r1 {
                for cc in c..=c1 {
                    visited[rr * w + cc] = true;
                }
            }
            out.push((c, r, c1, r1));
        }
    }
    out
}

/// AABB monde d'une instance : transforme les 8 coins de la boîte locale.
pub fn aabb_of_instance(m: &glam::Mat4, lo: Vec3, hi: Vec3) -> GpuAabb {
    let mut mn = Vec3::splat(f32::MAX);
    let mut mx = Vec3::splat(f32::MIN);
    for i in 0..8u32 {
        let c = Vec3::new(
            if i & 1 != 0 { hi.x } else { lo.x },
            if i & 2 != 0 { hi.y } else { lo.y },
            if i & 4 != 0 { hi.z } else { lo.z },
        );
        let w = m.transform_point3(c);
        mn = mn.min(w);
        mx = mx.max(w);
    }
    GpuAabb::from_min_max(mn, mx)
}

/// Modèles dont l'occlusion compte pour les rayons (le reste : déco fine,
/// câbles, affiches — trop petits pour justifier leur coût de test).
const RT_PROP_ALLOW: [&str; 8] = [
    "rack", "desk_set", "shelf", "box_small", "pillar", "breaker", "terminal", "exit_door",
];

/// Construit la scène statique complète : sol + plafond (2 boîtes),
/// murs fusionnés, mobilier autorisé transformé.
pub fn build_static_boxes(
    map: &MapData,
    insts: &[(String, InstanceData)],
    bounds_of: impl Fn(&str) -> Option<(Vec3, Vec3)>,
) -> Vec<GpuAabb> {
    let mut out: Vec<GpuAabb> = Vec::new();
    let (mw, mh) = (map.w as f32 * CELL, map.h as f32 * CELL);

    // Sol et plafond : deux grandes dalles (le plafond laisse la place aux
    // néons à y = WALL_H - 0.15 ; les boîtes commencent à WALL_H - 0.02).
    out.push(GpuAabb::from_min_max(
        Vec3::new(-0.5, -0.22, -0.5),
        Vec3::new(mw + 0.5, 0.0, mh + 0.5),
    ));
    out.push(GpuAabb::from_min_max(
        Vec3::new(-0.5, WALL_H - 0.02, -0.5),
        Vec3::new(mw + 0.5, WALL_H + 0.25, mh + 0.5),
    ));

    // Murs fusionnés (léger débord pour boucher les coutures).
    for (c0, r0, c1, r1) in merge_wall_rects() {
        let lo = Vec3::new(c0 as f32 * CELL - 0.02, 0.0, r0 as f32 * CELL - 0.02);
        let hi = Vec3::new((c1 as f32 + 1.0) * CELL + 0.02, WALL_H + 0.05, (r1 as f32 + 1.0) * CELL + 0.02);
        out.push(GpuAabb::from_min_max(lo, hi));
    }

    // Mobilier (baies serveurs = dynamiques, gérées par frame).
    for (name, inst) in insts {
        if !RT_PROP_ALLOW.contains(&name.as_str()) {
            continue;
        }
        if let Some((lo, hi)) = bounds_of(name) {
            out.push(aabb_of_instance(&inst.model, lo, hi));
        }
    }

    out.truncate(MAX_RT_STATIC_BOXES);
    out
}

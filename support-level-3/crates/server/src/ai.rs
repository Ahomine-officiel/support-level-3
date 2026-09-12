//! IA de « L'Auditeur » : patrouille, ouïe, vision, poursuite, capture.

use crate::room::{room_announce, room_sfx, Entity, Game};
use glam::Vec3;
use rand::Rng;
use sl3_shared::consts::*;
use sl3_shared::map::MapData;
use sl3_shared::protocol::SfxKind;

/// Bruit entendu (position + rayon d'audition).
pub fn hear_noise(entity: &mut Entity, map: &MapData, pos: Vec3, radius: f32) {
    if entity.state == ENTITY_CHASE {
        return; // en chasse : il sait déjà où vous êtes
    }
    let d = (entity.pos - pos).length();
    if d < radius {
        let cell = ((pos.x / map::CELL).floor() as isize, (pos.z / map::CELL).floor() as isize);
        if entity.state != ENTITY_INVESTIGATE || entity.path.is_empty() {
            entity.state = ENTITY_INVESTIGATE;
            entity.state_timer = 0.0;
            entity.path.clear();
            let from = map.nearest_floor_cell(entity.pos);
            if let Some(path) = map.bfs_path(from, cell) {
                entity.path = path.into();
            }
        }
    }
}

use sl3_shared::map as map;

fn bfs_to(map: &MapData, entity: &Entity, to: (isize, isize)) -> Option<std::collections::VecDeque<(isize, isize)>> {
    let from = map.nearest_floor_cell(entity.pos);
    map.bfs_path(from, to).map(std::collections::VecDeque::from)
}

pub fn update(g: &mut Game, dt: f32) {
    let map = g.map;

    // --- Détection (vision) ---
    let seen = if g.entity.state != ENTITY_COOLDOWN_STATE {
        let mut found: Option<u8> = None;
        for p in g.players.iter() {
            if p.state != PLAYER_ALIVE {
                continue;
            }
            let to_p = p.pos - g.entity.pos;
            let dist = to_p.length();
            let mut detect_dist = ENTITY_SIGHT_DIST;
            if p.sprint {
                detect_dist *= 1.25;
            }
            if dist < detect_dist && dist > 0.01 {
                let dir = to_p / dist;
                let facing = Vec3::new(g.entity.yaw.sin(), 0.0, g.entity.yaw.cos());
                let in_fov = facing.dot(Vec3::new(dir.x, 0.0, dir.z)) > ENTITY_SIGHT_COS;
                let very_close = dist < 2.5;
                if (in_fov || very_close)
                    && map.los(g.entity.pos + Vec3::new(0.0, 1.5, 0.0), p.pos + Vec3::new(0.0, 1.2, 0.0))
                {
                    found = Some(p.id);
                    g.entity.last_seen = p.pos;
                    break;
                }
            }
        }
        found
    } else {
        None
    };
    if let Some(pid) = seen {
        if g.entity.state != ENTITY_CHASE {
            g.entity.state = ENTITY_CHASE;
            g.entity.target = Some(pid);
            g.entity.state_timer = 0.0;
            g.entity.repath_timer = 0.0;
            announce_chase(g, true);
        }
    }

    // --- Machine à états ---
    match g.entity.state {
        ENTITY_PATROL => {
            if g.entity.path.is_empty() {
                for _ in 0..24 {
                    let c = g.rng.gen_range(0..map.w) as isize;
                    let r = g.rng.gen_range(0..map.h) as isize;
                    if map.is_floor(c, r) {
                        if let Some(path) = bfs_to(map, &g.entity, (c, r)) {
                            g.entity.path = path;
                            break;
                        }
                    }
                }
            }
        }
        ENTITY_INVESTIGATE => {
            g.entity.state_timer += dt;
            if g.entity.path.is_empty() {
                g.entity.state = ENTITY_SEARCH;
                g.entity.state_timer = 0.0;
            }
        }
        ENTITY_CHASE => {
            g.entity.state_timer += dt;
            g.entity.repath_timer -= dt;
            if g.entity.repath_timer <= 0.0 {
                g.entity.repath_timer = 0.4;
                if let Some(pid) = g.entity.target {
                    if let Some(p) = g
                        .players
                        .iter()
                        .find(|p| p.id == pid && p.state == PLAYER_ALIVE)
                    {
                        g.entity.last_seen = p.pos;
                        let to = map.nearest_floor_cell(p.pos);
                        if let Some(path) = bfs_to(map, &g.entity, to) {
                            g.entity.path = path;
                        }
                    }
                }
            }
            let target_alive = g
                .entity
                .target
                .and_then(|pid| g.players.iter().find(|p| p.id == pid))
                .map(|p| p.state == PLAYER_ALIVE)
                .unwrap_or(false);
            if !target_alive {
                g.entity.state = ENTITY_SEARCH;
                g.entity.state_timer = 0.0;
                g.entity.target = None;
                announce_chase(g, false);
            } else if g.entity.state_timer > 30.0 {
                g.entity.state = ENTITY_SEARCH;
                g.entity.state_timer = 0.0;
                announce_chase(g, false);
            }
        }
        ENTITY_SEARCH => {
            g.entity.state_timer += dt;
            if g.entity.path.is_empty() {
                let base = map.nearest_floor_cell(g.entity.last_seen);
                for _ in 0..8 {
                    let c = base.0 + g.rng.gen_range(-3..4);
                    let r = base.1 + g.rng.gen_range(-3..4);
                    if map.is_floor(c, r) {
                        if let Some(path) = bfs_to(map, &g.entity, (c, r)) {
                            g.entity.path = path;
                            break;
                        }
                    }
                }
            }
            if g.entity.state_timer > ENTITY_SEARCH_TIME {
                g.entity.state = ENTITY_PATROL;
                g.entity.path.clear();
            }
        }
        ENTITY_COOLDOWN_STATE => {
            g.entity.state_timer -= dt;
            if g.entity.state_timer <= 0.0 {
                g.entity.state = ENTITY_PATROL;
                g.entity.path.clear();
            }
        }
        _ => {}
    }

    // --- Déplacement le long du chemin ---
    let move_speed = match g.entity.state {
        ENTITY_CHASE => ENTITY_CHASE_SPEED,
        ENTITY_INVESTIGATE => ENTITY_INVESTIGATE_SPEED,
        ENTITY_SEARCH => ENTITY_PATROL_SPEED * 1.3,
        ENTITY_COOLDOWN_STATE => ENTITY_PATROL_SPEED * 1.4,
        _ => ENTITY_PATROL_SPEED,
    };
    if !g.entity.path.is_empty() {
        let next = *g.entity.path.front().unwrap();
        let target = map.cell_center(next.0 as usize, next.1 as usize);
        let to = target - g.entity.pos;
        let dist = to.length();
        if dist < 0.15 {
            g.entity.path.pop_front();
        } else {
            let dir = to / dist;
            g.entity.pos += dir * (move_speed * dt).min(dist);
            g.entity.yaw = dir.x.atan2(dir.z);
        }

        // Bruits de pas.
        g.entity.step_acc += move_speed * dt;
        if g.entity.step_acc > 1.15 {
            g.entity.step_acc = 0.0;
            room_sfx(g, SfxKind::EntityStep, g.entity.pos);
        }
    }

    // --- Capture ---
    if g.entity.state == ENTITY_CHASE {
        if let Some(pid) = g.entity.target {
            let mut caught = false;
            let mut cpos = Vec3::ZERO;
            for p in g.players.iter() {
                if p.id == pid
                    && p.state == PLAYER_ALIVE
                    && (p.pos - g.entity.pos).length() < ENTITY_CATCH_DIST
                {
                    caught = true;
                    cpos = p.pos;
                    break;
                }
            }
            if caught {
                for p in g.players.iter_mut() {
                    if p.id == pid {
                        p.state = PLAYER_DOWNED;
                        p.downed_at = g.time;
                    }
                }
                room_sfx(g, SfxKind::Caught, cpos);
                room_sfx(g, SfxKind::Jumpscare, cpos);
                let name = g
                    .players
                    .iter()
                    .find(|p| p.id == pid)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                let (fr, en) = sl3_shared::texts::announce("downed", &[&name], &[&name]);
                room_announce(g, fr, en);
                // Téléportation loin + temps de recharge.
                g.entity.state = ENTITY_COOLDOWN_STATE;
                g.entity.state_timer = ENTITY_COOLDOWN;
                g.entity.target = None;
                g.entity.path.clear();
                let mut best = map.entity_spawn;
                let mut best_d = 0.0f32;
                for _ in 0..24 {
                    let c = g.rng.gen_range(0..map.w) as isize;
                    let r = g.rng.gen_range(0..map.h) as isize;
                    if map.is_floor(c, r) {
                        let p = map.cell_center(c as usize, r as usize);
                        let d = (p - cpos).length();
                        if d > best_d {
                            best_d = d;
                            best = p;
                        }
                    }
                }
                g.entity.pos = best;
            }
        }
    }

    // --- Les portes s'ouvrent (trop violemment) sur son passage ---
    for (i, dd) in map.doors.iter().enumerate() {
        if !g.doors[i] && (g.entity.pos - dd.pos).length() < 1.4 {
            g.doors[i] = true;
            room_sfx(g, SfxKind::DoorSlam, dd.pos);
        }
    }

    // --- Murmures de proximité ---
    let mut nearest = f32::MAX;
    for p in g.players.iter() {
        if p.state == PLAYER_ALIVE {
            nearest = nearest.min((p.pos - g.entity.pos).length());
        }
    }
    if nearest < 7.0 && g.time - g.entity_near_warned > 25.0 {
        g.entity_near_warned = g.time;
        room_sfx(g, SfxKind::Whisper, g.entity.pos);
    }
}

fn announce_chase(g: &Game, start: bool) {
    let (fr, en) = sl3_shared::texts::announce(if start { "chase" } else { "chase_end" }, &[], &[]);
    room_announce(g, fr, en);
    if start {
        room_sfx(g, SfxKind::Screech, g.entity.pos);
    }
}

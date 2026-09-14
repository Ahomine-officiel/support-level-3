//! Bots de test : pathfinding BFS vers les serveurs cassés, réparation.
//! Utilisé pour le smoke test serveur (`--bots N`).

use crate::{Room, RoomCmd};
use glam::Vec3;
use rand::Rng;
use sl3_shared::consts::*;
use sl3_shared::map::MapData;
use sl3_shared::protocol::*;
use std::collections::VecDeque;
use std::sync::Arc;

pub fn spawn_bots(room: &Arc<Room>, n: usize) {
    let map: &'static MapData = Box::leak(Box::new(MapData::parse()));
    for i in 0..n {
        // Enregistrement SYNCHRONE : la liste du lobby est complète immédiatement
        // (important pour l'hébergement local, où le client broadcast juste après).
        let id;
        {
            let mut next = room.next_id.lock().unwrap();
            id = *next;
            *next = next.wrapping_add(1).max(1);
        }
        room.players.lock().unwrap().push(crate::PlayerSlot {
            id,
            name: format!("BOT-{i}"),
            tx: std::sync::mpsc::channel().0,
            alive_conn: false,
        });

        let room2 = room.clone();
        let map = map;
        std::thread::spawn(move || {
            let bot_id = id;
            let bot_index = i;
            let mut rng = rand::thread_rng();
            let mut pos = map.spawn + Vec3::new(bot_index as f32 * 1.5, 0.0, 0.0);
            let mut path: VecDeque<(isize, isize)> = VecDeque::new();
            let mut target_srv = bot_index % SERVER_COUNT;
            let mut hold_ticks = 0u32;
            let mut holding = false;

            loop {
                std::thread::sleep(std::time::Duration::from_secs_f32(0.1));
                let goal_cell = map.nearest_floor_cell(map.servers[target_srv].pos + Vec3::new(1.5, 0.0, 0.0));
                let goal_center = map.cell_center(goal_cell.0 as usize, goal_cell.1 as usize);
                let at_goal = (goal_center - pos).length() < 1.2;

                if path.is_empty() && !at_goal {
                    let from = map.nearest_floor_cell(pos);
                    if let Some(p) = map.bfs_path(from, goal_cell) {
                        path = p.into();
                    } else {
                        target_srv = rng.gen_range(0..SERVER_COUNT);
                        continue;
                    }
                }

                let mut sprint = false;
                if at_goal {
                    // Arrivé à côté du serveur : maintenir E.
                    if !holding {
                        let _ = room2.tx.send(RoomCmd::Player(
                            bot_id,
                            ClientMsg::InteractStart { target: Interactable::Server(target_srv as u8) },
                        ));
                        holding = true;
                    }
                    hold_ticks += 1;
                    if hold_ticks > 80 {
                        let _ = room2.tx.send(RoomCmd::Player(bot_id, ClientMsg::InteractStop));
                        holding = false;
                        hold_ticks = 0;
                        target_srv = rng.gen_range(0..SERVER_COUNT);
                        path.clear();
                    }
                } else if let Some(&(c, r)) = path.front() {
                    let target = map.cell_center(c as usize, r as usize);
                    let to = target - pos;
                    let dist = to.length();
                    if dist < 0.3 {
                        path.pop_front();
                        continue;
                    }
                    sprint = dist > 6.0;
                    let speed = if sprint { SPRINT_SPEED } else { WALK_SPEED };
                    let dir = to / dist;
                    let step = (speed * 0.1).min(dist);
                    pos += dir * step;
                    let _ = dir.x.atan2(dir.z);
                } else {
                    // Arrivé à côté du serveur : maintenir E.
                    if !holding {
                        let _ = room2.tx.send(RoomCmd::Player(
                            bot_id,
                            ClientMsg::InteractStart { target: Interactable::Server(target_srv as u8) },
                        ));
                        holding = true;
                    }
                    hold_ticks += 1;
                    if hold_ticks > 70 {
                        let _ = room2.tx.send(RoomCmd::Player(bot_id, ClientMsg::InteractStop));
                        holding = false;
                        hold_ticks = 0;
                        target_srv = rng.gen_range(0..SERVER_COUNT);
                    }
                }

                let _ = room2.tx.send(RoomCmd::Player(
                    bot_id,
                    ClientMsg::Input { pos, yaw: 0.0, pitch: 0.0, sprint, light_on: true },
                ));
            }
        });
    }
}

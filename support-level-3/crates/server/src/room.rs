//! Simulation d'une partie : tick à 20 Hz, interactions, objectifs, victoire.
//! Un thread par room, propriétaire exclusif de l'état de jeu.

use crate::ai;
use crate::{Room, RoomCmd};
use glam::Vec3;
use rand::Rng;
use sl3_shared::consts::*;
use sl3_shared::map::MapData;
use sl3_shared::protocol::*;
use sl3_shared::texts;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

/// Diffusions publiques utilisées aussi par l'IA.
pub fn room_announce(g: &Game, fr: String, en: String) {
    announce(g, fr, en);
}

pub fn room_sfx(g: &Game, kind: SfxKind, pos: Vec3) {
    sfx(g, kind, pos);
}

pub struct GamePlayer {
    pub id: u8,
    pub name: String,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub state: u8,
    pub downed_at: f32,
    pub sprint: bool,
    pub light_on: bool,
}

pub struct Entity {
    pub pos: Vec3,
    pub yaw: f32,
    pub state: u8,
    pub path: std::collections::VecDeque<(isize, isize)>,
    pub target: Option<u8>,
    pub last_seen: Vec3,
    pub state_timer: f32,
    pub repath_timer: f32,
    pub step_acc: f32,
}

pub struct ServerState {
    pub state: u8,
    pub progress: f32,
}

pub struct Game {
    pub map: &'static MapData,
    pub players: Vec<GamePlayer>,
    pub entity: Entity,
    pub servers: Vec<ServerState>,
    pub receipts: Vec<bool>,
    pub batteries: Vec<bool>,
    pub doors: Vec<bool>,
    pub holdings: HashMap<u8, (Interactable, f32)>,
    pub terminal_done: bool,
    pub fee_approved: bool,
    pub blackout: Option<f32>,
    pub next_blackout: f32,
    pub exit_open: bool,
    pub time: f32,
    pub over: bool,
    pub rng: rand::rngs::StdRng,
    pub entity_near_warned: f32,
}

pub fn room_thread(rx: Receiver<RoomCmd>, room: std::sync::Arc<Room>) {
    set_current_room(room.clone());
    let mut game: Option<Game> = None;
    let mut snap_acc = 0.0f32;
    let mut status_acc = 0.0f32;
    let mut empty_ticks = 0u32;

    loop {
        // Purge des commandes.
        loop {
            match rx.try_recv() {
                Ok(RoomCmd::Start) => {
                    if game.is_none() {
                        let _host = room.host_id();
                        let mut g = Game::new(&room);
                        // Spawns dispersés.
                        let n = g.players.len();
                        for (i, p) in g.players.iter_mut().enumerate() {
                            p.pos += Vec3::new(0.0, 0.0, i as f32 * 1.2 - n as f32 * 0.5);
                        }
                        let snap = snapshot(&g);
                        let spawns: Vec<(u8, Vec3)> =
                            g.players.iter().map(|p| (p.id, p.pos)).collect();
                        {
                            let players = room.players.lock().unwrap();
                            for slot in players.iter() {
                                let mut s = snap.clone();
                                if let Some(mp) = s.players.iter_mut().find(|p| p.id == slot.id) {
                                    mp.pos = spawns
                                        .iter()
                                        .find(|(id, _)| *id == slot.id)
                                        .map(|(_, pos)| *pos)
                                        .unwrap_or(g.map.spawn);
                                }
                                let _ = slot.tx.send(ServerMsg::GameStarted {
                                    your_id: slot.id,
                                    spawn: mp_spawn(&spawns, slot.id, g.map.spawn),
                                    snapshot: s,
                                });
                            }
                        }
                        println!(
                            "[SL3][{}] Partie démarrée ({} joueurs)",
                            room.code,
                            g.players.len()
                        );
                        game = Some(g);
                    }
                }
                Ok(RoomCmd::Player(id, msg)) => {
                    if let Some(g) = game.as_mut() {
                        handle_msg(g, id, msg);
                    }
                }
                Ok(RoomCmd::PlayerGone(id)) => {
                    if let Some(g) = game.as_mut() {
                        for p in g.players.iter_mut() {
                            if p.id == id && p.state != PLAYER_OUT && p.state != PLAYER_ESCAPED {
                                p.state = PLAYER_OUT;
                            }
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }

        if let Some(g) = game.as_mut() {
            tick(g);
            snap_acc += TICK_DT;
            if snap_acc >= TICK_DT {
                snap_acc = 0.0;
                room.broadcast(&ServerMsg::Snapshot(snapshot(g)));
            }
            status_acc += TICK_DT;
            if status_acc >= 10.0 {
                status_acc = 0.0;
                let fixed = g.servers.iter().filter(|s| s.state == SRV_ONLINE).count();
                let e = &g.entity;
                println!(
                    "[SL3][{}] t={:.0}s | serveurs en ligne: {}/{} | Auditeur: état={} pos=({:.0},{:.0}) | joueurs: {}",
                    room.code,
                    g.time,
                    fixed,
                    g.servers.len(),
                    e.state,
                    e.pos.x,
                    e.pos.z,
                    g.players.iter().map(|p| format!("{}:{}@({:.0},{:.0})", p.id, p.state, p.pos.x, p.pos.z)).collect::<Vec<_>>().join(" ")
                );
            }
        }

        // Arrêt si la room se vide.
        if room.players.lock().unwrap().is_empty() {
            empty_ticks += 1;
            if empty_ticks > 40 {
                println!("[SL3][{}] Room fermée (vide)", room.code);
                return;
            }
        } else {
            empty_ticks = 0;
        }

        std::thread::sleep(Duration::from_secs_f32(TICK_DT));
    }
}

fn mp_spawn(spawns: &[(u8, Vec3)], id: u8, fallback: Vec3) -> Vec3 {
    spawns.iter().find(|(i, _)| *i == id).map(|(_, p)| *p).unwrap_or(fallback)
}

impl Game {
    pub fn new(room: &Room) -> Game {
        let map: &'static MapData = Box::leak(Box::new(MapData::parse()));
        let players_in_room: Vec<(u8, String)> = {
            let players = room.players.lock().unwrap();
            players.iter().map(|p| (p.id, p.name.clone())).collect()
        };
        let mut game = Game {
            map,
            players: Vec::new(),
            entity: Entity {
                pos: map.entity_spawn,
                yaw: 0.0,
                state: ENTITY_PATROL,
                path: Default::default(),
                target: None,
                last_seen: map.entity_spawn,
                state_timer: 0.0,
                repath_timer: 0.0,
                step_acc: 0.0,
            },
            servers: (0..SERVER_COUNT)
                .map(|_| ServerState { state: SRV_BROKEN, progress: 0.0 })
                .collect(),
            receipts: vec![false; RECEIPT_COUNT],
            batteries: vec![false; map.batteries.len()],
            doors: vec![false; map.doors.len()],
            holdings: HashMap::new(),
            terminal_done: false,
            fee_approved: false,
            blackout: None,
            next_blackout: crate::blackout_bounds().0,
            exit_open: false,
            time: 0.0,
            over: false,
            rng: rand::SeedableRng::seed_from_u64(rand::random()),
            entity_near_warned: 0.0,
        };
        for (id, name) in players_in_room {
            game.players.push(GamePlayer {
                id,
                name,
                pos: map.spawn,
                yaw: 0.0,
                pitch: 0.0,
                state: PLAYER_ALIVE,
                downed_at: 0.0,
                sprint: false,
                light_on: false,
            });
        }
        game
    }
}

fn handle_msg(g: &mut Game, id: u8, msg: ClientMsg) {
    match msg {
        ClientMsg::Input { pos, yaw, pitch, sprint, light_on } => {
            if let Some(p) = g.players.iter_mut().find(|p| p.id == id) {
                if p.state == PLAYER_ALIVE {
                    p.pos = pos;
                    p.yaw = yaw;
                    p.pitch = pitch;
                    p.sprint = sprint;
                    p.light_on = light_on;
                }
            }
        }
        ClientMsg::InteractStart { target } => {
            let p = match g.players.iter().find(|p| p.id == id) {
                Some(p) if p.state == PLAYER_ALIVE => (p.pos, p.name.clone()),
                _ => return,
            };
            let (ppos, pname) = p;
            let ok = match &target {
                Interactable::Server(i) => {
                    let idx = *i as usize;
                    idx < g.servers.len()
                        && g.servers[idx].state == SRV_BROKEN
                        && (ppos - g.map.servers[idx].pos).length() < INTERACT_DIST + 0.6
                }
                Interactable::Receipt(i) => {
                    let idx = *i as usize;
                    idx < g.receipts.len()
                        && !g.receipts[idx]
                        && (ppos - g.map.receipts[idx]).length() < INTERACT_DIST
                }
                Interactable::Terminal => {
                    !g.terminal_done && (ppos - g.map.terminal).length() < INTERACT_DIST
                }
                Interactable::Breaker => g.blackout.is_some(),
                Interactable::Battery(i) => {
                    let idx = *i as usize;
                    idx < g.batteries.len()
                        && !g.batteries[idx]
                        && (ppos - g.map.batteries[idx]).length() < INTERACT_DIST
                }
                Interactable::Revive(t) => g
                    .players
                    .iter()
                    .find(|p| p.id == *t)
                    .map(|tp| tp.state == PLAYER_DOWNED && (ppos - tp.pos).length() < INTERACT_DIST)
                    .unwrap_or(false),
                Interactable::Door(_) => true,
            };
            if ok {
                #[cfg(debug_assertions)]
                eprintln!("[DBG] InteractStart p{} {:?} accepté", id, target);
                match &target {
                    Interactable::Receipt(i) => {
                        let idx = *i as usize;
                        g.receipts[idx] = true;
                        let rname = texts::RECEIPT_NAMES[idx];
                        let count = g.receipts.iter().filter(|r| **r).count();
                        let (fr, en) = texts::announce(
                            "receipt",
                            &[&pname, rname.0, &count.to_string()],
                            &[&pname, rname.1, &count.to_string()],
                        );
                        announce(g, fr, en);
                        sfx(g, SfxKind::Pickup, ppos);
                        if count == RECEIPT_COUNT {
                            let (fr, en) = texts::announce("receipts_all", &[], &[]);
                            announce(g, fr, en);
                        }
                    }
                    Interactable::Battery(i) => {
                        g.batteries[*i as usize] = true;
                        sfx(g, SfxKind::Battery, ppos);
                    }
                    Interactable::Door(d) => {
                        if let Some(dd) = g.map.doors.get(*d as usize) {
                            let idx = *d as usize;
                            g.doors[idx] = !g.doors[idx];
                            let now_open = g.doors[idx];
                            sfx(g, if now_open { SfxKind::DoorOpen } else { SfxKind::DoorSlam }, dd.pos);
                        }
                    }
                    _ => {
                        g.holdings.insert(id, (target, 0.0));
                    }
                }
            }
        }
        ClientMsg::InteractStop => {
            g.holdings.remove(&id);
        }
        ClientMsg::DoorToggle { id: d } => {
            if let Some(p) = g.players.iter().find(|p| p.id == id) {
                if p.state == PLAYER_ALIVE {
                    if let Some(dd) = g.map.doors.get(d as usize) {
                        if (p.pos - dd.pos).length() < INTERACT_DIST {
                            g.doors[d as usize] = !g.doors[d as usize];
                            let now_open = g.doors[d as usize];
                            sfx(g, if now_open { SfxKind::DoorOpen } else { SfxKind::DoorSlam }, dd.pos);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

// Room courante (thread-local : un thread = une room).
thread_local! {
    static CURRENT_ROOM: std::cell::RefCell<Option<std::sync::Arc<Room>>> =
        const { std::cell::RefCell::new(None) };
}

pub fn set_current_room(room: std::sync::Arc<Room>) {
    CURRENT_ROOM.with(|c| *c.borrow_mut() = Some(room));
}

fn room_of() -> Option<std::sync::Arc<Room>> {
    CURRENT_ROOM.with(|c| c.borrow().clone())
}

fn announce(g: &Game, fr: String, en: String) {
    let _ = g;
    if let Some(room) = room_of() {
        room.broadcast(&ServerMsg::Announcement { fr, en });
    }
}

fn sfx(g: &Game, kind: SfxKind, pos: Vec3) {
    let _ = g;
    if let Some(room) = room_of() {
        room.broadcast(&ServerMsg::Sfx { kind, pos });
    }
}

fn tick(g: &mut Game) {
    if g.over {
        return;
    }
    g.time += TICK_DT;

    // 1) Interactions maintenues.
    let ids: Vec<u8> = g.holdings.keys().copied().collect();
    for id in ids {
        let (target, progress) = *g.holdings.get(&id).unwrap();
        let alive = g
            .players
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.state == PLAYER_ALIVE)
            .unwrap_or(false);
        if !alive {
            g.holdings.remove(&id);
            continue;
        }
        match target {
            Interactable::Server(i) => {
                let idx = i as usize;
                let in_range = g
                    .players
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| (p.pos - g.map.servers[idx].pos).length() < INTERACT_DIST + 0.6)
                    .unwrap_or(false);
                if g.servers[idx].state != SRV_ONLINE && in_range {
                    g.servers[idx].state = SRV_FIXING;
                    g.servers[idx].progress = progress + TICK_DT / REBOOT_TIME;
                    if let Some(p) = g.players.iter().find(|p| p.id == id) {
                        ai::hear_noise(&mut g.entity, g.map, p.pos, ENTITY_HEAR_WORK);
                    }
                    if g.servers[idx].progress >= 1.0 {
                        g.servers[idx].state = SRV_ONLINE;
                        g.holdings.remove(&id);
                        let online = g.servers.iter().filter(|s| s.state == SRV_ONLINE).count();
                        let (fr, en) = texts::announce(
                            "server_fixed",
                            &[&format!("srv-0{i}"), &online.to_string()],
                            &[&format!("srv-0{i}"), &online.to_string()],
                        );
                        announce(g, fr, en);
                        sfx(g, SfxKind::RebootDone, g.map.servers[idx].pos);
                        if online == SERVER_COUNT {
                            let (fr, en) = texts::announce("server_all", &[], &[]);
                            announce(g, fr, en);
                            check_exit(g);
                        }
                    } else {
                        g.holdings.get_mut(&id).unwrap().1 = g.servers[idx].progress;
                    }
                } else {
                    g.holdings.remove(&id);
                }
            }
            Interactable::Terminal => {
                if g.receipts.iter().all(|r| *r) && !g.terminal_done {
                    let (_, prog) = g.holdings.get_mut(&id).unwrap();
                    *prog += TICK_DT / TERMINAL_TIME;
                    if *prog >= 1.0 {
                        g.terminal_done = true;
                        g.fee_approved = true;
                        g.holdings.remove(&id);
                        let who = g
                            .players
                            .iter()
                            .find(|p| p.id == id)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        let (fr, en) = texts::announce("fee_ok", &[&who], &[&who]);
                        announce(g, fr, en);
                        sfx(g, SfxKind::Stamp, g.map.terminal);
                        check_exit(g);
                    }
                } else {
                    g.holdings.remove(&id);
                }
            }
            Interactable::Breaker => {
                if g.blackout.is_some() {
                    let (_, prog) = g.holdings.get_mut(&id).unwrap();
                    *prog += TICK_DT / BREAKER_TIME;
                    if *prog >= 1.0 {
                        g.blackout = None;
                        g.holdings.remove(&id);
                        let (fr, en) = texts::announce("lights_restored", &[], &[]);
                        announce(g, fr, en);
                        sfx(g, SfxKind::Breaker, g.map.breaker);
                        sfx(g, SfxKind::LightsOn, g.map.breaker);
                    }
                } else {
                    g.holdings.remove(&id);
                }
            }
            Interactable::Revive(t) => {
                let valid = g
                    .players
                    .iter()
                    .find(|p| p.id == t)
                    .map(|p| p.state == PLAYER_DOWNED)
                    .unwrap_or(false);
                if valid {
                    let (_, prog) = g.holdings.get_mut(&id).unwrap();
                    *prog += TICK_DT / REVIVE_TIME;
                    if *prog >= 1.0 {
                        g.holdings.remove(&id);
                        if let Some(tp) = g.players.iter_mut().find(|p| p.id == t) {
                            tp.state = PLAYER_ALIVE;
                            tp.downed_at = 0.0;
                        }
                        let name = g
                            .players
                            .iter()
                            .find(|p| p.id == t)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        let (fr, en) = texts::announce("revived", &[&name], &[&name]);
                        announce(g, fr, en);
                    }
                } else {
                    g.holdings.remove(&id);
                }
            }
            _ => {
                g.holdings.remove(&id);
            }
        }
    }

    // 2) Bleed-out.
    let mut bled_out: Vec<String> = Vec::new();
    for p in g.players.iter_mut() {
        if p.state == PLAYER_DOWNED && g.time - p.downed_at > 45.0 {
            p.state = PLAYER_OUT;
            bled_out.push(p.name.clone());
        }
    }
    for name in bled_out {
        let (fr, en) = texts::announce("out", &[&name], &[&name]);
        announce(g, fr, en);
    }

    // 3) Bruits de déplacement.
    for p in g.players.iter() {
        if p.state != PLAYER_ALIVE {
            continue;
        }
        if p.sprint {
            ai::hear_noise(&mut g.entity, g.map, p.pos, ENTITY_HEAR_SPRINT);
        } else {
            ai::hear_noise(&mut g.entity, g.map, p.pos, ENTITY_HEAR_WALK);
        }
    }

    // 4) IA de l'Auditeur (désactivable pour la QA via SL3_AUDITOR=0).
    if sl3_shared::consts::auditor_enabled() {
        ai::update(g, TICK_DT);
    }

    // 4bis) La sortie s'ouvre dès que les deux objectifs sont remplis.
    check_exit(g);

    // 5) Blackouts.
    if let Some(timer) = g.blackout.as_mut() {
        *timer -= TICK_DT;
        if *timer <= 0.0 {
            g.blackout = None;
        }
    } else {
        g.next_blackout -= TICK_DT;
        if g.next_blackout <= 0.0 {
            let (bmin, bmax) = crate::blackout_bounds();
            g.next_blackout = g.rng.gen_range(bmin..bmax);
            g.blackout = Some(BLACKOUT_DURATION);
            let (fr, en) = texts::announce("blackout", &[], &[]);
            announce(g, fr, en);
            sfx(g, SfxKind::Blackout, g.map.breaker);
        }
    }

    // 6) Sortie.
    if g.exit_open {
        let mut escaped_names: Vec<String> = Vec::new();
        for p in g.players.iter_mut() {
            if p.state == PLAYER_ALIVE {
                for xz in &g.map.exit_cells {
                    if (p.pos - *xz).length() < 1.6 {
                        p.state = PLAYER_ESCAPED;
                        escaped_names.push(p.name.clone());
                        break;
                    }
                }
            }
        }
        for name in escaped_names {
            let (fr, en) = texts::announce("escaped", &[&name], &[&name]);
            announce(g, fr, en);
        }
    }

    // 7) Fin de partie.
    let all_done = g
        .players
        .iter()
        .all(|p| p.state == PLAYER_OUT || p.state == PLAYER_ESCAPED);
    if all_done && !g.players.is_empty() {
        g.over = true;
        let escaped = g.players.iter().filter(|p| p.state == PLAYER_ESCAPED).count() as u8;
        let fixed = g.servers.iter().filter(|s| s.state == SRV_ONLINE).count() as u8;
        if let Some(room) = room_of() {
            room.broadcast(&ServerMsg::GameOver {
                win: escaped > 0,
                time: g.time,
                servers_fixed: fixed,
                escaped,
            });
            println!(
            "[SL3][{}] Fin de partie : {} — {} serveur(s), {} rescapé(s), {:.0}s",
            room.code,
            if escaped > 0 { "VICTOIRE" } else { "DÉFAITE" },
            fixed,
            escaped,
            g.time
        );
        }
    }
}

fn check_exit(g: &mut Game) {
    let all_online = g.servers.iter().all(|s| s.state == SRV_ONLINE);
    if all_online && g.fee_approved && !g.exit_open {
        g.exit_open = true;
        let (fr, en) = texts::announce("exit_open", &[], &[]);
        announce(g, fr, en);
    }
}

pub fn snapshot(g: &Game) -> Snapshot {
    Snapshot {
        time: g.time,
        players: g
            .players
            .iter()
            .map(|p| NetPlayer {
                id: p.id,
                pos: p.pos,
                yaw: p.yaw,
                pitch: p.pitch,
                state: p.state,
                sprint: p.sprint,
                light_on: p.light_on,
            })
            .collect(),
        entity: NetEntity {
            pos: g.entity.pos,
            yaw: g.entity.yaw,
            state: g.entity.state,
        },
        servers: g
            .servers
            .iter()
            .map(|s| NetServer { state: s.state, progress: s.progress })
            .collect(),
        receipts: g.receipts.clone(),
        batteries: g.batteries.clone(),
        doors: g.doors.clone(),
        terminal_done: g.terminal_done,
        fee_approved: g.fee_approved,
        blackout: g.blackout.is_some(),
        exit_open: g.exit_open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sl3_shared::map::MapData;

    fn test_game() -> Game {
        let map: &'static MapData = Box::leak(Box::new(MapData::parse()));
        Game {
            map,
            players: vec![GamePlayer {
                id: 1,
                name: "Test".into(),
                pos: map.spawn,
                yaw: 0.0,
                pitch: 0.0,
                state: PLAYER_ALIVE,
                downed_at: 0.0,
                sprint: false,
                light_on: false,
            }],
            entity: Entity {
                pos: map.entity_spawn,
                yaw: 0.0,
                state: ENTITY_PATROL,
                path: Default::default(),
                target: None,
                last_seen: map.entity_spawn,
                state_timer: 0.0,
                repath_timer: 0.0,
                step_acc: 0.0,
            },
            servers: (0..SERVER_COUNT)
                .map(|_| ServerState { state: SRV_BROKEN, progress: 0.0 })
                .collect(),
            receipts: vec![false; RECEIPT_COUNT],
            batteries: vec![false; map.batteries.len()],
            doors: vec![false; map.doors.len()],
            holdings: HashMap::new(),
            terminal_done: false,
            fee_approved: false,
            blackout: None,
            next_blackout: 9999.0,
            exit_open: false,
            time: 0.0,
            over: false,
            rng: rand::SeedableRng::seed_from_u64(42),
            entity_near_warned: 0.0,
        }
    }

    #[test]
    fn test_reboot_flow() {
        let mut g = test_game();
        let srv = g.map.servers[0].pos;
        g.players[0].pos = srv + Vec3::new(1.5, 0.0, 0.0);
        handle_msg(
            &mut g,
            1,
            ClientMsg::InteractStart { target: Interactable::Server(0) },
        );
        assert!(g.holdings.contains_key(&1), "interaction refusée");
        let ticks = (REBOOT_TIME / TICK_DT).ceil() as usize + 2;
        for _ in 0..ticks {
            tick(&mut g);
        }
        assert_eq!(g.servers[0].state, SRV_ONLINE, "serveur non réparé");
        assert!(!g.holdings.contains_key(&1), "hold non libéré");
    }

    #[test]
    fn test_fee_and_exit() {
        let mut g = test_game();
        // Ramasser les 4 justificatifs.
        for i in 0..RECEIPT_COUNT {
            g.players[0].pos = g.map.receipts[i];
            handle_msg(
                &mut g,
                1,
                ClientMsg::InteractStart { target: Interactable::Receipt(i as u8) },
            );
        }
        assert!(g.receipts.iter().all(|r| *r), "justificatifs non ramassés");
        // Valider la note de frais au terminal.
        g.players[0].pos = g.map.terminal + Vec3::new(0.0, 0.0, 1.2);
        handle_msg(&mut g, 1, ClientMsg::InteractStart { target: Interactable::Terminal });
        let ticks = (TERMINAL_TIME / TICK_DT).ceil() as usize + 2;
        for _ in 0..ticks {
            tick(&mut g);
        }
        assert!(g.fee_approved, "note non validée");
        // Réparer tous les serveurs "magiquement" puis vérifier la sortie.
        for s in g.servers.iter_mut() {
            s.state = SRV_ONLINE;
        }
        tick(&mut g);
        assert!(g.exit_open, "sortie non ouverte");
        // Le joueur atteint la sortie.
        g.players[0].pos = g.map.exit_cells[0];
        tick(&mut g);
        assert_eq!(g.players[0].state, PLAYER_ESCAPED, "joueur non évacué");
        tick(&mut g);
        assert!(g.over, "partie non terminée");
    }

    #[test]
    fn test_entity_hearing_and_chase() {
        let mut g = test_game();
        // Joueur proche + sprint → l'entité enquête.
        g.players[0].pos = g.entity.pos + Vec3::new(4.0, 0.0, 0.0);
        g.players[0].sprint = true;
        for _ in 0..5 {
            tick(&mut g);
        }
        assert!(
            g.entity.state == ENTITY_INVESTIGATE || g.entity.state == ENTITY_CHASE,
            "entité indifférente au bruit (état {})",
            g.entity.state
        );
    }
}

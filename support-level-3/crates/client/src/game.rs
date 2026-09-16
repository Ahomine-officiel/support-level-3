//! État de jeu côté client : mouvement FPS, collisions, interpolation réseau,
//! construction du monde statique, dynamiques d'instances par frame.

use glam::{Mat4, Vec3};
use sl3_shared::consts::*;
use sl3_shared::map::{MapData, CELL};
use sl3_shared::protocol::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::gpu::{InstanceData, Renderer, MAX_LIGHTS};
use crate::gpu::model::Model;
use crate::gpu::rtscene::{self, GpuAabb};

pub struct RemoteView {
    pub pos: Vec3,
    pub yaw: f32,
    pub state: u8,
    pub light_on: bool,
}

pub struct Game {
    pub map: &'static MapData,
    pub my_id: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stamina: f32,
    pub battery: f32,
    pub light_on: bool,
    pub sprinting: bool,
    pub bob_phase: f32,
    pub step_acc: f32,
    pub snap: Option<Snapshot>,
    pub entity_vis: (Vec3, f32),
    pub remote: HashMap<u8, RemoteView>,
    pub feed: VecDeque<(Instant, String)>,
    pub fear: f32,
    pub heart_acc: f32,
    pub hold_key: bool,
    pub door_anim: Vec<f32>,
    pub exit_anim: f32,
    pub next_input: Instant,
    pub time: f32,
    pub statics: crate::gpu::StaticBatches,
}

impl Game {
    pub fn new(map: &'static MapData, my_id: u8, spawn: Vec3, renderer: &mut Renderer) -> Game {
        let insts = Self::build_static_list(map);
        let statics = renderer.build_static(&insts);
        // Scène de ray tracing : murs fusionnés + mobilier (une fois par partie).
        renderer.update_rt_statics(map, &insts);
        Game {
            map,
            my_id,
            pos: spawn,
            yaw: 0.0,
            pitch: 0.0,
            stamina: STAMINA_MAX,
            battery: 100.0,
            light_on: true,
            sprinting: false,
            bob_phase: 0.0,
            step_acc: 0.0,
            snap: None,
            entity_vis: (map.entity_spawn, 0.0),
            remote: HashMap::new(),
            feed: VecDeque::new(),
            fear: 0.0,
            heart_acc: 0.0,
            hold_key: false,
            door_anim: vec![0.0; map.doors.len()],
            exit_anim: 0.0,
            next_input: Instant::now(),
            time: 0.0,
            statics,
        }
    }

    /// Monde statique : murs, sols, plafonds, mobilier, déco.
    fn build_static_list(map: &'static MapData) -> Vec<(String, InstanceData)> {
        let mut out: Vec<(String, InstanceData)> = Vec::new();
        let yaw_mat = |yaw: f32| Mat4::from_rotation_y(yaw);
        let trs = |pos: Vec3, yaw: f32| Mat4::from_translation(pos) * yaw_mat(yaw);

        for (name, pos, yaw) in &map.props {
            out.push((name.clone(), InstanceData::new(trs(*pos, *yaw))));
        }

        // Chemins de câbles + gaines de ventilation dans les couloirs.
        for r in 0..map.h {
            for c in 0..map.w {
                if map.floor[map.idx(c, r)] == Some(sl3_shared::map::FloorKind::Corridor) {
                    let p = map.cell_center(c, r);
                    let par = (c + r) % 3;
                    if par == 0 {
                        out.push(("cable_tray".into(), InstanceData::new(trs(Vec3::new(p.x, 2.74, p.z), 0.0))));
                    } else if par == 1 {
                        out.push(("duct_seg".into(), InstanceData::new(trs(Vec3::new(p.x, 2.8, p.z), 0.0))));
                    }
                }
            }
        }

        // Affiches et extincteurs contre les murs (déterministe par cellule).
        let mut poster_i = 0u32;
        let mut ext_i = 0u32;
        for r in 0..map.h {
            for c in 0..map.w {
                let i = map.idx(c, r);
                if map.floor[i].is_none() || map.solid[i] {
                    continue;
                }
                let kind = map.floor[i];
                let decor = matches!(
                    kind,
                    Some(sl3_shared::map::FloorKind::Office)
                        | Some(sl3_shared::map::FloorKind::Hall)
                        | Some(sl3_shared::map::FloorKind::Archives)
                );
                if !decor {
                    continue;
                }
                let h = (c as u64).wrapping_mul(73856093) ^ (r as u64).wrapping_mul(19349663);
                if h % 7 != 0 {
                    continue;
                }
                // premier mur voisin
                let wall = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .find(|&(dc, dr)| map.is_solid(c as isize + dc, r as isize + dr));
                if let Some((dc, dr)) = wall {
                    let center = map.cell_center(c, r);
                    let dir = Vec3::new(dc as f32, 0.0, dr as f32);
                    let yaw = (-dir.x).atan2(-dir.z);
                    if h % 2 == 0 {
                        let models = ["poster_a", "poster_b", "poster_c", "poster_d"];
                        let m = models[(poster_i % 4) as usize];
                        poster_i += 1;
                        let pos = center + dir * (CELL * 0.5 - 0.04) + Vec3::new(0.0, 1.65, 0.0);
                        out.push((m.into(), InstanceData::new(trs(pos, yaw))));
                    } else if ext_i < 4 {
                        ext_i += 1;
                        let pos = center + dir * (CELL * 0.5 - 0.12) + Vec3::new(0.0, 0.85, 0.0);
                        out.push(("extinguisher".into(), InstanceData::new(trs(pos, yaw))));
                    }
                }
            }
        }

        // Dormants de portes (le panneau est dynamique).
        for d in &map.doors {
            let yaw = if d.vertical_passage { 0.0 } else { std::f32::consts::FRAC_PI_2 };
            out.push(("door_frame".into(), InstanceData::new(trs(d.pos, yaw))));
        }

        // Porte de sortie (hall nord).
        let exit_center = if map.exit_cells.is_empty() {
            Vec3::ZERO
        } else {
            let n = map.exit_cells.len() as f32;
            map.exit_cells.iter().sum::<Vec3>() / n + Vec3::new(0.0, 0.0, 0.35)
        };
        out.push(("exit_door".into(), InstanceData::new(trs(exit_center, 0.0))));

        // Coffret disjoncteur et terminal RH (statiques).
        let breaker_pos = map.breaker + Vec3::new(0.0, 0.0, -0.7);
        out.push(("breaker".into(), InstanceData::new(trs(breaker_pos, 0.0))));
        out.push(("terminal".into(), InstanceData::new(trs(map.terminal, 0.0))));

        out
    }

    /// Cellules de portes fermées (pour la collision).
    pub fn closed_door_cells(&self) -> HashSet<(isize, isize)> {
        let mut set = HashSet::new();
        if let Some(snap) = &self.snap {
            for (i, open) in snap.doors.iter().enumerate() {
                if !open {
                    if let Some(d) = self.map.doors.get(i) {
                        set.insert(self.map.world_cell(d.pos));
                    }
                }
            }
        }
        set
    }

    pub fn add_feed(&mut self, msg: String) {
        self.feed.push_back((Instant::now(), msg));
        while self.feed.len() > 7 {
            self.feed.pop_front();
        }
    }

    pub fn apply_snapshot(&mut self, snap: Snapshot) {
        let mut new_remote: HashMap<u8, RemoteView> = HashMap::new();
        for p in &snap.players {
            if p.id == self.my_id {
                continue;
            }
            new_remote.insert(
                p.id,
                RemoteView { pos: p.pos, yaw: p.yaw, state: p.state, light_on: p.light_on },
            );
        }
        // Interpolation douce.
        let new_remote_keys: Vec<u8> = new_remote.keys().copied().collect();
        for (id, rv) in new_remote.into_iter() {
            match self.remote.get_mut(&id) {
                Some(old) => {
                    old.pos = old.pos.lerp(rv.pos, 0.25);
                    old.yaw = lerp_angle(old.yaw, rv.yaw, 0.3);
                    old.state = rv.state;
                    old.light_on = rv.light_on;
                }
                None => {
                    self.remote.insert(id, rv);
                }
            }
        }
        // retain déjà effectué par le traitement ci-dessus : on reconstruit les clés
        let keys: Vec<u8> = new_remote_keys.clone();
        self.remote.retain(|id, _| keys.contains(id));

        self.entity_vis.0 = self.entity_vis.0.lerp(snap.entity.pos, 0.22);
        self.entity_vis.1 = lerp_angle(self.entity_vis.1, snap.entity.yaw, 0.25);
        self.snap = Some(snap);
    }

    /// Déplacement avec collision cercle/grille + portes fermées.
    fn try_move(&mut self, delta: Vec3, doors: &HashSet<(isize, isize)>) {
        let r = PLAYER_RADIUS;
        // axe X
        let nx = self.pos.x + delta.x;
        if !self.hits(Vec3::new(nx, 0.0, self.pos.z), r, doors) {
            self.pos.x = nx;
        }
        // axe Z
        let nz = self.pos.z + delta.z;
        if !self.hits(Vec3::new(self.pos.x, 0.0, nz), r, doors) {
            self.pos.z = nz;
        }
    }

    fn hits(&self, pos: Vec3, r: f32, doors: &HashSet<(isize, isize)>) -> bool {
        let (c0, r0) = self.map.world_cell(pos);
        for dc in [-1isize, 0, 1] {
            for dr in [-1isize, 0, 1] {
                let (c, rr) = (c0 + dc, r0 + dr);
                let solid = self.map.is_solid(c, rr) || doors.contains(&(c, rr));
                if !solid {
                    continue;
                }
                let minx = c as f32 * CELL;
                let maxx = minx + CELL;
                let minz = rr as f32 * CELL;
                let maxz = minz + CELL;
                let cx = pos.x.clamp(minx, maxx);
                let cz = pos.z.clamp(minz, maxz);
                let d = (pos.x - cx) * (pos.x - cx) + (pos.z - cz) * (pos.z - cz);
                if d < r * r {
                    return true;
                }
            }
        }
        false
    }

    /// Trouve l'interaction la plus proche sous le regard.
    pub fn find_target(&self) -> Option<(Interactable, Vec3)> {
        let snap = self.snap.as_ref()?;
        let mut best: Option<(Interactable, Vec3, f32)> = None;
        let consider = |best: &mut Option<(Interactable, Vec3, f32)>,
                        t: Interactable,
                        pos: Vec3,
                        my: Vec3,
                        fwd: Vec3| {
            let to = pos - my;
            let d = to.length();
            if d > INTERACT_DIST {
                return;
            }
            let dir = to / d.max(0.001);
            let cos = dir.dot(fwd);
            if cos < 0.35 {
                return;
            }
            let score = cos / d.max(0.5);
            if best.map(|b| score > b.2).unwrap_or(true) {
                *best = Some((t, pos, score));
            }
        };
        let fwd = self.forward();
        let my = self.pos;
        for (i, s) in self.map.servers.iter().enumerate() {
            if snap.servers[i].state != SRV_ONLINE {
                consider(&mut best, Interactable::Server(s.id), s.pos + Vec3::new(0.0, 1.1, 0.0), my, fwd);
            }
        }
        for (i, r) in self.map.receipts.iter().enumerate() {
            if !snap.receipts[i] {
                consider(&mut best, Interactable::Receipt(i as u8), *r + Vec3::new(0.0, 0.4, 0.0), my, fwd);
            }
        }
        if !snap.terminal_done {
            consider(&mut best, Interactable::Terminal, self.map.terminal + Vec3::new(0.0, 1.2, 0.0), my, fwd);
        }
        if snap.blackout {
            consider(&mut best, Interactable::Breaker, self.map.breaker + Vec3::new(0.0, 1.2, 0.0), my, fwd);
        }
        for (i, b) in self.map.batteries.iter().enumerate() {
            if !snap.batteries[i] {
                consider(&mut best, Interactable::Battery(i as u8), *b + Vec3::new(0.0, 0.2, 0.0), my, fwd);
            }
        }
        for p in self.remote.values() {
            if p.state == PLAYER_DOWNED {
                consider(&mut best, Interactable::Revive(0), p.pos + Vec3::new(0.0, 0.6, 0.0), my, fwd);
            }
        }
        for d in &self.map.doors {
            consider(&mut best, Interactable::Door(d.id), d.pos + Vec3::new(0.0, 1.1, 0.0), my, fwd);
        }
        best.map(|(t, p, _)| (t, p))
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn update(
        &mut self,
        dt: f32,
        keys: &HashSet<winit::keyboard::KeyCode>,
        tx: &std::sync::mpsc::Sender<ClientMsg>,
    ) -> Vec<(String, f32)> {
        // sons locaux (pas + heartbeat géré côté app)
        let mut local_sfx = Vec::new();
        self.time += dt;
        let doors = self.closed_door_cells();

        let my_state = self
            .snap
            .as_ref()
            .and_then(|s| s.players.iter().find(|p| p.id == self.my_id))
            .map(|p| p.state)
            .unwrap_or(PLAYER_ALIVE);
        let alive = my_state == PLAYER_ALIVE;

        if alive {
            // ---- Mouvement ----
            let mut mv = Vec3::ZERO;
            let fwd = Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos());
            let right = Vec3::new(-self.yaw.cos(), 0.0, self.yaw.sin());
            let k = |c: winit::keyboard::KeyCode| keys.contains(&c);
            use winit::keyboard::KeyCode as K;
            let f = (k(K::KeyW) || k(K::KeyZ)) as i32 - k(K::KeyS) as i32;
            let s = k(K::KeyD) as i32 - (k(K::KeyA) || k(K::KeyQ)) as i32;
            mv += fwd * f as f32;
            mv += right * s as f32;
            let moving = mv.length_squared() > 0.001;
            if moving {
                mv = mv.normalize();
            }
            let want_sprint = k(K::ShiftLeft) || k(K::ShiftRight);
            let can_sprint = want_sprint && moving && self.stamina > 0.05;
            self.sprinting = can_sprint;
            if can_sprint {
                self.stamina = (self.stamina - dt).max(0.0);
            } else {
                self.stamina = (self.stamina + dt * STAMINA_REGEN).min(STAMINA_MAX);
            }
            let speed = if can_sprint { SPRINT_SPEED } else { WALK_SPEED };
            self.try_move(mv * speed * dt, &doors);

            // Pas + balancement.
            if moving {
                self.step_acc += speed * dt;
                self.bob_phase += speed * dt * 1.9;
                let step_len = if can_sprint { 0.38 } else { 0.46 };
                if self.step_acc > step_len {
                    self.step_acc = 0.0;
                    let idx = (self.time * 10.0) as usize % 3;
                    local_sfx.push((format!("footstep{}", idx + 1), if can_sprint { 0.30 } else { 0.18 }));
                }
            }

            // Batterie torche.
            if self.light_on {
                self.battery = (self.battery - FLASHLIGHT_DRAIN * dt).max(0.0);
                if self.battery <= 0.0 {
                    self.light_on = false;
                }
            }

            // ---- Envoi des inputs (20 Hz) ----
            if Instant::now() >= self.next_input {
                self.next_input = Instant::now() + std::time::Duration::from_secs_f32(TICK_DT);
                let _ = tx.send(ClientMsg::Input {
                    pos: self.pos,
                    yaw: self.yaw,
                    pitch: self.pitch,
                    sprint: self.sprinting,
                    light_on: self.light_on,
                });
            }
        }

        // ---- Peur ----
        if let Some(snap) = &self.snap {
            let dist = snap.entity.pos.distance(self.pos);
            let mut fear = (1.0 - dist / 16.0).clamp(0.0, 1.0);
            if snap.entity.state == ENTITY_CHASE {
                fear = fear.max(0.6);
            }
            self.fear += (fear - self.fear) * dt.min(1.0) * 3.0;
        }

        // ---- Battement de cœur ----
        if self.fear > 0.25 {
            self.heart_acc += dt;
            let interval = 1.05 - 0.62 * self.fear;
            if self.heart_acc > interval {
                self.heart_acc = 0.0;
                local_sfx.push(("heartbeat".into(), 0.25 + 0.5 * self.fear));
            }
        }

        // ---- Animations portes / sortie ----
        if let Some(snap) = &self.snap {
            for (i, open) in snap.doors.iter().enumerate() {
                let target = if *open { 1.0 } else { 0.0 };
                self.door_anim[i] += (target - self.door_anim[i]) * dt.min(1.0) * 8.0;
            }
            let target = if snap.exit_open { 1.0 } else { 0.0 };
            self.exit_anim += (target - self.exit_anim) * dt.min(1.0) * 2.0;
        }

        local_sfx
    }

    /// Instanciations dynamiques du frame.
    pub fn dynamics(&self) -> Vec<(String, Vec<InstanceData>)> {
        let mut out: Vec<(String, Vec<InstanceData>)> = Vec::new();
        let t = self.time;
        let trs = |pos: Vec3, yaw: f32| Mat4::from_translation(pos) * Mat4::from_rotation_y(yaw);

        // Panneaux de porte coulissants.
        let mut panels = Vec::new();
        for (i, d) in self.map.doors.iter().enumerate() {
            let yaw = if d.vertical_passage { 0.0 } else { std::f32::consts::FRAC_PI_2 };
            let slide = self.door_anim[i];
            let off_local = Vec3::new(slide * 1.08, 0.0, 0.0);
            let offset = Mat4::from_rotation_y(yaw).transform_vector3(off_local);
            panels.push(InstanceData::new(trs(d.pos + offset, yaw)));
        }
        out.push(("door_panel".into(), panels));

        // Panneau de la sortie (monte quand ouverte).
        let exit_center = if self.map.exit_cells.is_empty() {
            Vec3::ZERO
        } else {
            let n = self.map.exit_cells.len() as f32;
            self.map.exit_cells.iter().sum::<Vec3>() / n + Vec3::new(0.0, 0.0, 0.35)
        };
        out.push((
            "exit_panel".into(),
            vec![InstanceData::new(Mat4::from_translation(exit_center + Vec3::new(0.0, self.exit_anim * 2.55, 0.0)))],
        ));

        if let Some(snap) = &self.snap {
            // Baies serveurs : LED par état.
            let mut bays = Vec::new();
            for (i, s) in self.map.servers.iter().enumerate() {
                let st = &snap.servers[i];
                let emis = match st.state {
                    SRV_ONLINE => [0.25, 1.0, 0.35, 1.0],
                    SRV_FIXING => [1.0, 0.75, 0.15, 0.5 + 0.5 * (t * 8.0).sin()],
                    _ => [1.0, 0.12, 0.08, 0.5 + 0.5 * (t * 3.0).sin()],
                };
                bays.push(InstanceData::new(trs(s.pos, s.yaw)).with_emissive(emis));
            }
            out.push(("server_bay".into(), bays));

            // Justificatifs non ramassés.
            let mut papers = Vec::new();
            for (i, r) in self.map.receipts.iter().enumerate() {
                if !snap.receipts[i] {
                    let yaw = i as f32 * 0.9;
                    papers.push(
                        InstanceData::new(trs(*r + Vec3::new(0.0, 0.03, 0.0), yaw))
                            .with_emissive([1.0, 1.0, 0.9, 0.4]),
                    );
                }
            }
            out.push(("receipt".into(), papers));

            // Batteries — VISIBLES : le modèle (20 cm) était quasi invisible
            // dans le noir (aucun émissif, contrairement aux justificatifs).
            // Glow cyan + lévitation + rotation + taille x1.5 : un pickup se
            // repère de loin, pas une aiguille dans une botte de foin.
            let mut bats = Vec::new();
            for (i, b) in self.map.batteries.iter().enumerate() {
                if !snap.batteries[i] {
                    let bob = 0.05 + 0.035 * (t * 2.1 + i as f32 * 1.31).sin();
                    bats.push(
                        InstanceData::new(
                            Mat4::from_translation(*b + Vec3::new(0.0, bob, 0.0))
                                * Mat4::from_rotation_y(t * 0.9 + i as f32 * 1.7)
                                * Mat4::from_scale(Vec3::splat(1.5)),
                        )
                        .with_emissive([0.3, 0.75, 1.0, 2.6]),
                    );
                }
            }
            out.push(("battery".into(), bats));

            // Néons (éteints si blackout / clignotants).
            let mut fixtures = Vec::new();
            for (i, l) in self.map.lights.iter().enumerate() {
                let on = if snap.blackout {
                    0.0
                } else if l.flicker {
                    let n = (t * 9.0 + i as f32 * 3.7).sin() * (t * 23.0 + i as f32).sin();
                    if n > 0.3 { 0.1 } else { 1.0 }
                } else {
                    1.0
                };
                fixtures.push(
                    InstanceData::new(trs(Vec3::new(l.pos.x, l.pos.y + 0.08, l.pos.z), 0.0))
                        .with_emissive([1.0, 0.98, 0.9, on]),
                );
            }
            out.push(("light_fixture".into(), fixtures));

            // Joueurs distants.
            for p in self.remote.values() {
                let m = if p.state == PLAYER_DOWNED {
                    Mat4::from_translation(p.pos + Vec3::new(0.0, 0.25, 0.0))
                        * Mat4::from_rotation_y(p.yaw)
                        * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)
                } else {
                    trs(p.pos, p.yaw)
                };
                out.push(("tech".into(), vec![InstanceData::new(m)]));
            }

            // L'Auditeur.
            let (epos, eyaw) = self.entity_vis;
            let bob = (t * 6.5).sin() * 0.04;
            out.push((
                "entity".into(),
                vec![InstanceData::new(trs(epos + Vec3::new(0.0, bob.abs(), 0.0), eyaw))],
            ));
        }

        out
    }

    /// Matrice view_proj inversée (reconstruction de position dans la passe RT).
    pub fn inv_view_proj(&self, aspect: f32, jitter_ndc: [f32; 2]) -> [[f32; 4]; 4] {
        self.view_proj_mat(aspect, jitter_ndc).inverse().to_cols_array_2d()
    }

    /// (proj * vue), avec décalage NDC du jitter subpixel (upscaling temporel).
    fn view_proj_mat(&self, aspect: f32, jitter_ndc: [f32; 2]) -> glam::Mat4 {
        let eye = self.cam_eye();
        let dir = self.forward();
        let view = glam::Mat4::look_at_rh(eye, eye + dir, Vec3::Y);
        let proj = glam::Mat4::perspective_rh(74f32.to_radians(), aspect, 0.05, 120.0);
        let vp = proj * view;
        if jitter_ndc[0] == 0.0 && jitter_ndc[1] == 0.0 {
            vp
        } else {
            glam::Mat4::from_translation([jitter_ndc[0], jitter_ndc[1], 0.0].into()) * vp
        }
    }

    fn cam_eye(&self) -> Vec3 {
        self.pos + Vec3::new(0.0, 1.62 + (self.bob_phase.sin() * 0.035), 0.0)
    }

    /// Boîtes d'occlusion dynamiques du frame : panneaux de porte (bloquent la
    /// lumière quand fermés), baies serveurs, joueurs distants, l'Auditeur.
    pub fn rt_dynamic_boxes(&self, models: &HashMap<String, Model>) -> Vec<GpuAabb> {
        let mut out: Vec<GpuAabb> = Vec::new();
        let trs = |pos: Vec3, yaw: f32| Mat4::from_translation(pos) * Mat4::from_rotation_y(yaw);
        let push = |out: &mut Vec<GpuAabb>, name: &str, m: Mat4| {
            if out.len() >= rtscene::MAX_RT_DYN_BOXES {
                return;
            }
            if let Some(model) = models.get(name) {
                out.push(rtscene::aabb_of_instance(&m, model.bounds.0, model.bounds.1));
            }
        };

        // Panneaux de porte à leur position animée (coulissés quand ouverts).
        for (i, d) in self.map.doors.iter().enumerate() {
            let yaw = if d.vertical_passage { 0.0 } else { std::f32::consts::FRAC_PI_2 };
            let slide = self.door_anim.get(i).copied().unwrap_or(0.0);
            let off_local = Vec3::new(slide * 1.08, 0.0, 0.0);
            let offset = Mat4::from_rotation_y(yaw).transform_vector3(off_local);
            push(&mut out, "door_panel", trs(d.pos + offset, yaw));
        }

        if self.snap.is_some() {
            // Baies serveurs.
            for s in &self.map.servers {
                push(&mut out, "server_bay", trs(s.pos, s.yaw));
            }
            // Joueurs distants + entité (boîte verticale approximative).
            for p in self.remote.values() {
                if p.state != PLAYER_OUT && p.state != PLAYER_ESCAPED {
                    push(&mut out, "tech", trs(p.pos, p.yaw));
                }
            }
            push(&mut out, "entity", trs(self.entity_vis.0, self.entity_vis.1));
        }
        out
    }

    /// Uniform du monde (view_proj, lumières, torche, brouillard).
    pub fn world_uniform(&self, aspect: f32, jitter_ndc: [f32; 2]) -> WorldUniform {
        let eye = self.cam_eye();
        let dir = self.forward();
        let view_proj = self.view_proj_mat(aspect, jitter_ndc).to_cols_array_2d();

        // Début de partie CALME : ce n'est pas un jeu d'horreur dès la première
        // seconde — l'équipe arrive sur un site de travail éclairé. Le calme
        // tient 90 s PUIS glisse vers l'horreur sur ~60 s (l'ancienne version
        // décroissait dès t=0 : à 30 s de jeu tout l'apport était déjà parti).
        let calm_raw = ((self.time - 90.0) / 60.0).clamp(0.0, 1.0);
        let calm = 1.0 - calm_raw * calm_raw * (3.0 - 2.0 * calm_raw);

        let mut u = WorldUniform {
            view_proj,
            cam_pos: [eye.x, eye.y, eye.z, 0.0],
            light_pos: [[0.0; 4]; MAX_LIGHTS],
            light_col: [[0.0; 4]; MAX_LIGHTS],
            flash_pos: [eye.x, eye.y, eye.z, 0.0],
            flash_dir: [dir.x, dir.y, dir.z, 0.955],
            misc: [0.0, self.time, 0.055 - 0.027 * calm, 0.90],
            flash_col: [1.0, 0.92, 0.78, 0.0],
            mood: [calm, self.fear, 0.0, 0.0],
        };

        if self.light_on {
            let flicker = 0.9 + 0.1 * (self.time * 30.0).sin() * (self.battery / 100.0);
            u.flash_col[3] = (self.battery / 100.0 * 1.15).max(0.0) * flicker;
        }

        let mut n = 0usize;
        if let Some(snap) = &self.snap {
            for (i, l) in self.map.lights.iter().enumerate() {
                if n >= MAX_LIGHTS {
                    break;
                }
                let on = if snap.blackout {
                    0.0
                } else if l.flicker {
                    let x = (self.time * 9.0 + i as f32 * 3.7).sin() * (self.time * 23.0 + i as f32).sin();
                    if x > 0.3 { 0.05 } else { 1.0 }
                } else {
                    1.0
                };
                // Au calme : néons plus intenses ET plus portée — le site est
                // un lieu de travail en état de marche, on voit toute la salle.
                u.light_pos[n] = [l.pos.x, l.pos.y, l.pos.z, 9.0 + 6.0 * u.mood[0]];
                u.light_col[n] = [l.color[0], l.color[1], l.color[2], (1.5 + 1.0 * u.mood[0]) * on];
                n += 1;
            }
        }
        u.misc[0] = n as f32;
        u
    }

    pub fn post_params(&self) -> [f32; 4] {
        let my_state = self
            .snap
            .as_ref()
            .and_then(|s| s.players.iter().find(|p| p.id == self.my_id))
            .map(|p| p.state)
            .unwrap_or(PLAYER_ALIVE);
        let downed = if my_state == PLAYER_DOWNED { 1.0 } else { 0.0 };
        let chase = if self.snap.as_ref().map(|s| s.entity.state == ENTITY_CHASE).unwrap_or(false) {
            1.0
        } else {
            0.0
        };
        [self.fear, self.time, downed, chase]
    }

    pub fn my_state(&self) -> u8 {
        self.snap
            .as_ref()
            .and_then(|s| s.players.iter().find(|p| p.id == self.my_id))
            .map(|p| p.state)
            .unwrap_or(PLAYER_ALIVE)
    }
}

pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = b - a;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    a + d * t
}

use crate::gpu::WorldUniform;

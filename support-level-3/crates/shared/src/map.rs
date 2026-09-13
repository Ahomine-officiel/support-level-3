//! Carte du sous-sol « NIVEAU -3 » : grille ASCII 48×30, cellule = 2 m.
//!
//! Légende :
//!   `#` mur      ` ` vide
//!   `_` hall     `.` couloir   `,` bureaux   `:` salle serveurs   `;` archives   `=` local électrique
//!   `P` spawn joueur    `E` spawn entité    `X` zone de sortie
//!   `0..5` baies serveurs à réparer    `r` rack décoratif    `d` bureau+chaise+écran
//!   `A` rayonnage d'archives   `o` carton   `p` pilier   `D` porte
//!   `R` justificatif   `T` terminal RH (remboursement)   `B` disjoncteur
//!   `b` batterie   `L` néon   `l` néon fatigué (clignote)

use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const CELL: f32 = 2.0;
pub const WALL_H: f32 = 3.0;

pub const MAP: &[&str] = &[
"################################################",
"##___XXXX____####################,,,,,,,,,,,,,##",
"##_L_______L_####################d,R,,,,T,L,o,##",
"##________b__####################,,,,,,,,,,,,,##",
"##_____P_____####################,,,,,,,d,,,,,##",
"##_L_________####################,,,,,,,,,,,,,##",
"#######D###############################D########",
"##...L..................l........L.........L..##",
"##..............p..........p..................##",
"#######D#######..####D#####..########D##########",
"##,,,,,,,,,,,,#..#,,,,,,,,#..#::L:::::L:::::::##",
"##,,d,,,d,,,d,#..#,,o,,,,,#..#::::::::::::::0:##",
"##,,,,,,,,,,,,#..#,,,,,,,,#..#:r:r:r:r:r:r::1:##",
"##,,d,,,d,,,d,#..#,,,l,,,,#.E#::::::::::::::2:##",
"##,,,,,,,,,,,,#..#,,,,b,,,#..#:r:r:r:r:r:r::3:##",
"##,,,,R,,,,,,,#..#,,,,,,,,#..#::::::::::::::4:##",
"##,,,,,,,,,,,,#..#,,,,,,,,#..#:r:r:r:r:r:r::5:##",
"##,,,,,l,,,,,,#..#,,,,,,,,#..#:::b::::::::::::##",
"###############..##########..#::::::::::::::::##",
"###############..##########..#::L:::::L:::::::##",
"###############..##########..########D##########",
"##...L........l.........L........l.........L..##",
"##............................................##",
"########D############################D##########",
"##;;o;;;;;o;;;;;;################====l=====#####",
"##;A;A;A;A;A;A;;;################====B=====#####",
"##;;;R;;;;;;;L;;;################======l===#####",
"##;A;A;A;A;A;AR;;################========b=#####",
"##;;b;;;;;;;;;;;;################==========#####",
"################################################",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloorKind {
    Hall,
    Corridor,
    Office,
    Server,
    Archives,
    Electric,
}

impl FloorKind {
    pub fn from_char(c: char) -> Option<FloorKind> {
        Some(match c {
            '_' => FloorKind::Hall,
            '.' => FloorKind::Corridor,
            ',' => FloorKind::Office,
            ':' => FloorKind::Server,
            ';' => FloorKind::Archives,
            '=' => FloorKind::Electric,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LightDef {
    pub pos: Vec3,
    pub flicker: bool,
    pub color: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ServerDef {
    pub id: u8,
    pub pos: Vec3,
    pub yaw: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DoorDef {
    pub id: u16,
    /// Centre de la porte (monde).
    pub pos: Vec3,
    /// true : passage nord-sud (murs à gauche et à droite), panneau le long de X.
    pub vertical_passage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapData {
    pub w: usize,
    pub h: usize,
    /// Type de sol par cellule (None = vide/mur).
    pub floor: Vec<Option<FloorKind>>,
    /// Cellules bloquantes statiques (murs + gros mobilier).
    pub solid: Vec<bool>,
    /// Mobilier décoratif : (type, pos monde, yaw).
    pub props: Vec<(String, Vec3, f32)>,
    /// Néons.
    pub lights: Vec<LightDef>,
    /// Baies serveurs interactives.
    pub servers: Vec<ServerDef>,
    /// Justificatifs.
    pub receipts: Vec<Vec3>,
    /// Batteries.
    pub batteries: Vec<Vec3>,
    /// Terminal RH.
    pub terminal: Vec3,
    /// Disjoncteur.
    pub breaker: Vec3,
    /// Portes.
    pub doors: Vec<DoorDef>,
    /// Spawn joueur.
    pub spawn: Vec3,
    /// Spawn entité.
    pub entity_spawn: Vec3,
    /// Cellules de sortie (monde).
    pub exit_cells: Vec<Vec3>,
}

impl MapData {
    pub fn idx(&self, c: usize, r: usize) -> usize {
        r * self.w + c
    }

    pub fn in_bounds(&self, c: isize, r: isize) -> bool {
        c >= 0 && r >= 0 && (c as usize) < self.w && (r as usize) < self.h
    }

    /// Centre monde d'une cellule.
    pub fn cell_center(&self, c: usize, r: usize) -> Vec3 {
        Vec3::new(c as f32 * CELL + CELL * 0.5, 0.0, r as f32 * CELL + CELL * 0.5)
    }

    /// Cellule contenant une position monde.
    pub fn world_cell(&self, p: Vec3) -> (isize, isize) {
        ((p.x / CELL).floor() as isize, (p.z / CELL).floor() as isize)
    }

    pub fn is_solid(&self, c: isize, r: isize) -> bool {
        if !self.in_bounds(c, r) {
            return true;
        }
        self.solid[self.idx(c as usize, r as usize)]
    }

    pub fn is_floor(&self, c: isize, r: isize) -> bool {
        self.in_bounds(c, r)
            && self.floor[self.idx(c as usize, r as usize)].is_some()
            && !self.solid[self.idx(c as usize, r as usize)]
    }

    /// Raycast grille : true si ligne de vue dégagée entre deux points (y ignoré).
    pub fn los(&self, a: Vec3, b: Vec3) -> bool {
        let steps = ((b - a).length() / (CELL * 0.35)).ceil() as isize + 1;
        for i in 1..steps {
            let t = i as f32 / steps as f32;
            let p = a.lerp(b, t);
            let (c, r) = self.world_cell(p);
            if self.is_solid(c, r) {
                return false;
            }
        }
        true
    }

    /// BFS : plus court chemin (cellules) de `from` vers `to`.
    pub fn bfs_path(
        &self,
        from: (isize, isize),
        to: (isize, isize),
    ) -> Option<Vec<(isize, isize)>> {
        if !self.is_floor(to.0, to.1) || !self.is_floor(from.0, from.1) {
            return None;
        }
        let (w, h) = (self.w as isize, self.h as isize);
        let mut prev: Vec<Option<(isize, isize)>> = vec![None; self.w * self.h];
        let mut seen = vec![false; self.w * self.h];
        let mut q = VecDeque::new();
        q.push_back(from);
        seen[self.idx(from.0 as usize, from.1 as usize)] = true;
        while let Some((c, r)) = q.pop_front() {
            if (c, r) == to {
                let mut path = vec![(c, r)];
                let mut cur = (c, r);
                while let Some(p) = prev[self.idx(cur.0 as usize, cur.1 as usize)] {
                    path.push(p);
                    cur = p;
                }
                path.reverse();
                return Some(path);
            }
            for (dc, dr) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                let nc = c + dc;
                let nr = r + dr;
                if nc < 0 || nr < 0 || nc >= w || nr >= h {
                    continue;
                }
                let i = self.idx(nc as usize, nr as usize);
                if seen[i] || !self.is_floor(nc, nr) {
                    continue;
                }
                seen[i] = true;
                prev[i] = Some((c, r));
                q.push_back((nc, nr));
            }
        }
        None
    }

    /// Parse la carte ASCII.
    pub fn parse() -> MapData {
        let h = MAP.len();
        let w = MAP[0].len();
        let mut m = MapData {
            w,
            h,
            floor: vec![None; w * h],
            solid: vec![true; w * h],
            props: Vec::new(),
            lights: Vec::new(),
            servers: Vec::new(),
            receipts: Vec::new(),
            batteries: Vec::new(),
            terminal: Vec3::ZERO,
            breaker: Vec3::ZERO,
            doors: Vec::new(),
            spawn: Vec3::ZERO,
            entity_spawn: Vec3::ZERO,
            exit_cells: Vec::new(),
        };

        let door_cells: Vec<(usize, usize)> = Vec::new();
        let _ = door_cells;

        // 1re passe : sols explicites.
        for (r, row) in MAP.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if let Some(fk) = FloorKind::from_char(ch) {
                    let i = m.idx(c, r);
                    m.floor[i] = Some(fk);
                    m.solid[i] = false;
                }
            }
        }

        // 2e passe : les cellules objets héritent du sol d'un voisin (propagation).
        let obj_chars: Vec<char> = "PXE0123456rdaoRBTbLlD".chars().collect();
        loop {
            let mut changed = false;
            for (r, row) in MAP.iter().enumerate() {
                for (c, ch) in row.chars().enumerate() {
                    if obj_chars.contains(&ch) {
                        let i = m.idx(c, r);
                        if m.floor[i].is_none() {
                            for (dc, dr) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                                let nc = c as isize + dc;
                                let nr = r as isize + dr;
                                if m.in_bounds(nc, nr) {
                                    if let Some(f) = m.floor[m.idx(nc as usize, nr as usize)] {
                                        m.floor[i] = Some(f);
                                        m.solid[i] = false;
                                        changed = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // 3e passe : objets.
        for (r, row) in MAP.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                let pos = m.cell_center(c, r);
                let i = m.idx(c, r);
                let floor_kind = m.floor[i].unwrap_or(FloorKind::Corridor);
                match ch {
                    '#' | ' ' => {}
                    'D' => {}
                    'P' => m.spawn = pos,
                    'E' => m.entity_spawn = pos,
                    'X' => m.exit_cells.push(pos),
                    '0'..='5' => {
                        let id = ch as u8 - b'0';
                        // Regarde vers la première cellule praticable voisine.
                        let mut yaw = 0.0f32;
                        for (dc, dr, y) in [
                            (1isize, 0isize, -1.5708f32),
                            (-1, 0, 1.5708),
                            (0, 1, 3.14159),
                            (0, -1, 0.0),
                        ] {
                            let nc = c as isize + dc;
                            let nr = r as isize + dr;
                            if m.in_bounds(nc, nr) && m.is_floor(nc, nr) {
                                yaw = y;
                                break;
                            }
                        }
                        m.servers.push(ServerDef { id, pos, yaw });
                        m.solid[i] = true;
                    }
                    'r' => {
                        m.props.push(("rack".into(), pos, 0.0));
                        m.solid[i] = true;
                    }
                    'd' => {
                        m.props.push(("desk_set".into(), pos, 0.0));
                        m.solid[i] = true;
                    }
                    'A' => {
                        m.props.push(("shelf".into(), pos, 0.0));
                        m.solid[i] = true;
                    }
                    'o' => {
                        m.props.push(("box_small".into(), pos, 0.0));
                        m.solid[i] = true;
                    }
                    'p' => {
                        m.props.push(("pillar".into(), pos, 0.0));
                        m.solid[i] = true;
                    }
                    'R' => m.receipts.push(pos),
                    'T' => m.terminal = pos,
                    'B' => m.breaker = pos,
                    'b' => m.batteries.push(pos),
                    'L' | 'l' => {
                        let color: [f32; 3] = match floor_kind {
                            FloorKind::Hall => [1.0, 0.85, 0.6],
                            FloorKind::Office => [0.85, 0.95, 1.0],
                            FloorKind::Server => [0.7, 0.9, 1.0],
                            FloorKind::Archives => [1.0, 0.8, 0.5],
                            FloorKind::Electric => [1.0, 0.75, 0.4],
                            FloorKind::Corridor => [0.85, 1.0, 0.85],
                        };
                        m.lights.push(LightDef {
                            pos: Vec3::new(pos.x, WALL_H - 0.15, pos.z),
                            flicker: ch == 'l',
                            color,
                        });
                        m.props.push((
                            "light_fixture".into(),
                            Vec3::new(pos.x, WALL_H - 0.08, pos.z),
                            0.0,
                        ));
                    }
                    _ => {}
                }
            }
        }

        // Murs : toute cellule '#' collée à une cellule qui a un sol.
        for r in 0..h {
            for c in 0..w {
                if MAP[r].chars().nth(c) == Some('#') {
                    let near_floor = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)].iter().any(
                        |&(dc, dr)| {
                            let nc = c as isize + dc;
                            let nr = r as isize + dr;
                            m.in_bounds(nc, nr)
                                && m.floor[m.idx(nc as usize, nr as usize)].is_some()
                        },
                    );
                    if near_floor {
                        m.props.push(("wall".into(), m.cell_center(c, r), 0.0));
                    }
                }
            }
        }

        // Sols + plafonds instanciés.
        for r in 0..h {
            for c in 0..w {
                if let Some(fk) = m.floor[m.idx(c, r)] {
                    let name = match fk {
                        FloorKind::Hall => "floor_hall",
                        FloorKind::Corridor => "floor_corridor",
                        FloorKind::Office => "floor_office",
                        FloorKind::Server => "floor_server",
                        FloorKind::Archives => "floor_arch",
                        FloorKind::Electric => "floor_elec",
                    };
                    m.props.push((name.into(), m.cell_center(c, r), 0.0));
                    m.props.push(("ceil".into(), m.cell_center(c, r), 0.0));
                }
            }
        }

        // Portes : orientation selon les murs voisins gauche/droite.
        let mut id = 0u16;
        for (r, row) in MAP.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if ch == 'D' {
                    let vertical_passage =
                        m.is_solid(c as isize - 1, r as isize) && m.is_solid(c as isize + 1, r as isize);
                    m.doors.push(DoorDef {
                        id,
                        pos: m.cell_center(c, r),
                        vertical_passage,
                    });
                    id += 1;
                }
            }
        }

        m
    }

    /// Cellule praticable la plus proche d'un point (pour l'IA).
    pub fn nearest_floor_cell(&self, p: Vec3) -> (isize, isize) {
        let (c0, r0) = self.world_cell(p);
        if self.is_floor(c0, r0) {
            return (c0, r0);
        }
        let mut best = (c0, r0);
        let mut best_d = f32::MAX;
        for r in 0..self.h {
            for c in 0..self.w {
                if self.is_floor(c as isize, r as isize) {
                    let d = (self.cell_center(c, r) - p).length_squared();
                    if d < best_d {
                        best_d = d;
                        best = (c as isize, r as isize);
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_rows_48() {
        for (i, row) in MAP.iter().enumerate() {
            assert_eq!(row.chars().count(), 48, "ligne {i} ≠ 48 caractères");
        }
    }

    #[test]
    fn test_objectives_reachable() {
        let m = MapData::parse();
        assert_eq!(m.servers.len(), 6, "6 baies serveurs");
        assert_eq!(m.receipts.len(), 4, "4 justificatifs");
        assert_eq!(m.batteries.len(), 5, "5 batteries");
        assert_eq!(m.exit_cells.len(), 4, "zone de sortie");
        assert!(m.doors.len() >= 8, "portes");
        let from = m.nearest_floor_cell(m.spawn);
        for s in &m.servers {
            let to = m.nearest_floor_cell(s.pos + Vec3::new(1.5, 0.0, 0.0));
            assert!(m.bfs_path(from, to).is_some(), "serveur {} inaccessible", s.id);
        }
        for r in &m.receipts {
            let to = m.nearest_floor_cell(*r);
            assert!(m.bfs_path(from, to).is_some(), "justificatif inaccessible");
        }
        let t = m.nearest_floor_cell(m.terminal);
        assert!(m.bfs_path(from, t).is_some(), "terminal RH inaccessible");
        let x = m.nearest_floor_cell(m.exit_cells[0]);
        assert!(m.bfs_path(from, x).is_some(), "sortie inaccessible");
        let e = m.nearest_floor_cell(m.entity_spawn);
        assert!(m.bfs_path(from, e).is_some(), "spawn entité inaccessible");
    }

    #[test]
    fn test_protocol_roundtrip() {
        use crate::protocol::*;
        let snap = Snapshot {
            time: 12.5,
            players: vec![NetPlayer { id: 1, pos: Vec3::ONE, yaw: 0.5, pitch: 0.1, state: 0, sprint: true, light_on: true }],
            entity: NetEntity { pos: Vec3::ZERO, yaw: 1.0, state: 2 },
            servers: vec![NetServer { state: 1, progress: 0.5 }; 6],
            receipts: vec![false, true, false, false],
            batteries: vec![false; 5],
            doors: vec![true, false],
            terminal_done: false,
            fee_approved: false,
            blackout: true,
            exit_open: false,
        };
        let bytes = encode(&ServerMsg::Snapshot(snap.clone()));
        let mut fr = FrameReader::new();
        fr.feed(&bytes);
        match fr.next::<ServerMsg>() {
            Some(ServerMsg::Snapshot(s2)) => {
                assert_eq!(s2.time, 12.5);
                assert_eq!(s2.players.len(), 1);
                assert_eq!(s2.entity.state, 2);
            }
            other => panic!("roundtrip failed: {:?}", other.is_some()),
        }
    }
}

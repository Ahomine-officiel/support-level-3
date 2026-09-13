//! Protocole réseau client <-> serveur (TCP, préfixe de longueur u32 LE + bincode).

use crate::map::FloorKind;
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Interactable {
    Server(u8),
    Receipt(u8),
    Terminal,
    Breaker,
    Battery(u8),
    Revive(u8),
    Door(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello { name: String, lang: u8 },
    Ping { t: f64 },
    CreateRoom { password: String },
    JoinRoom { code: String, password: String },
    LeaveRoom,
    StartGame,
    Input { pos: Vec3, yaw: f32, pitch: f32, sprint: bool, light_on: bool },
    InteractStart { target: Interactable },
    InteractStop,
    DoorToggle { id: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyPlayer {
    pub id: u8,
    pub name: String,
    pub is_host: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetPlayer {
    pub id: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub state: u8,
    pub sprint: bool,
    pub light_on: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetEntity {
    pub pos: Vec3,
    pub yaw: f32,
    pub state: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetServer {
    pub state: u8,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub time: f32,
    pub players: Vec<NetPlayer>,
    pub entity: NetEntity,
    pub servers: Vec<NetServer>,
    pub receipts: Vec<bool>,   // true = ramassé
    pub batteries: Vec<bool>,  // true = ramassé
    pub doors: Vec<bool>,      // true = ouverte
    pub terminal_done: bool,
    pub fee_approved: bool,
    pub blackout: bool,
    pub exit_open: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SfxKind {
    EntityStep,
    DoorOpen,
    DoorSlam,
    Jumpscare,
    Screech,
    RebootDone,
    Pickup,
    Stamp,
    Battery,
    Blackout,
    LightsOn,
    Breaker,
    Caught,
    Whisper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome { player_id: u8, version: u16 },
    Pong { t: f64 },
    RoomCreated { code: String },
    RoomJoined { code: String, players: Vec<LobbyPlayer>, is_host: bool },
    LobbyUpdate { players: Vec<LobbyPlayer> },
    RoomError { msg_key: u8 },
    GameStarted { your_id: u8, spawn: Vec3, snapshot: Snapshot },
    Snapshot(Snapshot),
    Event(EventMsg),
    Announcement { fr: String, en: String },
    Sfx { kind: SfxKind, pos: Vec3 },
    GameOver { win: bool, time: f32, servers_fixed: u8, escaped: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventMsg {
    ServerFixed { who: u8, server: u8 },
    ReceiptFound { who: u8, idx: u8 },
    FeeApproved { who: u8 },
    ExitOpen,
    ChaseStart,
    ChaseEnd,
    Blackout,
    LightsRestored,
    PlayerDowned { who: u8 },
    PlayerRevived { who: u8 },
    PlayerOut { who: u8 },
    PlayerEscaped { who: u8 },
    PlayerJoined { who: String },
    PlayerLeft { who: String },
}

/// Erreurs de room (clés localisées côté client).
pub const ERR_ROOM_NOT_FOUND: u8 = 0;
pub const ERR_WRONG_PASSWORD: u8 = 1;
pub const ERR_ROOM_FULL: u8 = 2;
pub const ERR_GAME_RUNNING: u8 = 3;
pub const ERR_NO_ROOM: u8 = 4;
pub const ERR_NOT_HOST: u8 = 5;

/// Encodage : u32 LE (longueur) + payload bincode.
pub fn encode(msg: &impl Serialize) -> Vec<u8> {
    let payload = bincode::serialize(msg).expect("bincode serialize");
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        FrameReader { buf: Vec::new() }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Tente d'extraire un message complet.
    pub fn next<T: serde::de::DeserializeOwned>(&mut self) -> Option<T> {
        if self.buf.len() < 4 {
            return None;
        }
        let len = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len > 1 << 20 {
            // frame invalide : on purge
            self.buf.clear();
            return None;
        }
        if self.buf.len() < 4 + len {
            return None;
        }
        let payload: Vec<u8> = self.buf[4..4 + len].to_vec();
        self.buf.drain(0..4 + len);
        bincode::deserialize(&payload).ok()
    }
}

/// Couleur de sol -> nom de zone (debug/notes).
pub fn floor_name(f: FloorKind) -> &'static str {
    match f {
        FloorKind::Hall => "hall",
        FloorKind::Corridor => "couloir",
        FloorKind::Office => "bureaux",
        FloorKind::Server => "salle serveurs",
        FloorKind::Archives => "archives",
        FloorKind::Electric => "local électrique",
    }
}

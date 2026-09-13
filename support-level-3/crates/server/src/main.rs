//! Serveur dédié « SUPPORT LEVEL -3 » : lobby, rooms avec mot de passe, simulation.

mod bots;
mod ai;
mod room;

use sl3_shared::consts::*;
use sl3_shared::protocol::*;
use std::collections::HashMap;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PlayerSlot {
    pub id: u8,
    pub name: String,
    pub tx: std::sync::mpsc::Sender<ServerMsg>,
    pub alive_conn: bool,
}

pub enum RoomCmd {
    Player(u8, ClientMsg),
    PlayerGone(u8),
    Start,
}

pub struct Room {
    pub code: String,
    pub password: Option<String>,
    pub players: Mutex<Vec<PlayerSlot>>,
    pub tx: std::sync::mpsc::Sender<RoomCmd>,
    pub next_id: Mutex<u8>,
}

impl Room {
    pub fn broadcast(&self, msg: &ServerMsg) {
        let mut players = self.players.lock().unwrap();
        players.retain(|p| !p.alive_conn || p.tx.send(msg.clone()).is_ok());
    }

    pub fn send_to(&self, id: u8, msg: &ServerMsg) {
        let mut players = self.players.lock().unwrap();
        players.retain(|p| {
            p.id != id || !p.alive_conn || p.tx.send(msg.clone()).is_ok()
        });
    }

    pub fn lobby_list(&self) -> Vec<LobbyPlayer> {
        self.players
            .lock()
            .unwrap()
            .iter()
            .map(|p| LobbyPlayer { id: p.id, name: p.name.clone(), is_host: false })
            .collect()
    }

    pub fn host_id(&self) -> u8 {
        self.players.lock().unwrap().first().map(|p| p.id).unwrap_or(0)
    }
}

pub struct Lobby {
    pub rooms: Mutex<HashMap<String, Arc<Room>>>,
}

impl Lobby {
    pub fn new() -> Self {
        Lobby { rooms: Mutex::new(HashMap::new()) }
    }

    pub fn make_code(&self) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
        let rooms = self.rooms.lock().unwrap();
        loop {
            let code: String = (0..CODE_LEN)
                .map(|_| ALPHABET[rand::random::<usize>() % ALPHABET.len()] as char)
                .collect();
            if !rooms.contains_key(&code) {
                return code;
            }
        }
    }

    pub fn create_room(&self, password: String) -> Arc<Room> {
        let code = self.make_code();
        let (tx, rx) = std::sync::mpsc::channel::<RoomCmd>();
        let room = Arc::new(Room {
            code: code.clone(),
            password: if password.is_empty() { None } else { Some(password) },
            players: Mutex::new(Vec::new()),
            tx,
            next_id: Mutex::new(1),
        });
        self.rooms.lock().unwrap().insert(code.clone(), room.clone());
        let room_for_thread = room.clone();
        thread::spawn(move || room::room_thread(rx, room_for_thread));
        room
    }

    pub fn remove_room(&self, code: &str) {
        self.rooms.lock().unwrap().remove(code);
    }
}

fn main() {
    let mut port = DEFAULT_PORT;
    let mut bots = 0usize;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PORT);
            }
            "--bots" => {
                i += 1;
                bots = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
            }
            other => {
                eprintln!("Argument inconnu : {other} (usage: server [--port 27070] [--bots N])");
            }
        }
        i += 1;
    }

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).expect("bind");
    let lobby = Arc::new(Lobby::new());
    println!("[SL3] Serveur dédié à l'écoute sur {addr}");
    println!("[SL3] Rooms max simultanées : {MAX_ROOMS}");

    if bots > 0 {
        let room = lobby.create_room(String::new());
        bots::spawn_bots(&room, bots);
        println!("[SL3] Room de test « {} » avec {bots} bots (mode smoke test)", room.code);
        // Auto-démarrage de la partie après l'enregistrement des bots.
        let room_tx = room.tx.clone();
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(1500));
            let _ = room_tx.send(RoomCmd::Start);
        });
    }

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let lobby = lobby.clone();
                thread::spawn(move || handle_conn(s, lobby));
            }
            Err(e) => eprintln!("[SL3] accept error: {e}"),
        }
    }
}

fn handle_conn(stream: TcpStream, lobby: Arc<Lobby>) {
    let write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = FrameReader::new();
    let mut read_stream = stream;

    // 1) Attendre Hello.
    let hello = loop {
        let mut buf = [0u8; 8192];
        match read_stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => reader.feed(&buf[..n]),
        }
        if let Some(msg) = reader.next::<ClientMsg>() {
            match msg {
                ClientMsg::Hello { name, .. } => break name,
                _ => return, // protocole : Hello d'abord
            }
        }
    };

    let name: String = hello.chars().take(MAX_NAME_LEN).collect();
    println!("[SL3] Connexion : {name}");
    let (out_tx, out_rx) = std::sync::mpsc::channel::<ServerMsg>();

    // Thread écrivain.
    {
        let mut w = write_stream;
        thread::spawn(move || {
            use std::io::Write;
            for msg in out_rx {
                if w.write_all(&encode(&msg)).is_err() {
                    break;
                }
                let _ = w.flush();
            }
        });
    }

    let _ = out_tx.send(ServerMsg::Welcome { player_id: 0, version: PROTOCOL_VERSION });

    // 2) Boucle principale : rooms.
    let mut current: Option<(Arc<Room>, u8)> = None;
    loop {
        let mut buf = [0u8; 8192];
        let n = match read_stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        reader.feed(&buf[..n]);
        while let Some(msg) = reader.next::<ClientMsg>() {
            match msg {
                ClientMsg::Ping { t } => {
                    let _ = out_tx.send(ServerMsg::Pong { t });
                }
                ClientMsg::CreateRoom { password } => {
                    if current.is_some() {
                        leave(&current, &lobby);
                    }
                    let rooms = lobby.rooms.lock().unwrap();
                    if rooms.len() >= MAX_ROOMS {
                        let _ = out_tx.send(ServerMsg::RoomError { msg_key: ERR_ROOM_NOT_FOUND });
                        continue;
                    }
                    drop(rooms);
                    let room = lobby.create_room(password);
                    let id = join_room(&room, name.clone(), &out_tx);
                    let code_str = room.code.clone();
                    let _ = out_tx.send(ServerMsg::RoomCreated { code: code_str.clone() });
                    let players = room.lobby_list();
                    let is_host = room.host_id() == id;
                    let _ = out_tx.send(ServerMsg::RoomJoined { code: code_str.clone(), players, is_host });
                    room.broadcast(&ServerMsg::LobbyUpdate { players: room.lobby_list() });
                    println!("[SL3] {name} crée la room {code_str}");
                    current = Some((room, id));
                }
                ClientMsg::JoinRoom { code, password } => {
                    if current.is_some() {
                        leave(&current, &lobby);
                    }
                    let room = lobby.rooms.lock().unwrap().get(&code).cloned();
                    match room {
                        None => {
                            let _ = out_tx.send(ServerMsg::RoomError { msg_key: ERR_ROOM_NOT_FOUND });
                        }
                        Some(room) => {
                            if let Some(pw) = &room.password {
                                if *pw != password {
                                    let _ = out_tx.send(ServerMsg::RoomError { msg_key: ERR_WRONG_PASSWORD });
                                    continue;
                                }
                            }
                            let count = room.players.lock().unwrap().len();
                            if count >= MAX_PLAYERS {
                                let _ = out_tx.send(ServerMsg::RoomError { msg_key: ERR_ROOM_FULL });
                                continue;
                            }
                            let id = join_room(&room, name.clone(), &out_tx);
                            let code_str = room.code.clone();
                            let players = room.lobby_list();
                            let is_host = room.host_id() == id;
                            let _ = out_tx.send(ServerMsg::RoomJoined { code: code_str, players, is_host });
                            room.broadcast(&ServerMsg::LobbyUpdate { players: room.lobby_list() });
                            println!("[SL3] {name} rejoint la room {code}");
                            current = Some((room, id));
                        }
                    }
                }
                ClientMsg::LeaveRoom => {
                    leave(&current, &lobby);
                    current = None;
                }
                ClientMsg::StartGame => {
                    if let Some((room, id)) = &current {
                        let _ = room.tx.send(RoomCmd::Start);
                        let _ = id; // le serveur vérifie l'hôte dans room_thread
                    }
                }
                ClientMsg::Input { .. } | ClientMsg::InteractStart { .. }
                | ClientMsg::InteractStop | ClientMsg::DoorToggle { .. } => {
                    if let Some((room, id)) = &current {
                        let _ = room.tx.send(RoomCmd::Player(*id, msg));
                    }
                }
                ClientMsg::Hello { .. } => {}
            }
        }
    }

    println!("[SL3] Déconnexion : {name}");
    leave(&current, &lobby);
}

fn join_room(room: &Arc<Room>, name: String, tx: &std::sync::mpsc::Sender<ServerMsg>) -> u8 {
    let mut next = room.next_id.lock().unwrap();
    let id = *next;
    *next = next.wrapping_add(1).max(1);
    drop(next);
    room.players.lock().unwrap().push(PlayerSlot {
        id,
        name,
        tx: tx.clone(),
        alive_conn: true,
    });
    id
}

fn leave(current: &Option<(Arc<Room>, u8)>, lobby: &Arc<Lobby>) {
    if let Some((room, id)) = current {
        let _ = room.tx.send(RoomCmd::PlayerGone(*id));
        room.players.lock().unwrap().retain(|p| p.id != *id);
        let remaining = room.players.lock().unwrap().len();
        if remaining == 0 {
            lobby.remove_room(&room.code);
        } else {
            room.broadcast(&ServerMsg::LobbyUpdate { players: room.lobby_list() });
        }
    }
}

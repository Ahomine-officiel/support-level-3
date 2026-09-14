//! Application winit : menus bilingues, lobby, boucle de jeu, audio, réseau.

use crate::audio::Audio;
use crate::config::Config;
use crate::game::Game;
use crate::gpu::{Renderer, UiOp};
use crate::hud;
use crate::lang::{t, Lang, *};
use crate::net::{NetEvent, NetHandle};
use glam::Vec3;
use sl3_shared::consts::*;
use sl3_shared::map::MapData;
use sl3_shared::protocol::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

#[derive(Clone)]
enum MenuScreen {
    Main,
    /// Création de partie : mot de passe + nombre de bots (compagnons IA).
    /// L'adresse n'est plus demandée : un serveur local intégré démarre tout seul.
    HostSetup { buf: String, bots: u8 },
    AskJoinAddr { buf: String },
    AskCode { buf: String },
    AskPassword { code: String, buf: String },
    AskName { buf: String },
    Options,
}

enum Mode {
    MainMenu(MenuScreen),
    Lobby { code: String, is_host: bool },
    Playing,
    GameOver { win: bool, time: f32, servers: u8, escaped: u8 },
}

enum Pending {
    Create { password: String },
    Join { code: String, password: String },
}

// ----- Autopilot (scénario de rendu automatisé, via SL3_AUTOPILOT) -----

struct AutoStep {
    at: f64,
    act: AutoAct,
}

enum AutoAct {
    Key(KeyCode),
    Shot(String),
    Yaw(f32),
    /// Déplace le curseur (x, y en pixels fenêtre).
    Mouse(f64, f64),
    /// Clic gauche à (x, y) — menus à boutons.
    Click(f64, f64),
    /// Regard caméra : mêmes maths que le raw input (DeviceEvent::MouseMotion).
    Look(f64, f64),
    /// Tape du texte dans le champ actif (comme event.text de winit).
    Type(String),
    /// Maintient une touche enfoncée (marche : down W … up W).
    Down(KeyCode),
    Up(KeyCode),
    /// Téléport QA : tp x z yaw (place le joueur exactement).
    Tp(f32, f32, f32),
    Exit,
}

impl Clone for AutoAct {
    fn clone(&self) -> Self {
        match self {
            AutoAct::Key(k) => AutoAct::Key(*k),
            AutoAct::Shot(p) => AutoAct::Shot(p.clone()),
            AutoAct::Yaw(a) => AutoAct::Yaw(*a),
            AutoAct::Mouse(x, y) => AutoAct::Mouse(*x, *y),
            AutoAct::Click(x, y) => AutoAct::Click(*x, *y),
            AutoAct::Look(x, y) => AutoAct::Look(*x, *y),
            AutoAct::Type(s) => AutoAct::Type(s.clone()),
            AutoAct::Down(k) => AutoAct::Down(*k),
            AutoAct::Up(k) => AutoAct::Up(*k),
            AutoAct::Tp(x, z, y) => AutoAct::Tp(*x, *z, *y),
            AutoAct::Exit => AutoAct::Exit,
        }
    }
}

struct Auto {
    t0: Instant,
    steps: Vec<AutoStep>,
    next: usize,
}

fn parse_key(name: &str) -> Option<KeyCode> {
    Some(match name {
        "Enter" | "NumpadEnter" => KeyCode::Enter,
        "Digit1" | "1" => KeyCode::Digit1,
        "Digit2" | "2" => KeyCode::Digit2,
        "Digit3" | "3" => KeyCode::Digit3,
        "Space" => KeyCode::Space,
        "Escape" => KeyCode::Escape,
        "Backspace" => KeyCode::Backspace,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "KeyF" | "F" => KeyCode::KeyF,
        "KeyE" | "E" => KeyCode::KeyE,
        "KeyW" | "W" | "KeyZ" | "Z" => KeyCode::KeyW,
        "KeyA" | "A" | "KeyQ" | "Q" => KeyCode::KeyA,
        "KeyS" | "S" => KeyCode::KeyS,
        "KeyD" | "D" => KeyCode::KeyD,
        _ => return None,
    })
}

/// Format : « key Digit1@1.0 ; shot /tmp/x.png@3.0 ; exit@3.2 »
fn parse_autopilot(s: &str) -> Option<Auto> {
    let mut steps = Vec::new();
    for tok in s.split(';') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let (head, at) = tok.rsplit_once('@')?;
        let at: f64 = at.trim().parse().ok()?;
        let mut parts = head.splitn(2, ' ');
        let verb = parts.next().unwrap_or("").trim();
        let arg = parts.next().unwrap_or("").trim();
        let act = match verb {
            "key" => AutoAct::Key(parse_key(arg)?),
            "shot" => AutoAct::Shot(arg.to_string()),
            "yaw" => AutoAct::Yaw(arg.parse::<f32>().ok()?.to_radians()),
            "type" => AutoAct::Type(arg.to_string()),
            "down" => AutoAct::Down(parse_key(arg)?),
            "up" => AutoAct::Up(parse_key(arg)?),
            "exit" => AutoAct::Exit,
            _ => {
                // « mouse x y » et « click x y » : pilotage des boutons cliquables.
                let mut it = head.split_whitespace();
                let verb2 = it.next().unwrap_or("");
                let x: f64 = it.next()?.parse().ok()?;
                let y: f64 = it.next()?.parse().ok()?;
                match verb2 {
                    "mouse" => AutoAct::Mouse(x, y),
                    "click" => AutoAct::Click(x, y),
                    "look" => AutoAct::Look(x, y),
                    "tp" => AutoAct::Tp(x as f32, y as f32, 0.0),
                    _ => return None,
                }
            }
        };
        steps.push(AutoStep { at, act });
    }
    steps.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    if steps.is_empty() {
        None
    } else {
        Some(Auto { t0: Instant::now(), steps, next: 0 })
    }
}

const WHITE_C: [f32; 4] = [0.9, 0.92, 0.95, 1.0];
const DIM_C: [f32; 4] = [0.6, 0.63, 0.66, 0.85];
const GREEN_C: [f32; 4] = [0.5, 0.95, 0.55, 1.0];
const RED_C: [f32; 4] = [1.0, 0.4, 0.35, 1.0];
const AMBER_C: [f32; 4] = [1.0, 0.78, 0.3, 1.0];

// ----- Boutons cliquables (souris) -----

#[derive(Clone, Copy, PartialEq, Debug)]
enum BtnAction {
    Host,
    Join,
    OptionsMenu,
    Quit,
    Lang,
    Name,
    SensDown,
    SensUp,
    /// Échelle de rendu : 0 = auto, 1..4 = 100/85/70/55 %.
    Res(u8),
    /// 0 = natif, 1 = FSR 3, 2 = DLSS.
    Ups(u8),
    /// Preset qualité upscaling : 0 qualité, 1 équilibré, 2 performance.
    UpsQ(u8),
    /// 0 off, 1 qualité, 2 ultra.
    Rt(u8),
    /// Bots (compagnons IA) à l'hébergement.
    BotsUp,
    BotsDown,
    Back,
    StartGame,
    LeaveLobby,
    Resume,
    QuitToMenu,
    OverMenu,
}

struct HotBtn {
    action: BtnAction,
    /// x, y, w, h (pixels de la fenêtre).
    rect: [f32; 4],
}

#[derive(Clone, Copy, PartialEq)]
enum BtnState {
    Normal,
    Selected,
    Disabled,
}

/// Compteur de frames (debug : SL3_DEBUG=1).
static FRAME_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub struct App {
    config: Config,
    lang: Lang,
    mode: Mode,
    net: Option<NetHandle>,
    pending: Option<Pending>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    game: Option<Box<Game>>,
    keys: HashSet<KeyCode>,
    cursor_locked: bool,
    paused: bool,
    last_frame: Instant,
    err: Option<(Instant, String)>,
    /// Moyenne glissante du temps de frame (pour la résolution dynamique).
    ema_frame: f32,
    last_scale_adj: Instant,
    audio: Option<Audio>,
    map: Option<&'static MapData>,
    players: Vec<LobbyPlayer>,
    /// Vrai si aucun fichier de config n'existait (1er lancement : auto-config RT).
    fresh_config: bool,
    /// Scénario autopilot (SL3_AUTOPILOT) : capture d'écrans automatisée.
    auto: Option<Auto>,
    /// Position du curseur (pixels fenêtre) pour les menus.
    cursor_pos: (f32, f32),
    /// Boutons cliquables reconstruits à chaque frame de rendu UI.
    hot_btns: Vec<HotBtn>,
}

impl App {
    fn new() -> App {
        let fresh_config = !Config::path().exists();
        let config = Config::load();
        let lang = Lang::from_str(&config.lang);
        let audio = Audio::new(config.volume);
        let auto = std::env::var("SL3_AUTOPILOT")
            .ok()
            .and_then(|s| parse_autopilot(&s));
        if std::env::var("SL3_DEBUG").is_ok() {
            match &auto {
                Some(a) => eprintln!("[sl3-debug] autopilot : {} steps", a.steps.len()),
                None => eprintln!("[sl3-debug] autopilot : absent ou non parsé"),
            }
        }
        App {
            config,
            lang,
            mode: Mode::MainMenu(MenuScreen::Main),
            net: None,
            pending: None,
            window: None,
            renderer: None,
            game: None,
            keys: HashSet::new(),
            cursor_locked: false,
            paused: false,
            last_frame: Instant::now(),
            err: None,
            ema_frame: 1.0 / 60.0,
            last_scale_adj: Instant::now(),
            audio,
            map: None,
            players: Vec::new(),
            fresh_config,
            auto,
            cursor_pos: (0.0, 0.0),
            hot_btns: Vec::new(),
        }
    }

    /// Cycle le mode ray tracing (off -> qualité -> ultra -> overdrive), applique et persiste.
    fn cycle_rt_mode(&mut self) -> &'static str {
        self.config.rt_mode = (self.config.rt_mode + 1) % 4;
        self.config.save();
        if let Some(r) = self.renderer.as_mut() {
            r.set_rt_mode(self.config.rt_mode);
        }
        match self.config.rt_mode {
            1 => t(self.lang, RT_QUAL),
            2 => t(self.lang, RT_ULTRA),
            3 => t(self.lang, RT_OVERDRIVE),
            _ => t(self.lang, RT_OFF),
        }
    }

    /// Cycle l'upscaler (natif -> FSR 3 -> DLSS si RTX), applique et persiste.
    fn cycle_upscaler(&mut self) -> &'static str {
        let rtx = self
            .renderer
            .as_ref()
            .map(|r| r.adapter_name.to_uppercase().contains("RTX"))
            .unwrap_or(false);
        self.config.upscaler = match self.config.upscaler {
            0 => 1,
            1 if rtx => 2,
            _ => 0,
        };
        self.apply_upscaler();
        match self.config.upscaler {
            1 => t(self.lang, UP_FSR3),
            2 => t(self.lang, UP_DLSS),
            _ => t(self.lang, UP_NATIVE),
        }
    }

    /// Applique upscaler + preset qualité au renderer (DLSS -> FSR 3 si pas de RTX).
    fn apply_upscaler(&mut self) {
        let mut mode = self.config.upscaler;
        let rtx = self
            .renderer
            .as_ref()
            .map(|r| r.adapter_name.to_uppercase().contains("RTX"))
            .unwrap_or(false);
        if mode == 2 && !rtx {
            mode = 1; // DLSS indisponible : repli transparent sur FSR 3
        }
        self.config.upscaler = mode;
        self.config.save();
        if let Some(r) = self.renderer.as_mut() {
            r.set_upscaler(mode, self.config.upscale_quality);
        }
    }

    /// Clic souris (menus) : déclenche le bouton sous le curseur, s'il existe.
    fn on_click(&mut self) {
        let (cx, cy) = self.cursor_pos;
        let hit = self
            .hot_btns
            .iter()
            .find(|b| cx >= b.rect[0] && cx <= b.rect[0] + b.rect[2] && cy >= b.rect[1] && cy <= b.rect[1] + b.rect[3])
            .map(|b| b.action);
        if std::env::var("SL3_DEBUG").is_ok() {
            eprintln!("[sl3-debug] clic ({cx},{cy}) sur {hit:?} ({} boutons chauds)", self.hot_btns.len());
        }
        if let Some(action) = hit {
            self.dispatch(action);
        }
    }

    /// Exécute l'action d'un bouton (partagée clavier / souris).
    fn dispatch(&mut self, action: BtnAction) {
        match action {
            BtnAction::Host => {
                // Hébergement : plus d'adresse à saisir — un serveur local intégré
                // démarre automatiquement (l'erreur « connexion refusée » est morte).
                self.mode = Mode::MainMenu(MenuScreen::HostSetup { buf: String::new(), bots: 1 });
            }
            BtnAction::Join => {
                self.mode = Mode::MainMenu(MenuScreen::AskJoinAddr {
                    buf: self.config.host_default.clone(),
                });
            }
            BtnAction::OptionsMenu => {
                self.mode = Mode::MainMenu(MenuScreen::Options);
            }
            BtnAction::Quit => std::process::exit(0),
            BtnAction::Lang => {
                self.lang.toggle();
                self.config.lang = match self.lang {
                    Lang::Fr => "fr".into(),
                    Lang::En => "en".into(),
                };
                self.config.save();
            }
            BtnAction::Name => {
                self.mode = Mode::MainMenu(MenuScreen::AskName {
                    buf: self.config.name.clone(),
                });
            }
            BtnAction::SensDown => {
                self.config.sensitivity = (self.config.sensitivity - 0.1).max(0.2);
                self.config.save();
            }
            BtnAction::SensUp => {
                self.config.sensitivity = (self.config.sensitivity + 0.1).min(3.0);
                self.config.save();
            }
            BtnAction::Res(v) => {
                self.config.render_scale = match v {
                    0 => 0.0,
                    1 => 1.0,
                    2 => 0.85,
                    3 => 0.7,
                    _ => 0.55,
                };
                self.config.save();
                if let Some(r) = self.renderer.as_mut() {
                    if self.config.render_scale > 0.0 {
                        r.set_render_scale(self.config.render_scale);
                    }
                }
            }
            BtnAction::Ups(v) => {
                self.config.upscaler = v;
                self.apply_upscaler();
            }
            BtnAction::UpsQ(v) => {
                self.config.upscale_quality = v;
                self.apply_upscaler();
            }
            BtnAction::Rt(v) => {
                self.config.rt_mode = v;
                self.config.save();
                if let Some(r) = self.renderer.as_mut() {
                    r.set_rt_mode(v);
                }
            }
            BtnAction::BotsUp => {
                if let Mode::MainMenu(MenuScreen::HostSetup { buf, bots }) = &self.mode {
                    let (buf, bots) = (buf.clone(), (*bots).min(2) + 1);
                    self.mode = Mode::MainMenu(MenuScreen::HostSetup { buf, bots });
                }
            }
            BtnAction::BotsDown => {
                if let Mode::MainMenu(MenuScreen::HostSetup { buf, bots }) = &self.mode {
                    let (buf, bots) = (buf.clone(), (*bots).max(1) - 1);
                    self.mode = Mode::MainMenu(MenuScreen::HostSetup { buf, bots });
                }
            }
            BtnAction::Back => {
                self.mode = Mode::MainMenu(MenuScreen::Main);
            }
            BtnAction::StartGame => {
                if let Some(n) = &self.net {
                    n.send(ClientMsg::StartGame);
                }
            }
            BtnAction::LeaveLobby => self.leave_room(),
            BtnAction::Resume => {
                self.paused = false;
                self.set_cursor_locked(true);
            }
            BtnAction::QuitToMenu => self.back_to_menu(),
            BtnAction::OverMenu => self.leave_room(),
        }
    }

    fn show_err(&mut self, msg: String) {
        self.err = Some((Instant::now(), msg));
    }

    fn connect(&mut self, addr: String, pending: Pending) {
        let lang_flag = if matches!(self.lang, Lang::En) { 1 } else { 0 };
        match crate::net::connect(&addr, self.config.name.clone(), lang_flag) {
            Ok(h) => {
                self.net = Some(h);
                self.pending = Some(pending);
            }
            Err(e) => self.show_err(format!("{} : {e}", t(self.lang, CONNECTING))),
        }
    }

    /// Hébergement : démarre un serveur local intégré dans ce processus puis s'y
    /// connecte. Plus besoin de lancer sl3-server à la main : cliquer « Héberger »
    /// suffit (fini l'erreur « connexion refusée (os error 10061) »).
    fn start_hosting(&mut self, password: String, bots: usize) {
        match sl3_server::spawn_local(bots) {
            Ok(port) => {
                let addr = format!("127.0.0.1:{port}");
                self.connect(addr, Pending::Create { password });
            }
            Err(e) => self.show_err(format!("{} : {e}", t(self.lang, E_LOCAL_SERVER))),
        }
    }

    /// Saisie clavier caractère par caractère (winit : `event.text`) pour les
    /// champs texte (mot de passe, code de room, adresse, nom). Avant cette
    /// fonction, il était IMPOSSIBLE de taper du texte dans ces champs.
    fn handle_text_char(&mut self, ch: char) {
        if !matches!(self.mode, Mode::MainMenu(_)) {
            return;
        }
        if let Mode::MainMenu(screen) = &self.mode {
            let next = match screen {
                MenuScreen::AskJoinAddr { buf } => {
                    if buf.chars().count() < 32 {
                        let mut b = buf.clone();
                        b.push(ch);
                        Some(MenuScreen::AskJoinAddr { buf: b })
                    } else {
                        None
                    }
                }
                MenuScreen::AskCode { buf } => {
                    if buf.chars().count() < CODE_LEN {
                        let mut b = buf.clone();
                        b.push(ch);
                        Some(MenuScreen::AskCode { buf: b })
                    } else {
                        None
                    }
                }
                MenuScreen::AskPassword { code, buf } => {
                    if buf.chars().count() < 24 {
                        let mut b = buf.clone();
                        b.push(ch);
                        Some(MenuScreen::AskPassword { code: code.clone(), buf: b })
                    } else {
                        None
                    }
                }
                MenuScreen::AskName { buf } => {
                    if buf.chars().count() < MAX_NAME_LEN {
                        let mut b = buf.clone();
                        b.push(ch);
                        Some(MenuScreen::AskName { buf: b })
                    } else {
                        None
                    }
                }
                MenuScreen::HostSetup { buf, bots } => {
                    if buf.chars().count() < 24 {
                        let mut b = buf.clone();
                        b.push(ch);
                        Some(MenuScreen::HostSetup { buf: b, bots: *bots })
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(s) = next {
                self.mode = Mode::MainMenu(s);
            }
        }
    }

    /// Regard caméra par deltas SOURIS BRUTS (raw input).
    /// Appelé depuis `DeviceEvent::MouseMotion` (WM_INPUT sous Windows : deltas
    /// indépendants de la position du curseur, de l'accélération système et du
    /// pointer lock) et depuis l'autopilot (« look dx dy »).
    fn apply_look(&mut self, dx: f64, dy: f64) {
        if let Some(g) = self.game.as_mut() {
            let sens = self.config.sensitivity;
            g.yaw += (dx as f32) * 0.0022 * sens;
            g.pitch -= (dy as f32) * 0.0022 * sens;
            g.pitch = g.pitch.clamp(-1.45, 1.45);
        }
    }

    fn back_to_menu(&mut self) {
        self.mode = Mode::MainMenu(MenuScreen::Main);
        self.paused = false;
        self.game = None;
        self.set_cursor_locked(false);
        if let Some(a) = self.audio.as_mut() {
            a.stop_loop("ambience");
            a.stop_loop("fluorescent");
            a.stop_loop("whisper");
        }
    }

    fn leave_room(&mut self) {
        if let Some(n) = &self.net {
            n.send(ClientMsg::LeaveRoom);
        }
        self.back_to_menu();
    }

    fn set_cursor_locked(&mut self, locked: bool) {
        self.cursor_locked = locked;
        if let Some(w) = &self.window {
            if locked {
                let _ = w
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                    .or_else(|_| w.set_cursor_grab(winit::window::CursorGrabMode::Confined));
                let _ = w.set_cursor_visible(false);
            } else {
                let _ = w.set_cursor_grab(winit::window::CursorGrabMode::None);
                let _ = w.set_cursor_visible(true);
            }
        }
    }

    // ---------------- Réseau ----------------

    fn handle_net_events(&mut self) {
        let events: Vec<NetEvent> = match &self.net {
            Some(n) => n.rx.try_iter().collect(),
            None => return,
        };
        for ev in events {
            match ev {
                NetEvent::Connected => {}
                NetEvent::Closed => {
                    if self.net.is_some() {
                        self.show_err(t(self.lang, E_ERR_CLOSED).to_string());
                        self.net = None;
                        self.back_to_menu();
                    }
                }
                NetEvent::Msg(msg) => self.handle_msg(msg),
            }
        }
    }

    fn handle_msg(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Welcome { .. } => {
                if let Some(p) = self.pending.take() {
                    if let Some(n) = self.net.as_ref() {
                        match p {
                            Pending::Create { password } => n.send(ClientMsg::CreateRoom { password }),
                            Pending::Join { code, password } => {
                                n.send(ClientMsg::JoinRoom { code, password })
                            }
                        }
                    }
                }
            }
            ServerMsg::RoomCreated { .. } => {}
            ServerMsg::RoomJoined { code, players, is_host } => {
                self.mode = Mode::Lobby { code, is_host };
                self.players = players;
            }
            ServerMsg::LobbyUpdate { players } => self.players = players,
            ServerMsg::RoomError { msg_key } => {
                let key = match msg_key {
                    ERR_ROOM_NOT_FOUND => E_ERR_NOT_FOUND,
                    ERR_WRONG_PASSWORD => E_ERR_PASSWORD,
                    ERR_ROOM_FULL => E_ERR_FULL,
                    ERR_GAME_RUNNING => E_ERR_GAME,
                    ERR_NO_ROOM => E_ERR_NO_ROOM,
                    ERR_NOT_HOST => E_ERR_NOT_HOST,
                    _ => E_ERR_CLOSED,
                };
                self.show_err(t(self.lang, key).to_string());
                self.mode = Mode::MainMenu(MenuScreen::Main);
            }
            ServerMsg::GameStarted { your_id, spawn, snapshot } => {
                let map: &'static MapData = Box::leak(Box::new(MapData::parse()));
                self.map = Some(map);
                if let Some(r) = self.renderer.as_mut() {
                    r.reset_upscale = true;
                    let mut g = Game::new(map, your_id, spawn, r);
                    g.apply_snapshot(snapshot);
                    self.game = Some(Box::new(g));
                    self.mode = Mode::Playing;
                    self.paused = false;
                    self.set_cursor_locked(true);
                    if let Some(a) = self.audio.as_mut() {
                        a.start_loop("ambience", 0.5);
                        a.start_loop("fluorescent", 0.12);
                    }
                }
            }
            ServerMsg::Snapshot(snap) => {
                if let Some(g) = self.game.as_mut() {
                    g.apply_snapshot(snap);
                }
            }
            ServerMsg::Announcement { fr, en } => {
                if let Some(g) = self.game.as_mut() {
                    let msg = match self.lang {
                        Lang::Fr => fr,
                        Lang::En => en,
                    };
                    g.add_feed(msg);
                }
            }
            ServerMsg::Sfx { kind, pos } => self.play_sfx(kind, pos),
            ServerMsg::Event(_) => {}
            ServerMsg::GameOver { win, time, servers_fixed, escaped } => {
                self.mode = Mode::GameOver { win, time, servers: servers_fixed, escaped };
                self.set_cursor_locked(false);
                if let Some(a) = self.audio.as_mut() {
                    a.stop_loop("ambience");
                    a.stop_loop("fluorescent");
                    a.stop_loop("whisper");
                }
            }
            ServerMsg::Pong { .. } => {}
        }
    }

    fn play_sfx(&mut self, kind: SfxKind, pos: Vec3) {
        let my = self.game.as_ref().map(|g| g.pos);
        let gain = match my {
            Some(my) => (1.0 - my.distance(pos) / 26.0).clamp(0.0, 1.0),
            None => 0.5,
        };
        if let Some(a) = &self.audio {
            let name = match kind {
                SfxKind::EntityStep => {
                    if rand_int(2) == 0 {
                        "entity_step1"
                    } else {
                        "entity_step2"
                    }
                }
                SfxKind::DoorOpen => "door_open",
                SfxKind::DoorSlam => "door_slam",
                SfxKind::Jumpscare => "jumpscare",
                SfxKind::Screech => "screech",
                SfxKind::RebootDone => "reboot_done",
                SfxKind::Pickup => "pickup",
                SfxKind::Stamp => "stamp",
                SfxKind::Battery => "battery",
                SfxKind::Blackout => "blackout",
                SfxKind::LightsOn => "lights_on",
                SfxKind::Breaker => "breaker",
                SfxKind::Caught => "caught",
                SfxKind::Whisper => "whisper",
            };
            let g2 = match kind {
                SfxKind::Jumpscare | SfxKind::Caught => 1.0,
                SfxKind::Whisper => gain * 0.5,
                _ => gain,
            };
            a.play(name, g2);
        }
    }

    // ---------------- Clavier ----------------

    fn on_key_pressed(&mut self, code: KeyCode, _el: &ActiveEventLoop) {
        if code == KeyCode::F12 {
            // Capture d'écran instantanée -> PNG à côté de l'exécutable.
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            if let Some(r) = &mut self.renderer {
                r.capture_path = Some(std::path::PathBuf::from(format!("sl3_shot_{ms}.png")));
            }
            return;
        }
        if code == KeyCode::F1 {
            self.lang.toggle();
            self.config.lang = match self.lang {
                Lang::Fr => "fr".into(),
                Lang::En => "en".into(),
            };
            self.config.save();
            return;
        }

        let in_menu = matches!(self.mode, Mode::MainMenu(_));
        let in_lobby = matches!(self.mode, Mode::Lobby { .. });
        let in_game = matches!(self.mode, Mode::Playing);
        let in_over = matches!(self.mode, Mode::GameOver { .. });

        if in_menu {
            if let Mode::MainMenu(screen) = &self.mode {
                let screen = screen.clone();
                self.menu_key(&screen, code);
            }
        } else if in_lobby {
            match code {
                KeyCode::Space => {
                    if let Some(n) = &self.net {
                        n.send(ClientMsg::StartGame);
                    }
                }
                KeyCode::Escape => self.leave_room(),
                _ => {}
            }
        } else if in_game {
            match code {
                KeyCode::Escape => {
                    self.paused = !self.paused;
                    self.set_cursor_locked(!self.paused);
                }
                KeyCode::KeyF if !self.paused => {
                    if let Some(g) = self.game.as_mut() {
                        if g.battery > 0.0 {
                            g.light_on = !g.light_on;
                        }
                    }
                }
                KeyCode::KeyE if !self.paused => {
                    self.interact_pressed();
                }
                KeyCode::F5 => {
                    let label = self.cycle_rt_mode();
                    if let Some(g) = self.game.as_mut() {
                        g.add_feed(format!("{} : {}", t(self.lang, RT_TOAST), label));
                    }
                }
                KeyCode::F6 => {
                    let label = self.cycle_upscaler();
                    if let Some(g) = self.game.as_mut() {
                        g.add_feed(format!("{} : {}", t(self.lang, UP_TOAST), label));
                    }
                }
                _ => {}
            }
        } else if in_over && code == KeyCode::Enter {
            self.leave_room();
        }
    }

    fn on_key_released(&mut self, code: KeyCode) {
        if code == KeyCode::KeyE && matches!(self.mode, Mode::Playing) {
            if let Some(n) = &self.net {
                n.send(ClientMsg::InteractStop);
            }
        }
    }

    fn interact_pressed(&mut self) {
        let target = self.game.as_ref().and_then(|g| g.find_target().map(|(t, _)| t));
        if let (Some(n), Some(target)) = (self.net.as_ref(), target) {
            match target {
                Interactable::Door(id) => n.send(ClientMsg::DoorToggle { id }),
                other => n.send(ClientMsg::InteractStart { target: other }),
            }
        }
    }

    // ---------------- Menus ----------------

    fn menu_key(&mut self, screen: &MenuScreen, code: KeyCode) {
        let enter = code == KeyCode::Enter || code == KeyCode::NumpadEnter;
        let back = code == KeyCode::Escape;
        match screen {
            MenuScreen::Main => match code {
                KeyCode::Digit1 => {
                    self.mode = Mode::MainMenu(MenuScreen::HostSetup { buf: String::new(), bots: 1 });
                }
                KeyCode::Digit2 => {
                    self.mode = Mode::MainMenu(MenuScreen::AskJoinAddr {
                        buf: self.config.host_default.clone(),
                    });
                }
                KeyCode::Digit3 => {
                    self.mode = Mode::MainMenu(MenuScreen::Options);
                }
                KeyCode::Escape => std::process::exit(0),
                _ => {}
            },
            MenuScreen::HostSetup { buf, bots } => {
                if back {
                    self.back_to_menu();
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    let bots = *bots;
                    self.mode = Mode::MainMenu(MenuScreen::HostSetup { buf: b, bots });
                } else if enter {
                    let password = buf.clone();
                    let bots = *bots as usize;
                    // Démarre un serveur local intégré puis s'y connecte (création de room).
                    self.start_hosting(password, bots);
                }
            }
            MenuScreen::AskJoinAddr { buf } => {
                if back {
                    self.back_to_menu();
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    self.mode = Mode::MainMenu(MenuScreen::AskJoinAddr { buf: b });
                } else if enter {
                    self.config.host_default = buf.clone();
                    self.config.save();
                    self.mode = Mode::MainMenu(MenuScreen::AskCode { buf: String::new() });
                }
            }
            MenuScreen::AskCode { buf } => {
                if back {
                    self.back_to_menu();
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    self.mode = Mode::MainMenu(MenuScreen::AskCode { buf: b });
                } else if enter {
                    let code_s = buf.trim().to_uppercase();
                    self.mode = Mode::MainMenu(MenuScreen::AskPassword {
                        code: code_s,
                        buf: String::new(),
                    });
                }
            }
            MenuScreen::AskPassword { code: room_code, buf } => {
                if back {
                    self.back_to_menu();
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    self.mode = Mode::MainMenu(MenuScreen::AskPassword {
                        code: room_code.clone(),
                        buf: b,
                    });
                } else if enter {
                    let password = buf.clone();
                    let code_s = room_code.clone();
                    let addr = self.config.host_default.clone();
                    self.connect(addr, Pending::Join { code: code_s, password });
                }
            }
            MenuScreen::AskName { buf } => {
                if back || enter {
                    if enter && !buf.trim().is_empty() {
                        self.config.name = buf.trim().chars().take(MAX_NAME_LEN).collect();
                        self.config.save();
                    }
                    self.mode = Mode::MainMenu(MenuScreen::Options);
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    self.mode = Mode::MainMenu(MenuScreen::AskName { buf: b });
                }
            }
            MenuScreen::Options => match code {
                KeyCode::Escape | KeyCode::Enter => {
                    self.mode = Mode::MainMenu(MenuScreen::Main);
                }
                KeyCode::KeyL => {
                    self.lang.toggle();
                    self.config.lang = match self.lang {
                        Lang::Fr => "fr".into(),
                        Lang::En => "en".into(),
                    };
                    self.config.save();
                }
                KeyCode::KeyN => {
                    self.mode = Mode::MainMenu(MenuScreen::AskName {
                        buf: self.config.name.clone(),
                    });
                }
                KeyCode::KeyR => {
                    // Cycle : auto -> 100 % -> 85 % -> 70 % -> 55 % -> auto.
                    self.config.render_scale = if self.config.render_scale <= 0.0 {
                        1.0
                    } else if (self.config.render_scale - 1.0).abs() < 1e-3 {
                        0.85
                    } else if (self.config.render_scale - 0.85).abs() < 1e-3 {
                        0.7
                    } else if (self.config.render_scale - 0.7).abs() < 1e-3 {
                        0.55
                    } else {
                        0.0
                    };
                    self.config.save();
                    if self.config.render_scale > 0.0 {
                        if let Some(r) = self.renderer.as_mut() {
                            r.set_render_scale(self.config.render_scale);
                        }
                    }
                }
                KeyCode::KeyT => {
                    let _ = self.cycle_rt_mode();
                }
                KeyCode::KeyU => {
                    let _ = self.cycle_upscaler();
                }
                KeyCode::ArrowLeft => {
                    self.config.sensitivity = (self.config.sensitivity - 0.1).max(0.2);
                    self.config.save();
                }
                KeyCode::ArrowRight => {
                    self.config.sensitivity = (self.config.sensitivity + 0.1).min(3.0);
                    self.config.save();
                }
                _ => {}
            },
        }
    }

    // ---------------- Rendu par frame ----------------

    fn frame(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        self.handle_net_events();
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();

        // Résolution dynamique : adapte l'échelle de rendu au temps de frame réel
        // (cible ~60 fps ; plafond 1.0, plancher 0.45) — pensé pour iGPU / vieilles machines.
        self.ema_frame = self.ema_frame * 0.92 + dt * 0.08;
        if let Some(r) = self.renderer.as_mut() {
            if self.config.render_scale <= 0.0 {
                if self.last_scale_adj.elapsed().as_secs_f32() > 0.75 {
                    let cur = r.render_scale();
                    if self.ema_frame > 0.021 && cur > 0.45 {
                        r.set_render_scale(cur - 0.05);
                        self.last_scale_adj = Instant::now();
                    } else if self.ema_frame < 0.0145 && cur < 1.0 {
                        r.set_render_scale(cur + 0.05);
                        self.last_scale_adj = Instant::now();
                    }
                }
            } else {
                r.set_render_scale(self.config.render_scale);
            }
        }
        let (w, h) = {
            let s = self.renderer.as_ref().unwrap().size();
            (s.0 as f32, s.1 as f32)
        };
        let font = self.renderer.as_ref().unwrap().font.clone();
        // Les boutons cliquables sont reconstruits à chaque frame (UI immédiate).
        self.hot_btns.clear();
        let mut ui_ops: Vec<UiOp> = Vec::new();

        let tag = match &self.mode {
            Mode::MainMenu(_) => 0,
            Mode::Lobby { .. } => 1,
            Mode::Playing => 2,
            Mode::GameOver { .. } => 3,
        };

        let mut world_data: Option<(
            crate::gpu::WorldUniform,
            [f32; 4],
            Vec<(String, Vec<crate::gpu::InstanceData>)>,
            [[f32; 4]; 4],
            Vec<crate::gpu::rtscene::GpuAabb>,
        )> = None;

        match tag {
            0 => {
                let screen = match &self.mode {
                    Mode::MainMenu(s) => s.clone(),
                    _ => MenuScreen::Main,
                };
                self.draw_menu(&mut ui_ops, &font, &screen, (w, h));
            }
            1 => {
                let (code, is_host) = match &self.mode {
                    Mode::Lobby { code, is_host } => (code.clone(), *is_host),
                    _ => (String::new(), false),
                };
                self.draw_lobby(&mut ui_ops, &font, &code, is_host, (w, h));
            }
            2 => {
                let Some(tx) = self.net.as_ref().map(|n| n.tx.clone()) else { return };
                // Jitter subpixel de l'upscaling temporel (une phase par frame rendue).
                let ups_on = self.renderer.as_ref().unwrap().ups_active();
                let (jx, jy, iw, ih) = if ups_on {
                    let r = self.renderer.as_mut().unwrap();
                    let (jx, jy) = r.jitter_xy();
                    let (iw, ih) = r.internal_size();
                    (jx, jy, iw as f32, ih as f32)
                } else {
                    (0.0, 0.0, 1.0, 1.0)
                };
                let jitter_ndc = [jx * 2.0 / iw, jy * 2.0 / ih];
                let (wu, post, dyns, fear, inv_vp, rt_boxes) = {
                    let Some(g) = self.game.as_mut() else { return };
                    let local_sfx = g.update(dt, &self.keys, &tx);
                    if let Some(a) = self.audio.as_ref() {
                        for (name, gain) in local_sfx {
                            a.play(&name, gain);
                        }
                    }
                    let dyns = g.dynamics();
                    let wu = g.world_uniform(w / h, jitter_ndc);
                    let inv_vp = g.inv_view_proj(w / h, jitter_ndc);
                    let rt_boxes = match self.renderer.as_ref() {
                        Some(r) => g.rt_dynamic_boxes(&r.models),
                        None => Vec::new(),
                    };
                    let post = g.post_params();
                    (wu, post, dyns, g.fear, inv_vp, rt_boxes)
                };
                if let Some(a) = self.audio.as_ref() {
                    a.set_loop_volume("whisper", fear * 0.6);
                }
                if let Some(g) = self.game.as_ref() {
                    hud::draw(&mut ui_ops, g, &font, self.lang, (w, h));
                }
                // Ligne perf (haut-droite) : FPS + échelle de rendu + indicateur RT/upscaling.
                let fps = (1.0 / self.ema_frame.max(1e-4)) as u32;
                let renderer = self.renderer.as_ref().unwrap();
                let internal_pct = ((renderer.render_scale()
                    * if renderer.ups_active() {
                        match self.config.upscale_quality {
                            0 => 1.0 / 1.5,
                            1 => 1.0 / 1.7,
                            _ => 0.5,
                        }
                    } else {
                        1.0
                    })
                    * 100.0) as u32;
                let rt_tag = match self.config.rt_mode {
                    1 => format!(" – {} – {} r/px", t(self.lang, RT_TAG_Q), crate::gpu::Renderer::rt_ray_count(1)),
                    2 => format!(" – {} – {} r/px", t(self.lang, RT_TAG_U), crate::gpu::Renderer::rt_ray_count(2)),
                    3 => format!(" – {} – {} r/px", t(self.lang, RT_TAG_OD), crate::gpu::Renderer::rt_ray_count(3)),
                    _ => String::new(),
                };
                let ups_tag = if renderer.ups_active() {
                    format!(
                        " – {} {}",
                        if renderer.upscaler == 2 { t(self.lang, UP_DLSS) } else { t(self.lang, UP_FSR3) },
                        t(self.lang, match self.config.upscale_quality {
                            0 => UP_QUALITY,
                            1 => UP_BALANCED,
                            _ => UP_PERF,
                        })
                    )
                } else {
                    String::new()
                };
                let perf = format!(
                    "{fps} FPS – rendu {internal_pct}%{rt_tag}{ups_tag}"
                );
                let tw = crate::gpu::ui::text_width(&font, &perf, 13.0);
                ui_ops.push(UiOp::text(w - tw - 12.0, 10.0, 13.0, [0.62, 0.68, 0.62, 0.75], &perf));
                if self.paused {
                    draw_center(&mut ui_ops, &font, t(self.lang, PAUSED), 34.0, WHITE_C, (w, h * 0.42));
                    self.button(&mut ui_ops, &font, BtnAction::Resume, w / 2.0 - 130.0, h * 0.42 + 60.0, 260.0, 40.0, t(self.lang, BTN_RESUME), BtnState::Normal);
                    self.button(&mut ui_ops, &font, BtnAction::QuitToMenu, w / 2.0 - 130.0, h * 0.42 + 110.0, 260.0, 40.0, t(self.lang, BTN_QUITMENU), BtnState::Normal);
                }
                world_data = Some((wu, post, dyns, inv_vp, rt_boxes));
            }
            _ => {
                let (win, time, servers, escaped) = match &self.mode {
                    Mode::GameOver { win, time, servers, escaped } => (*win, *time, *servers, *escaped),
                    _ => (false, 0.0, 0, 0),
                };
                self.draw_gameover(&mut ui_ops, &font, win, time, servers, escaped, (w, h));
            }
        }

        // Erreur temporaire.
        if let Some((at, msg)) = &self.err {
            let age = at.elapsed().as_secs_f32();
            if age < 4.0 {
                let alpha = 1.0 - age / 4.0;
                draw_center(&mut ui_ops, &font, msg, 18.0, [1.0, 0.45, 0.4, alpha], (w, h * 0.82));
            } else {
                self.err = None;
            }
        }

        // Rendu final.
        match world_data {
            Some((wu, post, dyns, inv_vp, rt_boxes)) => {
                let statics: &crate::gpu::StaticBatches = match self.game.as_ref() {
                    Some(g) => &g.statics,
                    None => empty_statics(),
                };
                let renderer = self.renderer.as_mut().unwrap();
                let rt = if renderer.rt_mode > 0 {
                    Some(crate::gpu::RtFrame {
                        inv_vp,
                        dyn_boxes: &rt_boxes,
                    })
                } else {
                    None
                };
                let ups = if renderer.ups_active() {
                    Some(crate::gpu::UpsFrame { inv_vp })
                } else {
                    None
                };
                let _ = renderer.render(&wu, rt, statics, &dyns, &ui_ops, post, ups);
            }
            None => {
                let _ = self.renderer.as_mut().unwrap().render(
                    &identity_uniform(),
                    None,
                    empty_statics(),
                    &Vec::new(),
                    &ui_ops,
                    [0.0, 0.0, 0.0, 0.0],
                    None,
                );
            }
        }
    }

    fn draw_menu(&mut self, ui: &mut Vec<UiOp>, font: &crate::gpu::ui::FontData, screen: &MenuScreen, size: (f32, f32)) {
        let (w, h) = size;
        ui.push(UiOp::Rect { x: 0.0, y: 0.0, w, h, color: [0.012, 0.015, 0.024, 1.0] });
        ui.push(UiOp::Rect { x: 0.0, y: h * 0.5, w, h: h * 0.5, color: [0.02, 0.028, 0.04, 0.5] });
        draw_center(ui, &font, t(self.lang, TITLE), 44.0, [0.92, 0.55, 0.35, 1.0], (w, h * 0.16));
        draw_center(ui, &font, t(self.lang, SUBTITLE), 17.0, DIM_C, (w, h * 0.16 + 58.0));
        draw_center(ui, &font, t(self.lang, CONTROLS), 13.0, [0.55, 0.58, 0.6, 0.8], (w, h * 0.94));

        match screen {
            MenuScreen::Main => {
                self.button(ui, &font, BtnAction::Host, w / 2.0 - 190.0, h * 0.40, 380.0, 44.0, t(self.lang, BTN_HOST), BtnState::Normal);
                self.button(ui, &font, BtnAction::Join, w / 2.0 - 190.0, h * 0.40 + 56.0, 380.0, 44.0, t(self.lang, BTN_JOIN), BtnState::Normal);
                self.button(ui, &font, BtnAction::OptionsMenu, w / 2.0 - 190.0, h * 0.40 + 112.0, 380.0, 44.0, t(self.lang, BTN_OPTIONS), BtnState::Normal);
                self.button(ui, &font, BtnAction::Quit, w / 2.0 - 130.0, h * 0.40 + 180.0, 260.0, 36.0, t(self.lang, BTN_QUIT), BtnState::Normal);
            }
            MenuScreen::HostSetup { buf, bots } => {
                // Création de partie : serveur local intégré (auto), mot de passe, bots.
                draw_center(ui, &font, t(self.lang, HOST_SETUP), 26.0, WHITE_C, (w, h * 0.28));
                draw_center(ui, &font, t(self.lang, PASSWORD_PROMPT), 17.0, DIM_C, (w, h * 0.38));
                let shown: String = buf.chars().map(|_| '*').collect();
                draw_center(ui, &font, &format!("{shown}_"), 22.0, AMBER_C, (w, h * 0.38 + 38.0));
                // Rangée bots : - Bots : N + (même géométrie que la rangée sensibilité)
                let by = h * 0.52;
                self.button(ui, &font, BtnAction::BotsDown, w / 2.0 - 180.0, by, 44.0, 36.0, "-", BtnState::Normal);
                self.button(ui, &font, BtnAction::BotsUp, w / 2.0 + 136.0, by, 44.0, 36.0, "+", BtnState::Normal);
                let lbl = format!("{} : {}", t(self.lang, BOTS_LABEL), bots);
                let tw = crate::gpu::ui::text_width(&font, &lbl, 18.0);
                ui.push(UiOp::text(w / 2.0 - tw / 2.0, by + 9.0, 18.0, WHITE_C, &lbl));
                let sub = t(self.lang, BOTS_SUB);
                let sw = crate::gpu::ui::text_width(&font, sub, 12.0);
                ui.push(UiOp::text(w / 2.0 - sw / 2.0, by + 44.0, 12.0, DIM_C, sub));
                draw_center(ui, &font, t(self.lang, LOCAL_NOTE), 14.0, GREEN_C, (w, h * 0.64));
                draw_center(ui, &font, &format!("{} – [Entrée] créer", t(self.lang, BACK_HINT)), 14.0, DIM_C, (w, h * 0.8));
            }
            MenuScreen::AskJoinAddr { buf } => {
                draw_center(ui, &font, t(self.lang, ADDR_PROMPT), 20.0, WHITE_C, (w, h * 0.45));
                draw_center(ui, &font, &format!("{buf}_"), 22.0, AMBER_C, (w, h * 0.45 + 40.0));
                draw_center(ui, &font, t(self.lang, BACK_HINT), 14.0, DIM_C, (w, h * 0.8));
            }
            MenuScreen::AskCode { buf } => {
                draw_center(ui, &font, t(self.lang, CODE_PROMPT), 20.0, WHITE_C, (w, h * 0.45));
                draw_center(ui, &font, &format!("{buf}_"), 26.0, AMBER_C, (w, h * 0.45 + 44.0));
                draw_center(ui, &font, t(self.lang, BACK_HINT), 14.0, DIM_C, (w, h * 0.8));
            }
            MenuScreen::AskPassword { code: room_code, buf } => {
                let shown: String = buf.chars().map(|_| '*').collect();
                draw_center(ui, &font, &format!("{} {room_code}", t(self.lang, LOBBY)), 18.0, DIM_C, (w, h * 0.4));
                draw_center(ui, &font, t(self.lang, PASSWORD_PROMPT), 20.0, WHITE_C, (w, h * 0.45));
                draw_center(ui, &font, &format!("{shown}_"), 22.0, AMBER_C, (w, h * 0.45 + 40.0));
                draw_center(ui, &font, t(self.lang, BACK_HINT), 14.0, DIM_C, (w, h * 0.8));
            }
            MenuScreen::AskName { buf } => {
                draw_center(ui, &font, t(self.lang, NAME_PROMPT), 20.0, WHITE_C, (w, h * 0.45));
                draw_center(ui, &font, &format!("{buf}_"), 22.0, AMBER_C, (w, h * 0.45 + 40.0));
            }
            MenuScreen::Options => {
                // Voile : masque le titre de fond pour la lisibilité du panneau.
                ui.push(UiOp::Rect { x: 0.0, y: 0.0, w, h, color: [0.012, 0.015, 0.024, 1.0] });
                draw_center(ui, &font, t(self.lang, BTN_OPTIONS), 30.0, WHITE_C, (w, h * 0.10));

                // Layout adaptatif : sous 620 px de haut, rangées compactes.
                let compact = h < 620.0;
                let gap = if compact { 40.0 } else { 48.0 };
                let bh = if compact { 28.0 } else { 36.0 };
                let top = h * 0.18;

                // Langue + nom (les libellés contiennent déjà leur valeur).
                let bh_lang = if compact { 28.0 } else { 40.0 };
                self.button(ui, &font, BtnAction::Lang, w / 2.0 - 320.0, top, 310.0, bh_lang, t(self.lang, OPT_LANG), BtnState::Normal);
                self.button(ui, &font, BtnAction::Name, w / 2.0 + 10.0, top, 310.0, bh_lang, &format!("{} : {}", t(self.lang, OPT_NAME).replace("[N] ", ""), self.config.name), BtnState::Normal);

                // Sensibilité.
                let row_y = top + gap;
                let sens_label = format!("{} : {:.1}", t(self.lang, OPT_SENS).replace("[<> ] ", ""), self.config.sensitivity);
                let sens_w = crate::gpu::ui::text_width(&font, &sens_label, 18.0);
                self.button(ui, &font, BtnAction::SensDown, w / 2.0 - 180.0, row_y, 44.0, bh, "-", BtnState::Normal);
                self.button(ui, &font, BtnAction::SensUp, w / 2.0 + 136.0, row_y, 44.0, bh, "+", BtnState::Normal);
                ui.push(UiOp::text(w / 2.0 - sens_w / 2.0, row_y + bh / 2.0 - 9.0, 18.0, WHITE_C, &sens_label));

                // Résolution (DRS manuel).
                let row_y = top + gap * 2.0;
                ui.push(UiOp::text(w / 2.0 - 320.0, row_y + bh / 2.0 - 8.0, 17.0, DIM_C, t(self.lang, OPT_RESOLUTION)));
                let rs_vals: [(u8, &str); 5] = [
                    (0, t(self.lang, RS_AUTO)),
                    (1, "100%"),
                    (2, "85%"),
                    (3, "70%"),
                    (4, "55%"),
                ];
                let cur_res = if self.config.render_scale <= 0.0 {
                    0
                } else if (self.config.render_scale - 1.0).abs() < 1e-3 {
                    1
                } else if (self.config.render_scale - 0.85).abs() < 1e-3 {
                    2
                } else if (self.config.render_scale - 0.7).abs() < 1e-3 {
                    3
                } else {
                    4
                };
                for (i, (v, label)) in rs_vals.iter().enumerate() {
                    let state = if cur_res == *v { BtnState::Selected } else { BtnState::Normal };
                    self.button(ui, &font, BtnAction::Res(*v), w / 2.0 - 100.0 + i as f32 * 84.0, row_y, 78.0, bh, label, state);
                }

                // Upscaling : Natif / FSR 3 / DLSS.
                let row_y = top + gap * 3.0;
                ui.push(UiOp::text(w / 2.0 - 320.0, row_y + bh / 2.0 - 8.0, 17.0, DIM_C, t(self.lang, OPT_UPSCALING)));
                let rtx = self.renderer.as_ref().map(|r| r.adapter_name.to_uppercase().contains("RTX")).unwrap_or(false);
                let ups_items: [(u8, &str, BtnState); 3] = [
                    (0, t(self.lang, UP_NATIVE), if self.config.upscaler == 0 { BtnState::Selected } else { BtnState::Normal }),
                    (1, t(self.lang, UP_FSR3), if self.config.upscaler == 1 { BtnState::Selected } else { BtnState::Normal }),
                    (2, t(self.lang, UP_DLSS), if self.config.upscaler == 2 { BtnState::Selected } else if rtx { BtnState::Normal } else { BtnState::Disabled }),
                ];
                for (i, (v, label, state)) in ups_items.iter().enumerate() {
                    self.button(ui, &font, BtnAction::Ups(*v), w / 2.0 - 100.0 + i as f32 * 110.0, row_y, 104.0, bh, label, *state);
                }
                if !rtx {
                    ui.push(UiOp::text(w / 2.0 + 236.0, row_y + bh / 2.0 - 6.0, 13.0, DIM_C, t(self.lang, DLSS_NEED_RTX)));
                }

                // Preset qualité (visible si upscaling actif).
                if self.config.upscaler > 0 {
                    let row_y = top + gap * 4.0;
                    ui.push(UiOp::text(w / 2.0 - 320.0, row_y + bh / 2.0 - 8.0, 17.0, DIM_C, t(self.lang, UP_Q_LABEL)));
                    let q_items: [(u8, &str); 3] = [
                        (0, t(self.lang, UP_QUALITY)),
                        (1, t(self.lang, UP_BALANCED)),
                        (2, t(self.lang, UP_PERF)),
                    ];
                    for (i, (v, label)) in q_items.iter().enumerate() {
                        let state = if self.config.upscale_quality == *v { BtnState::Selected } else { BtnState::Normal };
                        self.button(ui, &font, BtnAction::UpsQ(*v), w / 2.0 - 100.0 + i as f32 * 110.0, row_y, 104.0, bh, label, state);
                    }
                    if !compact {
                        ui.push(UiOp::text(w / 2.0 - 100.0, row_y + bh + 8.0, 12.0, DIM_C, t(self.lang, UP_NOTE)));
                    }
                }

                // Ray tracing (décalé sous la note d'upscaling quand elle est affichée).
                let row_y = if self.config.upscaler > 0 { top + gap * 5.0 } else { top + gap * 4.0 };
                let row_y = row_y + if self.config.upscaler > 0 && !compact { 20.0 } else { 0.0 };
                ui.push(UiOp::text(w / 2.0 - 320.0, row_y + bh / 2.0 - 8.0, 17.0, DIM_C, &t(self.lang, OPT_RT).replace("[T] ", "")));
                let rt_items: [(u8, &str); 4] = [
                    (0, t(self.lang, RT_OFF)),
                    (1, t(self.lang, UP_QUALITY)),
                    (2, t(self.lang, RT_ULTRA).split(" — ").next().unwrap_or("Ultra")),
                    (3, "Overdrive"),
                ];
                for (i, (v, label)) in rt_items.iter().enumerate() {
                    let state = if self.config.rt_mode == *v { BtnState::Selected } else { BtnState::Normal };
                    self.button(ui, &font, BtnAction::Rt(*v), w / 2.0 - 215.0 + i as f32 * 110.0, row_y, 104.0, bh, label, state);
                }

                let back_y = (row_y + bh + if compact { 14.0 } else { 30.0 }).max(h * 0.86);
                let back_h = if compact { 30.0 } else { 38.0 };
                self.button(ui, &font, BtnAction::Back, w / 2.0 - 110.0, back_y.min(h - back_h - 8.0), 220.0, back_h, t(self.lang, BTN_BACK), BtnState::Normal);
            }
        }
    }

    /// Dessine un bouton (fond, bordure, label) et l'enregistre pour les clics.
    fn button(&mut self, ui: &mut Vec<UiOp>, font: &crate::gpu::ui::FontData, action: BtnAction, x: f32, y: f32, w: f32, h: f32, label: &str, state: BtnState) {
        let hovered = self.cursor_pos.0 >= x
            && self.cursor_pos.0 <= x + w
            && self.cursor_pos.1 >= y
            && self.cursor_pos.1 <= y + h;
        let (border, bg, txt) = match state {
            BtnState::Disabled => ([0.14, 0.16, 0.2, 0.7], [0.05, 0.06, 0.08, 0.85], [0.38, 0.4, 0.44, 0.8]),
            BtnState::Selected => ([0.95, 0.65, 0.2, 1.0], [0.1, 0.12, 0.09, 0.95], [1.0, 0.85, 0.5, 1.0]),
            BtnState::Normal if hovered => ([1.0, 0.78, 0.3, 1.0], [0.09, 0.11, 0.14, 0.95], WHITE_C),
            BtnState::Normal => ([0.28, 0.33, 0.38, 0.9], [0.07, 0.09, 0.12, 0.9], WHITE_C),
        };
        ui.push(UiOp::Rect { x, y, w, h, color: border });
        ui.push(UiOp::Rect { x: x + 2.0, y: y + 2.0, w: w - 4.0, h: h - 4.0, color: bg });
        let tw = crate::gpu::ui::text_width(font, label, 18.0);
        let size = if tw > w - 16.0 { 15.0 } else { 18.0 };
        let tw = crate::gpu::ui::text_width(font, label, size);
        ui.push(UiOp::text(x + w / 2.0 - tw / 2.0, y + h / 2.0 - size * 0.62, size, txt, label));
        if state != BtnState::Disabled {
            self.hot_btns.push(HotBtn { action, rect: [x, y, w, h] });
        }
    }

    fn draw_lobby(&mut self, ui: &mut Vec<UiOp>, font: &crate::gpu::ui::FontData, code: &str, is_host: bool, size: (f32, f32)) {
        let (w, h) = size;
        ui.push(UiOp::Rect { x: 0.0, y: 0.0, w, h, color: [0.012, 0.015, 0.024, 1.0] });
        draw_center(
            ui,
            &font,
            &format!("{} {code}", t(self.lang, LOBBY)),
            36.0,
            [0.5, 0.9, 0.6, 1.0],
            (w, h * 0.2),
        );
        let mut y = h * 0.35;
        let host_id = self.players.first().map(|p| p.id);
        for p in &self.players {
            let tag = if host_id == Some(p.id) {
                format!(" — {}", t(self.lang, HOST_OF))
            } else {
                String::new()
            };
            draw_center(ui, &font, &format!("{}{tag}", p.name), 22.0, WHITE_C, (w, y));
            y += 36.0;
        }
        if is_host {
            draw_center(ui, &font, t(self.lang, LOBBY_HOST), 22.0, AMBER_C, (w, h * 0.66));
            self.button(ui, &font, BtnAction::StartGame, w / 2.0 - 160.0, h * 0.74, 320.0, 44.0, t(self.lang, BTN_START), BtnState::Normal);
        } else {
            draw_center(ui, &font, t(self.lang, LOBBY_WAIT), 22.0, DIM_C, (w, h * 0.72));
        }
        self.button(ui, &font, BtnAction::LeaveLobby, w / 2.0 - 130.0, h * 0.88, 260.0, 36.0, t(self.lang, BTN_LEAVE), BtnState::Normal);
    }

    fn draw_gameover(
        &mut self,
        ui: &mut Vec<UiOp>,
        font: &crate::gpu::ui::FontData,
        win: bool,
        time: f32,
        servers: u8,
        escaped: u8,
        size: (f32, f32),
    ) {
        let (w, h) = size;
        ui.push(UiOp::Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
            color: if win { [0.02, 0.05, 0.02, 1.0] } else { [0.06, 0.01, 0.01, 1.0] },
        });
        let title = if win { t(self.lang, WIN_TITLE) } else { t(self.lang, LOSE_TITLE) };
        let sub = if win { t(self.lang, WIN_SUB) } else { t(self.lang, LOSE_SUB) };
        let color = if win { GREEN_C } else { RED_C };
        draw_center(ui, &font, title, 42.0, color, (w, h * 0.3));
        draw_center(ui, &font, sub, 18.0, DIM_C, (w, h * 0.3 + 60.0));
        let mins = (time as u32) / 60;
        let secs = (time as u32) % 60;
        draw_center(
            ui,
            &font,
            &format!(
                "{} {mins:02}:{secs:02}   –   {} {servers}/6   –   {} {escaped}",
                t(self.lang, TIME),
                t(self.lang, SERVERS),
                t(self.lang, ESCAPED)
            ),
            20.0,
            WHITE_C,
            (w, h * 0.55),
        );
        draw_center(ui, &font, t(self.lang, AGAIN_HINT), 18.0, AMBER_C, (w, h * 0.75));
        self.button(ui, &font, BtnAction::OverMenu, w / 2.0 - 130.0, h * 0.84, 260.0, 40.0, t(self.lang, BTN_TO_MENU), BtnState::Normal);
    }
}

fn draw_center(
    ui: &mut Vec<UiOp>,
    font: &crate::gpu::ui::FontData,
    s: &str,
    size: f32,
    color: [f32; 4],
    (w, y): (f32, f32),
) {
    let tw = crate::gpu::ui::text_width(font, s, size);
    ui.push(UiOp::text(w / 2.0 - tw / 2.0, y, size, color, s));
}

fn identity_uniform() -> crate::gpu::WorldUniform {
    crate::gpu::WorldUniform {
        view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        cam_pos: [0.0; 4],
        light_pos: [[0.0; 4]; crate::gpu::MAX_LIGHTS],
        light_col: [[0.0; 4]; crate::gpu::MAX_LIGHTS],
        flash_pos: [0.0; 4],
        flash_dir: [0.0; 4],
        misc: [0.0; 4],
        flash_col: [0.0; 4],
    }
}

fn empty_statics() -> &'static crate::gpu::StaticBatches {
    static EMPTY: std::sync::OnceLock<crate::gpu::StaticBatches> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| crate::gpu::StaticBatches { batches: Vec::new(), total_instances: 0 })
}

fn rand_int(n: u32) -> u32 {
    use std::cell::Cell;
    thread_local! {
        static S: Cell<u32> = Cell::new(0x9e3779b9);
    }
    S.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        s.set(x);
        x % n.max(1)
    })
}

impl App {
    fn pump_autopilot(&mut self, el: &ActiveEventLoop) {
        let mut due: Vec<AutoAct> = Vec::new();
        if let Some(auto) = self.auto.as_mut() {
            let t = auto.t0.elapsed().as_secs_f64();
            while auto.next < auto.steps.len() && auto.steps[auto.next].at <= t {
                due.push(auto.steps[auto.next].act.clone());
                auto.next += 1;
            }
        } else {
            return;
        }
        for act in due {
            match act {
                AutoAct::Key(c) => {
                    self.keys.insert(c);
                    self.on_key_pressed(c, el);
                    self.keys.remove(&c);
                    self.on_key_released(c);
                }
                AutoAct::Shot(p) => {
                    if let Some(r) = self.renderer.as_mut() {
                        r.capture_path = Some(std::path::PathBuf::from(&p));
                        println!("[autopilot] capture -> {p}");
                    }
                    if std::env::var("SL3_DEBUG").is_ok() {
                        if let Some(g) = &self.game {
                            eprintln!("[sl3-debug] pos=({:.1},{:.1}) yaw={:.2}", g.pos.x, g.pos.z, g.yaw);
                        }
                    }
                }
                AutoAct::Yaw(a) => {
                    if let Some(g) = self.game.as_mut() {
                        g.yaw += a;
                    }
                }
                AutoAct::Mouse(x, y) => {
                    self.cursor_pos = (x as f32, y as f32);
                }
                AutoAct::Click(x, y) => {
                    self.cursor_pos = (x as f32, y as f32);
                    self.on_click();
                }
                AutoAct::Look(dx, dy) => {
                    // Même chemin que DeviceEvent::MouseMotion (raw input).
                    if self.cursor_locked {
                        self.apply_look(dx, dy);
                    }
                }
                AutoAct::Type(s) => {
                    for ch in s.chars() {
                        self.handle_text_char(ch);
                    }
                }
                AutoAct::Down(k) => {
                    self.keys.insert(k);
                    if std::env::var("SL3_DEBUG").is_ok() {
                        eprintln!("[sl3-debug] down {k:?} ({} touches)", self.keys.len());
                    }
                }
                AutoAct::Up(k) => {
                    self.keys.remove(&k);
                    if std::env::var("SL3_DEBUG").is_ok() {
                        eprintln!("[sl3-debug] up {k:?}");
                    }
                }
                AutoAct::Tp(x, z, _yaw) => {
                    if let Some(g) = self.game.as_mut() {
                        g.pos = Vec3::new(x, 0.0, z);
                        if std::env::var("SL3_DEBUG").is_ok() {
                            eprintln!("[sl3-debug] tp ({x},{z})");
                        }
                    }
                }
                AutoAct::Exit => {
                    self.config.save();
                    println!("[autopilot] fin");
                    el.exit();
                }
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (ww, wh) = std::env::var("SL3_WINDOW_SIZE")
            .ok()
            .and_then(|s| {
                let (a, b) = s.split_once('x')?;
                Some((a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?))
            })
            .unwrap_or((1280.0, 720.0));
        let attrs = Window::default_attributes()
            .with_title("SUPPORT LEVEL -3")
            .with_inner_size(winit::dpi::LogicalSize::new(ww, wh));
        let window = el.create_window(attrs).expect("fenêtre");
        let window = Arc::new(window);
        let mut renderer = Renderer::new(window.clone());
        if std::env::var("SL3_DEBUG").is_ok() {
            eprintln!("[sl3-debug] renderer créé : {}", renderer.adapter_name);
        }
        // 1er lancement sur un GPU ray tracing (RTX) : active le mode Qualité.
        if self.fresh_config && renderer.adapter_name.to_lowercase().contains("rtx") {
            self.config.rt_mode = 1;
            self.config.save();
        }
        renderer.set_rt_mode(self.config.rt_mode);
        renderer.set_upscaler(self.config.upscaler, self.config.upscale_quality);
        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: DeviceEvent) {
        // RAW INPUT (demande explicite du joueur) : les deltas souris viennent de
        // la souris elle-même (WM_INPUT sous Windows), pas de la position du
        // curseur : ça marche quel que soit le pointer lock, la DPI/échelle ou
        // l'accélération du pointeur. Uniquement en partie (menus = curseur libre).
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.cursor_locked {
                self.apply_look(delta.0, delta.1);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.config.save();
                el.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Menus uniquement : suivi du curseur (survol + clic des boutons).
                // En jeu, le regard vient du RAW INPUT (device_event) : la position
                // du curseur est ignorée et on ne la recentre plus de force (ça
                // luttait avec le grab et tuait la souris sous Windows).
                if !self.cursor_locked {
                    self.cursor_pos = (position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !self.cursor_locked
                    && button == winit::event::MouseButton::Left
                    && state == ElementState::Pressed
                {
                    self.on_click();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let code = match event.physical_key {
                    PhysicalKey::Code(c) => c,
                    _ => return,
                };
                match event.state {
                    ElementState::Pressed => {
                        // Saisie de texte réelle (champs mot de passe / code / nom) :
                        // event.text respecte la disposition clavier (AZERTY inclus).
                        if let Some(txt) = &event.text {
                            for ch in txt.chars() {
                                if !ch.is_control() {
                                    self.handle_text_char(ch);
                                }
                            }
                        }
                        if self.keys.insert(code) {
                            self.on_key_pressed(code, el);
                        }
                    }
                    ElementState::Released => {
                        self.keys.remove(&code);
                        self.on_key_released(code);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if std::env::var("SL3_DEBUG").is_ok() {
                    static FIRST: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
                    let t0 = FIRST.get_or_init(Instant::now);
                    let n = FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if n < 3 || n % 25 == 0 {
                        eprintln!("[sl3-debug] frame {n} (t={:?})", t0.elapsed());
                    }
                }
                self.frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.handle_net_events();
        self.pump_autopilot(el);
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }
}

pub fn run() {
    let event_loop = EventLoop::builder().build().expect("event loop");
    let mut app = App::new();
    let _ = event_loop.run_app(&mut app);
}

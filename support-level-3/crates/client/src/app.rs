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
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

#[derive(Clone)]
enum MenuScreen {
    Main,
    AskHostAddr { buf: String },
    HostPassword { buf: String },
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

const WHITE_C: [f32; 4] = [0.9, 0.92, 0.95, 1.0];
const DIM_C: [f32; 4] = [0.6, 0.63, 0.66, 0.85];
const GREEN_C: [f32; 4] = [0.5, 0.95, 0.55, 1.0];
const RED_C: [f32; 4] = [1.0, 0.4, 0.35, 1.0];
const AMBER_C: [f32; 4] = [1.0, 0.78, 0.3, 1.0];

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
}

impl App {
    fn new() -> App {
        let fresh_config = !std::path::Path::new("sl3_config.json").exists();
        let config = Config::load();
        let lang = Lang::from_str(&config.lang);
        let audio = Audio::new(config.volume);
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
        }
    }

    /// Cycle le mode ray tracing (off -> qualité -> ultra), applique et persiste.
    fn cycle_rt_mode(&mut self) -> &'static str {
        self.config.rt_mode = (self.config.rt_mode + 1) % 3;
        self.config.save();
        if let Some(r) = self.renderer.as_mut() {
            r.set_rt_mode(self.config.rt_mode);
        }
        match self.config.rt_mode {
            1 => t(self.lang, RT_QUAL),
            2 => t(self.lang, RT_ULTRA),
            _ => t(self.lang, RT_OFF),
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
                    self.mode = Mode::MainMenu(MenuScreen::AskHostAddr {
                        buf: self.config.host_default.clone(),
                    });
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
            MenuScreen::AskHostAddr { buf } => {
                if back {
                    self.back_to_menu();
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    self.mode = Mode::MainMenu(MenuScreen::AskHostAddr { buf: b });
                } else if enter {
                    self.config.host_default = buf.clone();
                    self.config.save();
                    self.mode = Mode::MainMenu(MenuScreen::HostPassword { buf: String::new() });
                }
            }
            MenuScreen::HostPassword { buf } => {
                if back {
                    self.back_to_menu();
                } else if code == KeyCode::Backspace {
                    let mut b = buf.clone();
                    b.pop();
                    self.mode = Mode::MainMenu(MenuScreen::HostPassword { buf: b });
                } else if enter {
                    let password = buf.clone();
                    let addr = self.config.host_default.clone();
                    self.connect(addr, Pending::Create { password });
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
                let (wu, post, dyns, fear, inv_vp, rt_boxes) = {
                    let Some(g) = self.game.as_mut() else { return };
                    let local_sfx = g.update(dt, &self.keys, &tx);
                    if let Some(a) = self.audio.as_ref() {
                        for (name, gain) in local_sfx {
                            a.play(&name, gain);
                        }
                    }
                    let dyns = g.dynamics();
                    let wu = g.world_uniform(w / h);
                    let inv_vp = g.inv_view_proj(w / h);
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
                // Ligne perf (haut-droite) : FPS + échelle de rendu + indicateur RT.
                let fps = (1.0 / self.ema_frame.max(1e-4)) as u32;
                let pct = (self.renderer.as_ref().unwrap().render_scale() * 100.0).round() as u32;
                let rt_tag = if self.config.rt_mode > 0 { " · RT" } else { "" };
                let perf = format!("{fps} FPS · rendu {pct}%{rt_tag}");
                let tw = crate::gpu::ui::text_width(&font, &perf, 13.0);
                ui_ops.push(UiOp::text(w - tw - 12.0, 10.0, 13.0, [0.62, 0.68, 0.62, 0.75], &perf));
                if self.paused {
                    draw_center(&mut ui_ops, &font, t(self.lang, PAUSED), 34.0, WHITE_C, (w, h * 0.42));
                    draw_center(&mut ui_ops, &font, t(self.lang, RESUME_HINT), 18.0, DIM_C, (w, h * 0.42 + 50.0));
                    draw_center(&mut ui_ops, &font, t(self.lang, QUIT_HINT), 18.0, DIM_C, (w, h * 0.42 + 78.0));
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
                let _ = renderer.render(&wu, rt, statics, &dyns, &ui_ops, post);
            }
            None => {
                let _ = self.renderer.as_mut().unwrap().render(
                    &identity_uniform(),
                    None,
                    empty_statics(),
                    &Vec::new(),
                    &ui_ops,
                    [0.0, 0.0, 0.0, 0.0],
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
                draw_center(ui, &font, t(self.lang, MENU_HOST), 24.0, WHITE_C, (w, h * 0.42));
                draw_center(ui, &font, t(self.lang, MENU_JOIN), 24.0, WHITE_C, (w, h * 0.42 + 44.0));
                draw_center(ui, &font, t(self.lang, MENU_OPTIONS), 24.0, WHITE_C, (w, h * 0.42 + 88.0));
                draw_center(ui, &font, t(self.lang, MENU_QUIT), 17.0, DIM_C, (w, h * 0.42 + 150.0));
            }
            MenuScreen::AskHostAddr { buf } => {
                draw_center(ui, &font, t(self.lang, ADDR_PROMPT), 20.0, WHITE_C, (w, h * 0.45));
                draw_center(ui, &font, &format!("{buf}_"), 22.0, AMBER_C, (w, h * 0.45 + 40.0));
                draw_center(ui, &font, &format!("{} · [Entrée] valider", t(self.lang, BACK_HINT)), 14.0, DIM_C, (w, h * 0.8));
            }
            MenuScreen::HostPassword { buf } => {
                let shown: String = buf.chars().map(|_| '*').collect();
                draw_center(ui, &font, t(self.lang, PASSWORD_PROMPT), 20.0, WHITE_C, (w, h * 0.45));
                draw_center(ui, &font, &format!("{shown}_"), 22.0, AMBER_C, (w, h * 0.45 + 40.0));
                draw_center(ui, &font, t(self.lang, BACK_HINT), 14.0, DIM_C, (w, h * 0.8));
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
                draw_center(ui, &font, t(self.lang, OPT_LANG), 20.0, WHITE_C, (w, h * 0.4));
                draw_center(
                    ui,
                    &font,
                    &format!("{} : {:.1}", t(self.lang, OPT_SENS), self.config.sensitivity),
                    20.0,
                    WHITE_C,
                    (w, h * 0.4 + 40.0),
                );
                draw_center(
                    ui,
                    &font,
                    &format!("{} : {}", t(self.lang, OPT_NAME), self.config.name),
                    20.0,
                    WHITE_C,
                    (w, h * 0.4 + 80.0),
                );
                let rs_label = if self.config.render_scale <= 0.0 {
                    let cur = self
                        .renderer
                        .as_ref()
                        .map(|r| r.render_scale())
                        .unwrap_or(1.0);
                    format!("auto ({:.0}%)", cur * 100.0)
                } else {
                    format!("{:.0}%", self.config.render_scale * 100.0)
                };
                draw_center(
                    ui,
                    &font,
                    &format!("{} : {}", t(self.lang, OPT_RENDER), rs_label),
                    20.0,
                    WHITE_C,
                    (w, h * 0.4 + 120.0),
                );
                let rt_label = match self.config.rt_mode {
                    1 => t(self.lang, RT_QUAL),
                    2 => t(self.lang, RT_ULTRA),
                    _ => t(self.lang, RT_OFF),
                };
                draw_center(
                    ui,
                    &font,
                    &format!("{} : {}", t(self.lang, OPT_RT), rt_label),
                    16.0,
                    if self.config.rt_mode > 0 { AMBER_C } else { WHITE_C },
                    (w, h * 0.4 + 160.0),
                );
                draw_center(ui, &font, t(self.lang, BACK_HINT), 16.0, DIM_C, (w, h * 0.75));
            }
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
            draw_center(ui, &font, t(self.lang, LOBBY_HOST), 22.0, AMBER_C, (w, h * 0.72));
        } else {
            draw_center(ui, &font, t(self.lang, LOBBY_WAIT), 22.0, DIM_C, (w, h * 0.72));
        }
        draw_center(ui, &font, t(self.lang, LEAVE_HINT), 15.0, DIM_C, (w, h * 0.85));
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
                "{} {mins:02}:{secs:02}   ·   {} {servers}/6   ·   {} {escaped}",
                t(self.lang, TIME),
                t(self.lang, SERVERS),
                t(self.lang, ESCAPED)
            ),
            20.0,
            WHITE_C,
            (w, h * 0.55),
        );
        draw_center(ui, &font, t(self.lang, AGAIN_HINT), 18.0, AMBER_C, (w, h * 0.75));
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

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("SUPPORT LEVEL -3")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = el.create_window(attrs).expect("fenêtre");
        let window = Arc::new(window);
        let mut renderer = Renderer::new(window.clone());
        // 1er lancement sur un GPU ray tracing (RTX) : active le mode Qualité.
        if self.fresh_config && renderer.adapter_name.to_lowercase().contains("rtx") {
            self.config.rt_mode = 1;
            self.config.save();
        }
        renderer.set_rt_mode(self.config.rt_mode);
        self.window = Some(window);
        self.renderer = Some(renderer);
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
                if self.cursor_locked {
                    if let (Some(win), Some(r)) = (&self.window, &self.renderer) {
                        let (w, h) = r.size();
                        let cx = w as f64 / 2.0;
                        let cy = h as f64 / 2.0;
                        let dx = position.x - cx;
                        let dy = position.y - cy;
                        if dx.abs() < 600.0 && dy.abs() < 600.0 {
                            if let Some(g) = self.game.as_mut() {
                                let sens = self.config.sensitivity;
                                g.yaw += (dx as f32) * 0.0022 * sens;
                                g.pitch -= (dy as f32) * 0.0022 * sens;
                                g.pitch = g.pitch.clamp(-1.45, 1.45);
                            }
                        }
                        if let Ok(op) = win.outer_position() {
                            let _ = win.set_cursor_position(PhysicalPosition::new(
                                op.x + cx as i32,
                                op.y + cy as i32,
                            ));
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let code = match event.physical_key {
                    PhysicalKey::Code(c) => c,
                    _ => return,
                };
                match event.state {
                    ElementState::Pressed => {
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
                self.frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        self.handle_net_events();
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

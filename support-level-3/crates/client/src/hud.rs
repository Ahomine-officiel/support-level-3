//! HUD en jeu : crosshair, objectifs, tickets, interactions, messages,
//! marqueur de guidage vers l'objectif courant.

use crate::gpu::ui::{text_width, FontData, UiOp};
use crate::game::Game;
use crate::lang::{t, Lang};
use sl3_shared::consts::*;
use sl3_shared::protocol::{Interactable, Snapshot};

/// Marqueur d'objectif du guidage : position projetée à l'écran (calculée
/// côté app avec la view_proj), distance en mètres et type d'objectif
/// (0 serveur · 1 justificatif · 2 terminal · 3 sortie · 4 batterie).
pub struct Marker {
    pub sx: f32,
    pub sy: f32,
    pub on_screen: bool,
    pub dist: f32,
    pub kind: u8,
}

#[allow(dead_code)]
const WHITE: [f32; 4] = [0.9, 0.92, 0.95, 1.0];
const DIM: [f32; 4] = [0.75, 0.78, 0.8, 0.75];
const GREEN: [f32; 4] = [0.5, 0.95, 0.55, 1.0];
const RED: [f32; 4] = [1.0, 0.4, 0.35, 1.0];
const AMBER: [f32; 4] = [1.0, 0.78, 0.3, 1.0];

pub fn draw(ui: &mut Vec<UiOp>, g: &Game, font: &FontData, lang: Lang, size: (f32, f32), marker: Option<Marker>) {
    let snap = match &g.snap {
        Some(s) => s,
        None => return,
    };
    let my_state = g.my_state();
    let (w, h) = size;
    draw_hud(ui, g, snap, my_state, font, lang, (w, h), marker);
}

fn draw_hud(
    ui: &mut Vec<UiOp>,
    g: &Game,
    snap: &Snapshot,
    my_state: u8,
    font: &FontData,
    lang: Lang,
    size: (f32, f32),
    marker: Option<Marker>,
) {
    let (w, h) = size;
    let font_size = 17.0;

    // ---- Crosshair ----
    if my_state == PLAYER_ALIVE {
        let cx = w / 2.0;
        let cy = h / 2.0;
        ui.push(UiOp::Rect { x: cx - 5.0, y: cy - 1.0, w: 10.0, h: 2.0, color: [0.85, 0.85, 0.85, 0.7] });
        ui.push(UiOp::Rect { x: cx - 1.0, y: cy - 5.0, w: 2.0, h: 10.0, color: [0.85, 0.85, 0.85, 0.7] });
    }

    // ---- Objectifs (haut gauche) ----
    let online = snap.servers.iter().filter(|s| s.state == SRV_ONLINE).count();
    let receipts = snap.receipts.iter().filter(|r| **r).count();
    let x = 16.0;
    let mut y = 14.0;
    ui.push(UiOp::text(x, y, 15.0, DIM, &format!(
        "{}: {}/6",
        t(lang, crate::lang::SERVERS),
        online
    )));
    y += 21.0;
    ui.push(UiOp::text(x, y, 15.0, DIM, &format!(
        "{}: {}/4",
        t(lang, crate::lang::RECEIPTS),
        receipts
    )));
    y += 21.0;
    let fee_label = if snap.fee_approved {
        t(lang, crate::lang::FEE_OK).to_string()
    } else {
        format!("{}: {}/4", t(lang, crate::lang::FEE), receipts)
    };
    ui.push(UiOp::text(x, y, 15.0, if snap.fee_approved { GREEN } else { DIM }, &fee_label));
    y += 25.0;

    if snap.exit_open {
        ui.push(UiOp::text(x, y, 17.0, GREEN, t(lang, crate::lang::EXIT_OPEN)));
    }

    // ---- Batterie / endurance (bas centre-gauche) ----
    let bx = w / 2.0 - 150.0;
    let by = h - 46.0;
    ui.push(UiOp::text(bx - 110.0, by - 2.0, 14.0, DIM, t(lang, crate::lang::BATTERY)));
    let frac = g.battery / 100.0;
    ui.push(UiOp::Rect { x: bx, y: by, w: 140.0, h: 12.0, color: [0.1, 0.1, 0.12, 0.8] });
    let col = if frac > 0.3 { GREEN } else { RED };
    ui.push(UiOp::Rect { x: bx + 1.0, y: by + 1.0, w: 138.0 * frac, h: 10.0, color: col });

    let sy = h - 24.0;
    ui.push(UiOp::text(bx - 110.0, sy - 2.0, 14.0, DIM, t(lang, crate::lang::STAMINA)));
    let sfrac = g.stamina / STAMINA_MAX;
    ui.push(UiOp::Rect { x: bx, y: sy, w: 140.0, h: 8.0, color: [0.1, 0.1, 0.12, 0.8] });
    ui.push(UiOp::Rect { x: bx + 1.0, y: sy + 1.0, w: 138.0 * sfrac, h: 6.0, color: [0.55, 0.75, 1.0, 0.9] });

    // ---- Interaction (centre-bas) ----
    if my_state == PLAYER_ALIVE {
        if let Some((target, _)) = g.find_target() {
            let (label, progress, can_hold) = interact_label(g, snap, target, lang);
            let mut line = label.clone();
            if can_hold && progress > 0.0 {
                line = format!("{label}  [{:.0}%]", progress * 100.0);
            }
            let tw = text_width(font, &line, font_size);
            let px = w / 2.0 - tw / 2.0;
            let py = h * 0.62;
            ui.push(UiOp::Rect { x: px - 10.0, y: py - 6.0, w: tw + 20.0, h: font_size + 14.0, color: [0.02, 0.03, 0.05, 0.75] });
            ui.push(UiOp::text(px, py, font_size, AMBER, &line));

            // Barre de progression des interactions maintenues.
            if can_hold && progress > 0.0 {
                ui.push(UiOp::Rect { x: w / 2.0 - 70.0, y: py + font_size + 8.0, w: 140.0, h: 8.0, color: [0.1, 0.1, 0.12, 0.8] });
                ui.push(UiOp::Rect { x: w / 2.0 - 69.0, y: py + font_size + 9.0, w: 138.0 * progress, h: 6.0, color: GREEN });
            }
        }
    }

    // ---- Journal d'événements (bas gauche) ----
    let mut fy = h - 90.0;
    for (at, msg) in g.feed.iter().rev().take(6) {
        let age = at.elapsed().as_secs_f32();
        let alpha = (1.0 - age / 8.0).clamp(0.0, 1.0) * 0.95;
        if alpha <= 0.01 {
            continue;
        }
        ui.push(UiOp::text(16.0, fy, 14.0, [0.85, 0.88, 0.9, alpha], msg));
        fy -= 19.0;
    }

    // ---- Ticket affiché en haut au centre (rappel) ----
    if my_state == PLAYER_ALIVE {
        let i = (g.time as usize / 7) % SERVER_COUNT.max(1);
        let ticket = crate::lang::ticket_text(lang, i);
        let tw2 = text_width(font, &ticket, 15.0);
        if tw2 < w - 40.0 {
            ui.push(UiOp::text(w / 2.0 - tw2 / 2.0, 12.0, 15.0, [0.7, 0.72, 0.75, 0.55], &ticket));
        }
    }

    // ---- Marqueur de guidage vers l'objectif courant ----
    // Sur l'écran : crochet [ ] autour du point + distance. Hors champ :
    // flèche pixel + étiquette collée au bord le plus proche.
    if my_state == PLAYER_ALIVE {
        if let Some(m) = marker {
            let (label_key, col) = match m.kind {
                0 => (crate::lang::GUIDE_SERVER, AMBER),
                1 => (crate::lang::GUIDE_RECEIPT, AMBER),
                2 => (crate::lang::GUIDE_TERMINAL, GREEN),
                3 => (crate::lang::GUIDE_EXIT, GREEN),
                _ => (crate::lang::GUIDE_BATTERY, [0.55, 0.85, 1.0, 1.0]),
            };
            let label = t(lang, label_key);
            let dm = format!("{} m", m.dist as u32);
            if m.on_screen {
                let s = 13.0; // demi-taille du crochet
                let (x0, y0, x1, y1) = (m.sx - s, m.sy - s, m.sx + s, m.sy + s);
                for (rx, ry, rw, rh) in [
                    (x0, y0, 10.0, 2.0),
                    (x1 - 10.0, y0, 10.0, 2.0),
                    (x0, y1 - 2.0, 10.0, 2.0),
                    (x1 - 10.0, y1 - 2.0, 10.0, 2.0),
                    (x0, y0, 2.0, 10.0),
                    (x0, y1 - 10.0, 2.0, 10.0),
                    (x1 - 2.0, y0, 2.0, 10.0),
                    (x1 - 2.0, y1 - 10.0, 2.0, 10.0),
                ] {
                    ui.push(UiOp::Rect { x: rx, y: ry, w: rw, h: rh, color: col });
                }
                let dtw = text_width(font, &dm, 13.0);
                ui.push(UiOp::text(m.sx - dtw / 2.0, y1 + 5.0, 13.0, col, &dm));
            } else {
                // Flèche pixel-art pointant la direction dominante.
                let dx = m.sx - w / 2.0;
                let dy = m.sy - h / 2.0;
                if dx.abs() >= dy.abs() {
                    if dx >= 0.0 {
                        ui.push(UiOp::Rect { x: m.sx + 8.0, y: m.sy - 7.0, w: 2.0, h: 14.0, color: col });
                        ui.push(UiOp::Rect { x: m.sx + 4.0, y: m.sy - 4.0, w: 4.0, h: 8.0, color: col });
                        ui.push(UiOp::Rect { x: m.sx, y: m.sy - 1.5, w: 4.0, h: 3.0, color: col });
                    } else {
                        ui.push(UiOp::Rect { x: m.sx - 10.0, y: m.sy - 7.0, w: 2.0, h: 14.0, color: col });
                        ui.push(UiOp::Rect { x: m.sx - 6.0, y: m.sy - 4.0, w: 4.0, h: 8.0, color: col });
                        ui.push(UiOp::Rect { x: m.sx - 4.0, y: m.sy - 1.5, w: 4.0, h: 3.0, color: col });
                    }
                } else if dy >= 0.0 {
                    ui.push(UiOp::Rect { x: m.sx - 7.0, y: m.sy + 8.0, w: 14.0, h: 2.0, color: col });
                    ui.push(UiOp::Rect { x: m.sx - 4.0, y: m.sy + 4.0, w: 8.0, h: 4.0, color: col });
                    ui.push(UiOp::Rect { x: m.sx - 1.5, y: m.sy, w: 3.0, h: 4.0, color: col });
                } else {
                    ui.push(UiOp::Rect { x: m.sx - 7.0, y: m.sy - 10.0, w: 14.0, h: 2.0, color: col });
                    ui.push(UiOp::Rect { x: m.sx - 4.0, y: m.sy - 6.0, w: 8.0, h: 4.0, color: col });
                    ui.push(UiOp::Rect { x: m.sx - 1.5, y: m.sy - 4.0, w: 3.0, h: 4.0, color: col });
                }
                let line = format!("{label} - {dm}");
                let ltw = text_width(font, &line, 13.0);
                let lx = (m.sx - ltw / 2.0).clamp(8.0, w - ltw - 8.0);
                let ly = (m.sy + 14.0).min(h - 22.0);
                ui.push(UiOp::Rect { x: lx - 6.0, y: ly - 5.0, w: ltw + 12.0, h: 18.0, color: [0.02, 0.03, 0.05, 0.6] });
                ui.push(UiOp::text(lx, ly, 13.0, col, &line));
            }
        }
    }

    // ---- États spéciaux ----
    if my_state == PLAYER_DOWNED {
        let msg = t(lang, crate::lang::DOWNED);
        let tw2 = text_width(font, &msg, 22.0);
        ui.push(UiOp::text(w / 2.0 - tw2 / 2.0, h * 0.42, 22.0, RED, msg));
    } else if my_state == PLAYER_OUT {
        let msg = t(lang, crate::lang::SPECTATOR);
        let tw2 = text_width(font, &msg, 18.0);
        ui.push(UiOp::text(w / 2.0 - tw2 / 2.0, h * 0.42, 18.0, DIM, msg));
    } else if my_state == PLAYER_ESCAPED {
        let msg = t(lang, crate::lang::WIN_TITLE);
        let tw2 = text_width(font, &msg, 20.0);
        ui.push(UiOp::text(w / 2.0 - tw2 / 2.0, h * 0.42, 20.0, GREEN, msg));
    }
}

/// (libellé, progression 0..1, interaction maintenue ?)
fn interact_label(_g: &Game, snap: &Snapshot, target: Interactable, lang: Lang) -> (String, f32, bool) {
    match target {
        Interactable::Server(i) => {
            let s = &snap.servers[i as usize];
            let prog = if s.state == SRV_FIXING { s.progress } else { 0.0 };
            (t(lang, crate::lang::INTERACT_REBOOT).to_string(), prog, true)
        }
        Interactable::Receipt(_) => (t(lang, crate::lang::INTERACT_TAKE).to_string(), 0.0, false),
        Interactable::Terminal => {
            let all = snap.receipts.iter().all(|r| *r);
            if all {
                (t(lang, crate::lang::INTERACT_TERMINAL).to_string(), 0.0, true)
            } else {
                (t(lang, crate::lang::INTERACT_NEED_RECEIPTS).to_string(), 0.0, false)
            }
        }
        Interactable::Breaker => (t(lang, crate::lang::INTERACT_BREAKER).to_string(), 0.0, true),
        Interactable::Battery(_) => (t(lang, crate::lang::INTERACT_BATTERY).to_string(), 0.0, false),
        Interactable::Revive(_) => (t(lang, crate::lang::INTERACT_REVIVE).to_string(), 0.0, true),
        Interactable::Door(_) => (t(lang, crate::lang::INTERACT_DOOR).to_string(), 0.0, false),
    }
}

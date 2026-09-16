/// Constantes de gameplay et de réseau partagées serveur/client.

pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 27070;
pub const MAX_NAME_LEN: usize = 16;
pub const MAX_PLAYERS: usize = 4;
pub const MAX_ROOMS: usize = 16;
pub const CODE_LEN: usize = 4;

/// Simulation serveur : 20 ticks / s.
pub const TICK_HZ: f32 = 20.0;
pub const TICK_DT: f32 = 1.0 / TICK_HZ;

/// Joueur.
pub const PLAYER_RADIUS: f32 = 0.34;
pub const WALK_SPEED: f32 = 3.0;
pub const SPRINT_SPEED: f32 = 4.7;
pub const STAMINA_MAX: f32 = 6.0; // secondes de sprint
pub const STAMINA_REGEN: f32 = 0.7; // facteur de régén / s

/// Lampe torche (100 = batterie pleine).
pub const FLASHLIGHT_DRAIN: f32 = 100.0 / 95.0; // % par seconde (≈95 s d'autonomie)
pub const BATTERY_CHARGE: f32 = 60.0;

/// Interactions.
pub const INTERACT_DIST: f32 = 2.3;
pub const REBOOT_TIME: f32 = 6.0;
pub const TERMINAL_TIME: f32 = 3.5;
pub const REVIVE_TIME: f32 = 3.0;
pub const BREAKER_TIME: f32 = 2.0;

/// Objectifs.
pub const SERVER_COUNT: usize = 6;
pub const RECEIPT_COUNT: usize = 4;
pub const FEE_AMOUNT: f32 = 42.73;

/// L'Auditeur.
pub const ENTITY_PATROL_SPEED: f32 = 1.75;
pub const ENTITY_INVESTIGATE_SPEED: f32 = 2.6;
pub const ENTITY_CHASE_SPEED: f32 = 3.62;
pub const ENTITY_SIGHT_DIST: f32 = 9.5;
pub const ENTITY_SIGHT_COS: f32 = 0.25; // ~75° demi-angle
pub const ENTITY_HEAR_WALK: f32 = 4.5;
pub const ENTITY_HEAR_SPRINT: f32 = 10.5;
pub const ENTITY_HEAR_WORK: f32 = 12.0;
pub const ENTITY_CATCH_DIST: f32 = 0.9;
pub const ENTITY_CHASE_LOSE_TIME: f32 = 4.0;
pub const ENTITY_SEARCH_TIME: f32 = 9.0;
pub const ENTITY_COOLDOWN: f32 = 12.0; // après une capture

/// États d'un joueur côté partie.
pub const PLAYER_ALIVE: u8 = 0;
pub const PLAYER_DOWNED: u8 = 1;
pub const PLAYER_OUT: u8 = 2; // éliminé
pub const PLAYER_ESCAPED: u8 = 3;

/// États de l'entité.
pub const ENTITY_PATROL: u8 = 0;
pub const ENTITY_INVESTIGATE: u8 = 1;
pub const ENTITY_CHASE: u8 = 2;
pub const ENTITY_SEARCH: u8 = 3;
pub const ENTITY_COOLDOWN_STATE: u8 = 4;

/// États d'un serveur.
pub const SRV_BROKEN: u8 = 0;
pub const SRV_FIXING: u8 = 1;
pub const SRV_ONLINE: u8 = 2;

/// Blackout (événement). Les bornes sont surchargeables pour les tests/QA via
/// SL3_BLACKOUT_MIN / SL3_BLACKOUT_MAX (secondes) — pratique pour valider le
/// rendu RT sans coupure pendant les captures automatisées.
pub const BLACKOUT_INTERVAL_MIN: f32 = 150.0;
pub const BLACKOUT_INTERVAL_MAX: f32 = 240.0;

pub fn blackout_bounds() -> (f32, f32) {
    let min = std::env::var("SL3_BLACKOUT_MIN")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(BLACKOUT_INTERVAL_MIN);
    let max = std::env::var("SL3_BLACKOUT_MAX")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(BLACKOUT_INTERVAL_MAX);
    (min, max.max(min))
}

/// SL3_AUDITOR=0 — désactive l'IA de l'Auditeur (tests/QA : captures de rendu
/// sans que l'entité ne déplace le regard du joueur pendant les scénarios).
pub fn auditor_enabled() -> bool {
    std::env::var("SL3_AUDITOR").ok().map(|v| v != "0").unwrap_or(true)
}
pub const BLACKOUT_DURATION: f32 = 22.0;

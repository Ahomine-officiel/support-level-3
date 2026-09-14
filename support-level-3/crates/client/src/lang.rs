//! Interface bilingue FR/EN (bascule F1 ou options).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    pub fn from_str(s: &str) -> Lang {
        if s.eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::Fr
        }
    }

    pub fn toggle(&mut self) {
        *self = match self {
            Lang::Fr => Lang::En,
            Lang::En => Lang::Fr,
        };
    }
}

/// Clés de texte de l'interface.
pub const TITLE: usize = 0;
pub const SUBTITLE: usize = 1;
pub const MENU_HOST: usize = 2;
pub const MENU_JOIN: usize = 3;
pub const MENU_OPTIONS: usize = 4;
pub const MENU_QUIT: usize = 5;
pub const ADDR_PROMPT: usize = 6;
pub const PASSWORD_PROMPT: usize = 8;
pub const CODE_PROMPT: usize = 9;
pub const NAME_PROMPT: usize = 10;
pub const CONNECTING: usize = 11;
pub const LOBBY: usize = 12;
pub const LOBBY_HOST: usize = 13;
pub const LOBBY_WAIT: usize = 14;
pub const START_HINT: usize = 15;
pub const LEAVE_HINT: usize = 16;
pub const E_ERR_NOT_FOUND: usize = 17;
pub const E_ERR_PASSWORD: usize = 18;
pub const E_ERR_FULL: usize = 19;
pub const E_ERR_GAME: usize = 20;
pub const E_ERR_NO_ROOM: usize = 21;
pub const E_ERR_NOT_HOST: usize = 22;
pub const E_ERR_CLOSED: usize = 23;
pub const SERVERS: usize = 24;
pub const RECEIPTS: usize = 25;
pub const FEE: usize = 26;
pub const FEE_OK: usize = 27;
pub const BATTERY: usize = 28;
pub const EXIT_OPEN: usize = 29;
pub const INTERACT_REBOOT: usize = 30;
pub const INTERACT_TAKE: usize = 31;
pub const INTERACT_TERMINAL: usize = 32;
pub const INTERACT_BREAKER: usize = 33;
pub const INTERACT_BATTERY: usize = 34;
pub const INTERACT_REVIVE: usize = 35;
pub const INTERACT_DOOR: usize = 36;
pub const INTERACT_NEED_RECEIPTS: usize = 37;
pub const DOWNED: usize = 38;
pub const SPECTATOR: usize = 39;
pub const WIN_TITLE: usize = 40;
pub const WIN_SUB: usize = 41;
pub const LOSE_TITLE: usize = 42;
pub const LOSE_SUB: usize = 43;
pub const TIME: usize = 44;
pub const ESCAPED: usize = 45;
pub const AGAIN_HINT: usize = 46;
pub const PAUSED: usize = 47;
pub const RESUME_HINT: usize = 48;
pub const QUIT_HINT: usize = 49;
pub const OPT_LANG: usize = 50;
pub const OPT_SENS: usize = 51;
pub const OPT_NAME: usize = 52;
pub const BACK_HINT: usize = 53;
pub const FLASH_ON: usize = 54;
pub const TICKET_LIST: usize = 55;
pub const CONTROLS: usize = 56;
pub const HOST_OF: usize = 57;
pub const WAITING: usize = 58;
pub const STAMINA: usize = 59;
pub const OPT_RENDER: usize = 60;
pub const OPT_RT: usize = 61;
pub const RT_OFF: usize = 62;
pub const RT_QUAL: usize = 63;
pub const RT_ULTRA: usize = 64;
pub const RT_TOAST: usize = 65;
pub const BTN_HOST: usize = 66;
pub const BTN_JOIN: usize = 67;
pub const BTN_OPTIONS: usize = 68;
pub const BTN_QUIT: usize = 69;
pub const BTN_BACK: usize = 70;
pub const BTN_START: usize = 71;
pub const BTN_LEAVE: usize = 72;
pub const BTN_RESUME: usize = 73;
pub const BTN_QUITMENU: usize = 74;
pub const BTN_TO_MENU: usize = 75;
pub const OPT_UPSCALING: usize = 76;
pub const UP_NATIVE: usize = 77;
pub const UP_FSR3: usize = 78;
pub const UP_DLSS: usize = 79;
pub const UP_Q_LABEL: usize = 80;
pub const UP_QUALITY: usize = 81;
pub const UP_BALANCED: usize = 82;
pub const UP_PERF: usize = 83;
pub const DLSS_NEED_RTX: usize = 84;
pub const OPT_RESOLUTION: usize = 85;
pub const RS_AUTO: usize = 86;
pub const UP_TOAST: usize = 87;
pub const UP_NOTE: usize = 88;
pub const HOST_SETUP: usize = 89;
pub const BOTS_LABEL: usize = 90;
pub const LOCAL_NOTE: usize = 91;
pub const E_LOCAL_SERVER: usize = 92;
pub const BOTS_SUB: usize = 93;

pub fn t(lang: Lang, key: usize) -> &'static str {
    let (fr, en) = TEXTS[key];
    match lang {
        Lang::Fr => fr,
        Lang::En => en,
    }
}

/// Texte du ticket d'incident i, dans la langue choisie.
pub fn ticket_text(lang: Lang, i: usize) -> &'static str {
    let (fr, en) = sl3_shared::texts::SERVER_TICKETS[i % sl3_shared::texts::SERVER_TICKETS.len()];
    match lang {
        Lang::Fr => fr,
        Lang::En => en,
    }
}

pub fn receipt_text(lang: Lang, i: usize) -> &'static str {
    let (fr, en) = sl3_shared::texts::RECEIPT_NAMES[i % sl3_shared::texts::RECEIPT_NAMES.len()];
    match lang {
        Lang::Fr => fr,
        Lang::En => en,
    }
}

pub const TEXTS: &[(&str, &str)] = &[
    // 0
    ("SUPPORT NIVEAU -3", "SUPPORT LEVEL -3"),
    // 1
    (
        "La maintenance est un travail de nuit.",
        "Maintenance is a night job.",
    ),
    // 2 ("[1] Héberger une partie")
    ("[1] Héberger une partie", "[1] Host a game"),
    // 3
    ("[2] Rejoindre une partie", "[2] Join a game"),
    // 4
    ("[3] Options", "[3] Options"),
    // 5
    ("[Échap] Quitter", "[Esc] Quit"),
    // 6
    ("Adresse du serveur :", "Server address:"),
    // 7 (unused alias)
    ("", ""),
    // 8
    ("Mot de passe (vide = aucun) :", "Password (empty = none):"),
    // 9
    ("Code de la room :", "Room code:"),
    // 10
    ("Votre nom :", "Your name:"),
    // 11
    ("Connexion…", "Connecting…"),
    // 12
    ("SALON — ROOM", "LOBBY — ROOM"),
    // 13
    ("[Espace] Démarrer la partie", "[Space] Start the game"),
    // 14
    ("En attente de l'hôte…", "Waiting for the host…"),
    // 15
    ("[Espace] Démarrer", "[Space] Start"),
    // 16
    ("[Échap] Quitter le salon", "[Esc] Leave lobby"),
    // 17
    ("Room introuvable.", "Room not found."),
    // 18
    ("Mot de passe incorrect.", "Wrong password."),
    // 19
    ("Room pleine (4 joueurs max).", "Room full (4 players max)."),
    // 20
    ("Partie déjà en cours.", "Game already running."),
    // 21
    ("Vous n'êtes pas dans une room.", "You are not in a room."),
    // 22
    ("Seul l'hôte peut lancer.", "Only the host can start."),
    // 23
    ("Connexion perdue.", "Connection lost."),
    // 24
    ("Serveurs", "Servers"),
    // 25
    ("Justificatifs", "Receipts"),
    // 26
    ("Note de frais", "Expense report"),
    // 27
    ("VALIDÉE — 42,73 €", "APPROVED — €42.73"),
    // 28
    ("Batterie", "Battery"),
    // 29
    ("SORTIE OUVERTE — HALL NORD", "EXIT OPEN — NORTH HALL"),
    // 30
    ("[E] Maintenir : reboot", "[E] Hold: reboot"),
    // 31
    ("[E] Ramasser", "[E] Pick up"),
    // 32
    ("[E] Valider la note de frais", "[E] Approve expense report"),
    // 33
    ("[E] Réarmer le disjoncteur", "[E] Reset breaker"),
    // 34
    ("[E] Prendre la batterie", "[E] Take battery"),
    // 35
    ("[E] Maintenir : relever", "[E] Hold: revive"),
    // 36
    ("[E] Ouvrir / fermer", "[E] Open / close"),
    // 37
    ("Il manque des justificatifs", "Missing receipts"),
    // 38
    ("VOUS ÊTES AU SOL — un coéquipier peut vous relever", "YOU ARE DOWN — a teammate can revive you"),
    // 39
    ("Mode spectateur — [Échap] quitter", "Spectator mode — [Esc] quit"),
    // 40
    ("PRIME DE NUIT ENCAISSÉE", "NIGHT SHIFT BONUS CASHED"),
    // 41
    ("Vous avez fui le niveau -3 avec votre remboursement.", "You fled level -3 with your refund."),
    // 42
    ("POSTE VACANT", "POSITION OPEN"),
    // 43
    ("Le support informatique recrute. Encore.", "IT support is hiring. Again."),
    // 44
    ("Temps", "Time"),
    // 45
    ("Rescapés", "Survivors"),
    // 46
    ("[Entrée] Retour au menu", "[Enter] Back to menu"),
    // 47
    ("PAUSE", "PAUSED"),
    // 48
    ("[Échap] Reprendre", "[Esc] Resume"),
    // 49
    ("[Q] Quitter vers le menu", "[Q] Quit to menu"),
    // 50
    ("[L] Langue / Language : FR", "[L] Language / Langue : EN"),
    // 51
    ("[<> ] Sensibilité souris", "[<> ] Mouse sensitivity"),
    // 52
    ("[N] Changer de nom", "[N] Change name"),
    // 53
    ("[Échap] Retour", "[Esc] Back"),
    // 54
    ("[F] Lampe torche", "[F] Flashlight"),
    // 55
    ("TICKETS EN COURS", "OPEN TICKETS"),
    // 56
    ("ZQSD/WASD bouger – Souris regarder – Maj courir – E interagir – F torche", "WASD move – Mouse look – Shift run – E interact – F torch"),
    // 57
    ("Hôte", "Host"),
    // 58
    ("…", "…"),
    // 59
    ("Endurance", "Stamina"),
    // 60
    ("Échelle de rendu [R]", "Render scale [R]"),
    // 61
    ("[T] Ray tracing", "[T] Ray tracing"),
    // 62
    ("Désactivé", "Off"),
    // 63
    ("Qualité — ombres douces + occlusion", "Quality — soft shadows + occlusion"),
    // 64
    ("Ultra — + rebond de lumière (GI)", "Ultra — + light bounce (GI)"),
    // 65
    ("Ray tracing", "Ray tracing"),
    // 66
    ("Héberger une partie", "Host a game"),
    // 67
    ("Rejoindre une partie", "Join a game"),
    // 68
    ("Options", "Options"),
    // 69
    ("Quitter", "Quit"),
    // 70
    ("Retour", "Back"),
    // 71
    ("Démarrer la partie", "Start the game"),
    // 72
    ("Quitter le salon", "Leave lobby"),
    // 73
    ("Reprendre", "Resume"),
    // 74
    ("Quitter vers le menu", "Quit to menu"),
    // 75
    ("Menu", "Menu"),
    // 76
    ("Upscaling", "Upscaling"),
    // 77
    ("Natif", "Native"),
    // 78
    ("FSR 3", "FSR 3"),
    // 79
    ("DLSS", "DLSS"),
    // 80
    ("Qualité d'upscaling", "Upscaling quality"),
    // 81
    ("Qualité", "Quality"),
    // 82
    ("Équilibré", "Balanced"),
    // 83
    ("Performance", "Performance"),
    // 84
    ("RTX requis", "RTX required"),
    // 85
    ("Résolution", "Resolution"),
    // 86
    ("Auto", "Auto"),
    // 87
    ("Upscaling", "Upscaling"),
    // 88
    ("Reconstruction temporelle avec vecteurs de mouvement (port FSR 3)", "Temporal reconstruction with motion vectors (FSR 3 port)"),
    // 89
    ("Créer une partie", "Create a game"),
    // 90
    ("Bots", "Bots"),
    // 91
    ("Serveur local intégré : démarrage automatique.", "Built-in local server: starts automatically."),
    // 92
    ("Serveur local impossible", "Local server failed"),
    // 93
    ("équipiers IA qui réparent les serveurs", "AI teammates repairing servers"),
];

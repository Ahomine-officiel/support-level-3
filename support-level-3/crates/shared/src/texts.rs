//! Textes de gameplay bilingues (FR/EN) : tickets d'incident, justificatifs, annonces.

/// Tickets d'incident des 6 baies serveurs (FR, EN).
pub const SERVER_TICKETS: [(&str, &str); 6] = [
    (
        "Incident #0231 — srv-00 : mise à jour bloquée à 34 % depuis 3 ans.",
        "Ticket #0231 — srv-00 : update stuck at 34% for 3 years.",
    ),
    (
        "Incident #0232 — srv-01 : café renversé sur l'onduleur.",
        "Ticket #0232 — srv-01 : coffee spilled on the UPS.",
    ),
    (
        "Incident #0233 — srv-02 : RAID saturé de memes légaux.",
        "Ticket #0233 — srv-02 : RAID full of legal memes.",
    ),
    (
        "Incident #0234 — srv-03 : firmware hanté, à rebooter.",
        "Ticket #0234 — srv-03 : haunted firmware, reboot required.",
    ),
    (
        "Incident #0235 — srv-04 : câble RJ45 rongé (par quoi ?).",
        "Ticket #0235 — srv-04 : RJ45 cable chewed (by what?).",
    ),
    (
        "Incident #0236 — srv-05 : horloge CMOS revenue en 1970.",
        "Ticket #0236 — srv-05 : CMOS clock back to 1970.",
    ),
];

/// Les 4 justificatifs à collecter pour la note de frais (FR, EN).
pub const RECEIPT_NAMES: [(&str, &str); 4] = [
    ("Ticket restaurant", "Lunch voucher"),
    ("Facture de taxi", "Taxi invoice"),
    ("Reçu de matériel", "Hardware receipt"),
    ("Note de péage", "Toll receipt"),
];

/// Annonces serveur (FR, EN), avec formattage {0}/{1}.
pub fn announce(key: &str, fr_args: &[&str], en_args: &[&str]) -> (String, String) {
    let (fr, en): (&str, &str) = match key {
        "server_fixed" => (
            "[Système] Ticket résolu — {0} en ligne ({1}/6)",
            "[System] Ticket resolved — {0} back online ({1}/6)",
        ),
        "server_all" => (
            "[Système] Tous les serveurs sont en ligne. En attente du remboursement…",
            "[System] All servers online. Awaiting expense approval…",
        ),
        "receipt" => (
            "[Radio] {0} a trouvé : {1} ({2}/4)",
            "[Radio] {0} found: {1} ({2}/4)",
        ),
        "receipts_all" => (
            "[Radio] Tous les justificatifs réunis. Direction le terminal RH !",
            "[Radio] All receipts collected. Head to the HR terminal!",
        ),
        "fee_ok" => (
            "[RH] Note de frais validée : 42,73 € remboursés. Code sortie : 1337.",
            "[HR] Expense report approved: €42.73 reimbursed. Exit code: 1337.",
        ),
        "exit_open" => (
            "[Système] PORTE DE SECOURS DÉVERROUILLÉE — évacuez par le hall.",
            "[System] EMERGENCY EXIT UNLOCKED — evacuate through the hall.",
        ),
        "chase" => (
            "L'Auditeur vous a repéré. COUREZ.",
            "The Auditor has spotted you. RUN.",
        ),
        "chase_end" => (
            "L'Auditeur a perdu votre trace… pour l'instant.",
            "The Auditor lost your trail… for now.",
        ),
        "downed" => (
            "{0} est au sol ! Maintenez E sur lui pour le relever.",
            "{0} is down! Hold E on them to revive.",
        ),
        "revived" => ("{0} est de retour debout.", "{0} is back on their feet."),
        "out" => (
            "{0} a été « audité ». Poste vacant.",
            "{0} has been \"audited\". Position open.",
        ),
        "escaped" => ("{0} a quitté le niveau -3.", "{0} left level -3."),
        "blackout" => (
            "[Électricité] Coupure générale. Le disjoncteur est au sud-est.",
            "[Power] Full blackout. Breaker is in the south-east room.",
        ),
        "lights_restored" => (
            "[Électricité] Courant rétabli. Ce n'était pas des rongeurs.",
            "[Power] Electricity restored. It wasn't rats.",
        ),
        "joined" => ("{0} pointe au support technique.", "{0} clocks in at IT support."),
        "left" => ("{0} a posé sa démission.", "{0} resigned."),
        "entity_near" => (
            "Vous entendez des pas dans le couloir…",
            "You hear footsteps in the corridor…",
        ),
        _ => (key, key),
    };
    (fmt(fr, fr_args), fmt(en, en_args))
}

fn fmt(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{}}}", i), a);
    }
    out
}

//! Binaire serveur dédié « SUPPORT LEVEL -3 » — fine couche CLI sur la lib sl3_server.
//! Le client embarque aussi ce serveur (voir `sl3_server::spawn_local`) : cliquer
//! « Héberger une partie » démarre un serveur local automatiquement.

fn main() {
    let mut port = sl3_shared::consts::DEFAULT_PORT;
    let mut bots = 0usize;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(sl3_shared::consts::DEFAULT_PORT);
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

    sl3_server::run_dedicated(port, bots);
}

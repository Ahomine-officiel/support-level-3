//! Test d'intégration : serveur local INTÉGRÉ (`spawn_local`) — le chemin exact
//! du bouton « Héberger une partie » du client. Le serveur tourne dans le
//! processus de test (pas de binaire séparé), des bots rejoignent la room.

use sl3_shared::consts::*;
use sl3_shared::protocol::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

struct Conn {
    stream: TcpStream,
    frames: FrameReader,
    buf: [u8; 65536],
}

impl Conn {
    fn connect(addr: &str) -> Conn {
        for _ in 0..60 {
            if let Ok(s) = TcpStream::connect(addr) {
                s.set_nodelay(true).ok();
                return Conn { stream: s, frames: FrameReader::new(), buf: [0u8; 65536] };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("serveur injoignable : {addr}");
    }

    fn send(&mut self, msg: &ClientMsg) {
        self.stream.write_all(&encode(msg)).unwrap();
        self.stream.flush().unwrap();
    }

    fn recv(&mut self, deadline: Instant) -> ServerMsg {
        self.stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        loop {
            if Instant::now() > deadline {
                panic!("timeout en attendant une réponse serveur");
            }
            if let Some(msg) = self.frames.next::<ServerMsg>() {
                return msg;
            }
            match self.stream.read(&mut self.buf) {
                Ok(0) => panic!("serveur déconnecté"),
                Ok(n) => self.frames.feed(&self.buf[..n]),
                Err(_) => continue,
            }
        }
    }

    fn drain_until(&mut self, deadline: Instant, pred: impl Fn(&ServerMsg) -> bool) -> ServerMsg {
        loop {
            let msg = self.recv(deadline);
            if pred(&msg) {
                return msg;
            }
        }
    }
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn embedded_server_hosts_with_bots() {
    // Exactement ce que fait le client quand on clique « Héberger » :
    // serveur local intégré + 2 bots équipiers.
    let port = sl3_server::spawn_local(2).expect("spawn_local");
    assert!(port > 0);
    let addr = format!("127.0.0.1:{port}");

    let mut s = Conn::connect(&addr);
    s.send(&ClientMsg::Hello { name: "Hote".into(), lang: 0 });
    assert!(matches!(s.recv(soon()), ServerMsg::Welcome { .. }));

    s.send(&ClientMsg::CreateRoom { password: String::new() });
    let created = s.drain_until(soon(), |m| matches!(m, ServerMsg::RoomCreated { .. }));
    let code = match created {
        ServerMsg::RoomCreated { code } => code,
        _ => unreachable!(),
    };
    assert_eq!(code.len(), CODE_LEN);

    let joined = s.drain_until(soon(), |m| matches!(m, ServerMsg::RoomJoined { .. }));
    match joined {
        // L'hôte + les 2 bots (enregistrés AVANT l'envoi de RoomJoined).
        ServerMsg::RoomJoined { players, is_host, .. } => {
            assert_eq!(players.len(), 3, "hôte + 2 bots attendus dans le lobby");
            assert!(is_host, "l'humain doit être l'hôte (premier joueur)");
            assert!(
                players.iter().filter(|p| p.name.starts_with("BOT-")).count() == 2,
                "2 bots attendus"
            );
        }
        _ => unreachable!(),
    }

    // L'hôte (humain) lance la partie : les bots participent.
    s.send(&ClientMsg::StartGame);
    let started = s.drain_until(soon(), |m| matches!(m, ServerMsg::GameStarted { .. }));
    match started {
        ServerMsg::GameStarted { snapshot, .. } => {
            assert_eq!(snapshot.players.len(), 3, "hôte + 2 bots en jeu");
        }
        _ => unreachable!(),
    }

    // Les boucles IA des bots tournent : leurs inputs arrivent (positions à jour).
    s.send(&ClientMsg::Input {
        pos: glam::Vec3::new(0.0, 0.0, 0.0),
        yaw: 0.0,
        pitch: 0.0,
        sprint: false,
        light_on: true,
    });
    let snap = s.drain_until(Instant::now() + Duration::from_secs(3), |m| {
        matches!(m, ServerMsg::Snapshot(_))
    });
    assert!(matches!(snap, ServerMsg::Snapshot(_)));
}

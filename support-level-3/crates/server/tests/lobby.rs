//! Test d'intégration : lobby + partie complète via le vrai binaire serveur (TCP).

use sl3_shared::consts::*;
use sl3_shared::protocol::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Flux TCP + lecteur de frames persistant (les octets restants sont conservés).
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

#[test]
fn full_lobby_and_game_flow() {
    // Port éphémère libre.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_sl3-server"))
        .arg("--port")
        .arg(port.to_string())
        .spawn()
        .expect("serveur lançable");

    let result = std::panic::catch_unwind(|| {
        let addr = format!("127.0.0.1:{port}");

        // --- Joueur 1 : crée une room avec mot de passe ---
        let mut s1 = Conn::connect(&addr);
        s1.send(&ClientMsg::Hello { name: "Alice".into(), lang: 0 });
        let welcome = s1.recv(soon());
        assert!(matches!(welcome, ServerMsg::Welcome { .. }));

        s1.send(&ClientMsg::CreateRoom { password: "1337".into() });
        let created = s1.drain_until(soon(), |m| matches!(m, ServerMsg::RoomCreated { .. }));
        let code = match created {
            ServerMsg::RoomCreated { code } => code,
            _ => unreachable!(),
        };
        assert_eq!(code.len(), CODE_LEN);
        s1.drain_until(soon(), |m| matches!(m, ServerMsg::RoomJoined { .. }));

        // --- Joueur 2 : mauvais mot de passe ---
        let mut s2 = Conn::connect(&addr);
        s2.send(&ClientMsg::Hello { name: "Bob".into(), lang: 1 });
        let _ = s2.recv(soon());
        s2.send(&ClientMsg::JoinRoom { code: code.clone(), password: "WRONG".into() });
        let err = s2.drain_until(soon(), |m| matches!(m, ServerMsg::RoomError { .. }));
        match err {
            ServerMsg::RoomError { msg_key } => assert_eq!(msg_key, ERR_WRONG_PASSWORD),
            _ => unreachable!(),
        }

        // --- Joueur 2 : bon mot de passe ---
        s2.send(&ClientMsg::JoinRoom { code: code.clone(), password: "1337".into() });
        let joined = s2.drain_until(soon(), |m| matches!(m, ServerMsg::RoomJoined { .. }));
        match joined {
            ServerMsg::RoomJoined { players, is_host, .. } => {
                assert_eq!(players.len(), 2);
                assert!(!is_host);
            }
            _ => unreachable!(),
        }

        // --- Démarrage par l'hôte ---
        s1.send(&ClientMsg::StartGame);
        let started = s1.drain_until(soon(), |m| matches!(m, ServerMsg::GameStarted { .. }));
        match started {
            ServerMsg::GameStarted { snapshot, .. } => {
                assert_eq!(snapshot.players.len(), 2);
                assert_eq!(snapshot.servers.len(), SERVER_COUNT);
                assert!(snapshot.servers.iter().all(|s| s.state == SRV_BROKEN));
                assert_eq!(snapshot.receipts.len(), RECEIPT_COUNT);
            }
            _ => unreachable!(),
        }
        s2.drain_until(soon(), |m| matches!(m, ServerMsg::GameStarted { .. }));

        // --- Snapshots à 20 Hz ---
        let snap = s1.drain_until(Instant::now() + Duration::from_secs(3), |m| {
            matches!(m, ServerMsg::Snapshot(_))
        });
        assert!(matches!(snap, ServerMsg::Snapshot(_)));

        // --- Entrées acceptées et répliquées ---
        s1.send(&ClientMsg::Input {
            pos: glam::Vec3::new(15.0, 0.0, 9.0),
            yaw: 0.5,
            pitch: 0.0,
            sprint: false,
            light_on: true,
        });
        let snap = s1.drain_until(Instant::now() + Duration::from_secs(3), |m| {
            matches!(m, ServerMsg::Snapshot(_))
        });
        match snap {
            ServerMsg::Snapshot(s) => {
                let me = s.players.iter().find(|p| p.pos.x > 10.0);
                assert!(me.is_some(), "position du joueur pas répliquée");
            }
            _ => unreachable!(),
        }

        // --- Fin propre ---
        s1.send(&ClientMsg::LeaveRoom);
        s2.send(&ClientMsg::LeaveRoom);
    });

    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

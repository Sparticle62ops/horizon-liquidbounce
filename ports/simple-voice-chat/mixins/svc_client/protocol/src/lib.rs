//! The voice chat wire format Simple Voice Chat 2.6.x clients speak
//! (compatibility version 20), from the client's side: control messages on
//! `voicechat:` channels over the game connection, and AES-128-GCM UDP
//! datagrams for the voice itself. This is an independent implementation of
//! the protocol's wire layout so a Horizon player talks to ordinary servers
//! and players. The other end's reading of the same layout lives in the
//! horizon-mods voicechat-server relay.
//!
//! Numbers are big-endian, a UUID is 16 bytes (most significant half first),
//! strings and byte arrays are a VarInt length then the bytes.

/// The only version this client speaks.
pub const COMPATIBILITY_VERSION: i32 = 20;
/// Every UDP datagram starts with it.
pub const MAGIC: u8 = 0xFF;
/// The largest Opus frame sent or accepted.
pub const MAX_OPUS: usize = 1275;
/// The largest encrypted body read from a datagram.
pub const MAX_BODY: usize = 2048;
/// Group names and passwords, in characters.
pub const MAX_GROUP_TEXT: usize = 24;

pub const REQUEST_SECRET: &str = "voicechat:request_secret";
pub const SECRET: &str = "voicechat:secret";
pub const UPDATE_STATE: &str = "voicechat:update_state";
pub const STATE: &str = "voicechat:state";
pub const STATES: &str = "voicechat:states";
pub const REMOVE_STATE: &str = "voicechat:remove_state";
pub const CREATE_GROUP: &str = "voicechat:create_group";
pub const SET_GROUP: &str = "voicechat:set_group";
pub const LEAVE_GROUP: &str = "voicechat:leave_group";
pub const JOINED_GROUP: &str = "voicechat:joined_group";
pub const ADD_GROUP: &str = "voicechat:add_group";
pub const REMOVE_GROUP: &str = "voicechat:remove_group";

/// A message that can't be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

pub type Read<T> = Result<T, Malformed>;

pub struct In<'a> {
    rest: &'a [u8],
}

impl<'a> In<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { rest: data }
    }
    pub fn take(&mut self, n: usize) -> Read<&'a [u8]> {
        if self.rest.len() < n {
            return Err(Malformed);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }
    fn fixed<const N: usize>(&mut self) -> Read<[u8; N]> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
    pub fn byte(&mut self) -> Read<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn flag(&mut self) -> Read<bool> {
        Ok(self.byte()? != 0)
    }
    pub fn short(&mut self) -> Read<i16> {
        Ok(i16::from_be_bytes(self.fixed()?))
    }
    pub fn int(&mut self) -> Read<i32> {
        Ok(i32::from_be_bytes(self.fixed()?))
    }
    pub fn long(&mut self) -> Read<i64> {
        Ok(i64::from_be_bytes(self.fixed()?))
    }
    pub fn float(&mut self) -> Read<f32> {
        Ok(f32::from_be_bytes(self.fixed()?))
    }
    pub fn double(&mut self) -> Read<f64> {
        Ok(f64::from_be_bytes(self.fixed()?))
    }
    pub fn uuid(&mut self) -> Read<u128> {
        Ok(u128::from_be_bytes(self.fixed()?))
    }
    pub fn varint(&mut self) -> Read<i32> {
        let mut value = 0u32;
        for shift in 0..5 {
            let b = self.byte()?;
            value |= u32::from(b & 0x7F) << (shift * 7);
            if b & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        Err(Malformed)
    }
    /// A VarInt length of at most `max`, then that many bytes.
    pub fn bytes(&mut self, max: usize) -> Read<&'a [u8]> {
        let n = usize::try_from(self.varint()?).map_err(|_| Malformed)?;
        if n > max {
            return Err(Malformed);
        }
        self.take(n)
    }
    /// A string of at most `max` characters.
    pub fn text(&mut self, max: usize) -> Read<String> {
        let raw = self.bytes(max.saturating_mul(4))?;
        let s = std::str::from_utf8(raw).map_err(|_| Malformed)?;
        if s.chars().count() > max {
            return Err(Malformed);
        }
        Ok(s.to_owned())
    }
}

#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn byte(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn flag(&mut self, v: bool) -> &mut Self {
        self.byte(u8::from(v))
    }
    pub fn short(&mut self, v: i16) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn int(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn long(&mut self, v: i64) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn float(&mut self, v: f32) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn double(&mut self, v: f64) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn uuid(&mut self, v: u128) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn varint(&mut self, v: i32) -> &mut Self {
        let mut v = v as u32;
        while v >= 0x80 {
            self.0.push((v as u8) | 0x80);
            v >>= 7;
        }
        self.0.push(v as u8);
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.varint(b.len() as i32);
        self.0.extend_from_slice(b);
        self
    }
    pub fn text(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }
    pub fn done(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

// ------------------------------------------------------------ the game connection

/// How a group hears the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    /// Members hear each other anywhere and players nearby; nobody outside hears them.
    Normal,
    /// Members hear each other anywhere; players nearby hear them and are heard.
    Open,
    /// Members hear only each other.
    Isolated,
}

impl GroupKind {
    pub fn from_wire(v: i16) -> Self {
        match v {
            1 => Self::Open,
            2 => Self::Isolated,
            _ => Self::Normal,
        }
    }
    pub fn wire(self) -> i16 {
        match self {
            Self::Normal => 0,
            Self::Open => 1,
            Self::Isolated => 2,
        }
    }
}

/// What the client puts on a `voicechat:` channel.
pub fn request_secret() -> Vec<u8> {
    Out::default().int(COMPATIBILITY_VERSION).done()
}

pub fn update_state(disabled: bool) -> Vec<u8> {
    Out::default().flag(disabled).done()
}

pub fn create_group(name: &str, password: Option<&str>, kind: GroupKind) -> Vec<u8> {
    let mut o = Out::default();
    o.text(name);
    if let Some(p) = password {
        o.flag(true).text(p);
    } else {
        o.flag(false);
    }
    o.short(kind.wire()).done()
}

pub fn join_group(group: u128, password: Option<&str>) -> Vec<u8> {
    let mut o = Out::default();
    o.uuid(group);
    if let Some(p) = password {
        o.flag(true).text(p);
    } else {
        o.flag(false);
    }
    o.done()
}

pub fn leave_group() -> Vec<u8> {
    Vec::new()
}

/// What the server answers with on a `voicechat:` channel.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    /// `voicechat:secret`: everything the client needs to open its voice
    /// connection. An empty `voice_host` means the game server's own address.
    Secret {
        secret: [u8; 16],
        port: u16,
        player: u128,
        codec: u8,
        mtu: i32,
        distance: f64,
        keep_alive_ms: i32,
        groups: bool,
        voice_host: String,
        recording_allowed: bool,
    },
    /// One player's state, on `voicechat:state`.
    State(PlayerState),
    /// Every player's state, on `voicechat:states`.
    States(Vec<PlayerState>),
    /// A player left voice chat, on `voicechat:remove_state`.
    RemoveState(u128),
    /// A group exists, on `voicechat:add_group`.
    AddGroup {
        id: u128,
        name: String,
        password: bool,
        persistent: bool,
        hidden: bool,
        kind: GroupKind,
    },
    /// A group was removed, on `voicechat:remove_group`.
    RemoveGroup(u128),
    /// The answer to create, join or leave, on `voicechat:joined_group`.
    JoinedGroup {
        group: Option<u128>,
        wrong_password: bool,
    },
}

/// A player's voice chat state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerState {
    pub uuid: u128,
    pub name: String,
    pub disabled: bool,
    pub disconnected: bool,
    pub group: Option<u128>,
}

/// Read one server message. `None`: not a channel a server sends on.
pub fn read(channel: &str, data: &[u8]) -> Option<Read<ServerMessage>> {
    let mut r = In::new(data);
    let parsed = match channel {
        SECRET => (|| {
            let secret: [u8; 16] = r.fixed()?;
            let port = r.int()?.try_into().map_err(|_| Malformed)?;
            let player = r.uuid()?;
            let codec = r.byte()?;
            let mtu = r.int()?;
            let distance = r.double()?;
            let keep_alive_ms = r.int()?;
            let groups = r.flag()?;
            let voice_host = r.text(255)?;
            let recording_allowed = r.flag()?;
            Ok(ServerMessage::Secret {
                secret,
                port,
                player,
                codec,
                mtu,
                distance,
                keep_alive_ms,
                groups,
                voice_host,
                recording_allowed,
            })
        })(),
        STATE => (|| {
            let disabled = r.flag()?;
            let disconnected = r.flag()?;
            let uuid = r.uuid()?;
            let name = r.text(MAX_GROUP_TEXT.max(16))?;
            let group = if r.flag()? { Some(r.uuid()?) } else { None };
            Ok(ServerMessage::State(PlayerState {
                uuid,
                name,
                disabled,
                disconnected,
                group,
            }))
        })(),
        STATES => (|| {
            let n = r.int()?;
            if n < 0 {
                return Err(Malformed);
            }
            let mut all = Vec::new();
            for _ in 0..n {
                let disabled = r.flag()?;
                let disconnected = r.flag()?;
                let uuid = r.uuid()?;
                let name = r.text(MAX_GROUP_TEXT.max(16))?;
                let group = if r.flag()? { Some(r.uuid()?) } else { None };
                all.push(PlayerState {
                    uuid,
                    name,
                    disabled,
                    disconnected,
                    group,
                });
            }
            Ok(ServerMessage::States(all))
        })(),
        REMOVE_STATE => r.uuid().map(ServerMessage::RemoveState),
        ADD_GROUP => (|| {
            let id = r.uuid()?;
            let name = r.text(MAX_GROUP_TEXT)?;
            let password = r.flag()?;
            let persistent = r.flag()?;
            let hidden = r.flag()?;
            let kind = GroupKind::from_wire(r.short()?);
            Ok(ServerMessage::AddGroup {
                id,
                name,
                password,
                persistent,
                hidden,
                kind,
            })
        })(),
        REMOVE_GROUP => r.uuid().map(ServerMessage::RemoveGroup),
        JOINED_GROUP => (|| {
            let group = if r.flag()? { Some(r.uuid()?) } else { None };
            let wrong_password = r.flag()?;
            Ok(ServerMessage::JoinedGroup {
                group,
                wrong_password,
            })
        })(),
        _ => return None,
    };
    Some(parsed)
}

// ------------------------------------------------------------ UDP

/// A voice packet the client sends.
#[derive(Debug, Clone, PartialEq)]
pub enum ToServer {
    /// The microphone: an Opus frame (empty: stopped talking).
    Mic {
        data: Vec<u8>,
        sequence: i64,
        whispering: bool,
    },
    Authenticate {
        player: u128,
        secret: [u8; 16],
    },
    Ping {
        id: u128,
        timestamp: i64,
    },
    KeepAlive,
    ConnectionCheck,
}

impl ToServer {
    pub fn write(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Self::Mic {
                data,
                sequence,
                whispering,
            } => o.byte(1).bytes(data).long(*sequence).flag(*whispering),
            Self::Authenticate { player, secret } => {
                o.byte(5).uuid(*player);
                o.0.extend_from_slice(secret);
                &mut o
            }
            Self::Ping { id, timestamp } => o.byte(7).uuid(*id).long(*timestamp),
            Self::KeepAlive => o.byte(8),
            Self::ConnectionCheck => o.byte(9),
        };
        o.done()
    }
}

/// A voice packet the server sends.
#[derive(Debug, Clone, PartialEq)]
pub enum FromServer<'a> {
    /// A voice heard where `sender` stands.
    Positional {
        sender: u128,
        data: &'a [u8],
        sequence: i64,
        distance: f32,
        whispering: bool,
    },
    /// A group member's voice, heard everywhere.
    Group {
        sender: u128,
        data: &'a [u8],
        sequence: i64,
    },
    AuthenticateAck,
    Ping {
        id: u128,
        timestamp: i64,
    },
    KeepAlive,
    ConnectionCheckAck,
}

impl<'a> FromServer<'a> {
    pub fn read(plain: &'a [u8]) -> Read<Self> {
        let mut r = In::new(plain);
        Ok(match r.byte()? {
            2 => {
                let sender = r.uuid()?;
                let _channel = r.uuid()?;
                let data = r.bytes(MAX_OPUS)?;
                let sequence = r.long()?;
                let distance = r.float()?;
                let whispering = r.flag()?;
                Self::Positional {
                    sender,
                    data,
                    sequence,
                    distance,
                    whispering,
                }
            }
            3 => {
                let sender = r.uuid()?;
                let _channel = r.uuid()?;
                let data = r.bytes(MAX_OPUS)?;
                let sequence = r.long()?;
                let _stereo = r.byte()?;
                Self::Group {
                    sender,
                    data,
                    sequence,
                }
            }
            6 => Self::AuthenticateAck,
            7 => Self::Ping {
                id: r.uuid()?,
                timestamp: r.long()?,
            },
            8 => Self::KeepAlive,
            10 => Self::ConnectionCheckAck,
            _ => return Err(Malformed),
        })
    }
}

impl FromServer<'_> {
    /// The server side of the wire, for testing and for a future server mod.
    pub fn write(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Self::Positional { sender, data, sequence, distance, whispering } => {
                o.byte(2).uuid(*sender).uuid(0).bytes(data).long(*sequence)
                    .float(*distance).flag(*whispering);
                &mut o
            }
            Self::Group { sender, data, sequence } => {
                o.byte(3).uuid(*sender).uuid(0).bytes(data).long(*sequence).byte(0);
                &mut o
            }
            Self::AuthenticateAck => o.byte(6),
            Self::Ping { id, timestamp } => o.byte(7).uuid(*id).long(*timestamp),
            Self::KeepAlive => o.byte(8),
            Self::ConnectionCheckAck => o.byte(10),
        };
        o.done()
    }
}

impl ToServer {
    /// The server side of the wire: parse what a client sent.
    pub fn read(plain: &[u8]) -> Read<Self> {
        let mut r = In::new(plain);
        Ok(match r.byte()? {
            1 => Self::Mic {
                data: r.bytes(MAX_OPUS)?.to_vec(),
                sequence: r.long()?,
                whispering: r.flag()?,
            },
            5 => {
                let player = r.uuid()?;
                let secret: [u8; 16] = r.take(16)?.try_into().unwrap();
                Self::Authenticate { player, secret }
            }
            7 => Self::Ping { id: r.uuid()?, timestamp: r.long()? },
            8 => Self::KeepAlive,
            9 => Self::ConnectionCheck,
            _ => return Err(Malformed),
        })
    }
}

/// AES-128-GCM keyed by the player's secret: a random 12-byte nonce, then the
/// ciphertext and its 16-byte tag.
pub struct Cipher {
    key: ring::aead::LessSafeKey,
}

impl Cipher {
    pub fn new(secret: &[u8; 16]) -> Self {
        let key =
            ring::aead::UnboundKey::new(&ring::aead::AES_128_GCM, secret).expect("a 16-byte key");
        Self {
            key: ring::aead::LessSafeKey::new(key),
        }
    }
    pub fn seal(&self, rng: &dyn ring::rand::SecureRandom, plain: &[u8]) -> Option<Vec<u8>> {
        let mut nonce = [0u8; 12];
        rng.fill(&mut nonce).ok()?;
        let mut body = plain.to_vec();
        self.key
            .seal_in_place_append_tag(
                ring::aead::Nonce::assume_unique_for_key(nonce),
                ring::aead::Aad::empty(),
                &mut body,
            )
            .ok()?;
        let mut out = nonce.to_vec();
        out.extend(body);
        Some(out)
    }
    pub fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 12 + 16 {
            return None;
        }
        let (nonce, body) = sealed.split_at(12);
        let nonce = ring::aead::Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut body = body.to_vec();
        let plain = self
            .key
            .open_in_place(nonce, ring::aead::Aad::empty(), &mut body)
            .ok()?;
        Some(plain.to_vec())
    }
}

/// The datagram the client sends: magic, the player's UUID in clear (so the
/// server finds their key), then the sealed packet.
pub fn client_datagram(player: u128, sealed: &[u8]) -> Vec<u8> {
    Out::default().byte(MAGIC).uuid(player).bytes(sealed).done()
}

/// The datagram the server sends: magic, then the sealed packet.
pub fn server_datagram(datagram: &[u8]) -> Read<&[u8]> {
    let mut r = In::new(datagram);
    if r.byte()? != MAGIC {
        return Err(Malformed);
    }
    r.bytes(MAX_BODY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_match_minecraft() {
        for (v, bytes) in [
            (0, vec![0]),
            (127, vec![0x7F]),
            (128, vec![0x80, 1]),
            (300, vec![0xAC, 2]),
            (-1, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]),
        ] {
            assert_eq!(Out::default().varint(v).done(), bytes, "{v}");
            assert_eq!(In::new(&bytes).varint().unwrap(), v);
        }
        assert_eq!(In::new(&[0x80; 6]).varint(), Err(Malformed));
    }

    #[test]
    fn requests_have_the_server_end_layouts() {
        assert_eq!(request_secret(), vec![0, 0, 0, 20]);
        assert_eq!(update_state(true), vec![1]);
        // name "ab", a password "x", kind open.
        assert_eq!(
            create_group("ab", Some("x"), GroupKind::Open),
            vec![2, b'a', b'b', 1, 1, b'x', 0, 1]
        );
        let mut join = 7u128.to_be_bytes().to_vec();
        join.push(0);
        assert_eq!(join_group(7, None), join);
        assert_eq!(leave_group(), Vec::<u8>::new());
    }

    #[test]
    fn secret_layout() {
        // The server's own writer, mirrored byte for byte here.
        let key = [9u8; 16];
        let server_shape: Vec<u8> = key
            .iter()
            .copied()
            .chain(24454i32.to_be_bytes())
            .chain(0xABu128.to_be_bytes())
            .chain([0u8])
            .chain(1024i32.to_be_bytes())
            .chain(48.0f64.to_be_bytes())
            .chain(1000i32.to_be_bytes())
            .chain([1u8])
            .chain([0u8]) // text ""
            .chain([0u8])
            .collect();
        let msg = read(
            SECRET,
            &server_shape,
        );
        assert_eq!(
            msg,
            Some(Ok(ServerMessage::Secret {
                secret: key,
                port: 24454,
                player: 0xAB,
                codec: 0,
                mtu: 1024,
                distance: 48.0,
                keep_alive_ms: 1000,
                groups: true,
                voice_host: String::new(),
                recording_allowed: false,
            }))
        );
    }

    #[test]
    fn states_and_groups_layouts() {
        let a = PlayerState {
            uuid: 1,
            name: "a".into(),
            disabled: true,
            disconnected: false,
            group: Some(2),
        };
        let wire = [
            &[1u8, 0][..],
            &1u128.to_be_bytes()[..],
            &[1, b'a', 1],
            &2u128.to_be_bytes()[..],
        ]
        .concat();
        assert_eq!(read(STATE, &wire), Some(Ok(ServerMessage::State(a.clone()))));
        let both = [
            &Out::default().int(2).done()[..],
            &wire[..],
            &[
                &[1u8, 0][..],
                &1u128.to_be_bytes()[..],
                &[1, b'a', 0],
            ]
            .concat()[..],
        ]
        .concat();
        assert_eq!(
            read(STATES, &both),
            Some(Ok(ServerMessage::States(vec![
                a.clone(),
                PlayerState {
                    group: None,
                    ..a.clone()
                }
            ])))
        );
        let g = [
            &5u128.to_be_bytes()[..],
            &[4, b'B', b'a', b's', b'e', 1, 0, 0, 0, 2],
        ]
        .concat();
        assert_eq!(
            read(ADD_GROUP, &g),
            Some(Ok(ServerMessage::AddGroup {
                id: 5,
                name: "Base".into(),
                password: true,
                persistent: false,
                hidden: false,
                kind: GroupKind::Isolated,
            }))
        );
        assert_eq!(
            read(JOINED_GROUP, &[0, 1]),
            Some(Ok(ServerMessage::JoinedGroup {
                group: None,
                wrong_password: true
            }))
        );
        assert_eq!(
            read(JOINED_GROUP, &[&[1u8][..], &3u128.to_be_bytes(), &[0]].concat()),
            Some(Ok(ServerMessage::JoinedGroup {
                group: Some(3),
                wrong_password: false
            }))
        );
        assert_eq!(read(REMOVE_STATE, &7u128.to_be_bytes()), Some(Ok(ServerMessage::RemoveState(7))));
        assert_eq!(read(REMOVE_GROUP, &7u128.to_be_bytes()), Some(Ok(ServerMessage::RemoveGroup(7))));
        assert_eq!(read("voicechat:secret_x", &[]), None, "an unknown channel");
    }

    #[test]
    fn voice_packets() {
        let mic = ToServer::Mic {
            data: vec![0xAA, 0xBB],
            sequence: 5,
            whispering: true,
        }
        .write();
        assert_eq!(mic, [&[1u8, 2, 0xAA, 0xBB][..], &5i64.to_be_bytes(), &[1]].concat());
        let auth = ToServer::Authenticate {
            player: 7,
            secret: [3; 16],
        }
        .write();
        assert_eq!(auth, [&[5u8][..], &7u128.to_be_bytes(), &[3; 16]].concat());
        assert_eq!(ToServer::KeepAlive.write(), vec![8]);
        assert_eq!(ToServer::ConnectionCheck.write(), vec![9]);
        assert_eq!(ToServer::Ping { id: 1, timestamp: 2 }.write(), [&[7u8][..], &1u128.to_be_bytes(), &2i64.to_be_bytes()].concat());

        let sound = [
            &[2u8][..],
            &4u128.to_be_bytes()[..],
            &4u128.to_be_bytes()[..],
            &[2, 1, 2],
            &9i64.to_be_bytes()[..],
            &48.0f32.to_be_bytes()[..],
            &[0],
        ]
        .concat();
        assert_eq!(
            FromServer::read(&sound),
            Ok(FromServer::Positional {
                sender: 4,
                data: &[1, 2],
                sequence: 9,
                distance: 48.0,
                whispering: false,
            })
        );
        let group = [
            &[3u8][..],
            &4u128.to_be_bytes()[..],
            &4u128.to_be_bytes()[..],
            &[1, 1],
            &2i64.to_be_bytes()[..],
            &[0],
        ]
        .concat();
        assert_eq!(
            FromServer::read(&group),
            Ok(FromServer::Group {
                sender: 4,
                data: &[1],
                sequence: 2,
            })
        );
        assert_eq!(FromServer::read(&[6]), Ok(FromServer::AuthenticateAck));
        assert_eq!(FromServer::read(&[10]), Ok(FromServer::ConnectionCheckAck));
        assert_eq!(FromServer::read(&[1]), Err(Malformed), "a client's packet");
    }

    #[test]
    fn sealed_bodies_round_trip_and_reject_tampering() {
        let rng = ring::rand::SystemRandom::new();
        let c = Cipher::new(&[1; 16]);
        let sealed = c.seal(&rng, b"hello").unwrap();
        assert_eq!(sealed.len(), 12 + 5 + 16);
        assert_eq!(c.open(&sealed).unwrap(), b"hello");
        let mut bad = sealed.clone();
        bad[14] ^= 1;
        assert_eq!(c.open(&bad), None);
        assert_eq!(Cipher::new(&[2; 16]).open(&sealed), None);
        assert_ne!(c.seal(&rng, b"hello").unwrap(), sealed, "fresh nonce each time");

        let datagram = client_datagram(9, &sealed);
        assert_eq!(datagram[0], MAGIC);
        assert_eq!(&datagram[1..17], &9u128.to_be_bytes());
        // The server's reply shape: magic, then the sealed body.
        let reply = Out::default().byte(MAGIC).bytes(&sealed).done();
        let back = server_datagram(&reply).unwrap();
        assert_eq!(back, &sealed[..]);
        assert_eq!(server_datagram(&reply[1..]), Err(Malformed));
    }
}

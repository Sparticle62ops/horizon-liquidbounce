//! The client-vs-server smoke test: the clean-room client half (`svc_client`)
//! speaking over real UDP to Horizon's own voicechat-server relay, driven the
//! way the game drives it: a secret request over the mod channel, then a UDP
//! authenticate, then one mic frame relayed to a nearby listener.
//!
//! The relay crate lives in Horizon's repo, so this integration test is wired
//! to its checkout path and runs on the build box, not in CI.
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use svc_client::protocol::request_secret;
use svc_client::session::{Event, Session, State};
use voicechat_server_relay::{Config, Player, Server};

const A: u128 = 0xA;
const B: u128 = 0xB;
const SECRET_REPLY: &str = "voicechat:secret";

fn players() -> Vec<Player> {
    vec![
        Player {
            uuid: A,
            name: "alice".into(),
            dimension: "minecraft:overworld".into(),
            position: [0.0, 64.0, 0.0],
        },
        Player {
            uuid: B,
            name: "bob".into(),
            dimension: "minecraft:overworld".into(),
            position: [10.0, 64.0, 0.0],
        },
    ]
}

/// The game side, driven the way voicechat-server's glue drives the relay.
struct Game {
    server: Server,
}

impl Game {
    /// A message from a client's mixin, over the mod channel.
    fn from_client(&self, player: u128, channel: &str, data: &[u8]) {
        if !self.server.knows(player) {
            self.server.tick(players());
        }
        self.server.message(player, channel, data);
    }
    /// The mod-channel messages waiting for one player.
    fn for_player(&self, who: u128) -> Vec<(String, Vec<u8>)> {
        self.server
            .outgoing(64)
            .into_iter()
            .filter(|(p, _, _)| *p == who)
            .map(|(_, c, d)| (c, d))
            .collect()
    }
}

/// One player's full handshake: request a secret over the game connection,
/// then authenticate over real UDP with the client's own wire layer.
fn connect(game: &Game, session: &mut Session, uuid: u128) {
    session.request_secret();
    game.from_client(uuid, "voicechat:request_secret", &request_secret());
    let replies = game.for_player(uuid);
    let (_, body) = replies
        .iter()
        .find(|(c, _)| c == SECRET_REPLY)
        .expect("a secret reply");
    // The reply is secret(16) + port(int) + uuid, big endian on the wire.
    let secret: [u8; 16] = body[..16].try_into().unwrap();
    let port = u32::from_be_bytes(body[16..20].try_into().unwrap()) as u16;
    session
        .on_secret(secret, SocketAddr::from(([127, 0, 0, 1], port)))
        .expect("the UDP socket opened");
}

fn wait_connected(session: &mut Session, timeout_ms: u64) -> bool {
    let until = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < until {
        for e in session.pump() {
            if matches!(e, Event::Connected) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn wait_voice(session: &mut Session, timeout_ms: u64) -> Option<Event> {
    let until = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < until {
        for e in session.pump() {
            if matches!(e, Event::Voice { .. }) {
                return Some(e);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

#[test]
fn client_talks_to_relay_over_udp() {
    let server = Server::start(Config {
        bind: "127.0.0.1".into(),
        port: 0,
        ..Config::default()
    })
    .expect("the relay started");
    server.tick(players());
    let game = Game { server };

    let mut a = Session::new(A);
    let mut b = Session::new(B);
    connect(&game, &mut a, A);
    connect(&game, &mut b, B);

    assert!(wait_connected(&mut a, 2000), "A authenticated over UDP");
    assert!(wait_connected(&mut b, 2000), "B authenticated over UDP");
    assert_eq!(a.state(), State::Connected);
    assert_eq!(b.state(), State::Connected);

    // A and B stand 10 blocks apart, well inside the 48-block carry.
    a.set_position(B, [10.0, 64.0, 0.0]);
    b.set_position(A, [0.0, 64.0, 0.0]);
    a.set_listener([0.0, 64.0, 0.0]);
    b.set_listener([10.0, 64.0, 0.0]);
    game.server.tick(players());

    // The relay only relays voices for clients it has heard a keep-alive
    // from. In the game the first frame after connecting sends one; here the
    // clock is compressed to a single tick each.
    a.keep_alive_every = Duration::from_millis(1);
    b.keep_alive_every = Duration::from_millis(1);
    // A keep-alive each, with room for the relay to process them before the
    // first voice packet (it only relays for clients it has heard from).
    for _ in 0..3 {
        a.tick();
        b.tick();
        std::thread::sleep(Duration::from_millis(20));
    }

    // A speaks one frame; the relay must hand it to B over UDP, still sealed
    // for B's key, and the client's parser must open it.
    let frame: Vec<u8> = (0..64u8).collect();
    a.push_mic(&frame);

    let heard = wait_voice(&mut b, 2000).expect("B heard A");
    match heard {
        Event::Voice {
            sender,
            group,
            whispering,
            data,
            volume,
        } => {
            assert_eq!(sender, A);
            assert!(!group, "positional, not group");
            assert!(!whispering);
            assert_eq!(data, frame, "the opus frame arrives whole");
            assert!(volume > 0.0 && volume < 1.0, "distance-attenuated");
        }
        _ => unreachable!(),
    }

    // The other direction too: B talks back and A hears.
    let back: Vec<u8> = (100..164u8).collect();
    b.push_mic(&back);
    match wait_voice(&mut a, 2000).expect("A heard B") {
        Event::Voice { sender, data, .. } => {
            assert_eq!(sender, B);
            assert_eq!(data, back);
        }
        _ => unreachable!(),
    }
}

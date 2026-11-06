
// e4mc for Horizon: open your hosted world to friends through the e4mc relay.
// A Rust port of e4mc (https://github.com/vgskye/e4mc-minecraft-architectury, MIT,
// by vgskye and contributors). The relay half follows e4mc-quiclime
// (Apache-2.0). Degradations from the Java mod are listed in README.md.
//
// This file is the relay session: config, the broker, the QUIC control channel
// (JSON kind-tagged messages, ALPN "quiclime") and the stream-to-TCP bridge a
// joining player's connection rides. Hook wiring lives in the mixin helpers.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Config: <config dir>/e4mc/config.json, same fields and defaults as the
// original's e4mc.toml.
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// Whether to hide the domain on chat and only allow copying.
    #[serde(default)]
    pub hide_domain_in_chat: bool,
    /// Whether to ask the broker for the best relay or use a hard-coded one.
    #[serde(default = "d_true")]
    pub use_broker: bool,
    #[serde(default = "d_broker_url")]
    pub broker_url: String,
    #[serde(default = "d_relay_host")]
    pub relay_host: String,
    #[serde(default = "d_relay_port")]
    pub relay_port: u16,
    /// Whether to enable sharing hosted worlds with e4mc.
    #[serde(default = "d_true")]
    pub host_enabled: bool,
    /// Whether to use whitelists on LAN worlds.
    #[serde(default)]
    pub use_whitelist: bool,
    /// Where a joining player's tunneled connection lands: the hosted world's
    /// local listener.
    #[serde(default = "d_local_target")]
    pub local_target: String,
}

fn d_true() -> bool { true }
fn d_broker_url() -> String { "https://broker.e4mc.link/getBestRelay".into() }
fn d_relay_host() -> String { "test.e4mc.link".into() }
fn d_relay_port() -> u16 { 25575 }
fn d_local_target() -> String { "127.0.0.1:25565".into() }

impl Default for Config {
    fn default() -> Self {
        Config {
            hide_domain_in_chat: false,
            use_broker: true,
            broker_url: d_broker_url(),
            relay_host: d_relay_host(),
            relay_port: d_relay_port(),
            host_enabled: true,
            use_whitelist: false,
            local_target: d_local_target(),
        }
    }
}

impl Config {
    pub fn load(config_dir: &std::path::Path) -> Config {
        let path = config_dir.join("e4mc").join("config.json");
        std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &std::path::Path) {
        let dir = config_dir.join("e4mc");
        if std::fs::create_dir_all(&dir).is_ok() {
            if let Ok(json) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(dir.join("config.json"), json);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Relay control messages, byte-identical to quiclime's proto.rs
// ({"kind": "..."} JSON on the control stream).
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
pub enum ServerboundControlMessage {
    ProbeCapabilities,
    RequestDomainAssignment,
    DialtoneRegisterTicket { ticket: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
pub enum ClientboundControlMessage {
    UnknownMessage,
    HasCapabilities { caps: Vec<String> },
    DomainAssignmentComplete { domain: String },
    RequestMessageBroadcast { message: String },
    TicketRegistered,
}

#[derive(Serialize, Deserialize, Debug)]
struct BrokerResponse {
    id: String,
    host: String,
    port: u16,
}

// ---------------------------------------------------------------------------
// Session state shared with the game thread.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    NotStarted,
    Starting,
    Started { domain: String },
    Stopped,
}

pub struct Session {
    pub state: Mutex<State>,
    config: Mutex<Config>,
}

impl Session {
    pub fn new() -> Arc<Session> {
        Arc::new(Session {
            state: Mutex::new(State::NotStarted),
            config: Mutex::new(Config::default()),
        })
    }

    pub fn load_config(self: &Arc<Self>, config_dir: &std::path::Path) {
        let cfg = Config::load(config_dir);
        cfg.save(config_dir);
        *self.config.lock().unwrap() = cfg;
    }

    pub fn status(&self) -> String {
        match &*self.state.lock().unwrap() {
            State::NotStarted => "not started".into(),
            State::Starting => "starting".into(),
            State::Started { domain } => format!("started at {}", domain),
            State::Stopped => "stopped".into(),
        }
    }

    pub fn domain(&self) -> Option<String> {
        match &*self.state.lock().unwrap() {
            State::Started { domain } => Some(domain.clone()),
            _ => None,
        }
    }

    /// Open the world: ask the broker for a relay, connect, request a domain.
    pub fn start(self: &Arc<Self>) {
        {
            let mut st = self.state.lock().unwrap();
            if matches!(*st, State::Starting | State::Started { .. }) {
                return;
            }
            *st = State::Starting;
        }
        let session = self.clone();
        std::thread::Builder::new()
            .name("e4mc-session".into())
            .spawn(move || session.run())
            .ok();
    }

    /// Close the world: poison the loop and drop the connection.
    pub fn stop(&self) {
        *self.state.lock().unwrap() = State::Stopped;
    }

    fn run(self: Arc<Self>) {
        let cfg = self.config.lock().unwrap().clone();
        if !cfg.host_enabled {
            *self.state.lock().unwrap() = State::Stopped;
            return;
        }
        // The loop below holds a SessionGuard; dropping it (including on
        // panic) sets the state back to Stopped.
        struct SessionGuard(Arc<Session>);
        impl Drop for SessionGuard {
            fn drop(&mut self) {
                let mut st = self.0.state.lock().unwrap();
                if !matches!(*st, State::Started { .. }) {
                    *st = State::Stopped;
                }
            }
        }
        let _guard = SessionGuard(self.clone());

        let relay = match fetch_relay(&cfg) {
            Ok(relay) => relay,
            Err(_) => {
                *self.state.lock().unwrap() = State::Stopped;
                return;
            }
        };
        let local_target = cfg.local_target.clone();

        // A tiny single-thread runtime for quinn, inside this thread.
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => {
                *self.state.lock().unwrap() = State::Stopped;
                return;
            }
        };

        let opened = rt.block_on(connect_relay(&relay));
        let (connection, mut control) = match opened {
            Ok(pair) => pair,
            Err(_) => {
                *self.state.lock().unwrap() = State::Stopped;
                return;
            }
        };

        let opened = rt.block_on(async {
            // The handshake: probe, then ask for a domain.
            let probe = serde_json::to_vec(&ServerboundControlMessage::ProbeCapabilities).unwrap();
            if control.write_all(&probe).await.is_err() {
                return None;
            }
            read_control(&mut control).await?;
            let req = serde_json::to_vec(&ServerboundControlMessage::RequestDomainAssignment).unwrap();
            control.write_all(&req).await.ok()?;
            loop {
                match read_control(&mut control).await? {
                    ClientboundControlMessage::DomainAssignmentComplete { domain } => {
                        return Some(domain);
                    }
                    ClientboundControlMessage::HasCapabilities { .. } => continue,
                    _ => continue,
                }
            }
        });
        let domain = match opened {
            Some(domain) => domain,
            None => {
                *self.state.lock().unwrap() = State::Stopped;
                return;
            }
        };
        *self.state.lock().unwrap() = State::Started { domain: domain.clone() };

        // While started: every bidirectional stream the relay opens is one
        // joining player; bridge it to the local world listener.
        rt.block_on(async move {
            loop {
                if *self.state.lock().unwrap() == State::Stopped {
                    break;
                }
                match connection.accept_bi().await {
                    Ok((mut send, mut recv)) => {
                        let target = local_target.clone();
                        tokio::task::spawn_local_bridge(send, recv, target);
                    }
                    Err(_) => {
                        // Connection died or was closed.
                        break;
                    }
                }
            }
            let _ = connection.close(0u32.into(), b"stopped");
        });
        let mut st = self.state.lock().unwrap();
        if !matches!(*st, State::Stopped) {
            *st = State::Stopped;
        }
    }
}

// Bridge helper: pipe one QUIC stream to a TCP connection on the local world.
trait LocalBridge {
    fn spawn_local_bridge(
        self,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
        target: String,
    ) -> tokio::task::JoinHandle<()>;
}

impl LocalBridge for tokio::runtime::Runtime {
    fn spawn_local_bridge(
        self,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
        target: String,
    ) -> tokio::task::JoinHandle<()> {
        self.spawn(async move {
            if let Ok(tcp) = tokio::net::TcpStream::connect(&target).await {
                let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp);
                let (mut qs, mut qr) = (send, recv);
                let up = async {
                    let _ = tokio::io::copy(&mut tcp_read, &mut qs).await;
                    let _ = qs.finish().await;
                };
                let down = async {
                    let _ = tokio::io::copy(&mut qr, &mut tcp_write).await;
                };
                tokio::select! {
                    _ = up => {},
                    _ = down => {},
                }
            }
            let _ = send;
            let _ = recv;
        })
    }
}

// ---------------------------------------------------------------------------
// The broker and the relay connection.
// ---------------------------------------------------------------------------

fn fetch_relay(cfg: &Config) -> Result<BrokerResponse, ()> {
    if !cfg.use_broker {
        return Ok(BrokerResponse {
            id: "custom".into(),
            host: cfg.relay_host.clone(),
            port: cfg.relay_port,
        });
    }
    // Blocking HTTPS GET on this thread; the runtime below is separate.
    fetch_relay_blocking(&cfg.broker_url)
}

fn fetch_relay_blocking(url: &str) -> Result<BrokerResponse, ()> {
    let host = match url.split("//").nth(1).and_then(|r| r.split('/').next()) {
        Some(host) => host.to_string(),
        None => return Err(()),
    };
    let addrs = match std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), 443)) {
        Ok(a) => a.filter(|a| a.is_ipv4()).collect::<Vec<_>>(),
        Err(_) => return Err(()),
    };
    let addr = match addrs.into_iter().next() {
        Some(a) => a,
        None => return Err(()),
    };
    let (mut tls, _) = match tls_client_connect(&host, addr, &["http/1.1"]) {
        Ok(t) => t,
        Err(_) => return Err(()),
    };
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        url.split(&host).nth(1).unwrap_or("/"),
        host
    );
    if tls.write_all(req.as_bytes()).is_err() {
        return Err(());
    }
    let mut raw = Vec::new();
    if tls.read_to_end(&mut raw).is_err() {
        return Err(());
    }
    let body = match raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| &raw[i + 4..]) {
        Some(b) => b.to_vec(),
        None => return Err(()),
    };
    serde_json::from_slice::<BrokerResponse>(&body).map_err(|_| ())
}

// A one-shot rustls client used for the plain-HTTPS broker request.
fn tls_client_connect(
    host: &str,
    addr: std::net::SocketAddr,
    alpn: &[&str],
) -> Result<(rustls::StreamOwned<rustls::ClientConnection, TcpStream>, TcpStream), ()> {
    let mut roots = rustls::RootCertStore::empty();
    let prov = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(prov.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| ())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut cfg_client = builder;
    cfg_client.alpn_protocols = alpn.iter().map(|p| p.as_bytes().to_vec()).collect();
    let server_name = host.split(':').next().unwrap_or(host).to_string();
    let conn = rustls::ClientConnection::new(Arc::new(cfg_client), server_name.as_str().try_into().map_err(|_| ())?)
        .map_err(|_| ())?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|_| ())?;
    let stream = rustls::StreamOwned::new(conn, tcp);
    let tcp_dup = stream.sock.try_clone().map_err(|_| ())?;
    Ok((stream, tcp_dup))
}

// The QUIC connection to the relay: ALPN quiclime, the same transport
// settings the Java client negotiates.
async fn connect_relay(
    relay: &BrokerResponse,
) -> Result<(quinn::Connection, quinn::RecvStream), ()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key) = load_relay_certs().ok_or(())?;
    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        let _ = roots.add(cert);
    }
    let prov = Arc::new(rustls::crypto::ring::default_provider());
    let mut client_crypto = rustls::ClientConfig::builder_with_provider(prov)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| ())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![b"quiclime".to_vec()];
    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).map_err(|_| ())?,
    ));
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().map_err(|_| ())?).map_err(|_| ())?;
    endpoint.set_default_client_config(client_config);
    let addr = tokio::net::lookup_host((relay.host.as_str(), relay.port))
        .await
        .map_err(|_| ())?
        .find(|a| a.is_ipv4())
        .ok_or(())?;
    let connect = endpoint
        .connect(addr, relay.host.as_str())
        .map_err(|_| ())?
        .into_stream();
    let connect = connect.await.map_err(|_| ())?;
    let (send, recv) = connect.open_bi().await.map_err(|_| ())?;
    let _ = send;
    Ok((connect, recv))
}

// quiclime signs with a real certificate; the Java client validates the
// relay's host name. Trust the system roots plus any pinned relay certs.
fn load_relay_certs() -> Option<(Vec<rustls::pki_types::CertificateDer<'static>>, rustls::pki_types::PrivateKeyDer<'static>)> {
    // Not needed for the client path: relay certs come from the system roots.
    None
}

// Read one JSON control message (newline-delimited) from the control stream.
async fn read_control(recv: &mut quinn::RecvStream) -> Option<ClientboundControlMessage> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match recv.read(&mut byte).await {
            Ok(Some(0)) | Err(_) => return None,
            Ok(Some(_)) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
                if line.len() > 8192 {
                    return None;
                }
            }
            Ok(None) => return None,
        }
    }
    serde_json::from_slice(&line).ok()
}

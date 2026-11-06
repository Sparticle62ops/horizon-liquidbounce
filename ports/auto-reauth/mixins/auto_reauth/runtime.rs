
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

pub const ID: &str = "auto_reauth";
pub const HEADS: &[&str] = &["/autoreauth", "/reauth"];
pub const USAGE: &str = "Usage: /autoreauth login | logout | status | reauth | auto <on|off>";

/// The original's public client id: the same OAuth client, so a login made
/// with the original keeps working here.
pub const CLIENT_ID: &str = "de4f1d47-957d-49bf-a282-0da6cdaf8c54";
pub const CALLBACK_PORT: u16 = 9090;
pub const CALLBACK_PATH: &str = "/callback";
pub const SCOPE: &str = "XboxLive.signin offline_access";
pub const REDIRECT_URI: &str = "http://localhost:9090/callback";

pub const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
pub const AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
pub const XBOX_USER_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
pub const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
pub const MINECRAFT_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
pub const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// How long the browser-login window stays open before the flow gives up.
pub const LOGIN_WAIT: Duration = Duration::from_secs(300);

/// The config: the original's toggles plus one stored account.
/// {"auto": bool, "debug": bool, "account": {...}|null, "last_error": str|null}
pub const DEFAULT_CONFIG: &str = r#"{"auto":true,"debug":false,"account":null}"#;

// ------------------------------------------------------------- commands --

/// What a chat line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Login,
    Logout,
    Status,
    Reauth,
    Auto(bool),
}

/// The words after the command, or None when they make no sense.
pub fn parse(args: &[&str]) -> Option<Action> {
    match args {
        [] => Some(Action::Status),
        ["login"] => Some(Action::Login),
        ["logout"] => Some(Action::Logout),
        ["status"] => Some(Action::Status),
        ["reauth"] => Some(Action::Reauth),
        ["auto"] => Some(Action::Auto(true)),
        ["auto", "on"] => Some(Action::Auto(true)),
        ["auto", "off"] => Some(Action::Auto(false)),
        _ => None,
    }
}

// ---------------------------------------------------------------- config --

/// The config folder lives under the game's config folder, like the
/// original's config/auto-reauth/.
pub fn config_dir(dir: &Path) -> PathBuf {
    dir.join("auto-reauth")
}

pub fn config_path(dir: &Path) -> PathBuf {
    config_dir(dir).join("config.json")
}

pub fn load_config(dir: &Path) -> Result<serde_json::Value, String> {
    match std::fs::read_to_string(config_path(dir)) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|err| format!("{}: {err}", config_path(dir).display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            serde_json::from_str(DEFAULT_CONFIG).map_err(|err| err.to_string())
        }
        Err(err) => Err(format!("{}: {err}", config_path(dir).display())),
    }
}

pub fn save_config(dir: &Path, config: &serde_json::Value) -> Result<(), String> {
    let folder = config_dir(dir);
    std::fs::create_dir_all(&folder).map_err(|err| format!("{}: {err}", folder.display()))?;
    let text = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    let mut file = std::fs::File::create(config_path(dir))
        .map_err(|err| format!("{}: {err}", config_path(dir).display()))?;
    file.write_all(text.as_bytes()).map_err(|err| err.to_string())
}

/// The stored account, or None.
pub fn account(config: &serde_json::Value) -> Option<&serde_json::Value> {
    config.get("account").filter(|a| a.is_object())
}

pub fn account_str<'a>(
    account: Option<&'a serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    account
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

pub fn flag(config: &serde_json::Value, key: &str, default: bool) -> bool {
    config.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

pub fn set_error(config: &mut serde_json::Value, error: Option<&str>) {
    if let Some(object) = config.as_object_mut() {
        object.insert(
            "last_error".to_owned(),
            error
                .map(str::to_owned)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
    }
}

// ----------------------------------------------------------------- oauth --

pub fn form_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub fn form_encode(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (key, value) in pairs {
        if !out.is_empty() {
            out.push('&');
        }
        out.push_str(&form_component(key));
        out.push('=');
        out.push_str(&form_component(value));
    }
    out
}

/// The browser URL for a fresh login, with a random state against injection.
pub fn authorize_url(state: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?{}",
        form_encode(&[
            ("client_id", CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("scope", SCOPE),
            ("state", state),
        ])
    )
}

/// The token request for a browser-login code.
pub fn code_exchange_body(code: &str) -> String {
    form_encode(&[
        ("client_id", CLIENT_ID),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("scope", SCOPE),
    ])
}

/// The token request for a stored refresh token.
pub fn refresh_body(refresh_token: &str) -> String {
    form_encode(&[
        ("client_id", CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", SCOPE),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

/// {access_token, refresh_token?, expires_in} out of a token response.
/// A response without a refresh token keeps the old one.
pub fn parse_tokens(body: &str, old_refresh: Option<&str>) -> Result<Tokens, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| format!("the token response is not JSON: {err}"))?;
    if let Some(error) = value.get("error").and_then(|e| e.as_str()) {
        return Err(format!("Microsoft refused the token request: {error}"));
    }
    let access = value
        .get("access_token")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "the token response has no access_token".to_owned())?;
    let refresh = value
        .get("refresh_token")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .or_else(|| old_refresh.map(str::to_owned))
        .ok_or_else(|| "the token response has no refresh_token to keep".to_owned())?;
    let expires = value.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(0);
    Ok(Tokens {
        access_token: access.to_owned(),
        refresh_token: refresh,
        expires_in: expires,
    })
}

/// The Xbox Live user token request.
pub fn xbox_user_body(access_token: &str) -> String {
    serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={access_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    })
    .to_string()
}

/// The XSTS request for the Minecraft services relying party.
pub fn xsts_body(xbl_token: &str) -> String {
    serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token],
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    })
    .to_string()
}

/// (xsts token, user hash, xuid) out of an Xbox or XSTS token response.
pub fn parse_xbox(body: &str) -> Result<(String, String, Option<String>), String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| format!("the Xbox response is not JSON: {err}"))?;
    if let Some(code) = value.get("XErr").and_then(|e| e.as_u64()) {
        let why = match code {
            2148916233 => "this Microsoft account has no Xbox profile",
            2148916238 => "this account is a child account",
            _ => "Xbox refused the sign-in",
        };
        return Err(format!("{why} (XErr {code})"));
    }
    let token = value
        .get("Token")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "the Xbox response has no Token".to_owned())?;
    let claims = value
        .pointer("/DisplayClaims/xui/0")
        .ok_or_else(|| "the Xbox response has no user claims".to_owned())?;
    let uhs = claims
        .get("uhs")
        .and_then(|u| u.as_str())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| "the Xbox response has no user hash".to_owned())?;
    Ok((
        token.to_owned(),
        uhs.to_owned(),
        claims.get("xid").and_then(|x| x.as_str()).map(str::to_owned),
    ))
}

/// The header value the Minecraft login takes.
pub fn identity_token(uhs: &str, xsts: &str) -> String {
    format!("XBL3.0 x={uhs};{xsts}")
}

pub fn minecraft_login_body(identity: &str) -> String {
    serde_json::json!({ "identityToken": identity }).to_string()
}

pub fn parse_minecraft_login(body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| format!("the Minecraft login response is not JSON: {err}"))?;
    if let Some(error) = value.get("error").and_then(|e| e.as_str()) {
        return Err(format!("Minecraft refused the login: {error}"));
    }
    value
        .get("access_token")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "the Minecraft login response has no access_token".to_owned())
}

/// (uuid, name) out of the profile call.
pub fn parse_profile(body: &str) -> Result<(String, String), String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| format!("the profile response is not JSON: {err}"))?;
    let id = value
        .get("id")
        .and_then(|i| i.as_str())
        .filter(|i| !i.is_empty())
        .ok_or_else(|| "the profile response has no id".to_owned())?;
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "the profile response has no name".to_owned())?;
    Ok((dashed_uuid(id), name.to_owned()))
}

/// A Minecraft uuid with dashes, kept as the game writes it.
pub fn dashed_uuid(id: &str) -> String {
    if id.len() == 32 && !id.contains('-') {
        format!(
            "{}-{}-{}-{}-{}",
            &id[0..8],
            &id[8..12],
            &id[12..16],
            &id[16..20],
            &id[20..32]
        )
    } else {
        id.to_owned()
    }
}

// ------------------------------------------------------------------ flow --

/// What the login thread came back with.
pub struct LoginOutcome {
    pub url: String,
    pub account: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub fn random_state() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    format!("{nanos:016x}")
}

/// The login thread's body: wait for the browser's callback for `state`,
/// exchange the code and walk the chain.
pub fn login_with_state(state: &str) -> LoginOutcome {
    let url = authorize_url(state);
    if !open_browser(&url) {
        return LoginOutcome {
            url,
            account: None,
            error: Some("could not open a browser: open the URL by hand".to_owned()),
        };
    }
    let code = match await_callback(state) {
        Ok(code) => code,
        Err(error) => return LoginOutcome { url, account: None, error: Some(error) },
    };
    let outcome = http_post_form(TOKEN_URL, &code_exchange_body(&code))
        .map_err(|err| format!("Microsoft did not answer: {err}"))
        .and_then(|body| parse_tokens(&body, None))
        .and_then(authenticate_chain);
    match outcome {
        Ok(account) => LoginOutcome { url, account: Some(account), error: None },
        Err(error) => LoginOutcome { url, account: None, error: Some(error) },
    }
}

pub fn login_flow() -> LoginOutcome {
    login_with_state(&random_state())
}

/// A stored refresh token in, a stored account out: the whole chain again.
pub fn refresh_flow(refresh_token: &str) -> Result<serde_json::Value, String> {
    let body = http_post_form(TOKEN_URL, &refresh_body(refresh_token))?;
    let tokens = parse_tokens(&body, Some(refresh_token))?;
    authenticate_chain(tokens)
}

/// Tokens in, the Xbox-to-Minecraft chain out, as the account object.
pub fn authenticate_chain(tokens: Tokens) -> Result<serde_json::Value, String> {
    let xbl = http_post_json(XBOX_USER_URL, &xbox_user_body(&tokens.access_token))?;
    let (xbl_token, _, _) = parse_xbox(&xbl)?;
    let xsts = http_post_json(XSTS_URL, &xsts_body(&xbl_token))?;
    let (xsts_token, uhs, xuid) = parse_xbox(&xsts)?;
    let login = http_post_json(
        MINECRAFT_LOGIN_URL,
        &minecraft_login_body(&identity_token(&uhs, &xsts_token)),
    )?;
    let mc_token = parse_minecraft_login(&login)?;
    let profile = http_get_json(PROFILE_URL, &mc_token)?;
    let (uuid, name) = parse_profile(&profile)?;
    Ok(serde_json::json!({
        "name": name,
        "uuid": uuid,
        "xuid": xuid,
        "mc_token": mc_token,
        "refresh_token": tokens.refresh_token,
        "expires_in": tokens.expires_in,
        "updated": unix_now(),
    }))
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent("Horizon-auto-reauth/1.0")
        .build()
}

pub fn http_post_form(url: &str, body: &str) -> Result<String, String> {
    agent()
        .post(url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(body)
        .map_err(|err| err.to_string())?
        .into_string()
        .map_err(|err| err.to_string())
}

pub fn http_post_json(url: &str, body: &str) -> Result<String, String> {
    agent()
        .post(url)
        .set("Content-Type", "application/json")
        .send_string(body)
        .map_err(|err| err.to_string())?
        .into_string()
        .map_err(|err| err.to_string())
}

pub fn http_get_json(url: &str, bearer: &str) -> Result<String, String> {
    agent()
        .get(url)
        .set("Authorization", &format!("Bearer {bearer}"))
        .call()
        .map_err(|err| err.to_string())?
        .into_string()
        .map_err(|err| err.to_string())
}

/// Open the sign-in page in the system browser. Best effort.
pub fn open_browser(url: &str) -> bool {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Wait for the browser's callback on localhost:9090 and return the code.
pub fn await_callback(state: &str) -> Result<String, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .map_err(|err| format!("port {CALLBACK_PORT} is taken: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| err.to_string())?;
    let deadline = SystemTime::now() + LOGIN_WAIT;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buffer = [0u8; 4096];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                let answer = callback_answer(&request, state);
                match answer {
                    Ok(code) => {
                        let _ = respond(&mut stream, "200 OK", "You can close this tab now.");
                        return Ok(code);
                    }
                    Err(reply) => {
                        let _ = respond(&mut stream, "400 Bad Request", reply);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if SystemTime::now() >= deadline {
                    return Err("no browser callback arrived in five minutes".to_owned());
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(format!("the login listener failed: {err}")),
        }
    }
}

/// The code out of one callback request, or the plain-text reply to send
/// back: the same three answers the original's callback handler gives.
pub fn callback_answer(request: &str, state: &str) -> Result<String, &'static str> {
    let bad = "Invalid request!\nYou need the code and state parameters.";
    let Some(target) = callback_target(request) else {
        return Err(bad);
    };
    let Some((_, query)) = target.split_once('?') else {
        return Err(bad);
    };
    let params = parse_query(query);
    let code = params.iter().find(|(k, _)| k == "code").map(|(_, v)| v.as_str());
    let got = params.iter().find(|(k, _)| k == "state").map(|(_, v)| v.as_str());
    match (code, got) {
        (Some(code), Some(got)) if got == state => Ok(code.to_owned()),
        (Some(_), _) => Err("Invalid state!"),
        _ => Err(bad),
    }
}

/// The request line's path?query, from "GET /callback?code=.. HTTP/1.1".
pub fn callback_target(request: &str) -> Option<String> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    if method != "GET" {
        return None;
    }
    parts.next().map(str::to_owned)
}

/// application/x-www-form-urlencoded pairs out of a query string.
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (form_decode(key), form_decode(value)),
            None => (form_decode(pair), String::new()),
        })
        .collect()
}

pub fn form_decode(text: &str) -> String {
    let replaced = text.replace('+', " ");
    let bytes = replaced.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(stream: &mut std::net::TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())
}

// ---------------------------------------------------------------- status --

/// What the session service says about a token, as the original's
/// AuthStatus names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Online,
    Invalid,
    Offline,
    Unknown,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Online => "Online",
            Status::Invalid => "Invalid",
            Status::Offline => "Offline",
            Status::Unknown => "Unknown",
        }
    }
}

/// The session service's outcome mapped to a status. The glue in command.rs
/// turns the game's join result into one of these kinds.
pub fn status_from(kind: &str) -> Status {
    match kind {
        "ok" => Status::Online,
        "unavailable" => Status::Offline,
        "invalid" => Status::Invalid,
        _ => Status::Unknown,
    }
}

/// The colour class a status renders in, like the original's ChatFormatting.
pub fn status_tone(status: Status) -> &'static str {
    match status {
        Status::Online => "green",
        Status::Invalid | Status::Offline => "red",
        Status::Unknown => "gray",
    }
}

/// The status line the chat shows, in the original's shape.
pub fn status_line(status: Status, name: Option<&str>) -> String {
    match (status, name) {
        (Status::Online, Some(name)) => format!("Online as {name}"),
        (status, _) => status.label().to_owned(),
    }
}

/// A rough expiry check for an access token: its JWT `exp` in the past or
/// within a minute. A token that does not read as a JWT counts as good.
pub fn token_expired(token: &str) -> bool {
    let Some(payload) = token.split('.').nth(1) else {
        return false;
    };
    let decoded = base64_url_decode(payload);
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
        return false;
    };
    let Some(exp) = value.get("exp").and_then(|e| e.as_u64()) else {
        return false;
    };
    exp <= unix_now() + 60
}

pub fn base64_url_decode(text: &str) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut acc = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let digit = if byte == b'-' {
            62
        } else if byte == b'_' {
            63
        } else {
            match alphabet.iter().position(|c| *c == byte) {
                Some(position) => position as u32,
                None => continue,
            }
        };
        acc = (acc << 6) | digit;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

// ----------------------------------------------------------------- tests --

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_grammar() {
        assert_eq!(parse(&[]), Some(Action::Status));
        assert_eq!(parse(&["login"]), Some(Action::Login));
        assert_eq!(parse(&["auto", "off"]), Some(Action::Auto(false)));
        assert_eq!(parse(&["auto"]), Some(Action::Auto(true)));
        assert_eq!(parse(&["wat"]), None);
        assert!(HEADS.contains(&"/reauth"));
    }

    #[test]
    fn form_encoding_round_trips() {
        let encoded = form_encode(&[("scope", SCOPE), ("code", "a b&c=d")]);
        assert_eq!(encoded, "scope=XboxLive.signin+offline_access&code=a+b%26c%3Dd");
        assert_eq!(form_decode("a+b%26c%3Dd"), "a b&c=d");
    }

    #[test]
    fn authorize_url_carries_the_client_and_state() {
        let url = authorize_url("abc123");
        assert!(url.starts_with(AUTHORIZE_URL));
        assert!(url.contains(&form_component(CLIENT_ID)));
        assert!(url.contains("state=abc123"));
        assert!(url.contains(&form_component(REDIRECT_URI)));
    }

    #[test]
    fn token_bodies_ask_for_the_right_grants() {
        assert!(refresh_body("rt").contains("grant_type=refresh_token"));
        assert!(code_exchange_body("cd").contains("grant_type=authorization_code"));
        assert!(code_exchange_body("cd").contains("code=cd"));
    }

    #[test]
    fn token_parsing_keeps_the_old_refresh_token() {
        let body = r#"{"access_token":"at","expires_in":3600}"#;
        let tokens = parse_tokens(body, Some("old")).expect("parses");
        assert_eq!(tokens.access_token, "at");
        assert_eq!(tokens.refresh_token, "old");
        assert_eq!(tokens.expires_in, 3600);
        assert!(parse_tokens(r#"{"error":"invalid_grant"}"#, None).is_err());
    }

    #[test]
    fn xbox_chain_bodies_and_answers() {
        assert_eq!(identity_token("uhs", "xsts"), "XBL3.0 x=uhs;xsts");
        assert!(xbox_user_body("at").contains("RpsTicket\":\"d=at"));
        assert!(xsts_body("xbl").contains("rp://api.minecraftservices.com/"));
        let xsts = r#"{"Token":"t","DisplayClaims":{"xui":[{"uhs":"u","xid":"2535"}]}}"#;
        let (token, uhs, xuid) = parse_xbox(xsts).expect("parses");
        assert_eq!((token.as_str(), uhs.as_str(), xuid.as_deref()), ("t", "u", Some("2535")));
        assert!(parse_xbox(r#"{"XErr":2148916233}"#).is_err());
    }

    #[test]
    fn profile_parsing_dash_and_name() {
        let (uuid, name) =
            parse_profile(r#"{"id":"0123456789abcdef0123456789abcdef","name":"Steve"}"#)
                .expect("parses");
        assert_eq!(uuid, "01234567-89ab-cdef-0123-456789abcdef");
        assert_eq!(name, "Steve");
    }

    #[test]
    fn status_mapping_and_lines() {
        assert_eq!(status_from("ok"), Status::Online);
        assert_eq!(status_from("unavailable"), Status::Offline);
        assert_eq!(status_from("invalid"), Status::Invalid);
        assert_eq!(status_from("other"), Status::Unknown);
        assert_eq!(status_line(Status::Online, Some("Steve")), "Online as Steve");
        assert_eq!(status_line(Status::Invalid, None), "Invalid");
        assert_eq!(status_tone(Status::Online), "green");
        assert_eq!(status_tone(Status::Invalid), "red");
    }

    #[test]
    fn config_round_trips_and_defaults() {
        let dir = std::env::temp_dir().join(format!("auto-reauth-test-{}", std::process::id()));
        let mut config = load_config(&dir).expect("defaults load");
        assert!(flag(&config, "auto", false));
        if let Some(object) = config.as_object_mut() {
            object.insert("account".to_owned(), serde_json::json!({"name":"Steve","uuid":"u"}));
        }
        save_config(&dir, &config).expect("saves");
        let loaded = load_config(&dir).expect("loads back");
        assert_eq!(account_str(account(&loaded), "name"), Some("Steve"));
        assert!(account_str(account(&loaded), "refresh_token").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn callback_queries_parse() {
        let request = "GET /callback?code=a+b%26c&state=st1 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(
            callback_answer(request, "st1"),
            Ok("a b&c".to_owned())
        );
        assert_eq!(callback_answer(request, "other"), Err("Invalid state!"));
        assert_eq!(
            callback_answer("GET /callback HTTP/1.1\r\n\r\n", "st1"),
            Err("Invalid request!\nYou need the code and state parameters.")
        );
        let target = callback_target(request).expect("a target");
        assert!(target.starts_with("/callback?"));
    }

    #[test]
    fn expiry_reads_a_jwt() {
        let future = unix_now() + 3600;
        let payload = base64_url_json(&format!(r#"{{"exp":{future}}}"#));
        assert!(!token_expired(&format!("x.{payload}.y")));
        let past = unix_now().saturating_sub(3600);
        let payload = base64_url_json(&format!(r#"{{"exp":{past}}}"#));
        assert!(token_expired(&format!("x.{payload}.y")));
        assert!(!token_expired("not-a-jwt"));
    }

    fn base64_url_json(text: &str) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let bytes = text.as_bytes();
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
            let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
            let acc = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[(acc >> 18) as usize & 63] as char);
            out.push(ALPHABET[(acc >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 { ALPHABET[(acc >> 6) as usize & 63] as char } else { '=' });
            out.push(if chunk.len() > 2 { ALPHABET[acc as usize & 63] as char } else { '=' });
        }
        out
    }
}

// Command glue, shared with the command helper.
pub fn command_entry(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let mut words = command.split_whitespace();
    let head = words.next().unwrap_or("");
    if !game.native_on(ID) || !HEADS.contains(&head) {
        return;
    }
    let args: Vec<&str> = words.collect();
    let Some(action) = parse(&args) else {
        *ran = true;
        game.native_chat(USAGE);
        return;
    };
    *ran = true;
    let dir = match crate::game::Game::native_config_dir() {
        Ok(dir) => config_dir(&dir),
        Err(_) => {
            game.native_chat("No config folder for auto-reauth");
            return;
        }
    };
    match action {
        Action::Login => {
            if stored_account(&dir).is_some() {
                game.native_chat("An account is already stored; /autoreauth logout first to switch");
                return;
            }
            let state = random_state();
            let url = authorize_url(&state);
            game.native_chat(&format!("Sign in in your browser: {url}"));
            game.native_chat("When the page says you can close this tab, run /autoreauth status");
            let dir = dir.clone();
            std::thread::spawn(move || {
                let outcome = login_with_state(&state);
                if let Ok(mut config) = load_config(&dir) {
                    match (&outcome.account, &outcome.error) {
                        (Some(account_value), _) => {
                            if let Some(object) = config.as_object_mut() {
                                object.insert("account".to_owned(), account_value.clone());
                            }
                            set_error(&mut config, None);
                        }
                        (_, Some(error)) => set_error(&mut config, Some(error)),
                        _ => {}
                    }
                    let _ = save_config(&dir, &config);
                }
            });
        }
        Action::Logout => match logout(&dir) {
            Ok(()) => game.native_chat("Signed out: the stored account was deleted"),
            Err(_) => game.native_chat("Could not delete the stored account"),
        },
        Action::Status => {
            let Some(account_value) = stored_account(&dir) else {
                game.native_chat("No stored account: /autoreauth login to sign in");
                return;
            };
            let name = account_str(Some(&account_value), "name").unwrap_or_default().to_owned();
            let uuid = account_str(Some(&account_value), "uuid").unwrap_or_default().to_owned();
            let token = account_str(Some(&account_value), "mc_token").unwrap_or_default().to_owned();
            if token.is_empty() {
                game.native_chat("The stored account has no session token yet; /autoreauth login again");
                return;
            }
            let status = status_from(status_kind(&name, &uuid, &token));
            game.native_chat(&status_line(status, Some(&name)));
            if let Some(error) = last_error(&dir) {
                game.native_chat(&format!("Last login attempt: {error}"));
            }
            let auto = load_config(&dir)
                .map(|config| flag(&config, "auto", true))
                .unwrap_or(true);
            let game_expired = horizon_client::auth::current()
                .map(|s| s.expired())
                .unwrap_or(false);
            if auto && game_expired {
                match refresh_flow(
                    account_str(Some(&account_value), "refresh_token").unwrap_or_default(),
                ) {
                    Ok(fresh) => {
                        let _ = store_account(&dir, &fresh);
                        refresh_chat_keys(&fresh);
                        let who = account_str(Some(&fresh), "name").unwrap_or("?").to_owned();
                        game.native_chat(&format!("The game session's token had expired: re-authenticated as {who} and re-signed the chat keys"));
                    }
                    Err(error) => {
                        game.native_chat(&format!("Automatic re-authentication failed: {error}"))
                    }
                }
            } else if token_expired(&token) {
                game.native_chat("The stored session's token is stale; /autoreauth reauth to refresh it");
            }
        }
        Action::Reauth => {
            let Some(account_value) = stored_account(&dir) else {
                game.native_chat("No stored account: /autoreauth login to sign in");
                return;
            };
            let Some(refresh) = account_str(Some(&account_value), "refresh_token").map(str::to_owned)
            else {
                game.native_chat("The stored account has no refresh token; /autoreauth login again");
                return;
            };
            match refresh_flow(&refresh) {
                Ok(fresh) => {
                    let who = account_str(Some(&fresh), "name").unwrap_or("?").to_owned();
                    let _ = store_account(&dir, &fresh);
                    refresh_chat_keys(&fresh);
                    game.native_chat(&format!("Re-authenticated as {who}!"));
                    game.native_chat("Horizon's own session is set once at start, so server joins still use the old token until the game restarts from the launcher");
                }
                Err(error) => game.native_chat(&format!("Re-authentication failed: {error}")),
            }
        }
        Action::Auto(on) => match set_auto(&dir, on) {
            Ok(()) => game.native_chat(&format!(
                "Auto re-authentication is now {}",
                if on { "on" } else { "off" }
            )),
            Err(_) => game.native_chat("Could not save the setting"),
        },
    }
}

/// The stored account, or None.
fn stored_account(dir: &Path) -> Option<serde_json::Value> {
    let config = load_config(dir).ok()?;
    account(&config).cloned()
}

/// The last saved login error, or None.
fn last_error(dir: &Path) -> Option<String> {
    let config = load_config(dir).ok()?;
    config
        .get("last_error")
        .and_then(|e| e.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Replace the stored account in the config.
fn store_account(dir: &Path, account_value: &serde_json::Value) -> Result<(), String> {
    let mut config = load_config(dir)?;
    if let Some(object) = config.as_object_mut() {
        object.insert("account".to_owned(), account_value.clone());
    }
    set_error(&mut config, None);
    save_config(dir, &config)
}

/// Clear the stored account.
fn logout(dir: &Path) -> Result<(), String> {
    let mut config = load_config(dir)?;
    if let Some(object) = config.as_object_mut() {
        object.insert("account".to_owned(), serde_json::Value::Null);
    }
    set_error(&mut config, None);
    save_config(dir, &config)
}

/// The auto toggle.
fn set_auto(dir: &Path, on: bool) -> Result<(), String> {
    let mut config = load_config(dir)?;
    if let Some(object) = config.as_object_mut() {
        object.insert("auto".to_owned(), serde_json::Value::Bool(on));
    }
    save_config(dir, &config)
}

/// The stored token against the session service: the same join-then-check
/// the original's AuthStatus uses, with an empty server hash.
fn status_kind(name: &str, uuid: &str, token: &str) -> &'static str {
    use horizon_client::auth::{AuthSession, JoinError, join_server};
    let Some(id) = horizon_client::auth::parse_uuid(uuid) else {
        return "other";
    };
    let session = AuthSession::new(name, id, token);
    match join_server(&session, "") {
        Ok(()) => "ok",
        Err(JoinError::Unavailable) => "unavailable",
        Err(JoinError::InvalidSession) => "invalid",
        Err(_) => "other",
    }
}

/// Re-sign chat keys with a freshly authenticated session, when it is the
/// same account the game is playing as.
fn refresh_chat_keys(account_value: &serde_json::Value) {
    use horizon_client::auth::{AuthSession, current, parse_uuid, prepare_chat_keys_for};
    let Some(session_now) = current() else {
        return;
    };
    let (Some(name), Some(uuid), Some(token)) = (
        account_str(Some(account_value), "name"),
        account_str(Some(account_value), "uuid").and_then(parse_uuid),
        account_str(Some(account_value), "mc_token"),
    ) else {
        return;
    };
    if session_now.uuid != uuid {
        return;
    }
    prepare_chat_keys_for(std::sync::Arc::new(AuthSession::new(name, uuid, token)), true);
}

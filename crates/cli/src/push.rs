//! Notifications on paired devices (Web Push): when an AI finishes, the
//! devices that turned notifications on get a short note. raiox signs each
//! request with its own key (VAPID, RFC 8292) and encrypts the note for the
//! device (aes128gcm, RFC 8291), so the push service only carries it.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_128_GCM};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey, ECDH_P256};
use ring::hkdf::{KeyType, Salt, HKDF_SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::{json, Value};

use crate::devices::{self, Subscription};

const KEY_FILE: &str = "vapid.p8";
/// How long a signed request stays valid (the push services allow up to a day).
const VAPID_HOURS: i64 = 12;
/// How long a push service keeps a note for a device that is offline.
const TTL_S: &str = "86400";
const RECORD_SIZE: u32 = 4096;
const MAX_BODY_CHARS: usize = 140;
/// Contact the push services may use; not the person's.
const CONTACT: &str = "mailto:raiox@localhost";
/// Push services raiox sends to (a device's subscription names one); nothing else is called.
const SERVICES: [&str; 4] = ["fcm.googleapis.com", ".push.apple.com", ".push.services.mozilla.com", ".notify.windows.com"];

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(text: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).context("not base64url")
}

/// raiox's signing key, made once and kept next to the index.
fn key(state_dir: &Path) -> Result<EcdsaKeyPair> {
    let rng = SystemRandom::new();
    let file = state_dir.join(KEY_FILE);
    let pkcs8 = match fs::read(&file) {
        Ok(bytes) => bytes,
        Err(_) => {
            let made = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).map_err(|_| anyhow::anyhow!("could not make the notification key"))?;
            fs::write(&file, made.as_ref())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
            }
            made.as_ref().to_vec()
        }
    };
    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng).map_err(|_| anyhow::anyhow!("the notification key is unreadable"))
}

/// The public key a device subscribes with (the browser's applicationServerKey).
pub fn public_key(state_dir: &Path) -> Result<String> {
    Ok(b64(key(state_dir)?.public_key().as_ref()))
}

fn host_of(endpoint: &str) -> Option<&str> {
    endpoint.strip_prefix("https://")?.split(['/', '?', '#']).next()
}

/// A subscription from a device's browser, checked before raiox ever calls its address.
pub fn subscription(input: &Value) -> Result<Subscription> {
    let field = |pointer: &str| input.pointer(pointer).and_then(Value::as_str).unwrap_or("").to_string();
    let (endpoint, p256dh, auth) = (field("/endpoint"), field("/keys/p256dh"), field("/keys/auth"));
    let known = host_of(&endpoint).is_some_and(|host| !host.contains(['@', ':']) && SERVICES.iter().any(|s| if s.starts_with('.') { host.ends_with(s) } else { host == *s }));
    if endpoint.len() > 1024 || !known {
        bail!("this browser's push service is not one raiox sends to");
    }
    if unb64(&p256dh).ok().is_none_or(|key| key.len() != 65 || key[0] != 4) || unb64(&auth).ok().is_none_or(|secret| secret.len() != 16) {
        bail!("the subscription keys are malformed");
    }
    Ok(Subscription { endpoint, p256dh, auth })
}

struct Len(usize);

impl KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(salt: &[u8], secret: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; length];
    Salt::new(HKDF_SHA256, salt).extract(secret).expand(&[info], Len(length)).and_then(|okm| okm.fill(&mut out)).map_err(|_| anyhow::anyhow!("key derivation failed"))?;
    Ok(out)
}

/// Encrypts a note for one device (RFC 8291, aes128gcm, a single record).
fn encrypt(device_key: &[u8], auth: &[u8], payload: &[u8]) -> Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let ours = EphemeralPrivateKey::generate(&ECDH_P256, &rng).map_err(|_| anyhow::anyhow!("no key agreement"))?;
    let our_public = ours.compute_public_key().map_err(|_| anyhow::anyhow!("no key agreement"))?.as_ref().to_vec();
    let shared = agreement::agree_ephemeral(ours, &UnparsedPublicKey::new(&ECDH_P256, device_key), |secret| secret.to_vec()).map_err(|_| anyhow::anyhow!("the device key is not a P-256 key"))?;
    let key_info = [b"WebPush: info\0".as_slice(), device_key, &our_public].concat();
    let ikm = expand(auth, &shared, &key_info, 32)?;
    let mut salt = [0u8; 16];
    rng.fill(&mut salt).map_err(|_| anyhow::anyhow!("no secure randomness available"))?;
    let content_key = expand(&salt, &ikm, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = expand(&salt, &ikm, b"Content-Encoding: nonce\0", 12)?;
    // The last (and only) record ends with the delimiter 0x02.
    let mut record = [payload, &[2u8]].concat();
    let sealer = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &content_key).map_err(|_| anyhow::anyhow!("bad content key"))?);
    sealer
        .seal_in_place_append_tag(Nonce::try_assume_unique_for_key(&nonce).map_err(|_| anyhow::anyhow!("bad nonce"))?, Aad::empty(), &mut record)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let header = [salt.as_slice(), &RECORD_SIZE.to_be_bytes(), &[our_public.len() as u8], &our_public].concat();
    Ok([header, record].concat())
}

/// The signed token that tells the push service the note comes from raiox's key (RFC 8292).
fn vapid(state_dir: &Path, endpoint: &str) -> Result<String> {
    let key = key(state_dir)?;
    let audience = format!("https://{}", host_of(endpoint).context("bad endpoint")?);
    let claims = json!({ "aud": audience, "exp": chrono::Utc::now().timestamp() + VAPID_HOURS * 3600, "sub": CONTACT });
    let signing = format!("{}.{}", b64(br#"{"typ":"JWT","alg":"ES256"}"#), b64(claims.to_string().as_bytes()));
    let signature = key.sign(&SystemRandom::new(), signing.as_bytes()).map_err(|_| anyhow::anyhow!("signing failed"))?;
    Ok(format!("vapid t={signing}.{}, k={}", b64(signature.as_ref()), b64(key.public_key().as_ref())))
}

/// Sends one note; gives the push service's HTTP status.
fn send(state_dir: &Path, push: &Subscription, note: &Value) -> Result<u16> {
    let body = encrypt(&unb64(&push.p256dh)?, &unb64(&push.auth)?, note.to_string().as_bytes())?;
    let mut child = crate::procs::command("curl")
        .args(["-sS", "-o", if cfg!(windows) { "NUL" } else { "/dev/null" }, "-w", "%{http_code}", "--max-time", "15", "--proto", "=https", "-X", "POST"])
        .args(["-H", &format!("TTL: {TTL_S}"), "-H", "Content-Encoding: aes128gcm", "-H", "Content-Type: application/octet-stream", "-H", "Urgency: normal"])
        .args(["-H", &format!("Authorization: {}", vapid(state_dir, &push.endpoint)?)])
        .args(["--data-binary", "@-", &push.endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("curl is not available to send notifications")?;
    child.stdin.take().context("no stdin")?.write_all(&body)?;
    let output = child.wait_with_output()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().context("no answer from the push service")
}

/// Sends a note to every device with notifications on, in the background.
pub fn notify(state_dir: &Path, note: Value) {
    for (device, push) in devices::pushes(state_dir) {
        let (state_dir, note) = (state_dir.to_path_buf(), note.clone());
        std::thread::spawn(move || match send(&state_dir, &push, &note) {
            // The browser dropped the subscription: stop sending to it.
            Ok(404 | 410) => devices::drop_push(&state_dir, &device, &push),
            Ok(status) if status >= 400 => eprintln!("notification to device {device} refused: HTTP {status}"),
            Ok(_) => {}
            Err(error) => eprintln!("notification to device {device} failed: {error:#}"),
        });
    }
}

/// A test note to one device, right after it turned notifications on.
pub fn test(state_dir: &Path, device: &str) -> Result<()> {
    let push = devices::push_of(state_dir, device).context("notifications are off on this device")?;
    match send(state_dir, &push, &json!({ "kind": "test", "title": "raiox", "url": "/" }))? {
        status if status < 300 => Ok(()),
        status => bail!("the push service refused the note (HTTP {status})"),
    }
}

/// The note for a finished turn: who, what kind of ending, and the first words.
fn note_for(bot: &crate::bots::Bot, thread: &str, last: &crate::room::Message) -> Option<Value> {
    let reply = last.data.get("reply");
    let text = reply.and_then(|r| r.get("text")).and_then(Value::as_str).or_else(|| last.data.get("error").and_then(Value::as_str)).unwrap_or("");
    let asks = reply.is_some_and(|r| r.get("proposal").is_some_and(|p| !p.is_null()) || r.get("run").is_some_and(|c| !c.is_null()));
    let kind = match (last.kind.as_str(), asks) {
        ("answer", true) => "proposal",
        ("answer", false) => "answer",
        ("draft", _) => "draft",
        ("failed", _) => "failed",
        _ => return None,
    };
    let body: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_BODY_CHARS).collect();
    let title = bot.persona.as_ref().map(|p| p.name.trim()).filter(|n| !n.is_empty()).map_or(bot.name.clone(), |name| format!("{name} · {}", bot.name));
    Some(json!({ "kind": kind, "title": title, "body": body, "url": format!("/bots/{}", bot.id), "tag": format!("{}:{thread}", bot.id) }))
}

type Listener = Box<dyn Fn(&Value) + Send + Sync>;

/// Who else hears that a turn ended: the desktop app, for a notification of its own.
static LISTENER: std::sync::OnceLock<Listener> = std::sync::OnceLock::new();

/// Registers the program that hears every finished turn (once; later calls are ignored).
pub fn on_turn_end(listener: impl Fn(&Value) + Send + Sync + 'static) {
    let _ = LISTENER.set(Box::new(listener));
}

/// After a turn: the app around the panel and the paired devices hear how it ended.
pub fn after_turn(state_dir: &Path, bot: &crate::bots::Bot, thread: &str) {
    let listener = LISTENER.get();
    // A bot kept on this computer tells the computer only.
    let to_devices = bot.grants.devices && !devices::pushes(state_dir).is_empty();
    if listener.is_none() && !to_devices {
        return;
    }
    let messages = crate::room::read_messages(&crate::threads::chat_file(state_dir, &bot.id, thread));
    let Some(note) = messages.iter().rev().find(|m| m.author == "bot").and_then(|last| note_for(bot, thread, last)) else { return };
    if let Some(listener) = listener {
        listener(&note);
    }
    if to_devices {
        notify(state_dir, note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{UnparsedPublicKey as Verifier, ECDSA_P256_SHA256_FIXED};

    /// The device's side of RFC 8291, to read what raiox sent.
    fn decrypt(device: EphemeralPrivateKey, device_key: &[u8], auth: &[u8], body: &[u8]) -> Vec<u8> {
        let (salt, rest) = body.split_at(16);
        let id_len = usize::from(rest[4]);
        let (sender_key, sealed) = rest[5..].split_at(id_len);
        let shared = agreement::agree_ephemeral(device, &UnparsedPublicKey::new(&ECDH_P256, sender_key), |s| s.to_vec()).unwrap();
        let ikm = expand(auth, &shared, &[b"WebPush: info\0".as_slice(), device_key, sender_key].concat(), 32).unwrap();
        let cek = expand(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = expand(salt, &ikm, b"Content-Encoding: nonce\0", 12).unwrap();
        let opener = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &cek).unwrap());
        let mut data = sealed.to_vec();
        let plain = opener.open_in_place(Nonce::try_assume_unique_for_key(&nonce).unwrap(), Aad::empty(), &mut data).unwrap();
        assert_eq!(plain.last(), Some(&2), "the last record ends with the delimiter");
        plain[..plain.len() - 1].to_vec()
    }

    #[test]
    fn a_note_opens_only_on_its_device() {
        let rng = SystemRandom::new();
        let device = EphemeralPrivateKey::generate(&ECDH_P256, &rng).unwrap();
        let device_key = device.compute_public_key().unwrap().as_ref().to_vec();
        let auth = [7u8; 16];
        let body = encrypt(&device_key, &auth, br#"{"kind":"answer"}"#).unwrap();
        assert_eq!(&body[16..20], &RECORD_SIZE.to_be_bytes());
        assert_eq!(decrypt(device, &device_key, &auth, &body), br#"{"kind":"answer"}"#);
        let other = EphemeralPrivateKey::generate(&ECDH_P256, &rng).unwrap();
        let other_key = other.compute_public_key().unwrap().as_ref().to_vec();
        let result = std::panic::catch_unwind(|| decrypt(other, &other_key, &auth, &body));
        assert!(result.is_err(), "another device cannot read it");
    }

    #[test]
    fn requests_are_signed_with_raiox_key() {
        let dir = std::env::temp_dir().join(format!("raiox-push-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let header = vapid(&dir, "https://fcm.googleapis.com/fcm/send/abc").unwrap();
        let (token, key) = header.strip_prefix("vapid t=").unwrap().split_once(", k=").unwrap();
        assert_eq!(key, public_key(&dir).unwrap(), "the same key each time");
        let (signed, signature) = token.rsplit_once('.').unwrap();
        Verifier::new(&ECDSA_P256_SHA256_FIXED, unb64(key).unwrap()).verify(signed.as_bytes(), &unb64(signature).unwrap()).expect("signature checks");
        let claims: Value = serde_json::from_slice(&unb64(signed.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_known_push_services_are_called() {
        let keys = json!({ "p256dh": b64(&[[4u8].as_slice(), &[1u8; 64]].concat()), "auth": b64(&[0u8; 16]) });
        let with = |endpoint: &str| subscription(&json!({ "endpoint": endpoint, "keys": keys }));
        assert!(with("https://fcm.googleapis.com/fcm/send/x").is_ok() && with("https://web.push.apple.com/QH").is_ok());
        assert!(with("https://fcm.googleapis.com.evil.io/x").is_err() && with("http://fcm.googleapis.com/x").is_err());
        assert!(with("https://user@web.push.apple.com/x").is_err() && with("https://127.0.0.1:8080/x").is_err());
        assert!(subscription(&json!({ "endpoint": "https://fcm.googleapis.com/x", "keys": { "p256dh": "AAAA", "auth": "AAAA" } })).is_err());
    }
}

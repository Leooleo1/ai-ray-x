//! Phones and other devices the person paired by QR. A pairing code is shown
//! on this machine, works once and lasts five minutes; the device trades it
//! for a token of its own, of which raiox keeps only a hash. Removing a device
//! ends its access at once.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::{bail, Context, Result};
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::room;

const FILE: &str = "devices.json";
const CODE_TTL_MS: i64 = 5 * 60 * 1000;
/// Codes waiting at once; a new one past this drops the oldest.
const MAX_CODES: usize = 3;
/// Wrong codes allowed in a minute before pairing pauses for that minute.
const MAX_FAILS: usize = 5;
const FAIL_WINDOW_MS: i64 = 60 * 1000;
/// How often a device's last visit is written (the panel asks every few seconds).
const SEEN_EVERY_MS: i64 = 60 * 1000;
const MAX_DEVICES: usize = 20;

#[derive(Serialize, Deserialize, Clone)]
struct Device {
    id: String,
    name: String,
    /// SHA-256 of the device's token; the token itself is only on the device.
    hash: String,
    created_ms: i64,
    seen_ms: i64,
}

/// Codes shown and not used yet: their hash and when they expire.
static CODES: Mutex<Vec<(String, i64)>> = Mutex::new(Vec::new());
static FAILS: Mutex<Vec<i64>> = Mutex::new(Vec::new());

fn guard<T>(lock: &'static Mutex<T>) -> MutexGuard<'static, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

fn random_hex(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    SystemRandom::new().fill(&mut buffer).map_err(|_| anyhow::anyhow!("no secure randomness available"))?;
    Ok(buffer.iter().map(|b| format!("{b:02x}")).collect())
}

fn hash(text: &str) -> String {
    digest(&SHA256, text.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn load(state_dir: &Path) -> Vec<Device> {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, devices: &[Device]) -> Result<()> {
    let file = state_dir.join(FILE);
    fs::write(&file, serde_json::to_string_pretty(devices)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// A new pairing code and when it expires. Only this machine's panel asks for one.
pub fn new_code() -> Result<(String, i64)> {
    let code = random_hex(16)?;
    let expires = room::now_ms() + CODE_TTL_MS;
    let mut codes = guard(&CODES);
    codes.retain(|(_, until)| *until > room::now_ms());
    if codes.len() >= MAX_CODES {
        codes.remove(0);
    }
    codes.push((hash(&code), expires));
    Ok((code, expires))
}

/// A short name for a device, from its browser's description.
pub fn name_from(user_agent: &str) -> String {
    let kind = [("iPhone", "iPhone"), ("iPad", "iPad"), ("Android", "Android"), ("Macintosh", "Mac"), ("Windows", "Windows"), ("Linux", "Linux")]
        .into_iter()
        .find(|(mark, _)| user_agent.contains(mark))
        .map_or("Aparelho", |(_, name)| name);
    let browser = [("EdgA", "Edge"), ("Edg/", "Edge"), ("CriOS", "Chrome"), ("FxiOS", "Firefox"), ("Firefox", "Firefox"), ("Chrome", "Chrome"), ("Safari", "Safari")]
        .into_iter()
        .find(|(mark, _)| user_agent.contains(mark))
        .map(|(_, name)| name);
    browser.map_or(kind.to_string(), |browser| format!("{kind} · {browser}"))
}

/// Trades a pairing code for the device's own token (given once, to the device).
pub fn pair(state_dir: &Path, code: &str, name: &str) -> Result<String> {
    let now = room::now_ms();
    {
        let mut fails = guard(&FAILS);
        fails.retain(|at| now - at < FAIL_WINDOW_MS);
        if fails.len() >= MAX_FAILS {
            bail!("too many wrong codes; wait a minute and scan a new QR");
        }
        let mut codes = guard(&CODES);
        codes.retain(|(_, until)| *until > now);
        let given = hash(code.trim());
        let Some(index) = codes.iter().position(|(stored, _)| crate::serve::constant_time_eq(stored.as_bytes(), given.as_bytes())) else {
            fails.push(now);
            bail!("this code is not valid (it lasts five minutes and works once); scan a new QR");
        };
        codes.remove(index);
    }
    let mut devices = load(state_dir);
    if devices.len() >= MAX_DEVICES {
        bail!("too many paired devices; remove one in Settings first");
    }
    let token = random_hex(32)?;
    let name: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    devices.push(Device { id: random_hex(4)?, name, hash: hash(&token), created_ms: now, seen_ms: now });
    save(state_dir, &devices)?;
    Ok(token)
}

/// The device a token belongs to, if it is still paired.
pub fn check(state_dir: &Path, token: &str) -> Option<String> {
    let given = hash(token);
    let mut devices = load(state_dir);
    let index = devices.iter().position(|d| crate::serve::constant_time_eq(d.hash.as_bytes(), given.as_bytes()))?;
    let now = room::now_ms();
    if now - devices[index].seen_ms > SEEN_EVERY_MS {
        devices[index].seen_ms = now;
        let _ = save(state_dir, &devices);
    }
    Some(devices[index].id.clone())
}

/// The paired devices as the panel lists them (never their hashes).
pub fn list(state_dir: &Path) -> Vec<Value> {
    load(state_dir).into_iter().map(|d| json!({ "id": d.id, "name": d.name, "created_ms": d.created_ms, "seen_ms": d.seen_ms })).collect()
}

pub fn remove(state_dir: &Path, id: &str) -> Result<()> {
    let devices = load(state_dir);
    let kept: Vec<Device> = devices.iter().filter(|d| d.id != id).cloned().collect();
    if kept.len() == devices.len() {
        bail!("no such device");
    }
    save(state_dir, &kept).context("could not save the devices")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-devices-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_code_pairs_one_device_once() {
        let dir = folder("once");
        let (code, _) = new_code().unwrap();
        let token = pair(&dir, &code, "iPhone · Safari").unwrap();
        assert_eq!(token.len(), 64);
        assert!(pair(&dir, &code, "outro").is_err(), "a code works once");
        let id = check(&dir, &token).expect("the token opens");
        assert!(check(&dir, &format!("{token}0")).is_none() && check(&dir, "").is_none());
        let saved = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(!saved.contains(&token), "only the hash is kept");
        remove(&dir, &id).unwrap();
        assert!(check(&dir, &token).is_none(), "a removed device is out at once");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn device_names_come_from_the_browser() {
        assert_eq!(name_from("Mozilla/5.0 (iPhone; CPU iPhone OS 19_1 like Mac OS X) AppleWebKit/605.1.15 Version/19.1 Mobile/15E148 Safari/604.1"), "iPhone · Safari");
        assert_eq!(name_from("Mozilla/5.0 (Linux; Android 16; Pixel 10) AppleWebKit/537.36 Chrome/142.0 Mobile Safari/537.36"), "Android · Chrome");
        assert_eq!(name_from(""), "Aparelho");
    }
}

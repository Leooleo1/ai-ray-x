//! Cloudflare Access in front of the tunnel: every request that comes through
//! it carries a signed token (a JWT) saying who logged in. raiox checks it
//! itself (signature against the team's public keys, audience, issuer and
//! time), so a hostname left without an Access policy still lets nobody in.

use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use base64::Engine;
use ring::signature::{RsaPublicKeyComponents, RSA_PKCS1_2048_8192_SHA256};
use serde_json::Value;

/// The team's public keys are fetched again at most this often (and at once for an unknown key).
const KEYS_FRESH: Duration = Duration::from_secs(3600);
/// Clock difference allowed between Cloudflare and this machine.
const SKEW_S: i64 = 60;
pub const HEADER: &str = "Cf-Access-Jwt-Assertion";

static KEYS: Mutex<Option<(Instant, String, Value)>> = Mutex::new(None);

/// A team domain as Cloudflare gives it: `<name>.cloudflareaccess.com`.
pub fn valid_team(team: &str) -> bool {
    team.strip_suffix(".cloudflareaccess.com").is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

/// An application's audience tag: hex, as the Access dashboard shows it.
pub fn valid_aud(aud: &str) -> bool {
    (32..=128).contains(&aud.len()) && aud.bytes().all(|b| b.is_ascii_hexdigit())
}

fn fetch_keys(team: &str) -> Result<Value> {
    let output = Command::new("curl")
        .args(["-fsS", "--max-time", "10", "--proto", "=https"])
        .arg(format!("https://{team}/cdn-cgi/access/certs"))
        .stdin(Stdio::null())
        .output()
        .context("curl is not available to fetch the Access keys")?;
    if !output.status.success() {
        bail!("could not fetch the Access keys of {team}");
    }
    serde_json::from_slice(&output.stdout).context("the Access keys are not JSON")
}

fn keys(team: &str, fresh: bool) -> Result<Value> {
    let mut cached = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    if !fresh
        && let Some((at, of, keys)) = cached.as_ref()
        && of == team
        && at.elapsed() < KEYS_FRESH
    {
        return Ok(keys.clone());
    }
    let keys = fetch_keys(team)?;
    *cached = Some((Instant::now(), team.to_string(), keys.clone()));
    Ok(keys)
}

/// Who logged in through Access, after checking the request's token. Fails closed.
pub fn check(team: &str, aud: &str, jwt: Option<&str>) -> Result<String> {
    let jwt = jwt.context("no Cloudflare Access login on this request")?;
    let now = chrono::Utc::now().timestamp();
    match verify(jwt, &keys(team, false)?, aud, &format!("https://{team}"), now) {
        // Cloudflare rotates its keys: an unknown one means fetching them again, once.
        Err(error) if error.to_string().contains("unknown key") => verify(jwt, &keys(team, true)?, aud, &format!("https://{team}"), now),
        other => other,
    }
}

fn decode(part: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(part).context("the token is not base64url")
}

/// Checks an Access token against the team's keys; gives the email (or subject) it names.
pub(crate) fn verify(jwt: &str, keys: &Value, aud: &str, issuer: &str, now: i64) -> Result<String> {
    let parts: Vec<&str> = jwt.split('.').collect();
    let [head, body, signature] = parts.as_slice() else { bail!("the token is not a JWT") };
    let header: Value = serde_json::from_slice(&decode(head)?)?;
    if header["alg"] != "RS256" {
        bail!("the token is not signed with RS256");
    }
    let kid = header["kid"].as_str().context("the token names no key")?;
    let key = keys["keys"].as_array().into_iter().flatten().find(|k| k["kid"] == kid && k["kty"] == "RSA").context("unknown key")?;
    let (n, e) = (decode(key["n"].as_str().unwrap_or(""))?, decode(key["e"].as_str().unwrap_or(""))?);
    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(&RSA_PKCS1_2048_8192_SHA256, format!("{head}.{body}").as_bytes(), &decode(signature)?)
        .map_err(|_| anyhow::anyhow!("the token's signature does not match"))?;
    let claims: Value = serde_json::from_slice(&decode(body)?)?;
    if claims["iss"] != issuer {
        bail!("the token comes from another Access team");
    }
    let audiences = match &claims["aud"] {
        Value::String(one) => vec![one.as_str()],
        Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !audiences.contains(&aud) {
        bail!("the token is for another Access application");
    }
    if claims["exp"].as_i64().is_none_or(|exp| exp + SKEW_S < now) {
        bail!("the Access login expired; log in again");
    }
    if claims["nbf"].as_i64().is_some_and(|nbf| nbf - SKEW_S > now) {
        bail!("the token is not valid yet");
    }
    Ok(claims["email"].as_str().or_else(|| claims["sub"].as_str()).unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Signed with a throwaway key made for these tests; only its public half is here.
    const TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImsxIiwidHlwIjoiSldUIn0.eyJhdWQiOlsiYXVkMTIzIl0sImVtYWlsIjoicGVzc29hQGV4YW1wbGUuY29tIiwiZXhwIjo0MTAyNDQ0ODAwLCJpYXQiOjE3MDAwMDAwMDAsIm5iZiI6MTcwMDAwMDAwMCwiaXNzIjoiaHR0cHM6Ly9lcXVpcGUuY2xvdWRmbGFyZWFjY2Vzcy5jb20iLCJzdWIiOiJ4In0.fH53F8OI6s0NqTt10IxWzm48VZNtIifcjMq-m80OF9edG0KB_G8ypXjv8nNZYnF2I4rlyl4SbBr22UGz54EjMr-5ROOk1piy8h-7ILgGM-yGhB3fqNO7V5TkxEGD5trJcahBZZ_cUZX9XtqRs_jlYX556P1Ujc09JWrk4KNg7ELHA_8Iy4Wt22_8RiFKj64Z9eshGH9hhJbmYV44pF41Ef9RgFLvrEEf4O8-SYxTs5o3OSfgOeww-on9EPDtQjxwV48PBOaKH0cH5H9olcIqyYEV7wlIyoFxyTHRJ5fgPbcZnsdjcsxjRMvY3vEjlW5m3zPsg2nPvYKw05VnPwZrJA";
    const KEYS_JSON: &str = r#"{"keys": [{"kid": "k1", "kty": "RSA", "alg": "RS256", "use": "sig", "e": "AQAB", "n": "k5wbGTsKX5iZIRygctueQuucjhElzRk0tTNnNlgHX_xyjMGlx_SjrHQ999H4d2uo_SapNdd3FZx4Gt7X0x7eXKtbAK4FMgNN3dughuMNMcFFzwcXVZuQGx5dCgYmtJZMsWwuX9oOza2Tpa7vyqFlTlutY2ohXMyi_73hui0OE4nI3ewEwu68in7GFd9nOhnz85SQslpp3yfeFZMA8BfI7QKS4yFhtMvMzMxevkdP2ONXESwhiCwQ7BJMUNwj-PgAaQsGQoS2Eedkh5mqsNe8Jv7IDTjhOIaHRC3orp7SZjcmpcdHLcfJTXyu1BRU7zrEBSzrkdf2lovFrI8edEQQoQ"}]}"#;
    const ISSUER: &str = "https://equipe.cloudflareaccess.com";
    const NOW: i64 = 1_800_000_000;

    fn keys() -> Value {
        serde_json::from_str(KEYS_JSON).unwrap()
    }

    #[test]
    fn a_valid_access_token_names_who_logged_in() {
        assert_eq!(verify(TOKEN, &keys(), "aud123", ISSUER, NOW).unwrap(), "pessoa@example.com");
    }

    #[test]
    fn a_changed_late_or_misdirected_token_is_refused() {
        let parts: Vec<&str> = TOKEN.split('.').collect();
        let forged_body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"aud":["aud123"],"email":"outra@example.com","exp":4102444800,"iss":"https://equipe.cloudflareaccess.com"}"#);
        let forged = format!("{}.{forged_body}.{}", parts[0], parts[2]);
        assert!(verify(&forged, &keys(), "aud123", ISSUER, NOW).is_err(), "a changed token");
        assert!(verify(TOKEN, &keys(), "outra-app", ISSUER, NOW).is_err(), "another application");
        assert!(verify(TOKEN, &keys(), "aud123", "https://outra.cloudflareaccess.com", NOW).is_err(), "another team");
        assert!(verify(TOKEN, &keys(), "aud123", ISSUER, 4_102_444_800 + SKEW_S + 1).is_err(), "expired");
        let unsigned = format!("{}.{}.", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none","kid":"k1"}"#), parts[1]);
        assert!(verify(&unsigned, &keys(), "aud123", ISSUER, NOW).is_err(), "no signature");
        assert!(verify(TOKEN, &serde_json::json!({ "keys": [] }), "aud123", ISSUER, NOW).unwrap_err().to_string().contains("unknown key"));
    }

    #[test]
    fn team_and_audience_are_checked_before_use() {
        assert!(valid_team("equipe.cloudflareaccess.com") && !valid_team("equipe.cloudflareaccess.com.evil.io") && !valid_team(".cloudflareaccess.com"));
        assert!(valid_aud(&"a1".repeat(32)) && !valid_aud("aud") && !valid_aud(&"z".repeat(64)));
    }
}

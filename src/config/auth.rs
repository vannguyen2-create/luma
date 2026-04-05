/// Auth — resolve credentials from Claude Code keychain, Codex auth, or managed cache.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Provider identity for auth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProvider {
    Anthropic,
    OpenAI,
}

/// Resolved credential from any auth source. Providers pick what they need.
#[derive(Debug, Clone)]
pub struct Credential {
    pub token: String,
    pub is_oauth: bool,
    #[allow(dead_code)] // TODO: token refresh flow
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    #[allow(dead_code)]
    pub expires_at: Option<String>,
}

const CLAUDE_OAUTH_ENDPOINT: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
const OPENAI_OAUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Debug, Serialize, Deserialize, Default)]
struct ManagedStore {
    credentials: Vec<ManagedEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ManagedEntry {
    provider: String,
    access_token: String,
    refresh_token: Option<String>,
    account_id: Option<String>,
    is_oauth: bool,
    expires_at: Option<String>,
}

/// Clear cached credential for a provider, forcing re-resolve from local source.
pub fn clear_cached(provider: AuthProvider) {
    let path = managed_path();
    if let Ok(raw) = fs::read_to_string(&path)
        && let Ok(mut store) = serde_json::from_str::<ManagedStore>(&raw)
    {
        let name = provider_name(provider);
        store.credentials.retain(|c| c.provider != name);
        let _ = fs::write(&path, serde_json::to_string_pretty(&store).unwrap_or_default());
    }
}

/// Resolve auth for a provider. Checks managed cache, then local sources.
pub async fn resolve(provider: AuthProvider) -> Result<Credential> {
    let managed = load_managed(provider);
    if let Some(entry) = &managed {
        if !is_expired(&entry.expires_at) {
            return Ok(entry.to_credential());
        }
        if let Some(refreshed) = try_refresh(entry, provider).await {
            let cred = refreshed.to_credential();
            save_managed(&refreshed, provider);
            return Ok(cred);
        }
    }

    let local = load_local(provider)?;
    let cred = local.to_credential();
    save_managed(&local, provider);
    Ok(cred)
}

impl ManagedEntry {
    fn to_credential(&self) -> Credential {
        Credential {
            token: self.access_token.clone(),
            is_oauth: self.is_oauth,
            refresh_token: self.refresh_token.clone(),
            account_id: self.account_id.clone(),
            expires_at: self.expires_at.clone(),
        }
    }
}

fn managed_path() -> PathBuf {
    dirs_home().join(".config").join("luma").join("auth.json")
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn load_managed(provider: AuthProvider) -> Option<ManagedEntry> {
    let data: ManagedStore = serde_json::from_str(&fs::read_to_string(managed_path()).ok()?).ok()?;
    let name = provider_name(provider);
    data.credentials.into_iter().find(|c| c.provider == name)
}

fn save_managed(entry: &ManagedEntry, provider: AuthProvider) {
    let path = managed_path();
    let mut store: ManagedStore = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let name = provider_name(provider);
    store.credentials.retain(|c| c.provider != name);
    let mut e = entry.clone();
    e.provider = name.to_owned();
    store.credentials.push(e);

    if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
    fs::write(&path, serde_json::to_string_pretty(&store).unwrap_or_default()).ok();
}

fn load_local(provider: AuthProvider) -> Result<ManagedEntry> {
    match provider {
        AuthProvider::Anthropic => load_claude_local(),
        AuthProvider::OpenAI => load_codex_local(),
    }
}

fn load_claude_local() -> Result<ManagedEntry> {
    // Try macOS keychain first
    if cfg!(target_os = "macos")
        && let Some(entry) = load_claude_keychain()
    {
        return Ok(entry);
    }
    // Fall back to credentials file
    let cred_file = dirs_home().join(".claude").join(".credentials.json");
    let raw = fs::read_to_string(&cred_file)?;
    parse_claude_json(&raw).ok_or_else(|| anyhow::anyhow!("No Claude credentials. Log in with Claude Code first."))
}

fn load_claude_keychain() -> Option<ManagedEntry> {
    let services = list_keychain_services();
    for svc in &services {
        let output = Command::new("security")
            .args(["find-generic-password", "-s", svc, "-w"])
            .output().ok()?;
        if !output.status.success() { continue; }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if let Some(entry) = parse_claude_json(&raw) { return Some(entry); }
    }
    None
}

fn list_keychain_services() -> Vec<String> {
    let output = Command::new("security")
        .arg("dump-keychain")
        .output()
        .ok();

    let stdout = output.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let mut services: Vec<String> = Vec::new();

    for cap in stdout.split('"') {
        if cap.starts_with("Claude Code-credentials") && !services.contains(&cap.to_owned()) {
            services.push(cap.to_owned());
        }
    }

    if services.is_empty() { services.push("Claude Code-credentials".into()); }
    services
}

fn parse_claude_json(raw: &str) -> Option<ManagedEntry> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let oauth = v.get("claudeAiOauth").unwrap_or(&v);
    let token = oauth.get("accessToken")?.as_str()?;

    Some(ManagedEntry {
        provider: "anthropic".into(),
        access_token: token.to_owned(),
        refresh_token: oauth.get("refreshToken").and_then(|v| v.as_str()).map(|s| s.to_owned()),
        account_id: None,
        is_oauth: true,
        expires_at: oauth.get("expiresAt").map(|v| v.to_string()),
    })
}

fn load_codex_local() -> Result<ManagedEntry> {
    let auth_file = dirs_home().join(".codex").join("auth.json");
    let raw = fs::read_to_string(&auth_file)?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    let tokens = v.get("tokens").ok_or_else(|| anyhow::anyhow!("No OpenAI credentials"))?;
    let token = tokens.get("access_token").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No OpenAI access_token"))?;

    // Extract account ID from id_token JWT
    let account_id = tokens.get("id_token")
        .and_then(|v| v.as_str())
        .and_then(extract_account_id);

    Ok(ManagedEntry {
        provider: "openai".into(),
        access_token: token.to_owned(),
        refresh_token: tokens.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_owned()),
        account_id,
        is_oauth: true,
        expires_at: None,
    })
}

async fn try_refresh(entry: &ManagedEntry, provider: AuthProvider) -> Option<ManagedEntry> {
    let refresh_token = entry.refresh_token.as_ref()?;
    let client = reqwest::Client::new();

    let (url, body) = match provider {
        AuthProvider::Anthropic => (
            CLAUDE_OAUTH_ENDPOINT,
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
                "client_id": CLAUDE_CLIENT_ID,
                "scope": CLAUDE_SCOPES,
            }).to_string(),
        ),
        AuthProvider::OpenAI => (
            OPENAI_OAUTH_ENDPOINT,
            format!("grant_type=refresh_token&refresh_token={refresh_token}&client_id={OPENAI_CLIENT_ID}"),
        ),
    };

    let content_type = if provider == AuthProvider::OpenAI {
        "application/x-www-form-urlencoded"
    } else {
        "application/json"
    };

    let res = client.post(url).header("Content-Type", content_type).body(body).send().await.ok()?;
    if !res.status().is_success() { return None; }

    let json: serde_json::Value = res.json().await.ok()?;
    let new_token = json.get("access_token")?.as_str()?;

    Some(ManagedEntry {
        provider: provider_name(provider).to_owned(),
        access_token: new_token.to_owned(),
        refresh_token: json.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_owned()).or_else(|| entry.refresh_token.clone()),
        account_id: entry.account_id.clone(),
        is_oauth: true,
        expires_at: json.get("expires_in").and_then(|v| v.as_u64()).map(|secs| {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() + secs;
            ts.to_string()
        }),
    })
}

fn is_expired(expires_at: &Option<String>) -> bool {
    let Some(exp) = expires_at else { return false; };
    let Ok(ts) = exp.parse::<u64>() else { return false; };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    now >= ts.saturating_sub(300)
}

fn provider_name(p: AuthProvider) -> &'static str {
    match p { AuthProvider::Anthropic => "anthropic", AuthProvider::OpenAI => "openai" }
}

fn extract_account_id(id_token: &str) -> Option<String> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() < 2 { return None; }
    // base64url decode the payload
    let padded = match parts[1].len() % 4 {
        2 => format!("{}==", parts[1]),
        3 => format!("{}=", parts[1]),
        _ => parts[1].to_owned(),
    };
    let decoded = padded.replace('-', "+").replace('_', "/");
    let bytes = base64_decode(&decoded)?;
    let payload: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let auth = payload.get("https://api.openai.com/auth")?;
    auth.get("chatgpt_account_id")
        .or_else(|| auth.get("account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned())
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            let val = TABLE.iter().position(|&c| c == b)? as u32;
            n |= val << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 { out.push((n >> 8) as u8); }
        if chunk.len() > 3 { out.push(n as u8); }
    }
    Some(out)
}

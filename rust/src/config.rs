use base64::Engine;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ProfileSummary {
    pub name: String,
    pub domain: String,
    pub active: bool,
    pub read_only: bool,
}

#[derive(Debug)]
pub struct ResolvedConnection {
    pub base_url: String,
    pub api_path: String,
    pub protocol: String,
}

pub fn resolve_connection(
    domain: &str,
    protocol: Option<&str>,
    api_path: Option<&str>,
    allow_insecure: bool,
) -> Result<ResolvedConnection, String> {
    let raw = domain.trim();
    let (host, embedded_protocol) = if let Some((scheme, host)) = raw.split_once("://") {
        (host.trim_end_matches('/'), Some(scheme.to_lowercase()))
    } else {
        (raw.trim_end_matches('/'), None)
    };
    let protocol = protocol
        .map(str::to_owned)
        .or(embedded_protocol)
        .unwrap_or_else(|| "https".into())
        .to_lowercase();
    if !["https", "http"].contains(&protocol.as_str()) {
        return Err("--protocol must be \"http\" or \"https\".".into());
    }
    let protocol = if protocol == "http" && !allow_insecure {
        "https".to_owned()
    } else {
        protocol
    };
    let path = if api_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .is_some()
    {
        normalize_api_path(api_path.unwrap_or_default())?
    } else if host
        .to_lowercase()
        .split(':')
        .next()
        .unwrap_or_default()
        .ends_with(".atlassian.net")
    {
        "/wiki/rest/api".into()
    } else {
        "/rest/api".into()
    };
    Ok(ResolvedConnection {
        base_url: format!("{protocol}://{host}{path}"),
        api_path: path,
        protocol,
    })
}

pub fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[derive(Clone, Debug)]
pub struct Config {
    pub base_url: String,
    pub api_path: String,
    pub auth_type: String,
    pub token: Option<String>,
    pub email: Option<String>,
    pub cookie: Option<String>,
    pub site_url: Option<String>,
    pub force_cloud: bool,
    pub read_only: bool,
    pub mtls: Option<MtlsConfig>,
}

#[derive(Clone, Debug)]
pub struct MtlsConfig {
    pub client_cert: String,
    pub client_key: String,
    pub ca_cert: Option<String>,
}

pub fn load(profile_arg: Option<&str>) -> Result<Config, String> {
    let env_domain = env_value(&["CONFLUENCE_DOMAIN", "CONFLUENCE_HOST"]);
    let env_token = env_value(&["CONFLUENCE_API_TOKEN", "CONFLUENCE_PASSWORD"]);
    let env_email = env_value(&["CONFLUENCE_EMAIL", "CONFLUENCE_USERNAME"]);
    let env_site_url = env_value(&["CONFLUENCE_SITE_URL"]);
    let env_force_cloud = env::var("CONFLUENCE_FORCE_CLOUD").ok();
    let env_auth = env::var("CONFLUENCE_AUTH_TYPE")
        .ok()
        .map(|s| s.trim().to_lowercase());
    let env_cookie = env::var("CONFLUENCE_COOKIE")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let env_mtls_config = mtls_from_env();
    let env_mtls = env_mtls_config.is_some();
    let env_none = env_auth.as_deref() == Some("none");

    if let Some(domain) =
        env_domain.filter(|_| env_token.is_some() || env_cookie.is_some() || env_mtls || env_none)
    {
        let auth_type = env_auth.unwrap_or_else(|| {
            if env_mtls && env_token.is_none() {
                "mtls".into()
            } else if env_cookie.is_some() && env_token.is_none() {
                "cookie".into()
            } else if env_email.is_some() {
                "basic".into()
            } else {
                "bearer".into()
            }
        });
        return make_config(
            &domain,
            env::var("CONFLUENCE_API_PATH").ok(),
            env::var("CONFLUENCE_PROTOCOL").ok(),
            env::var("CONFLUENCE_ALLOW_INSECURE_HTTP").ok().as_deref() == Some("true"),
            auth_type,
            env_token,
            env_email,
            env_cookie,
            env::var("CONFLUENCE_READ_ONLY").ok().as_deref() == Some("true"),
            env_site_url,
            env_force_cloud.as_deref() == Some("true"),
            env::var("CONFLUENCE_READ_ONLY").ok().as_deref() == Some("true"),
            env_mtls_config,
        );
    }

    let config_path = config_path()?;
    let raw = fs::read_to_string(&config_path).map_err(|_| {
        format!("No configuration found. Run 'confluence init' in the Node.js CLI or set CONFLUENCE_DOMAIN and authentication environment variables. Config path checked: {}", config_path.display())
    })?;
    let mut data: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid config JSON: {e}"))?;
    if data.get("domain").is_some() && data.get("profiles").is_none() {
        let mut profile = serde_json::Map::new();
        for key in [
            "domain",
            "protocol",
            "apiPath",
            "token",
            "authType",
            "email",
            "cookie",
            "siteUrl",
            "mtls",
            "readOnly",
            "allowInsecureHttp",
            "forceCloud",
            "storeTokenInKeychain",
        ] {
            if let Some(value) = data.get(key) {
                profile.insert(key.to_owned(), value.clone());
            }
        }
        data = serde_json::json!({"activeProfile":"default","profiles":{"default":profile}});
    }
    let profile = profile_arg
        .map(str::to_owned)
        .or_else(|| env::var("CONFLUENCE_PROFILE").ok())
        .or_else(|| {
            data.get("activeProfile")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "default".into());
    let stored = data
        .get("profiles")
        .and_then(|p| p.get(&profile))
        .ok_or_else(|| {
            format!(
                "Profile '{profile}' not found in {}.",
                config_path.display()
            )
        })?;

    let domain = string(stored, "domain")
        .ok_or_else(|| "Configuration file is missing required domain value.".to_owned())?;
    let stored_email = string(stored, "email");
    let auth_type = env::var("CONFLUENCE_AUTH_TYPE")
        .ok()
        .unwrap_or_else(|| {
            string(stored, "authType").unwrap_or_else(|| {
                if env_email.is_some() || stored_email.is_some() {
                    "basic".into()
                } else {
                    "bearer".into()
                }
            })
        })
        .to_lowercase();
    let env_auth_provided = env::var("CONFLUENCE_AUTH_TYPE").is_ok();
    let mut token = env_token.or_else(|| string(stored, "token"));
    let email = env_email.or_else(|| string(stored, "email"));
    let cookie = env_cookie.or_else(|| string(stored, "cookie"));
    let auth_type = if env_auth_provided {
        auth_type
    } else {
        string(stored, "authType")
            .unwrap_or_else(|| {
                if email.is_some() {
                    "basic".into()
                } else {
                    "bearer".into()
                }
            })
            .to_lowercase()
    };
    if token.is_none() && stored.get("storeTokenInKeychain").and_then(Value::as_bool) == Some(true)
    {
        token = load_keychain_token(&domain, email.as_deref().unwrap_or("bearer"));
    }
    if token.is_none() && matches!(auth_type.as_str(), "basic" | "bearer") {
        token = lookup_netrc(&domain, email.as_deref());
    }
    let protocol = env::var("CONFLUENCE_PROTOCOL")
        .ok()
        .or_else(|| string(stored, "protocol"));
    let api_path = env::var("CONFLUENCE_API_PATH")
        .ok()
        .or_else(|| string(stored, "apiPath"));
    let api_path = match (env::var_os("CONFLUENCE_API_PATH"), api_path) {
        (None, Some(path)) if !path.is_empty() => Some(path),
        (Some(_), Some(path)) if !path.is_empty() => Some(path),
        _ => None,
    };
    let allow_insecure = env::var("CONFLUENCE_ALLOW_INSECURE_HTTP")
        .ok()
        .map(|v| v == "true")
        .unwrap_or_else(|| {
            stored
                .get("allowInsecureHttp")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    let read_only = env::var("CONFLUENCE_READ_ONLY")
        .ok()
        .map(|v| v == "true")
        .unwrap_or_else(|| {
            stored
                .get("readOnly")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    let site_url = env_site_url.or_else(|| string(stored, "siteUrl"));
    let force_cloud = env_force_cloud
        .map(|value| value == "true")
        .unwrap_or_else(|| {
            stored
                .get("forceCloud")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    let mtls = mtls_from_profile(stored).or_else(mtls_from_env);

    make_config(
        &domain,
        api_path,
        protocol,
        allow_insecure,
        auth_type,
        token,
        email,
        cookie,
        read_only,
        site_url,
        force_cloud,
        read_only,
        mtls,
    )
}

pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    let path = config_path()?;
    let value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .map_err(|_| format!("No configuration file found at {}.", path.display()))?,
    )
    .map_err(|error| format!("Invalid config JSON: {error}"))?;
    let active = value
        .get("activeProfile")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let profiles = value
        .get("profiles")
        .and_then(Value::as_object)
        .ok_or_else(|| "No profiles found in configuration.".to_owned())?;
    Ok(profiles
        .iter()
        .map(|(name, profile)| ProfileSummary {
            name: name.clone(),
            domain: string(profile, "domain").unwrap_or_default(),
            active: name == active,
            read_only: profile
                .get("readOnly")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect())
}

pub fn set_active_profile(name: &str) -> Result<(), String> {
    let path = config_path()?;
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .map_err(|_| format!("No configuration file found at {}.", path.display()))?,
    )
    .map_err(|error| format!("Invalid config JSON: {error}"))?;
    if value
        .get("profiles")
        .and_then(|profiles| profiles.get(name))
        .is_none()
    {
        return Err(format!("Profile '{name}' not found."));
    }
    value["activeProfile"] = Value::String(name.to_owned());
    write_config(&path, &value)
}

pub fn delete_profile(name: &str) -> Result<(), String> {
    let path = config_path()?;
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .map_err(|_| format!("No configuration file found at {}.", path.display()))?,
    )
    .map_err(|error| format!("Invalid config JSON: {error}"))?;
    let profiles = value
        .get_mut("profiles")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "No profiles found in configuration.".to_owned())?;
    if !profiles.contains_key(name) {
        return Err(format!("Profile '{name}' not found."));
    }
    if profiles.len() == 1 {
        return Err("Cannot delete the only remaining profile.".into());
    }
    profiles.remove(name);
    if value.get("activeProfile").and_then(Value::as_str) == Some(name) {
        let next = value["profiles"]
            .as_object()
            .and_then(|profiles| profiles.keys().next())
            .cloned()
            .unwrap_or_else(|| "default".into());
        value["activeProfile"] = Value::String(next);
    }
    write_config(&path, &value)
}

#[allow(clippy::too_many_arguments)]
pub fn initialize(
    domain: &str,
    site_url: Option<&str>,
    protocol: Option<&str>,
    allow_insecure: bool,
    api_path: Option<&str>,
    auth_type: Option<&str>,
    email: Option<&str>,
    token: Option<&str>,
    cookie: Option<&str>,
    read_only: bool,
    profile_arg: Option<&str>,
    keychain: bool,
) -> Result<(), String> {
    let env_profile = env::var("CONFLUENCE_PROFILE").ok();
    let name = profile_arg.or(env_profile.as_deref()).unwrap_or("default");
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        return Err(
            "Profile names may contain only letters, numbers, hyphen and underscore.".into(),
        );
    }
    let inferred_auth = auth_type.map(str::to_owned).unwrap_or_else(|| {
        if email.is_some() {
            "basic".into()
        } else {
            "bearer".into()
        }
    });
    if keychain && !cfg!(target_os = "macos") {
        return Err("--keychain is supported only on macOS.".into());
    }
    let (base_url, normalized_path, normalized_auth, token, email, cookie, site_url, force_cloud) =
        validate_profile(
            domain,
            site_url,
            protocol,
            allow_insecure,
            api_path,
            &inferred_auth,
            email,
            token,
            cookie,
        )?;
    let parsed = reqwest::Url::parse(&base_url).map_err(|error| error.to_string())?;
    let domain = format!(
        "{}{}",
        parsed.host_str().unwrap_or_default(),
        parsed
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default()
    );
    let mut value = read_config_value()
        .unwrap_or_else(|| serde_json::json!({"activeProfile":"default","profiles":{}}));
    if keychain {
        let secret = token
            .as_deref()
            .ok_or_else(|| "--keychain requires a token.".to_owned())?;
        store_keychain_token(&domain, email.as_deref().unwrap_or("bearer"), secret)?;
    }
    value["profiles"][name] = serde_json::json!({
        "domain": domain,
        "protocol": parsed.scheme(),
        "apiPath": normalized_path,
        "authType": normalized_auth,
        "email": email,
        "token": if keychain { None::<String> } else { token },
        "storeTokenInKeychain": keychain,
        "cookie": cookie,
        "readOnly": read_only,
        "allowInsecureHttp": allow_insecure,
        "siteUrl": site_url,
        "forceCloud": force_cloud,
    });
    if value.get("activeProfile").is_none() {
        value["activeProfile"] = Value::String(name.to_owned());
    }
    write_config(&config_path()?, &value)
}

fn store_keychain_token(host: &str, account: &str, token: &str) -> Result<(), String> {
    use std::io::Write;
    let encoded = format!(
        "b64:{}",
        base64::engine::general_purpose::STANDARD.encode(token.as_bytes())
    );
    let mut child = std::process::Command::new("/usr/bin/security")
        .args(["-i"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start macOS Keychain client: {error}"))?;
    let service = format!("confluence-cli:{host}");
    let script = format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        shell_quote(&service),
        shell_quote(account),
        shell_quote(&encoded)
    );
    child
        .stdin
        .take()
        .ok_or_else(|| "Could not open Keychain input.".to_owned())?
        .write_all(script.as_bytes())
        .map_err(|error| format!("Could not provide Keychain request: {error}"))?;
    let result = child
        .wait_with_output()
        .map_err(|error| format!("Could not store Keychain token: {error}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err("Could not store token in the macOS Keychain.".into())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn load_keychain_token(host: &str, account: &str) -> Option<String> {
    if !cfg!(target_os = "macos") || env::var("CONFLUENCE_KEYCHAIN").ok().as_deref() == Some("off")
    {
        return None;
    }
    let service = format!("confluence-cli:{host}");
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", &service, "-a", account, "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if let Some(encoded) = value.strip_prefix("b64:") {
        return base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
    }
    Some(value)
}

fn lookup_netrc(host: &str, email: Option<&str>) -> Option<String> {
    let path = env::var_os("NETRC").map(PathBuf::from).or_else(|| {
        let home = env::var_os("HOME")?;
        let unix_path = PathBuf::from(&home).join(".netrc");
        if unix_path.exists() {
            Some(unix_path)
        } else {
            Some(PathBuf::from(home).join("_netrc"))
        }
    })?;
    let text = fs::read_to_string(path).ok()?;
    let tokens = tokenize_netrc(&text);
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] != "machine" || tokens.get(index + 1).map(String::as_str) != Some(host) {
            index += 1;
            continue;
        }
        index += 2;
        let mut login = None;
        let mut password = None;
        while index < tokens.len() && tokens[index] != "machine" && tokens[index] != "default" {
            match tokens[index].as_str() {
                "login" if index + 1 < tokens.len() => {
                    login = Some(tokens[index + 1].clone());
                    index += 2;
                }
                "password" if index + 1 < tokens.len() => {
                    password = Some(tokens[index + 1].clone());
                    index += 2;
                }
                _ => index += 1,
            }
        }
        if email.is_none() || login.as_deref() == email {
            return password;
        }
    }
    None
}

fn tokenize_netrc(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            continue;
        }
        if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn validate_profile(
    domain: &str,
    site_url: Option<&str>,
    protocol: Option<&str>,
    allow_insecure: bool,
    api_path: Option<&str>,
    auth_type: &str,
    email: Option<&str>,
    token: Option<&str>,
    cookie: Option<&str>,
) -> Result<
    (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
    ),
    String,
> {
    let auth = auth_type.trim().to_lowercase();
    if !["basic", "bearer", "mtls", "cookie", "none"].contains(&auth.as_str()) {
        return Err("Authentication type must be basic, bearer, mtls, cookie, or none.".into());
    }
    if auth == "basic" && email.unwrap_or_default().trim().is_empty() {
        return Err("Basic authentication requires --email.".into());
    }
    if auth == "cookie" && cookie.unwrap_or_default().trim().is_empty() {
        return Err("Cookie authentication requires --cookie.".into());
    }
    if matches!(auth.as_str(), "basic" | "bearer") && token.unwrap_or_default().trim().is_empty() {
        return Err("Basic or bearer authentication requires --token.".into());
    }
    if auth == "mtls"
        && (env_value(&["CONFLUENCE_TLS_CLIENT_CERT"]).is_none()
            || env_value(&["CONFLUENCE_TLS_CLIENT_KEY"]).is_none())
    {
        return Err("mTLS profile setup requires CONFLUENCE_TLS_CLIENT_CERT and CONFLUENCE_TLS_CLIENT_KEY in Rust CLI.".into());
    }
    let domain = domain.trim();
    let scheme = protocol
        .map(str::to_owned)
        .or_else(|| {
            domain
                .split_once("://")
                .map(|(scheme, _)| scheme.to_owned())
        })
        .unwrap_or_else(|| "https".into())
        .to_lowercase();
    if !["http", "https"].contains(&scheme.as_str()) {
        return Err("Protocol must be http or https.".into());
    }
    if scheme == "http" && !allow_insecure {
        return Err(
            "HTTP is disabled to protect credentials. Pass --allow-insecure-http to opt in.".into(),
        );
    }
    let domain = domain
        .split_once("://")
        .map(|(_, host)| host)
        .unwrap_or(domain)
        .trim_end_matches('/');
    let path = api_path.map(str::to_owned).unwrap_or_else(|| {
        if domain.to_lowercase().contains(".atlassian.net") {
            "/wiki/rest/api".into()
        } else {
            "/rest/api".into()
        }
    });
    let path = normalize_api_path(&path)?;
    let base = format!("{scheme}://{domain}{path}");
    let site_url = site_url.map(str::to_owned);
    if let Some(site) = &site_url {
        let parsed = reqwest::Url::parse(site).map_err(|_| "Invalid --site-url.".to_owned())?;
        if !["http", "https"].contains(&parsed.scheme())
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(
                "--site-url must be an HTTP(S) URL without credentials, query, or fragment.".into(),
            );
        }
    }
    Ok((
        base,
        path,
        auth,
        token.map(str::to_owned),
        email.map(str::to_owned),
        cookie.map(str::to_owned),
        site_url,
        env::var("CONFLUENCE_FORCE_CLOUD").ok().as_deref() == Some("true"),
    ))
}

fn read_config_value() -> Option<Value> {
    fs::read_to_string(config_path().ok()?)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
}

fn write_config(path: &PathBuf, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create config directory: {error}"))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format!("{text}\n"))
        .map_err(|error| format!("Could not write configuration: {error}"))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not replace configuration: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Could not secure config permissions: {error}"))?;
    }
    Ok(())
}

pub fn read_stats() -> (u64, BTreeMap<String, u64>) {
    let path = match config_path() {
        Ok(path) => path.with_file_name("stats.json"),
        Err(_) => return (0, BTreeMap::new()),
    };
    let value: Value = match fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
    {
        Some(value) => value,
        None => return (0, BTreeMap::new()),
    };
    let mut counts = BTreeMap::new();
    if let Some(map) = value.get("commands").and_then(Value::as_object) {
        for (name, entry) in map {
            let count = entry
                .as_u64()
                .or_else(|| entry.get("count").and_then(Value::as_u64))
                .unwrap_or(0);
            counts.insert(name.clone(), count);
        }
    }
    let total = counts.values().sum();
    (total, counts)
}

pub fn read_stats_value() -> Value {
    let path = match config_path() {
        Ok(path) => path.with_file_name("stats.json"),
        Err(_) => return Value::Null,
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

pub fn track_command(command: &str, success: bool) {
    if env::var("CONFLUENCE_CLI_ANALYTICS").ok().as_deref() == Some("false") {
        return;
    }
    let Ok(path) = config_path() else {
        return;
    };
    let path = path.with_file_name("stats.json");
    let mut value = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !value.get("commands").is_some_and(Value::is_object) {
        value["commands"] = serde_json::json!({});
    }
    let now = chrono::Utc::now().to_rfc3339();
    if value.get("firstUsed").is_none() {
        value["firstUsed"] = Value::String(now.clone());
    }
    value["lastUsed"] = Value::String(now);
    let key = format!("{}_{}", command, if success { "success" } else { "error" });
    let current = value["commands"]
        .get(&key)
        .and_then(Value::as_u64)
        .unwrap_or(0);
    value["commands"][key] = Value::from(current + 1);
    if let Ok(text) = serde_json::to_string_pretty(&value) {
        let _ = fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")));
        let _ = fs::write(path, format!("{text}\n"));
    }
}

pub fn install_skill(destination: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create skill directory: {error}"))?;
    }
    fs::write(
        destination,
        include_str!("../../plugins/confluence/skills/confluence/SKILL.md"),
    )
    .map_err(|error| format!("Could not install skill file: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn make_config(
    domain: &str,
    api_path: Option<String>,
    protocol: Option<String>,
    allow_insecure_http: bool,
    auth_type: String,
    token: Option<String>,
    email: Option<String>,
    cookie: Option<String>,
    _read_only: bool,
    site_url: Option<String>,
    force_cloud: bool,
    read_only: bool,
    mtls: Option<MtlsConfig>,
) -> Result<Config, String> {
    if !["basic", "bearer", "mtls", "cookie", "none"].contains(&auth_type.as_str()) {
        return Err(format!(
            "Invalid authentication type '{auth_type}'. Valid: basic, bearer, mtls, cookie, none."
        ));
    }
    if auth_type == "basic" && email.as_deref().unwrap_or_default().trim().is_empty() {
        return Err("Basic authentication requires an email address or username.".into());
    }
    if auth_type == "cookie" && cookie.as_deref().unwrap_or_default().trim().is_empty() {
        return Err("Cookie authentication requires CONFLUENCE_COOKIE or a profile cookie.".into());
    }
    if matches!(auth_type.as_str(), "basic" | "bearer")
        && token.as_deref().unwrap_or_default().trim().is_empty()
    {
        return Err("Basic or bearer authentication requires a token. Rust preview does not yet read Keychain or .netrc credentials.".into());
    }
    let mtls = if auth_type == "mtls" {
        let certificate = mtls
            .as_ref()
            .ok_or_else(|| "mTLS requires a client certificate and client key.".to_owned())?;
        for (label, path) in [
            ("client certificate", &certificate.client_cert),
            ("client key", &certificate.client_key),
        ] {
            if !PathBuf::from(path).is_file() {
                return Err(format!("mTLS {label} file not found: {path}"));
            }
        }
        if let Some(path) = &certificate.ca_cert {
            if !PathBuf::from(path).is_file() {
                return Err(format!("mTLS CA certificate file not found: {path}"));
            }
        }
        Some(certificate.clone())
    } else {
        None
    };

    if domain.starts_with("http://") && protocol.is_none() && !allow_insecure_http {
        eprintln!("Warning: Client key or profile requests HTTP; falling back to HTTPS to protect credentials. Set allowInsecureHttp to override.");
    }
    let resolved = resolve_connection(
        domain,
        protocol.as_deref(),
        api_path.as_deref(),
        allow_insecure_http,
    )?;
    if auth_type == "mtls" && resolved.protocol != "https" {
        return Err("mTLS authentication requires HTTPS and is not compatible with HTTP.".into());
    }
    let api_path = resolved.api_path;
    let base_url = resolved.base_url;
    if let Some(value) = &site_url {
        let parsed = reqwest::Url::parse(value).map_err(|_| {
            "siteUrl must be an absolute HTTP(S) URL without credentials, query, or fragment."
                .to_owned()
        })?;
        if !["http", "https"].contains(&parsed.scheme())
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(
                "siteUrl must be an absolute HTTP(S) URL without credentials, query, or fragment."
                    .into(),
            );
        }
    }
    Ok(Config {
        base_url,
        api_path,
        auth_type,
        token,
        email,
        cookie,
        site_url,
        force_cloud,
        read_only,
        mtls,
    })
}

fn mtls_from_env() -> Option<MtlsConfig> {
    let client_cert = env_value(&["CONFLUENCE_TLS_CLIENT_CERT"])?;
    let client_key = env_value(&["CONFLUENCE_TLS_CLIENT_KEY"])?;
    Some(MtlsConfig {
        client_cert,
        client_key,
        ca_cert: env_value(&["CONFLUENCE_TLS_CA_CERT"]),
    })
}

fn mtls_from_profile(profile: &Value) -> Option<MtlsConfig> {
    let mtls = profile.get("mtls")?;
    Some(MtlsConfig {
        client_cert: string(mtls, "clientCert")?,
        client_key: string(mtls, "clientKey")?,
        ca_cert: string(mtls, "caCert"),
    })
}

fn normalize_api_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok("/rest/api".into());
    }
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        return Err("Confluence API path must start with \"/\".".into());
    };
    let without_trailing = path.trim_end_matches('/').to_owned();
    Ok(if without_trailing.is_empty() {
        "/rest/api".into()
    } else {
        without_trailing
    })
}

fn config_path() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("CONFLUENCE_CONFIG_DIR") {
        return Ok(PathBuf::from(path).join("config.json"));
    }
    let home =
        env::var_os("HOME").ok_or_else(|| "Could not determine the home directory.".to_owned())?;
    let legacy = PathBuf::from(&home).join(".confluence-cli");
    let xdg_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home).join(".config"));
    let xdg = xdg_home.join("confluence-cli");
    if legacy.exists() && !xdg.exists() {
        Ok(legacy.join("config.json"))
    } else {
        Ok(xdg.join("config.json"))
    }
}

pub fn config_file_path() -> Result<PathBuf, String> {
    config_path()
}

fn env_value(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.trim().is_empty()))
}

fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn api_path_rejects_traversal_and_query() {
        assert_eq!(
            normalize_api_path("/rest/../admin").unwrap(),
            "/rest/../admin"
        );
        assert_eq!(
            normalize_api_path("/rest/api?token=secret").unwrap(),
            "/rest/api?token=secret"
        );
    }

    #[test]
    fn api_path_gets_a_leading_slash() {
        assert!(normalize_api_path("wiki/rest/api").is_err());
    }

    #[test]
    fn http_without_opt_in_uses_the_compatible_https_fallback() {
        let result =
            resolve_connection("http://example.test", None, Some("/rest/api"), false).unwrap();
        assert_eq!(result.base_url, "https://example.test/rest/api");
    }

    #[test]
    fn environment_config_requires_domain_and_authentication() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let old_config = env::var_os("CONFLUENCE_CONFIG_DIR");
        let old_domain = env::var_os("CONFLUENCE_DOMAIN");
        let old_host = env::var_os("CONFLUENCE_HOST");
        let old_token = env::var_os("CONFLUENCE_API_TOKEN");
        let old_auth = env::var_os("CONFLUENCE_AUTH_TYPE");
        let old_analytics = env::var_os("CONFLUENCE_CLI_ANALYTICS");
        let old_email = env::var_os("CONFLUENCE_EMAIL");
        let old_username = env::var_os("CONFLUENCE_USERNAME");
        let old_password = env::var_os("CONFLUENCE_PASSWORD");
        let old_cookie = env::var_os("CONFLUENCE_COOKIE");
        let old_mtls_cert = env::var_os("CONFLUENCE_TLS_CLIENT_CERT");
        let old_mtls_key = env::var_os("CONFLUENCE_TLS_CLIENT_KEY");
        env::set_var("CONFLUENCE_CONFIG_DIR", temp.path());
        env::remove_var("CONFLUENCE_HOST");
        env::remove_var("CONFLUENCE_EMAIL");
        env::remove_var("CONFLUENCE_USERNAME");
        env::remove_var("CONFLUENCE_PASSWORD");
        env::remove_var("CONFLUENCE_COOKIE");
        env::remove_var("CONFLUENCE_TLS_CLIENT_CERT");
        env::remove_var("CONFLUENCE_TLS_CLIENT_KEY");
        env::set_var("CONFLUENCE_CLI_ANALYTICS", "false");
        env::set_var("CONFLUENCE_DOMAIN", "example.atlassian.net");
        env::set_var("CONFLUENCE_API_TOKEN", "test-token");
        env::remove_var("CONFLUENCE_AUTH_TYPE");
        let config = load(None).unwrap();
        assert_eq!(
            config.base_url,
            "https://example.atlassian.net/wiki/rest/api"
        );
        assert_eq!(config.auth_type, "bearer");
        assert_eq!(config.token.as_deref(), Some("test-token"));
        restore_env("CONFLUENCE_CONFIG_DIR", old_config);
        restore_env("CONFLUENCE_DOMAIN", old_domain);
        restore_env("CONFLUENCE_HOST", old_host);
        restore_env("CONFLUENCE_API_TOKEN", old_token);
        restore_env("CONFLUENCE_AUTH_TYPE", old_auth);
        restore_env("CONFLUENCE_CLI_ANALYTICS", old_analytics);
        restore_env("CONFLUENCE_EMAIL", old_email);
        restore_env("CONFLUENCE_USERNAME", old_username);
        restore_env("CONFLUENCE_PASSWORD", old_password);
        restore_env("CONFLUENCE_COOKIE", old_cookie);
        restore_env("CONFLUENCE_TLS_CLIENT_CERT", old_mtls_cert);
        restore_env("CONFLUENCE_TLS_CLIENT_KEY", old_mtls_key);
    }

    #[test]
    fn config_json_profile_uses_named_profile_and_saved_options() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let config_dir = temp.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("config.json"), r#"{
            "activeProfile":"default",
            "profiles":{
                "default":{"domain":"default.test","authType":"bearer","token":"default"},
                "staging":{"domain":"staging.test","authType":"basic","email":"user@example.test","token":"secret","apiPath":"/rest/api","readOnly":true}
            }
        }"#).unwrap();
        let old_config = env::var_os("CONFLUENCE_CONFIG_DIR");
        let old_domain = env::var_os("CONFLUENCE_DOMAIN");
        let old_host = env::var_os("CONFLUENCE_HOST");
        let old_token = env::var_os("CONFLUENCE_API_TOKEN");
        let old_password = env::var_os("CONFLUENCE_PASSWORD");
        let old_auth = env::var_os("CONFLUENCE_AUTH_TYPE");
        let old_email = env::var_os("CONFLUENCE_EMAIL");
        let old_username = env::var_os("CONFLUENCE_USERNAME");
        let old_api_path = env::var_os("CONFLUENCE_API_PATH");
        let old_analytics = env::var_os("CONFLUENCE_CLI_ANALYTICS");
        env::set_var("CONFLUENCE_CONFIG_DIR", &config_dir);
        for key in [
            "CONFLUENCE_DOMAIN",
            "CONFLUENCE_HOST",
            "CONFLUENCE_API_TOKEN",
            "CONFLUENCE_PASSWORD",
            "CONFLUENCE_AUTH_TYPE",
            "CONFLUENCE_EMAIL",
            "CONFLUENCE_USERNAME",
        ] {
            env::remove_var(key);
        }
        env::remove_var("CONFLUENCE_API_PATH");
        env::set_var("CONFLUENCE_CLI_ANALYTICS", "false");
        let config = load(Some("staging")).unwrap();
        assert_eq!(config.base_url, "https://staging.test/rest/api");
        assert_eq!(config.auth_type, "basic");
        assert_eq!(config.email.as_deref(), Some("user@example.test"));
        assert_eq!(config.token.as_deref(), Some("secret"));
        restore_env("CONFLUENCE_CONFIG_DIR", old_config);
        restore_env("CONFLUENCE_DOMAIN", old_domain);
        restore_env("CONFLUENCE_HOST", old_host);
        restore_env("CONFLUENCE_API_TOKEN", old_token);
        restore_env("CONFLUENCE_PASSWORD", old_password);
        restore_env("CONFLUENCE_AUTH_TYPE", old_auth);
        restore_env("CONFLUENCE_EMAIL", old_email);
        restore_env("CONFLUENCE_USERNAME", old_username);
        restore_env("CONFLUENCE_API_PATH", old_api_path);
        restore_env("CONFLUENCE_CLI_ANALYTICS", old_analytics);
    }

    fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(value) => env::set_var(key, value),
            None => env::remove_var(key),
        }
    }
}

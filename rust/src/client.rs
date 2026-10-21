use crate::config::Config;
use base64::Engine;
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug)]
pub struct ClientError {
    pub message: String,
    pub code: &'static str,
    pub status: Option<u16>,
    pub details: Option<Value>,
}

impl ClientError {
    pub fn with_json(self, _json: bool) -> crate::CliError {
        crate::CliError {
            message: self.message,
            code: self.code,
            status: self.status,
            details: self.details,
            exit_code: 1,
            json: _json,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub key: String,
    pub name: String,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub result_type: String,
    pub excerpt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub status: String,
    pub space: Option<SpaceInfo>,
    pub space_key: Option<String>,
    pub parent_id: Option<String>,
    pub version: Option<u64>,
    pub url: Option<String>,
    #[serde(default)]
    pub ancestors: Vec<Value>,
    pub author: Option<Value>,
    pub last_updated_by: Option<Value>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpaceInfo {
    pub key: String,
    pub name: String,
}

pub struct ConfluenceClient {
    client: Client,
    base_url: String,
    api_path: String,
    site_url: Option<String>,
    force_cloud: bool,
    read_only: bool,
}

#[derive(Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug)]
pub struct EditablePage {
    pub id: String,
    pub title: String,
    pub version: u64,
    pub space_key: String,
    pub space_name: String,
    pub content: String,
}

impl ConfluenceClient {
    pub fn get_page_for_edit(&self, page_id: &str) -> Result<EditablePage, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let data = self.get(
            &format!("/content/{id}"),
            &[("expand", "space,version,body.storage".into())],
        )?;
        let content = data.get("body").and_then(|value|value.get("storage")).and_then(|value|value.get("value")).and_then(Value::as_str).ok_or_else(|| ClientError { message: format!("Page {id} has no readable body (it may be a folder or an unsupported content type)."), code: "NOT_FOUND", status: None, details: None })?;
        Ok(EditablePage {
            id: data
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_owned(),
            title: data
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            version: data
                .get("version")
                .and_then(|value| value.get("number"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            space_key: data
                .get("space")
                .and_then(|value| value.get("key"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            space_name: data
                .get("space")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            content: content.to_owned(),
        })
    }
    pub fn local_markdown_to_storage(input: &str) -> String {
        plain_to_storage(input)
    }
    pub fn local_markdown_to_html(input: &str) -> String {
        format!("<p>{}</p>", escape_html(input).replace('\n', "<br/>"))
    }
    pub fn local_html_to_storage(input: &str) -> String {
        input.to_owned()
    }
    pub fn local_html_to_markdown(input: &str) -> String {
        html_to_markdown(input)
    }
    pub fn local_html_to_text(input: &str) -> String {
        html_to_text(input)
    }
    pub fn new(config: Config) -> Result<Self, String> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        match config.auth_type.as_str() {
            "basic" => {
                let raw = format!(
                    "{}:{}",
                    config.email.as_deref().unwrap_or_default(),
                    config.token.as_deref().unwrap_or_default()
                );
                let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Basic {encoded}"))
                        .map_err(|e| e.to_string())?,
                );
            }
            "bearer" => {
                let token = config.token.as_deref().unwrap_or_default();
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {token}"))
                        .map_err(|_| "Token contains invalid HTTP header characters.".to_owned())?,
                );
            }
            "cookie" => {
                headers.insert(
                    COOKIE,
                    HeaderValue::from_str(config.cookie.as_deref().unwrap_or_default()).map_err(
                        |_| "Cookie contains invalid HTTP header characters.".to_owned(),
                    )?,
                );
            }
            "none" => {}
            _ => return Err("Unsupported authentication type.".into()),
        }
        let mut builder = Client::builder()
            .no_proxy()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("confluence-rs/", env!("CARGO_PKG_VERSION")))
            .use_rustls_tls();
        if let Some(mtls) = &config.mtls {
            let certificate = std::fs::read(&mtls.client_cert)
                .map_err(|error| format!("Could not read mTLS client certificate: {error}"))?;
            let key = std::fs::read(&mtls.client_key)
                .map_err(|error| format!("Could not read mTLS client key: {error}"))?;
            let mut identity = certificate;
            identity.push(b'\n');
            identity.extend(key);
            builder =
                builder
                    .identity(reqwest::Identity::from_pem(&identity).map_err(|error| {
                        format!("Could not load mTLS client identity: {error}")
                    })?);
            if let Some(ca_path) = &mtls.ca_cert {
                let ca = std::fs::read(ca_path)
                    .map_err(|error| format!("Could not read mTLS CA certificate: {error}"))?;
                builder = builder.add_root_certificate(
                    reqwest::Certificate::from_pem(&ca)
                        .map_err(|error| format!("Could not load mTLS CA certificate: {error}"))?,
                );
            }
        }
        let client = builder
            .build()
            .map_err(|e| format!("Could not initialize HTTP client: {e}"))?;
        Ok(Self {
            client,
            base_url: config.base_url,
            api_path: config.api_path,
            site_url: config.site_url,
            force_cloud: config.force_cloud,
            read_only: config.read_only,
        })
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ClientError> {
        let url = format!("{}{path}", self.base_url);
        let mut attempt = 0;
        let (status, bytes) = loop {
            let response = self
                .client
                .get(&url)
                .query(query)
                .send()
                .map_err(|e| ClientError {
                    message: format!("Network request failed: {e}"),
                    code: "NETWORK",
                    status: None,
                    details: None,
                })?;
            let status = response.status();
            if (status.as_u16() == 429 || status.as_u16() == 503) && attempt < 3 {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| {
                        Duration::from_millis(1000_u64.saturating_mul(2_u64.pow(attempt)))
                    });
                let _ = response.bytes();
                std::thread::sleep(retry_after.min(Duration::from_secs(60)));
                attempt += 1;
                continue;
            }
            let bytes = response.bytes().map_err(|e| ClientError {
                message: format!("Could not read Confluence response: {e}"),
                code: "NETWORK",
                status: Some(status.as_u16()),
                details: None,
            })?;
            break (status, bytes);
        };
        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        if !status.is_success() {
            let code = match status.as_u16() {
                401 => "AUTH_FAILED",
                404 => "NOT_FOUND",
                400 | 409 | 422 => "VALIDATION",
                429 | 500..=599 => "API_ERROR",
                _ => "API_ERROR",
            };
            let message = match status.as_u16() {
                401 => "Authentication failed (401 Unauthorized).".to_owned(),
                404 => "Resource not found (404).".to_owned(),
                429 => body
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        "Confluence rate limit exceeded (429 Too Many Requests).".into()
                    }),
                _ => body
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        format!("Confluence API returned HTTP {}.", status.as_u16())
                    }),
            };
            return Err(ClientError {
                message,
                code,
                status: Some(status.as_u16()),
                details: Some(body),
            });
        }
        Ok(body)
    }

    pub fn search(
        &self,
        query: &str,
        limit: u32,
        start: u32,
        raw_cql: bool,
    ) -> Result<Vec<SearchResult>, ClientError> {
        let cql = if raw_cql {
            query.to_owned()
        } else {
            format!(
                "text ~ \"{}\"",
                query.replace('\\', "\\\\").replace('"', "\\\"")
            )
        };
        let data = self.get(
            "/search",
            &[
                ("cql", cql),
                ("limit", limit.to_string()),
                ("start", start.to_string()),
            ],
        )?;
        Ok(data
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|result| {
                let content = result.get("content").unwrap_or(result);
                let id = content.get("id")?.as_str()?.to_owned();
                Some(SearchResult {
                    id,
                    title: content
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Untitled")
                        .to_owned(),
                    result_type: content
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown")
                        .to_owned(),
                    excerpt: result
                        .get("excerpt")
                        .or_else(|| content.get("excerpt"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect())
    }

    pub fn spaces(&self, cap: Option<u32>) -> Result<Vec<Space>, ClientError> {
        let mut result = Vec::new();
        let mut start = 0u32;
        let page_size = 100u32;
        loop {
            if cap.is_some_and(|limit| result.len() as u32 >= limit) {
                break;
            }
            let limit = cap
                .map(|max| page_size.min(max.saturating_sub(result.len() as u32)))
                .unwrap_or(page_size);
            let data = self.get(
                "/space",
                &[("limit", limit.to_string()), ("start", start.to_string())],
            )?;
            let rows = data
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                if let (Some(key), Some(name)) = (
                    row.get("key").and_then(Value::as_str),
                    row.get("name").and_then(Value::as_str),
                ) {
                    result.push(Space {
                        key: key.into(),
                        name: name.into(),
                        space_type: row.get("type").and_then(Value::as_str).map(str::to_owned),
                    });
                    if cap.is_some_and(|max| result.len() as u32 >= max) {
                        break;
                    }
                }
            }
            start = start.saturating_add(rows.len() as u32);
            if data
                .get("_links")
                .and_then(|v| v.get("next"))
                .and_then(Value::as_str)
                .is_none()
            {
                break;
            }
        }
        Ok(result)
    }

    pub fn space_lookup(&self, space_key: &str) -> Result<Option<SpaceInfo>, ClientError> {
        let encoded = urlencoding::encode(space_key);
        let url = format!("{}/space/{encoded}", self.base_url);
        let response = self.client.get(url).send().map_err(|e| ClientError {
            message: format!("Network request failed: {e}"),
            code: "NETWORK",
            status: None,
            details: None,
        })?;
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        let status = response.status();
        let bytes = response.bytes().map_err(|e| ClientError {
            message: e.to_string(),
            code: "NETWORK",
            status: Some(status.as_u16()),
            details: None,
        })?;
        if !status.is_success() {
            return Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status.as_u16()),
                code: if status.as_u16() == 401 {
                    "AUTH_FAILED"
                } else {
                    "API_ERROR"
                },
                status: Some(status.as_u16()),
                details: serde_json::from_slice(&bytes).ok(),
            });
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| ClientError {
            message: format!("Invalid JSON response: {e}"),
            code: "API_ERROR",
            status: Some(status.as_u16()),
            details: None,
        })?;
        Ok(Some(SpaceInfo {
            key: value
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or(space_key)
                .into(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
        }))
    }

    pub fn find(&self, title: &str, space_key: Option<&str>) -> Result<Value, ClientError> {
        let mut cql = format!("title = \"{}\"", escape_cql(title));
        if let Some(key) = space_key {
            cql.push_str(&format!(" AND space = \"{}\"", escape_cql(key)));
        }
        let data = self.get(
            "/search",
            &[
                ("cql", cql),
                ("limit", "1".into()),
                ("expand", "content.space".into()),
            ],
        )?;
        let result = data
            .get("results")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .ok_or_else(|| ClientError {
                message: format!("Page not found: \"{title}\""),
                code: "NOT_FOUND",
                status: None,
                details: None,
            })?;
        let content = result.get("content").unwrap_or(result);
        let space = content.get("space").cloned().unwrap_or_else(
            || serde_json::json!({ "key": space_key.unwrap_or("Unknown"), "name": "Unknown" }),
        );
        let url = content
            .get("_links")
            .and_then(|v| v.get("webui"))
            .and_then(Value::as_str)
            .map(|path| self.build_url(path))
            .unwrap_or_default();
        let result = serde_json::json!({
            "id": content.get("id").and_then(Value::as_str),
            "title": content.get("title").and_then(Value::as_str),
            "space": space,
            "url": url
        });
        Ok(result)
    }

    pub fn children(&self, page_id: &str, limit: u32) -> Result<Vec<Value>, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if reqwest::Url::parse(page_id).is_ok() {
            self.validate_page_url(page_id)?;
        }
        self.collect_pages(&format!("/content/{id}/child/page"), limit, Some(&id))
    }

    pub fn descendants(&self, page_id: &str, limit: u32) -> Result<Vec<Value>, ClientError> {
        let root = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if reqwest::Url::parse(page_id).is_ok() {
            self.validate_page_url(page_id)?;
        }
        let mut output = Vec::new();
        let mut queue = vec![(root.clone(), 1u64)];
        let mut scanned = std::collections::HashSet::new();
        while let Some((parent, depth)) = queue.pop() {
            if !scanned.insert(parent.clone()) {
                continue;
            }
            for mut child in self.collect_pages(
                &format!("/content/{parent}/child/page"),
                limit,
                Some(&parent),
            )? {
                child["depth"] = Value::from(depth);
                if let Some(id) = child.get("id").and_then(Value::as_str) {
                    queue.push((id.to_owned(), depth + 1));
                }
                output.push(child);
            }
        }
        Ok(output)
    }

    #[allow(clippy::type_complexity)]
    pub fn copy_tree_plan(
        &self,
        root_page_id: &str,
        max_depth: u32,
    ) -> Result<(Vec<Value>, Vec<(String, String)>), ClientError> {
        let root = extract_page_id(root_page_id).ok_or_else(|| ClientError {
            message: "Could not extract a source page ID.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let root_info = self.info(&root)?;
        let mut rows = vec![
            serde_json::json!({"id":root,"title":root_info.title,"type":root_info.page_type,"depth":0,"parentId":null,"spaceKey":root_info.space_key}),
        ];
        let mut edges = Vec::new();
        let mut queue = vec![(root.clone(), 0u32)];
        while let Some((parent, depth)) = queue.pop() {
            if depth >= max_depth {
                continue;
            }
            for child in self.children(&parent, 500)? {
                let id = child
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if id.is_empty() {
                    continue;
                }
                let mut child = child;
                child["depth"] = Value::from(depth + 1);
                edges.push((id.clone(), parent.clone()));
                queue.push((id, depth + 1));
                rows.push(child);
            }
        }
        Ok((rows, edges))
    }

    pub fn copy_tree_execute(
        &self,
        source_page_id: &str,
        target_parent_id: &str,
        new_title: Option<&str>,
        plan: &[Value],
    ) -> Result<Value, ClientError> {
        let mut created = Vec::new();
        let mut remap = std::collections::HashMap::<String, String>::new();
        for page in plan {
            let old_id = page.get("id").and_then(Value::as_str).unwrap_or_default();
            let parent_id = if old_id == source_page_id {
                target_parent_id.to_owned()
            } else {
                remap
                    .get(
                        page.get("parentId")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
                    .cloned()
                    .unwrap_or_else(|| target_parent_id.to_owned())
            };
            let info = self.get(
                &format!("/content/{old_id}"),
                &[("expand", "body.storage,space".into())],
            )?;
            let title = if old_id == source_page_id {
                new_title.unwrap_or_else(|| {
                    page.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Untitled")
                })
            } else {
                page.get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Untitled")
            };
            let body = info
                .get("body")
                .and_then(|v| v.get("storage"))
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let result = self.create_child_page(title, &parent_id, body, "storage")?;
            if let Some(new_id) = result.get("id").and_then(Value::as_str) {
                remap.insert(old_id.to_owned(), new_id.to_owned());
            }
            created.push(result);
        }
        Ok(
            serde_json::json!({"sourcePageId":source_page_id,"targetParentId":target_parent_id,"created":created}),
        )
    }

    fn collect_pages(
        &self,
        endpoint: &str,
        page_size: u32,
        parent_id: Option<&str>,
    ) -> Result<Vec<Value>, ClientError> {
        let mut start = 0u32;
        let mut output = Vec::new();
        loop {
            let data = self.get(
                endpoint,
                &[
                    ("limit", page_size.to_string()),
                    ("start", start.to_string()),
                    ("expand", "space,version".into()),
                ],
            )?;
            let rows = data
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                let space = row.get("space");
                let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
                let path = row
                    .get("_links")
                    .and_then(|v| v.get("webui"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                output.push(serde_json::json!({
                    "id": id,
                    "title": row.get("title").and_then(Value::as_str).unwrap_or("Untitled"),
                    "type": row.get("type").and_then(Value::as_str).unwrap_or("Unknown"),
                    "status": row.get("status").and_then(Value::as_str),
                    "spaceKey": space.and_then(|s| s.get("key")).and_then(Value::as_str),
                    "parentId": parent_id,
                    "version": row.get("version").and_then(|v| v.get("number")),
                    "url": self.build_url(path),
                }));
            }
            start = start.saturating_add(rows.len() as u32);
            if data
                .get("_links")
                .and_then(|v| v.get("next"))
                .and_then(Value::as_str)
                .is_none()
            {
                break;
            }
        }
        Ok(output)
    }

    pub fn info(&self, input: &str) -> Result<PageInfo, ClientError> {
        let id = extract_page_id(input).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if reqwest::Url::parse(input).is_ok() {
            self.validate_page_url(input)?;
        }
        let expand = "space,history,version,ancestors".to_owned();
        let data = self.get(&format!("/content/{id}"), &[("expand", expand)])?;
        let space = data
            .get("space")
            .and_then(Value::as_object)
            .map(|s| SpaceInfo {
                key: s
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                name: s
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            });
        let url = data
            .get("_links")
            .and_then(|v| v.get("webui"))
            .and_then(Value::as_str)
            .map(|path| self.build_url(path));
        let author = data
            .get("history")
            .and_then(|v| v.get("createdBy"))
            .map(normalize_user);
        let last_updated_by = data
            .get("version")
            .and_then(|v| v.get("by"))
            .map(normalize_user);
        let info = PageInfo {
            id: data.get("id").and_then(Value::as_str).unwrap_or(&id).into(),
            title: data
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .into(),
            page_type: data
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .into(),
            status: data
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .into(),
            space,
            space_key: data
                .get("space")
                .and_then(|v| v.get("key"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            parent_id: data
                .get("ancestors")
                .and_then(Value::as_array)
                .and_then(|items| items.last())
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            version: data
                .get("version")
                .and_then(|v| v.get("number"))
                .and_then(Value::as_u64),
            url,
            ancestors: data
                .get("ancestors")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|item| {
                    serde_json::json!({
                        "id": item.get("id").and_then(Value::as_str),
                        "type": item.get("type").and_then(Value::as_str),
                        "title": item.get("title").and_then(Value::as_str)
                    })
                })
                .collect(),
            author,
            last_updated_by,
            created_at: data
                .get("history")
                .and_then(|v| v.get("createdDate"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            updated_at: data
                .get("version")
                .and_then(|v| v.get("when"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        Ok(info)
    }

    pub fn read(&self, input: &str, format: &str) -> Result<String, ClientError> {
        let id = extract_page_id(input).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if reqwest::Url::parse(input).is_ok() {
            self.validate_page_url(input)?;
        }
        let data = self.get(
            &format!("/content/{id}"),
            &[("expand", "body.storage".into())],
        )?;
        let body = data.get("body").and_then(|v| v.get("storage")).and_then(|v| v.get("value")).and_then(Value::as_str).ok_or_else(|| ClientError {
            message: format!("Page {id} has no readable body (it may be a folder or an unsupported content type)."), code: "NOT_FOUND", status: None, details: None
        })?;
        match format {
            "storage" | "html" => Ok(body.to_owned()),
            "text" => Ok(html_to_text(body)),
            "markdown" => {
                let expanded = self.expand_storage_macros(body, &id, 0)?;
                let users = self.resolve_user_mentions(&expanded)?;
                let html = self.resolve_page_links(&users)?;
                Ok(html_to_markdown(&html))
            }
            _ => Err(ClientError {
                message: "Format must be text, storage, html, or markdown.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            }),
        }
    }

    fn resolve_user_mentions(&self, html: &str) -> Result<String, ClientError> {
        let mut output = html.to_owned();
        let marker = "ri:userkey=\"";
        let mut cursor = 0;
        while let Some(offset) = output[cursor..].find(marker) {
            let start = cursor + offset + marker.len();
            let Some(end_offset) = output[start..].find('"') else {
                break;
            };
            let end = start + end_offset;
            let user_key = output[start..end].to_owned();
            let user = self.get("/user", &[("key", user_key.clone())]).ok();
            let display = user
                .as_ref()
                .and_then(|value| value.get("displayName").or_else(|| value.get("username")))
                .and_then(Value::as_str)
                .unwrap_or(&user_key)
                .to_owned();
            let replacement = format!("ri:userkey=\"{}\"", escape_xml_attribute(&display));
            output.replace_range(start - marker.len()..=end, &replacement);
            cursor = start - marker.len() + replacement.len();
        }
        Ok(output)
    }

    fn resolve_page_links(&self, html: &str) -> Result<String, ClientError> {
        let mut output = html.to_owned();
        let title_marker = "ri:content-title=\"";
        let mut cursor = 0;
        while let Some(offset) = output[cursor..].find(title_marker) {
            let start = cursor + offset + title_marker.len();
            let Some(end_offset) = output[start..].find('"') else {
                break;
            };
            let end = start + end_offset;
            let title = output[start..end].to_owned();
            let query = format!("title = \"{}\"", escape_cql(&title));
            let found = self
                .get(
                    "/search",
                    &[
                        ("cql", query),
                        ("limit", "1".into()),
                        ("expand", "content.space".into()),
                    ],
                )
                .ok();
            if let Some(result) = found
                .as_ref()
                .and_then(|value| value.get("results"))
                .and_then(Value::as_array)
                .and_then(|rows| rows.first())
            {
                let content = result.get("content").unwrap_or(result);
                let page_id = content
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !page_id.is_empty() {
                    let space = content
                        .get("space")
                        .and_then(|value| value.get("key"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let url = if let Some(path) = content
                        .get("_links")
                        .and_then(|value| value.get("webui"))
                        .and_then(Value::as_str)
                    {
                        self.build_url(path)
                    } else {
                        self.build_url(&format!("/spaces/{space}/pages/{page_id}"))
                    };
                    output.replace_range(
                        start - title_marker.len()..=end,
                        &format!("ri:url=\"{}\"", escape_xml_attribute(&url)),
                    );
                    cursor = start - title_marker.len() + url.len() + 12;
                    continue;
                }
            }
            cursor = end + 1;
        }
        Ok(output)
    }

    fn expand_storage_macros(
        &self,
        html: &str,
        page_id: &str,
        depth: u32,
    ) -> Result<String, ClientError> {
        if depth >= 4 {
            return Ok(html.to_owned());
        }
        let mut output = html.to_owned();
        for macro_name in ["include", "include-shared-block"] {
            let marker = format!("<ac:structured-macro ac:name=\"{macro_name}\"");
            let mut cursor = 0;
            while let Some(offset) = output[cursor..].find(&marker) {
                let start = cursor + offset;
                let Some(close_offset) = output[start..].find("</ac:structured-macro>") else {
                    break;
                };
                let end = start + close_offset + "</ac:structured-macro>".len();
                let macro_html = output[start..end].to_owned();
                let target = extract_macro_page_target(&macro_html);
                let replacement = if let Some(target) = target {
                    let id = if target.chars().all(|character| character.is_ascii_digit()) {
                        Some(target)
                    } else {
                        let search = self.search(&target, 1, 0, true)?;
                        search.first().map(|row| row.id.clone())
                    };
                    if let Some(id) = id {
                        self.read(&id, "storage")
                            .and_then(|body| self.expand_storage_macros(&body, page_id, depth + 1))
                            .unwrap_or(macro_html)
                    } else {
                        macro_html
                    }
                } else {
                    macro_html
                };
                output.replace_range(start..end, &replacement);
                cursor = start + replacement.len();
            }
        }
        let _ = page_id;
        Ok(output)
    }

    pub fn create_page(
        &self,
        title: &str,
        space_key: &str,
        content: &str,
        format: &str,
        page_type: &str,
    ) -> Result<Value, ClientError> {
        if !["page", "folder"].contains(&page_type) {
            return Err(ClientError {
                message: "Invalid type. Valid: page, folder.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        if page_type == "folder" {
            return self.create_folder(title, space_key, None);
        }
        let storage = to_storage(content, format)?;
        let payload = serde_json::json!({
            "type": page_type,
            "title": title,
            "space": { "key": space_key },
            "body": { "storage": { "value": storage, "representation": "storage" } }
        });
        self.write("POST", "/content", &payload)
    }

    pub fn create_child_page(
        &self,
        title: &str,
        parent_id: &str,
        content: &str,
        format: &str,
    ) -> Result<Value, ClientError> {
        self.create_child_page_typed(title, parent_id, content, format, "page")
    }

    pub fn create_child_page_typed(
        &self,
        title: &str,
        parent_id: &str,
        content: &str,
        format: &str,
        page_type: &str,
    ) -> Result<Value, ClientError> {
        if !["page", "folder"].contains(&page_type) {
            return Err(ClientError {
                message: "Invalid type. Valid: page, folder.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        if page_type == "folder" {
            return self.create_folder(title, "", Some(parent_id));
        }
        let storage = to_storage(content, format)?;
        let payload = serde_json::json!({
            "type": page_type,
            "title": title,
            "ancestors": [{ "id": extract_page_id(parent_id).unwrap_or_else(|| parent_id.to_owned()) }],
            "body": { "storage": { "value": storage, "representation": "storage" } }
        });
        self.write("POST", "/content", &payload)
    }

    fn create_folder(
        &self,
        title: &str,
        space_key: &str,
        parent_id: Option<&str>,
    ) -> Result<Value, ClientError> {
        if !self.is_cloud() {
            return Err(ClientError {
                message: "Folders are only supported on Confluence Cloud.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let mut payload =
            serde_json::json!({"title":title,"type":"folder","space":{"key":space_key}});
        if let Some(parent_id) = parent_id {
            payload["parentId"] =
                Value::String(extract_page_id(parent_id).unwrap_or_else(|| parent_id.to_owned()));
        }
        let endpoint = "/api/v2/folders";
        let response = self
            .client
            .post(format!(
                "{}{}",
                self.base_url.trim_end_matches(&self.api_path),
                endpoint
            ))
            .json(&payload)
            .send()
            .map_err(|error| ClientError {
                message: format!("Network request failed: {error}"),
                code: "NETWORK",
                status: None,
                details: None,
            })?;
        let status = response.status();
        let bytes = response.bytes().map_err(|error| ClientError {
            message: error.to_string(),
            code: "NETWORK",
            status: Some(status.as_u16()),
            details: None,
        })?;
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(ClientError {
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Could not create folder.")
                    .to_owned(),
                code: "API_ERROR",
                status: Some(status.as_u16()),
                details: Some(value),
            });
        }
        Ok(value)
    }

    fn is_cloud(&self) -> bool {
        self.force_cloud || self.base_url.contains(".atlassian.net")
    }

    pub fn update_page(
        &self,
        page_id: &str,
        title: Option<&str>,
        content: Option<&str>,
        format: &str,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let current = self.get(
            &format!("/content/{id}"),
            &[("expand", "space,version,body.storage".into())],
        )?;
        let current_version = current
            .get("version")
            .and_then(|v| v.get("number"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let body_value = if let Some(content) = content {
            to_storage(content, format)?
        } else {
            current
                .get("body")
                .and_then(|v| v.get("storage"))
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let payload = serde_json::json!({
            "id": id,
            "type": current.get("type").and_then(Value::as_str).unwrap_or("page"),
            "title": title.unwrap_or_else(|| current.get("title").and_then(Value::as_str).unwrap_or("Untitled")),
            "space": current.get("space"),
            "version": { "number": current_version + 1 },
            "body": { "storage": { "value": body_value, "representation": "storage" } }
        });
        self.write("PUT", &format!("/content/{id}"), &payload)
    }

    pub fn move_page(
        &self,
        page_id: &str,
        new_parent_id: &str,
        title: Option<&str>,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let parent = extract_page_id(new_parent_id).ok_or_else(|| ClientError {
            message: "Could not extract a parent page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let current = self.get(
            &format!("/content/{id}"),
            &[("expand", "space,version,body.storage".into())],
        )?;
        let payload = serde_json::json!({
            "id": id,
            "type": current.get("type").and_then(Value::as_str).unwrap_or("page"),
            "title": title.unwrap_or_else(|| current.get("title").and_then(Value::as_str).unwrap_or("Untitled")),
            "space": current.get("space"),
            "version": { "number": current.get("version").and_then(|v| v.get("number")).and_then(Value::as_u64).unwrap_or(0) + 1 },
            "ancestors": [{ "id": parent }],
            "body": current.get("body")
        });
        self.write("PUT", &format!("/content/{id}"), &payload)
    }

    pub fn delete_page(&self, page_id: &str) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let response = self
            .client
            .delete(format!("{}/content/{id}", self.base_url))
            .send()
            .map_err(|error| ClientError {
                message: format!("Network request failed: {error}"),
                code: "NETWORK",
                status: None,
                details: None,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status.as_u16()),
                code: if status.as_u16() == 404 {
                    "NOT_FOUND"
                } else {
                    "API_ERROR"
                },
                status: Some(status.as_u16()),
                details: None,
            });
        }
        Ok(serde_json::json!({"id": id, "deleted": true}))
    }

    pub fn property_list_page(
        &self,
        page_id: &str,
        limit: Option<u32>,
        start: u32,
        all: bool,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if !all {
            let data = self.get(
                &format!("/content/{id}/property"),
                &[
                    ("limit", limit.unwrap_or(25).to_string()),
                    ("start", start.to_string()),
                ],
            )?;
            return Ok(
                serde_json::json!({"properties":data.get("results").cloned().unwrap_or_else(|| Value::Array(Vec::new())),"nextStart":parse_next_start(data.get("_links").and_then(|links|links.get("next")).and_then(Value::as_str))}),
            );
        }
        let page_size = limit.unwrap_or(25);
        let mut cursor = start;
        let mut properties = Vec::new();
        loop {
            let data = self.get(
                &format!("/content/{id}/property"),
                &[
                    ("limit", page_size.to_string()),
                    ("start", cursor.to_string()),
                ],
            )?;
            let rows = data
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if rows.is_empty() {
                break;
            }
            cursor = cursor.saturating_add(rows.len() as u32);
            properties.extend(rows);
            if parse_next_start(
                data.get("_links")
                    .and_then(|links| links.get("next"))
                    .and_then(Value::as_str),
            )
            .is_none()
            {
                break;
            }
            if limit.is_some_and(|maximum| properties.len() >= maximum as usize) {
                properties.truncate(limit.unwrap_or_default() as usize);
                break;
            }
        }
        Ok(serde_json::json!({"properties":properties}))
    }

    pub fn property_get(&self, page_id: &str, key: &str) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        match self.get(
            &format!("/content/{id}/property/{}", urlencoding::encode(key)),
            &[],
        ) {
            Err(error) if error.status == Some(404) => {
                Ok(serde_json::json!({"found":false,"pageId":id,"key":key}))
            }
            result => result,
        }
    }

    pub fn property_set(
        &self,
        page_id: &str,
        key: &str,
        value: &Value,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let current = self.property_get(&id, key)?;
        let mut payload = if current.get("found") == Some(&Value::Bool(false)) {
            serde_json::json!({"key":key,"value":value})
        } else {
            let mut copy = current;
            copy["value"] = value.clone();
            copy
        };
        if payload.get("id").is_none() {
            self.write("POST", &format!("/content/{id}/property"), &payload)
        } else {
            payload["key"] = Value::String(key.to_owned());
            self.write(
                "PUT",
                &format!("/content/{id}/property/{}", urlencoding::encode(key)),
                &payload,
            )
        }
    }

    pub fn property_delete(&self, page_id: &str, key: &str) -> Result<(), ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        self.delete(&format!(
            "/content/{id}/property/{}",
            urlencoding::encode(key)
        ))
    }

    pub fn comments_page(
        &self,
        page_id: &str,
        limit: Option<u32>,
        start: u32,
        locations: &[String],
        depth: Option<&str>,
        all: bool,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let mut cql = format!("container = {id} AND type = comment");
        if let Some(depth) = depth {
            if depth.is_empty() {
                cql.push_str(" AND depth = 0");
            } else if depth == "all" { /* no depth clause */
            } else if let Ok(depth) = depth.parse::<u32>() {
                cql.push_str(&format!(" AND depth <= {depth}"));
            } else {
                return Err(ClientError {
                    message: "Depth must be a non-negative integer, empty for roots, or 'all'."
                        .into(),
                    code: "VALIDATION",
                    status: None,
                    details: None,
                });
            }
        }
        let request_limit = limit.unwrap_or(25);
        let mut current = start;
        let mut comments = Vec::new();
        let mut next_start: Option<u32>;
        loop {
            let mut query = vec![
                ("cql", cql.clone()),
                ("limit", request_limit.to_string()),
                ("start", current.to_string()),
                ("expand", "body.view,version,ancestors,container".into()),
            ];
            if locations.len() == 1 {
                query.push(("location", locations[0].clone()));
            }
            let response = self.get("/content/search", &query)?;
            let rows = response
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            next_start = parse_next_start(
                response
                    .get("_links")
                    .and_then(|links| links.get("next"))
                    .and_then(Value::as_str),
            );
            comments.extend(rows.into_iter().map(normalize_comment));
            if !all
                || next_start.is_none()
                || limit.is_some_and(|maximum| comments.len() >= maximum as usize)
            {
                break;
            }
            current = next_start.unwrap_or(current);
        }
        if let Some(maximum) = limit {
            comments.truncate(maximum as usize);
        }
        if locations.iter().any(|location| location == "resolved") {
            comments.retain(|comment| comment["resolved"] == true);
        } else if !locations.is_empty() {
            comments.retain(|comment| {
                locations
                    .iter()
                    .any(|location| comment["location"] == *location)
            });
        }
        Ok(
            serde_json::json!({"pageId":id,"commentCount":comments.len(),"comments":comments,"nextStart":if all { Value::Null } else { next_start.map(Value::from).unwrap_or(Value::Null) }}),
        )
    }

    pub fn comment_lookup(&self, comment_id: &str) -> Result<Value, ClientError> {
        let id = numeric_id(comment_id).ok_or_else(|| ClientError {
            message: "Comment ID must be numeric.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let url = format!("{}/comment/{id}", self.api_v2_base());
        let response = self.client.get(url).send().map_err(|error| ClientError {
            message: format!("Network request failed: {error}"),
            code: "NETWORK",
            status: None,
            details: None,
        })?;
        let status = response.status();
        let bytes = response.bytes().map_err(|error| ClientError {
            message: error.to_string(),
            code: "NETWORK",
            status: Some(status.as_u16()),
            details: None,
        })?;
        if !status.is_success() {
            return Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status.as_u16()),
                code: if status.as_u16() == 404 {
                    "NOT_FOUND"
                } else {
                    "API_ERROR"
                },
                status: Some(status.as_u16()),
                details: serde_json::from_slice(&bytes).ok(),
            });
        }
        serde_json::from_slice(&bytes).map_err(|error| ClientError {
            message: format!("Invalid comment response: {error}"),
            code: "API_ERROR",
            status: Some(status.as_u16()),
            details: None,
        })
    }

    pub fn create_comment(
        &self,
        page_id: &str,
        content: &str,
        format: &str,
        parent_id: Option<&str>,
        location: &str,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let storage = to_storage(content, format)?;
        let container = if let Some(parent) = parent_id {
            let parent = numeric_id(parent).ok_or_else(|| ClientError {
                message: "Parent comment ID must be numeric.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            })?;
            let parent_comment = self.comment_lookup(&parent)?;
            let parent_page = parent_comment
                .get("container")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if parent_page != id {
                return Err(ClientError {
                    message: "Parent comment belongs to a different page.".into(),
                    code: "VALIDATION",
                    status: None,
                    details: None,
                });
            }
            parent_comment
                .get("container")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"id":id,"type":"page"}))
        } else {
            serde_json::json!({"id":id,"type":"page"})
        };
        let mut payload = serde_json::json!({"type":"comment","container":container,"body":{"storage":{"value":storage,"representation":"storage"}}});
        if parent_id.is_none() && location == "inline" {
            payload["extensions"] = serde_json::json!({"location":"inline"});
        }
        self.write("POST", "/content", &payload)
    }

    pub fn comment_delete(&self, comment_id: &str) -> Result<(), ClientError> {
        let id = comment_id.trim();
        if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
            return Err(ClientError {
                message: "Comment ID must be numeric.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        self.delete(&format!("/content/{id}"))
    }

    pub fn attachments(&self, page_id: &str) -> Result<Value, ClientError> {
        self.attachments_page(page_id, None, None)
    }

    pub fn attachments_page(
        &self,
        page_id: &str,
        max_results: Option<u32>,
        pattern: Option<&str>,
    ) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let mut start = 0u32;
        let mut items = Vec::new();
        let page_size = 100u32;
        loop {
            if max_results.is_some_and(|maximum| items.len() >= maximum as usize) {
                break;
            }
            let limit = max_results
                .map(|maximum| page_size.min(maximum.saturating_sub(items.len() as u32)))
                .unwrap_or(page_size);
            let data = self.get(
                &format!("/content/{id}/child/attachment"),
                &[
                    ("limit", limit.to_string()),
                    ("start", start.to_string()),
                    ("expand", "version,metadata".into()),
                ],
            )?;
            let rows = data
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if rows.is_empty() {
                break;
            }
            start = start.saturating_add(rows.len() as u32);
            items.extend(rows.into_iter().filter(|item| {
                pattern.is_none_or(|pattern| {
                    glob_match(
                        pattern,
                        item.get("title").and_then(Value::as_str).unwrap_or(""),
                    )
                })
            }));
            if data
                .get("_links")
                .and_then(|value| value.get("next"))
                .and_then(Value::as_str)
                .is_none()
            {
                break;
            }
        }
        if let Some(maximum) = max_results {
            items.truncate(maximum as usize);
        }
        Ok(
            serde_json::json!({"pageId":id,"attachments":items.iter().map(|item| serde_json::json!({
            "id": item.get("id"),
            "title": item.get("title"),
            "mediaType": item.get("metadata").and_then(|v| v.get("mediaType")),
            "fileSize": item.get("extensions").and_then(|v| v.get("fileSize")),
            "version": item.get("version").and_then(|v| v.get("number")),
            "url": item.get("_links").and_then(|v| v.get("download"))
        })).collect::<Vec<_>>() }),
        )
    }

    pub fn attachment_lookup(&self, attachment_id: &str) -> Result<Value, ClientError> {
        let id = numeric_id(attachment_id).ok_or_else(|| ClientError {
            message: "Attachment ID must be numeric.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        self.get(
            &format!("/content/{id}"),
            &[("expand", "container,version,metadata".into())],
        )
    }

    pub fn attachment_download(
        &self,
        page_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, ClientError> {
        let page = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let id = numeric_id(attachment_id).ok_or_else(|| ClientError {
            message: "Attachment ID must be numeric.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let metadata = self.attachment_lookup(&id)?;
        let attachment_page = metadata
            .get("container")
            .and_then(|v| v.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if attachment_page != page {
            return Err(ClientError {
                message: format!("Attachment {id} does not belong to page {page}."),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let href = metadata
            .get("_links")
            .and_then(|v| v.get("download"))
            .and_then(Value::as_str)
            .ok_or_else(|| ClientError {
                message: "Attachment download URL is unavailable.".into(),
                code: "API_ERROR",
                status: None,
                details: None,
            })?;
        let url = if href.starts_with("http://") || href.starts_with("https://") {
            href.to_owned()
        } else {
            self.build_url(href)
        };
        // The download URL can originate from a server-supplied _links.download
        // value. Refuse to send credentials to anything but the configured
        // origin so a tampered or misconfigured API response (or an http
        // downgrade against an https-configured client) can't exfiltrate the
        // token. Mirrors the Node client's assertSameOrigin guard.
        self.validate_page_url(&url)?;
        let response = self.client.get(url).send().map_err(|error| ClientError {
            message: format!("Network request failed: {error}"),
            code: "NETWORK",
            status: None,
            details: None,
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ClientError {
                message: format!("Attachment download failed with HTTP {}.", status.as_u16()),
                code: "API_ERROR",
                status: Some(status.as_u16()),
                details: None,
            });
        }
        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|error| ClientError {
                message: error.to_string(),
                code: "NETWORK",
                status: None,
                details: None,
            })
    }

    pub fn attachment_upload(
        &self,
        page_id: &str,
        files: &[String],
        comment: Option<&str>,
        replace: bool,
        minor_edit: Option<bool>,
    ) -> Result<Value, ClientError> {
        if self.auth_is_read_only() {
            return Err(ClientError {
                message: "This profile is in read-only mode. Write operations are not allowed."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if files.is_empty() {
            return Err(ClientError {
                message: "At least one --file is required.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let mut uploaded = Vec::new();
        for filename in files {
            let bytes = std::fs::read(filename).map_err(|error| ClientError {
                message: format!("Could not read attachment {filename}: {error}"),
                code: "VALIDATION",
                status: None,
                details: None,
            })?;
            let name = std::path::Path::new(filename)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("attachment")
                .to_owned();
            let existing_id = if replace {
                self.attachments_page(page_id, None, None)
                    .ok()
                    .and_then(|existing| {
                        existing
                            .get("attachments")
                            .and_then(Value::as_array)
                            .and_then(|items| {
                                items.iter().find(|item| {
                                    item.get("title").and_then(Value::as_str) == Some(name.as_str())
                                })
                            })
                            .and_then(|item| item.get("id"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
            } else {
                None
            };
            let path = if let Some(existing_id) = existing_id {
                format!("/content/{id}/child/attachment/{existing_id}/data")
            } else {
                format!("/content/{id}/child/attachment")
            };
            let mut form = Form::new().part("file", Part::bytes(bytes).file_name(name));
            if let Some(comment) = comment {
                form = form.text("comment", comment.to_owned());
            }
            if let Some(minor_edit) = minor_edit {
                form = form.text("minorEdit", minor_edit.to_string());
            }
            let is_cloud = self.base_url.contains(".atlassian.net") || self.force_cloud;
            let builder = if replace && is_cloud {
                self.client.put(format!("{}{path}", self.base_url))
            } else {
                self.client.post(format!("{}{path}", self.base_url))
            };
            let response = builder
                .header("X-Atlassian-Token", "nocheck")
                .multipart(form)
                .send()
                .map_err(|error| ClientError {
                    message: format!("Network request failed: {error}"),
                    code: "NETWORK",
                    status: None,
                    details: None,
                })?;
            let status = response.status();
            let bytes = response.bytes().map_err(|error| ClientError {
                message: error.to_string(),
                code: "NETWORK",
                status: Some(status.as_u16()),
                details: None,
            })?;
            let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            if !status.is_success() {
                return Err(ClientError {
                    message: value
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Attachment upload failed.")
                        .to_owned(),
                    code: "API_ERROR",
                    status: Some(status.as_u16()),
                    details: Some(value),
                });
            }
            if let Some(results) = value.get("results").and_then(Value::as_array) {
                uploaded.extend(results.iter().cloned());
            } else if value.get("type").and_then(Value::as_str) == Some("attachment") {
                uploaded.push(value);
            }
        }
        Ok(serde_json::json!({"results":uploaded}))
    }

    pub fn attachment_delete(&self, page_id: &str, attachment_id: &str) -> Result<(), ClientError> {
        let page = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let id = numeric_id(attachment_id).ok_or_else(|| ClientError {
            message: "Attachment ID must be numeric.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let metadata = self.attachment_lookup(&id)?;
        if metadata
            .get("container")
            .and_then(|v| v.get("id"))
            .and_then(Value::as_str)
            != Some(page.as_str())
        {
            return Err(ClientError {
                message: format!("Attachment {id} does not belong to page {page}."),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        self.delete(&format!("/content/{id}"))
    }

    pub fn versions(&self, page_id: &str) -> Result<Value, ClientError> {
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let mut all = Vec::new();
        let mut start = 0u32;
        let limit = 200u32;
        loop {
            let response = match self.get(
                &format!("/content/{id}/version"),
                &[("limit", limit.to_string()), ("start", start.to_string())],
            ) {
                Ok(response) => response,
                Err(error) if matches!(error.status, Some(404 | 405)) => self.get(
                    &format!("/content/{id}/version"),
                    &[
                        ("limit", limit.to_string()),
                        ("start", start.to_string()),
                        ("api", "experimental".into()),
                    ],
                )?,
                Err(error) => return Err(error),
            };
            let rows = response
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let count = rows.len();
            all.extend(rows.into_iter().map(|version| serde_json::json!({
                "number":version.get("number"),
                "when":version.get("when"),
                "by":version.get("by").and_then(|person|person.get("displayName").or_else(||person.get("publicName")).or_else(||person.get("email"))),
                "minorEdit":version.get("minorEdit").and_then(Value::as_bool).unwrap_or(false),
                "message":version.get("message").and_then(Value::as_str).unwrap_or("")
            })));
            if count < limit as usize {
                break;
            }
            start += limit;
        }
        all.sort_by_key(|version| version.get("number").and_then(Value::as_u64).unwrap_or(0));
        Ok(serde_json::json!({"pageId":id,"versions":all}))
    }

    pub fn version_delete(&self, page_id: &str, version: u32) -> Result<(), ClientError> {
        if self.auth_is_read_only() {
            return Err(ClientError {
                message: "This profile is in read-only mode. Write operations are not allowed."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let versions = self.versions(&id)?;
        let rows = versions
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let current = rows
            .iter()
            .filter_map(|row| row.get("number").and_then(Value::as_u64))
            .max()
            .unwrap_or(0);
        if version as u64 >= current {
            return Err(ClientError {
                message: format!("Cannot delete current version {current}."),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        self.delete(&format!("/content/{id}/version/{version}"))
    }

    pub fn versions_purge_preview(&self, page_id: &str, keep: u32) -> Result<Value, ClientError> {
        if keep == 0 {
            return Err(ClientError {
                message: "--keep must be greater than zero.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let versions = self.versions(&id)?;
        let mut rows = versions
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        rows.sort_by_key(|row| row.get("number").and_then(Value::as_u64).unwrap_or(0));
        let deletions = rows
            .iter()
            .take(rows.len().saturating_sub(keep as usize))
            .filter_map(|row| row.get("number").and_then(Value::as_u64))
            .collect::<Vec<_>>();
        Ok(
            serde_json::json!({"pageId":id,"keep":keep,"deletions":deletions,"currentVersion":rows.last().and_then(|row| row.get("number")).and_then(Value::as_u64)}),
        )
    }

    pub fn versions_purge_execute(
        &self,
        page_id: &str,
        preview: &Value,
    ) -> Result<Value, ClientError> {
        if self.auth_is_read_only() {
            return Err(ClientError {
                message: "This profile is in read-only mode. Write operations are not allowed."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let id = extract_page_id(page_id).ok_or_else(|| ClientError {
            message: "Could not extract a page ID from the supplied value.".into(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if preview.get("pageId").and_then(Value::as_str) != Some(id.as_str()) {
            return Err(ClientError {
                message: "Purge preview page does not match execution target.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let fresh = self.versions_purge_preview(
            &id,
            preview.get("keep").and_then(Value::as_u64).unwrap_or(5) as u32,
        )?;
        if fresh["deletions"] != preview["deletions"]
            || fresh["currentVersion"] != preview["currentVersion"]
        {
            return Err(ClientError {
                message: "Version list changed since preview; run a new preview.".into(),
                code: "VALIDATION",
                status: None,
                details: Some(fresh),
            });
        }
        let mut deleted = Vec::new();
        let mut failures = Vec::new();
        for version in preview["deletions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
        {
            match self.delete(&format!("/content/{id}/version/{version}")) {
                Ok(()) => deleted.push(version),
                Err(error) => failures
                    .push(serde_json::json!({"versionNumber":version,"message":error.message})),
            }
        }
        Ok(
            serde_json::json!({"pageId":id,"deleted":deleted,"failed":failures.len(),"errors":failures}),
        )
    }

    /// Resolve or reopen a top-level inline comment thread (Confluence Cloud only).
    ///
    /// Mirrors the Node client's `setCommentResolved`: reads the current
    /// version first (the v2 update needs the next version number), rejects
    /// replies and dangling threads, and treats asking for the state the
    /// thread already has as a no-op.
    pub fn comment_resolve(&self, comment_id: &str, resolved: bool) -> Result<Value, ClientError> {
        if self.auth_is_read_only() {
            return Err(ClientError {
                message: "This profile is in read-only mode. Write operations are not allowed."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        if !self.is_cloud() {
            return Err(ClientError {
                message: "Resolving and reopening inline comments requires Confluence Cloud."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let id = numeric_id(comment_id).ok_or_else(|| ClientError {
            message: format!("Invalid comment ID \"{comment_id}\": comment IDs are numeric."),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let url = format!("{}/inline-comments/{id}", self.api_v2_base());
        let response = self.client.get(&url).send().map_err(|error| ClientError {
            message: format!("Network request failed: {error}"),
            code: "NETWORK",
            status: None,
            details: None,
        })?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(ClientError {
                message: format!(
                    "Inline comment {id} not found. Only inline comments can be resolved or reopened."
                ),
                code: "NOT_FOUND",
                status: Some(404),
                details: None,
            });
        }
        let bytes = response.bytes().map_err(|error| ClientError {
            message: error.to_string(),
            code: "NETWORK",
            status: Some(status.as_u16()),
            details: None,
        })?;
        if !status.is_success() {
            return Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status.as_u16()),
                code: "API_ERROR",
                status: Some(status.as_u16()),
                details: serde_json::from_slice(&bytes).ok(),
            });
        }
        let current: Value = serde_json::from_slice(&bytes).map_err(|error| ClientError {
            message: format!("Invalid comment response: {error}"),
            code: "API_ERROR",
            status: Some(status.as_u16()),
            details: None,
        })?;
        if current
            .get("parentCommentId")
            .is_some_and(|value| !value.is_null())
        {
            return Err(ClientError {
                message: format!(
                    "Comment {id} is a reply; only top-level inline comments can be resolved or reopened."
                ),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let resolution_status = current
            .get("resolutionStatus")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if resolution_status.as_deref() == Some("dangling") {
            return Err(ClientError {
                message: format!(
                    "Inline comment {id} is dangling (its highlighted text was removed) and cannot be updated."
                ),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        if (resolution_status.as_deref() == Some("resolved")) == resolved {
            return Ok(
                serde_json::json!({"id":id,"resolved":resolved,"changed":false,"status":resolution_status}),
            );
        }
        let version = current
            .get("version")
            .and_then(|value| value.get("number"))
            .and_then(Value::as_u64)
            .ok_or_else(|| ClientError {
                message: format!("Could not read the current version of inline comment {id}."),
                code: "API_ERROR",
                status: None,
                details: None,
            })?;
        let payload = serde_json::json!({"version":{"number":version + 1},"resolved":resolved});
        let response = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .map_err(|error| ClientError {
                message: format!("Network request failed: {error}"),
                code: "NETWORK",
                status: None,
                details: None,
            })?;
        let status_code = response.status();
        if !status_code.is_success() {
            let bytes = response.bytes().map_err(|error| ClientError {
                message: error.to_string(),
                code: "NETWORK",
                status: Some(status_code.as_u16()),
                details: None,
            })?;
            return Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status_code.as_u16()),
                code: "API_ERROR",
                status: Some(status_code.as_u16()),
                details: serde_json::from_slice(&bytes).ok(),
            });
        }
        Ok(
            serde_json::json!({"id":id,"resolved":resolved,"changed":true,"status":resolution_status}),
        )
    }

    fn api_v2_base(&self) -> String {
        let base = if self.api_path.ends_with("/rest/api") {
            self.api_path.trim_end_matches("/rest/api").to_owned()
        } else {
            self.api_path.trim_end_matches('/').to_owned()
        };
        format!(
            "{}{}{}",
            self.base_url.trim_end_matches(&self.api_path),
            base,
            "/api/v2"
        )
    }

    fn delete(&self, path: &str) -> Result<(), ClientError> {
        if self.auth_is_read_only() {
            return Err(ClientError {
                message: "This profile is in read-only mode. Write operations are not allowed."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let response = self
            .client
            .delete(format!("{}{path}", self.base_url))
            .send()
            .map_err(|error| ClientError {
                message: format!("Network request failed: {error}"),
                code: "NETWORK",
                status: None,
                details: None,
            })?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status.as_u16()),
                code: if status.as_u16() == 404 {
                    "NOT_FOUND"
                } else {
                    "API_ERROR"
                },
                status: Some(status.as_u16()),
                details: None,
            })
        }
    }

    fn write(&self, method: &str, path: &str, payload: &Value) -> Result<Value, ClientError> {
        if self.auth_is_read_only() {
            return Err(ClientError {
                message: "This profile is in read-only mode. Write operations are not allowed."
                    .into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let url = format!("{}{path}", self.base_url);
        let builder = match method {
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            _ => {
                return Err(ClientError {
                    message: "Unsupported HTTP method.".into(),
                    code: "VALIDATION",
                    status: None,
                    details: None,
                })
            }
        };
        let response = builder.json(payload).send().map_err(|error| ClientError {
            message: format!("Network request failed: {error}"),
            code: "NETWORK",
            status: None,
            details: None,
        })?;
        let status = response.status();
        let bytes = response.bytes().map_err(|error| ClientError {
            message: error.to_string(),
            code: "NETWORK",
            status: Some(status.as_u16()),
            details: None,
        })?;
        let value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        if !status.is_success() {
            return Err(ClientError {
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        format!("Confluence API returned HTTP {}.", status.as_u16())
                    }),
                code: "API_ERROR",
                status: Some(status.as_u16()),
                details: Some(value),
            });
        }
        Ok(value)
    }

    fn auth_is_read_only(&self) -> bool {
        self.read_only || std::env::var("CONFLUENCE_READ_ONLY").ok().as_deref() == Some("true")
    }

    pub fn is_read_only(&self) -> bool {
        self.auth_is_read_only()
    }

    pub fn raw_request(
        &self,
        method: &str,
        endpoint: &str,
        headers: &reqwest::header::HeaderMap,
        parameters: &serde_json::Map<String, Value>,
        body: Option<&Value>,
    ) -> Result<RawResponse, ClientError> {
        let url = self.resolve_api_url(endpoint)?;
        let method =
            reqwest::Method::from_bytes(method.as_bytes()).map_err(|error| ClientError {
                message: format!("Invalid HTTP method: {error}"),
                code: "VALIDATION",
                status: None,
                details: None,
            })?;
        let is_read = matches!(method, reqwest::Method::GET | reqwest::Method::HEAD);
        let builder = self
            .client
            .request(method.clone(), url)
            .headers(headers.clone());
        let builder = if is_read {
            if parameters.is_empty() {
                builder
            } else {
                builder.query(parameters)
            }
        } else if let Some(body) = body {
            let mut payload = body.clone();
            if !parameters.is_empty() {
                if let Some(object) = payload.as_object_mut() {
                    for (key, value) in parameters {
                        object.insert(key.clone(), value.clone());
                    }
                }
            }
            builder.json(&payload)
        } else if !parameters.is_empty() {
            builder.json(parameters)
        } else {
            builder
        };
        let response = builder.send().map_err(|error| ClientError {
            message: format!("Network request failed: {error}"),
            code: "NETWORK",
            status: None,
            details: None,
        })?;
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let bytes = response.bytes().map_err(|error| ClientError {
            message: error.to_string(),
            code: "NETWORK",
            status: Some(status.as_u16()),
            details: None,
        })?;
        let body_text = String::from_utf8_lossy(&bytes).to_string();
        if !status.is_success() {
            return Err(ClientError {
                message: format!("Confluence API returned HTTP {}.", status.as_u16()),
                code: if status.as_u16() == 401 {
                    "AUTH_FAILED"
                } else if status.as_u16() == 404 {
                    "NOT_FOUND"
                } else {
                    "API_ERROR"
                },
                status: Some(status.as_u16()),
                details: serde_json::from_slice(&bytes).ok(),
            });
        }
        Ok(RawResponse {
            status: status.as_u16(),
            headers,
            body: body_text,
        })
    }

    pub fn export_page(
        &self,
        page_id: &str,
        destination: &str,
        format: &str,
        filename: Option<&str>,
        skip_attachments: bool,
    ) -> Result<Value, ClientError> {
        let info = self.info(page_id)?;
        let title = info.title.clone();
        let folder = sanitize_path_component(&title);
        let extension = match format {
            "markdown" => "md",
            "html" => "html",
            "text" => "txt",
            "storage" => "xml",
            _ => {
                return Err(ClientError {
                    message: "Export format must be markdown, html, storage, or text.".into(),
                    code: "VALIDATION",
                    status: None,
                    details: None,
                })
            }
        };
        let directory = std::path::PathBuf::from(destination).join(folder);
        let default_filename = format!("page.{extension}");
        let content_path = directory.join(filename.unwrap_or(&default_filename));
        if !content_path.starts_with(&directory) {
            return Err(ClientError {
                message: "Export filename must remain inside export directory.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        let content = self.read(page_id, format)?;
        std::fs::create_dir_all(&directory).map_err(|error| ClientError {
            message: format!("Could not create export directory: {error}"),
            code: "UNKNOWN",
            status: None,
            details: None,
        })?;
        std::fs::write(&content_path, &content).map_err(|error| ClientError {
            message: format!("Could not write page export: {error}"),
            code: "UNKNOWN",
            status: None,
            details: None,
        })?;
        let mut downloaded = 0usize;
        if !skip_attachments {
            let attachments = self.attachments(page_id)?;
            let items = attachments
                .get("attachments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !items.is_empty() {
                let attachment_dir = directory.join("attachments");
                std::fs::create_dir_all(&attachment_dir).map_err(|error| ClientError {
                    message: format!("Could not create attachments directory: {error}"),
                    code: "UNKNOWN",
                    status: None,
                    details: None,
                })?;
                for item in items {
                    let Some(id) = item.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let name = item
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("attachment");
                    let path = attachment_dir.join(sanitize_path_component(name));
                    let data = self.attachment_download(page_id, id)?;
                    std::fs::write(path, data).map_err(|error| ClientError {
                        message: format!("Could not write attachment: {error}"),
                        code: "UNKNOWN",
                        status: None,
                        details: None,
                    })?;
                    downloaded += 1;
                }
            }
        }
        Ok(
            serde_json::json!({"title":title,"contentPath":content_path,"attachmentCount":downloaded,"pageId":info.id}),
        )
    }

    fn resolve_api_url(&self, endpoint: &str) -> Result<String, ClientError> {
        if endpoint.starts_with("https://") || endpoint.starts_with("http://") {
            let requested = reqwest::Url::parse(endpoint).map_err(|error| ClientError {
                message: format!("Invalid endpoint URL: {error}"),
                code: "VALIDATION",
                status: None,
                details: None,
            })?;
            let configured = reqwest::Url::parse(&self.base_url).map_err(|error| ClientError {
                message: error.to_string(),
                code: "VALIDATION",
                status: None,
                details: None,
            })?;
            if requested.origin() != configured.origin() {
                return Err(ClientError {
                    message: "Cross-origin API URL is refused to prevent credential leakage."
                        .into(),
                    code: "VALIDATION",
                    status: None,
                    details: None,
                });
            }
            return Ok(requested.to_string());
        }
        let origin = self.base_url.trim_end_matches(&self.api_path);
        if endpoint.starts_with('/') {
            Ok(format!("{origin}{endpoint}"))
        } else {
            Ok(format!("{}{endpoint}", self.base_url.trim_end_matches('/')))
        }
    }

    fn validate_page_url(&self, input: &str) -> Result<(), ClientError> {
        let requested = reqwest::Url::parse(input).map_err(|error| ClientError {
            message: error.to_string(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        let configured = reqwest::Url::parse(&self.base_url).map_err(|error| ClientError {
            message: error.to_string(),
            code: "VALIDATION",
            status: None,
            details: None,
        })?;
        if requested.origin() != configured.origin() {
            return Err(ClientError {
                message: "Page URL origin does not match the configured Confluence host.".into(),
                code: "VALIDATION",
                status: None,
                details: None,
            });
        }
        Ok(())
    }

    fn build_url(&self, path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        if let Some(site_url) = &self.site_url {
            if let Ok(mut url) = reqwest::Url::parse(site_url) {
                let base_path = url.path().trim_end_matches('/');
                if base_path.is_empty()
                    || path == base_path
                    || path.starts_with(&format!("{base_path}/"))
                {
                    url.set_path(&path);
                } else {
                    url.set_path(&format!("{base_path}{path}"));
                }
                return url.to_string();
            }
        }
        let origin = self.base_url.trim_end_matches(&self.api_path);
        let web_prefix = if self.api_path.contains("/wiki/")
            || (self.force_cloud && !self.api_path.starts_with("/ex/"))
        {
            "/wiki"
        } else {
            ""
        };
        format!("{origin}{web_prefix}{path}")
    }
}

fn html_to_text(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut previous_space = false;
    for character in html.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                if !previous_space {
                    output.push(' ');
                    previous_space = true;
                }
            }
            _ if !in_tag => {
                if character.is_whitespace() {
                    if !previous_space {
                        output.push(' ');
                        previous_space = true;
                    }
                } else {
                    output.push(character);
                    previous_space = false;
                }
            }
            _ => {}
        }
    }
    output.trim().to_owned()
}

fn html_to_markdown(html: &str) -> String {
    // A conservative fallback for basic HTML; full Confluence macro semantics
    // are intentionally still delegated to the existing Node implementation.
    let mut text = html.to_owned();
    for (open, close, marker) in [
        ("<strong>", "</strong>", "**"),
        ("<b>", "</b>", "**"),
        ("<em>", "</em>", "*"),
        ("<i>", "</i>", "*"),
    ] {
        text = text.replace(open, marker).replace(close, marker);
    }
    for (tag, marker) in [("h1", "# "), ("h2", "## "), ("h3", "### ")] {
        text = text
            .replace(&format!("<{tag}>"), marker)
            .replace(&format!("</{tag}>"), "\n\n");
    }
    text = text
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    text = text
        .replace("</p>", "\n\n")
        .replace("</li>", "\n")
        .replace("<li>", "- ");
    html_to_text(&text)
}

pub fn extract_page_id_for_cli(input: &str) -> Option<String> {
    extract_page_id(input)
}

fn extract_page_id(input: &str) -> Option<String> {
    if input.chars().all(|c| c.is_ascii_digit()) && !input.is_empty() {
        return Some(input.to_owned());
    }
    let url = reqwest::Url::parse(input).ok()?;
    if let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "pageId") {
        if value.chars().all(|c| c.is_ascii_digit()) && !value.is_empty() {
            return Some(value.into_owned());
        }
    }
    let segments: Vec<_> = url.path_segments()?.collect();
    let pages = segments.iter().position(|segment| *segment == "pages")?;
    let id = *segments.get(pages + 1)?;
    if id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty() {
        Some(id.to_owned())
    } else {
        None
    }
}

fn numeric_id(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.chars().all(|character| character.is_ascii_digit()))
        .then(|| value.to_owned())
}

fn parse_next_start(link: Option<&str>) -> Option<u32> {
    let link = link?;
    let url = reqwest::Url::parse(link).ok()?;
    url.query_pairs()
        .find(|(key, _)| key == "start")
        .and_then(|(_, value)| value.parse().ok())
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let value = value.to_lowercase();
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();
    let mut table = vec![vec![false; v.len() + 1]; p.len() + 1];
    table[0][0] = true;
    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            table[i][0] = table[i - 1][0];
        }
    }
    for i in 1..=p.len() {
        for j in 1..=v.len() {
            table[i][j] = match p[i - 1] {
                '*' => table[i - 1][j] || table[i][j - 1],
                '?' => table[i - 1][j - 1],
                char => char == v[j - 1] && table[i - 1][j - 1],
            };
        }
    }
    table[p.len()][v.len()]
}

fn normalize_comment(raw: Value) -> Value {
    let ancestors = raw
        .get("ancestors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let parent_id = ancestors
        .iter()
        .rev()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("comment"))
        .and_then(|item| item.get("id"))
        .cloned();
    let body = raw
        .get("body")
        .and_then(|body| body.get("storage"))
        .and_then(|storage| storage.get("value"))
        .cloned()
        .unwrap_or(Value::Null);
    let location = raw
        .get("extensions")
        .and_then(|extension| extension.get("location"))
        .and_then(Value::as_str)
        .unwrap_or("footer");
    serde_json::json!({
        "id": raw.get("id"),
        "pageId": raw.get("container").and_then(|value| value.get("id")),
        "parentId": parent_id,
        "body": body,
        "location": location,
        "version": raw.get("version").and_then(|value| value.get("number")),
        "resolved": raw.get("extensions").and_then(|value| value.get("resolution")).and_then(|value| value.get("status")).and_then(Value::as_str) == Some("resolved"),
        "ancestors": ancestors,
        "author": raw.get("history").and_then(|value| value.get("createdBy")).map(normalize_user),
        "createdAt": raw.get("history").and_then(|value| value.get("createdDate"))
    })
}

fn escape_cql(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn normalize_user(value: &Value) -> Value {
    let account_id = value
        .get("accountId")
        .or_else(|| value.get("account_id"))
        .cloned();
    let user_key = value
        .get("userKey")
        .or_else(|| value.get("userkey"))
        .or_else(|| value.get("key"))
        .cloned();
    let username = value.get("username").cloned();
    let display_name = value
        .get("displayName")
        .or_else(|| value.get("publicName"))
        .or_else(|| value.get("username"))
        .or_else(|| value.get("userKey"))
        .or_else(|| value.get("accountId"))
        .and_then(Value::as_str)
        .unwrap_or("Unknown");
    serde_json::json!({
        "displayName": display_name,
        "accountId": account_id,
        "userKey": user_key,
        "username": username,
        "email": value.get("email")
    })
}

fn to_storage(content: &str, format: &str) -> Result<String, ClientError> {
    match format.to_lowercase().as_str() {
        "storage" => Ok(content.to_owned()),
        "html" => Ok(content.to_owned()),
        "auto" => {
            let trimmed = content.trim_start();
            if trimmed.starts_with('<') {
                Ok(content.to_owned())
            } else {
                Ok(plain_to_storage(content))
            }
        }
        "markdown" => Ok(plain_to_storage(content)),
        _ => Err(ClientError {
            message: format!(
                "Invalid content format \"{format}\". Valid: auto, storage, html, markdown"
            ),
            code: "VALIDATION",
            status: None,
            details: None,
        }),
    }
}

fn plain_to_storage(content: &str) -> String {
    content
        .split("\n\n")
        .map(|paragraph| {
            let escaped = paragraph
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!("<p>{}</p>", escaped.replace('\n', "<br/>"))
        })
        .collect::<Vec<_>>()
        .join("")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_xml_attribute(value: &str) -> String {
    escape_html(value).replace('\'', "&apos;")
}

fn extract_macro_page_target(value: &str) -> Option<String> {
    for marker in ["ri:content-title=\"", "ri:page-id=\""] {
        if let Some(offset) = value.find(marker) {
            let start = offset + marker.len();
            let end = value[start..].find('"')? + start;
            return Some(value[start..end].to_owned());
        }
    }
    None
}

fn sanitize_path_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() {
        "page".into()
    } else {
        cleaned.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_numeric_ids_from_supported_url_shapes() {
        assert_eq!(extract_page_id("123"), Some("123".into()));
        assert_eq!(
            extract_page_id("https://example.test/wiki/viewpage.action?pageId=456"),
            Some("456".into())
        );
        assert_eq!(
            extract_page_id("https://example.test/wiki/spaces/ENG/pages/789/Title"),
            Some("789".into())
        );
        assert_eq!(
            extract_page_id("https://example.test/wiki/pages/nope/Title"),
            None
        );
    }
}

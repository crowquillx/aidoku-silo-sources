use crate::{models::*, settings};
use aidoku::{
    alloc::{format, string::String, vec::Vec},
    helpers::uri::encode_uri_component,
    imports::{
        defaults::{DefaultValue, defaults_get, defaults_set},
        net::{HttpMethod, Request},
        std::current_date,
    },
    prelude::*,
};

use aidoku::Result;
use serde::Serialize;
use serde::de::DeserializeOwned;

const ACCESS_TOKEN_KEY: &str = "accessToken";
const REFRESH_TOKEN_KEY: &str = "refreshToken";
const TOKEN_EXPIRY_KEY: &str = "tokenExpiry";
const PROFILE_ID_KEY: &str = "profileId";
const PROFILE_TOKEN_KEY: &str = "profileToken";

pub const PAGE_SIZE: i32 = 30;

#[derive(Serialize)]
struct LoginBody<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct RefreshBody<'a> {
    refresh_token: &'a str,
}

#[derive(Serialize)]
struct PinBody<'a> {
    pin: &'a str,
}

/// A small query-string builder. Keys are emitted verbatim (so bracketed rule
/// engine keys work), values are percent-encoded.
pub struct Query {
    pairs: Vec<(String, String)>,
}

impl Query {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    pub fn add(&mut self, key: &str, value: &str) {
        self.pairs.push((String::from(key), String::from(value)));
    }

    pub fn add_i64(&mut self, key: &str, value: i64) {
        self.add(key, &format!("{value}"));
    }

    pub fn encode(&self) -> String {
        let mut out = String::new();
        for (index, (key, value)) in self.pairs.iter().enumerate() {
            if index > 0 {
                out.push('&');
            }
            out.push_str(key);
            out.push('=');
            out.push_str(&encode_uri_component(value));
        }
        out
    }
}

pub struct Client {
    pub base: String,
    pub prefix: &'static str,
    pub is_v2: bool,
    pub token: String,
    pub profile_id: String,
    pub profile_token: Option<String>,
    pub image_size: String,
}

impl Client {
    /// Creates a client and completes authentication and profile selection.
    pub fn connect() -> Result<Self> {
        let base = settings::base_url()?;
        let is_v2 = detect_version(&base);
        let mut client = Self {
            base,
            prefix: if is_v2 { "/api/v2" } else { "/api/v1" },
            is_v2,
            token: String::new(),
            profile_id: String::new(),
            profile_token: None,
            image_size: settings::image_size(),
        };
        client.authenticate(false)?;
        client.resolve_profile()?;
        Ok(client)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}{}", self.base, self.prefix, path)
    }

    fn public_request(&self, method: HttpMethod, path: &str) -> Result<Request> {
        Request::new(self.url(path), method).map_err(|e| error!("Invalid Silo request: {e:?}"))
    }

    fn authed_request(&self, method: HttpMethod, path: &str) -> Result<Request> {
        let mut req = self.public_request(method, path)?;
        req = req.header("Accept", "application/json");
        let authorization = format!("Bearer {}", self.token);
        req = req.header("Authorization", authorization.as_str());
        if !self.profile_id.is_empty() {
            req = req.header("X-Profile-Id", self.profile_id.as_str());
        }
        if let Some(token) = &self.profile_token {
            req = req.header("X-Profile-Token", token.as_str());
        }
        Ok(req)
    }

    fn authenticate(&mut self, force: bool) -> Result<()> {
        if settings::auth_mode() == "apiKey" {
            let key = settings::api_key();
            if key.is_empty() {
                bail!("Set a Silo API key in the source settings.");
            }
            self.token = key;
            return Ok(());
        }

        if !force {
            let token = defaults_get::<String>(ACCESS_TOKEN_KEY);
            let expiry = defaults_get::<i32>(TOKEN_EXPIRY_KEY);
            if let (Some(token), Some(expiry)) = (token, expiry)
                && !token.is_empty()
                && current_date() < (expiry as i64) - 60
            {
                self.token = token;
                return Ok(());
            }
            if self.refresh_session() {
                return Ok(());
            }
        }

        let (username, password) = settings::credentials()
            .ok_or_else(|| error!("Sign in to Silo in the source settings."))?;
        let body = serde_json::to_string(&LoginBody {
            username: &username,
            password: &password,
        })
        .map_err(|e| error!("Failed to encode login: {e:?}"))?;
        let response = self
            .public_request(HttpMethod::Post, "/auth/login")?
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .map_err(|e| error!("Could not reach the Silo server: {e:?}"))?;
        let status = response.status_code();
        if status == 401 || status == 403 {
            bail!("Silo rejected those credentials.");
        }
        if !(200..300).contains(&status) {
            let text = response.get_string().unwrap_or_default();
            bail!("Silo login failed ({status}): {}", truncate(&text, 200));
        }
        let parsed: LoginResponse = response
            .get_json_owned()
            .map_err(|e| error!("Unexpected login response: {e:?}"))?;
        self.token = parsed.access_token.clone();
        store_tokens(&parsed);
        Ok(())
    }

    fn refresh_session(&mut self) -> bool {
        let refresh = defaults_get::<String>(REFRESH_TOKEN_KEY).unwrap_or_default();
        if refresh.is_empty() {
            return false;
        }
        let body = match serde_json::to_string(&RefreshBody {
            refresh_token: &refresh,
        }) {
            Ok(body) => body,
            Err(_) => return false,
        };
        let Ok(request) = self.public_request(HttpMethod::Post, "/auth/refresh") else {
            return false;
        };
        let Ok(response) = request
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .send()
        else {
            return false;
        };
        if !(200..300).contains(&response.status_code()) {
            return false;
        }
        let Ok(parsed) = response.get_json_owned::<LoginResponse>() else {
            return false;
        };
        if parsed.access_token.is_empty() {
            return false;
        }
        self.token = parsed.access_token.clone();
        store_tokens(&parsed);
        true
    }

    fn resolve_profile(&mut self) -> Result<()> {
        let list: ProfileListResponse = self.json_get("/profiles")?;
        let profiles = list.into_vec();
        if profiles.is_empty() {
            bail!("This Silo account has no profiles.");
        }
        let hint = settings::profile_hint();
        let selected = if hint.is_empty() {
            profiles
                .iter()
                .find(|p| p.is_primary)
                .or_else(|| profiles.first())
                .unwrap()
        } else {
            profiles
                .iter()
                .find(|p| p.id.as_string() == hint || p.name.eq_ignore_ascii_case(&hint))
                .ok_or_else(|| error!("Profile '{hint}' was not found on the Silo server."))?
        };

        let mut profile_token = None;
        if selected.has_pin && settings::auth_mode() != "apiKey" {
            let pin = settings::pin().ok_or_else(|| {
                error!(
                    "Profile '{}' is PIN protected. Set the PIN in the source settings.",
                    selected.name
                )
            })?;
            let body = serde_json::to_string(&PinBody { pin: &pin })
                .map_err(|e| error!("Failed to encode PIN: {e:?}"))?;
            let path = format!(
                "/profiles/{}/verify-pin",
                encode_uri_component(selected.id.as_string())
            );
            let response: VerifyPinResponse = self.json_post(&path, body)?;
            if !response.valid {
                bail!("Incorrect PIN for profile '{}'.", selected.name);
            }
            profile_token = response.profile_token;
        }

        self.profile_id = selected.id.as_string();
        self.profile_token = profile_token.clone();
        defaults_set(
            PROFILE_ID_KEY,
            DefaultValue::String(self.profile_id.clone()),
        );
        match profile_token {
            Some(token) => defaults_set(PROFILE_TOKEN_KEY, DefaultValue::String(token)),
            None => defaults_set(PROFILE_TOKEN_KEY, DefaultValue::Null),
        }
        Ok(())
    }

    fn json<T: DeserializeOwned>(
        &mut self,
        method: HttpMethod,
        path: &str,
        body: Option<String>,
    ) -> Result<T> {
        let mut attempts = 0;
        loop {
            let request = self.authed_request(method, path)?;
            let request = match &body {
                Some(body) => request
                    .header("Content-Type", "application/json")
                    .body(body.clone()),
                None => request,
            };
            let response = request
                .send()
                .map_err(|e| error!("Could not reach the Silo server: {e:?}"))?;
            let status = response.status_code();
            if status == 401 && attempts == 0 {
                attempts += 1;
                self.authenticate(true)?;
                self.resolve_profile()?;
                continue;
            }
            if !(200..300).contains(&status) {
                let text = response.get_string().unwrap_or_default();
                bail!("Silo request failed ({status}): {}", truncate(&text, 240));
            }
            return response
                .get_json_owned()
                .map_err(|e| error!("Unexpected Silo response: {e:?}"));
        }
    }

    fn json_get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
        self.json(HttpMethod::Get, path, None)
    }

    fn json_post<T: DeserializeOwned>(&mut self, path: &str, body: String) -> Result<T> {
        self.json(HttpMethod::Post, path, Some(body))
    }

    fn send_ok(&mut self, method: HttpMethod, path: &str) -> Result<()> {
        let mut attempts = 0;
        loop {
            let response = self
                .authed_request(method, path)?
                .send()
                .map_err(|e| error!("Could not reach the Silo server: {e:?}"))?;
            let status = response.status_code();
            if status == 401 && attempts == 0 {
                attempts += 1;
                self.authenticate(true)?;
                self.resolve_profile()?;
                continue;
            }
            if !(200..300).contains(&status) {
                let text = response.get_string().unwrap_or_default();
                bail!("Silo request failed ({status}): {}", truncate(&text, 200));
            }
            return Ok(());
        }
    }

    fn bytes(&mut self, path: &str) -> Result<Vec<u8>> {
        let mut attempts = 0;
        loop {
            let response = self
                .authed_request(HttpMethod::Get, path)?
                .send()
                .map_err(|e| error!("Could not reach the Silo server: {e:?}"))?;
            let status = response.status_code();
            if status == 401 && attempts == 0 {
                attempts += 1;
                self.authenticate(true)?;
                self.resolve_profile()?;
                continue;
            }
            if !(200..300).contains(&status) {
                let text = response.get_string().unwrap_or_default();
                bail!("Silo request failed ({status}): {}", truncate(&text, 200));
            }
            return response
                .get_data()
                .map_err(|e| error!("Failed to read Silo response: {e:?}"));
        }
    }

    // ----- endpoints -----

    /// Enabled libraries this account can access, filtered to manga/comic
    /// libraries. Silo scans both manga and western comics as `manga` type.
    pub fn manga_libraries(&mut self) -> Result<Vec<Library>> {
        let list: ListOrItems<Library> = self.json_get("/user/libraries")?;
        Ok(list
            .into_vec()
            .into_iter()
            .filter(|library| library.kind.eq_ignore_ascii_case("manga"))
            .collect())
    }

    pub fn catalog(&mut self, query: &Query) -> Result<CatalogResponse> {
        self.json_get(&format!("/catalog?{}", query.encode()))
    }

    pub fn item(&mut self, content_id: &str) -> Result<ItemDetail> {
        let mut query = Query::new();
        query.add("image_size", &self.image_size);
        self.json_get(&format!(
            "/catalog/items/{}?{}",
            encode_uri_component(content_id),
            query.encode()
        ))
    }

    pub fn catalog_filters(&mut self, library_id: Option<&str>) -> Result<FiltersResponse> {
        let mut query = Query::new();
        query.add("type", "manga");
        if let Some(id) = library_id {
            query.add("library_id", id);
        }
        self.json_get(&format!("/catalog/filters?{}", query.encode()))
    }

    pub fn library_sections(&mut self, library_id: &str) -> Result<SectionResponse> {
        let mut query = Query::new();
        query.add("image_size", &self.image_size);
        self.json_get(&format!(
            "/library/{}/sections?{}",
            encode_uri_component(library_id),
            query.encode()
        ))
    }

    /// Downloads a whole chapter archive (CBZ). Silo has no per-page endpoint.
    pub fn chapter_archive(&mut self, content_id: &str, file_id: &str) -> Result<Vec<u8>> {
        self.bytes(&format!(
            "/ebooks/{}/files/{}/read",
            encode_uri_component(content_id),
            encode_uri_component(file_id)
        ))
    }

    pub fn mark_read(&mut self, content_id: &str) -> Result<()> {
        self.send_ok(
            HttpMethod::Post,
            &format!("/watched/{}", encode_uri_component(content_id)),
        )
    }
}

fn detect_version(base: &str) -> bool {
    match settings::api_version().as_str() {
        "v2" => true,
        "v1" => false,
        _ => {
            let url = format!("{base}/api/v2/system/info");
            match Request::get(url) {
                Ok(request) => match request.header("Accept", "application/json").send() {
                    Ok(response) => response.status_code() == 200,
                    Err(_) => false,
                },
                Err(_) => false,
            }
        }
    }
}

fn store_tokens(response: &LoginResponse) {
    defaults_set(
        ACCESS_TOKEN_KEY,
        DefaultValue::String(response.access_token.clone()),
    );
    if let Some(refresh) = &response.refresh_token {
        defaults_set(REFRESH_TOKEN_KEY, DefaultValue::String(refresh.clone()));
    }
    if let Some(expires_in) = response.expires_in {
        let expiry = current_date() + expires_in;
        defaults_set(TOKEN_EXPIRY_KEY, DefaultValue::Int(expiry as i32));
    }
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return String::from(value);
    }
    let mut out = String::new();
    for c in value.chars().take(limit) {
        out.push(c);
    }
    out.push('…');
    out
}

/// Verifies credentials for the `BasicLoginHandler` without touching the
/// cached session (the app calls this when the user submits the login form).
pub fn validate_credentials(
    base: &str,
    is_v2: bool,
    username: &str,
    password: &str,
) -> Result<bool> {
    let prefix = if is_v2 { "/api/v2" } else { "/api/v1" };
    let body = serde_json::to_string(&LoginBody { username, password })
        .map_err(|e| error!("Failed to encode login: {e:?}"))?;
    let response = Request::post(format!("{base}{prefix}/auth/login"))
        .map_err(|e| error!("Invalid Silo request: {e:?}"))?
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .map_err(|e| error!("Could not reach the Silo server: {e:?}"))?;
    Ok((200..300).contains(&response.status_code()))
}

/// The version detection used before authentication (public probe).
pub fn probe_version(base: &str) -> bool {
    detect_version(base)
}

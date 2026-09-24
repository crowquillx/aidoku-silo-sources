use crate::{models::*, settings};
use aidoku::{
	AidokuError, Result,
	alloc::{format, string::String, vec::Vec},
	helpers::uri::encode_uri_component,
	imports::{
		defaults::{DefaultValue, defaults_get, defaults_set},
		net::{HttpMethod, Request, Response},
		std::current_date,
	},
	prelude::*,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

// Session state cached in defaults. Token expiries are stored as strings:
// Aidoku decodes an Int default as Int32 and traps on values whose postcard
// zig-zag encoding exceeds Int32 (any timestamp past ~2034-02).
const ACCESS_TOKEN_KEY: &str = "accessToken";
const REFRESH_TOKEN_KEY: &str = "refreshToken";
const TOKEN_EXPIRY_KEY: &str = "tokenExpiry";
const DETECTED_VERSION_KEY: &str = "detectedVersion";
const PROFILE_ID_KEY: &str = "profileId";
const PROFILE_TOKEN_KEY: &str = "profileToken";
const PROFILE_TOKEN_EXPIRY_KEY: &str = "profileTokenExpiry";
/// Digests of the settings the cached session belongs to, so changing the
/// server, account, API key, profile, or PIN never reuses stale state.
const AUTH_SCOPE_KEY: &str = "authScope";
const PROFILE_SCOPE_KEY: &str = "profileScope";

pub const PAGE_SIZE: i32 = 30;

#[derive(Deserialize, Default)]
#[serde(default)]
struct SystemInfo {
	api_major: i64,
}

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

/// A small query-string builder. Keys are emitted verbatim, values are
/// percent-encoded.
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
	pub fn new(base: String, is_v2: bool) -> Self {
		Self {
			base,
			prefix: if is_v2 { "/api/v2" } else { "/api/v1" },
			is_v2,
			token: String::new(),
			profile_id: String::new(),
			profile_token: None,
			image_size: settings::image_size(),
		}
	}

	/// Creates a client and completes authentication and profile selection.
	/// A warm cache makes this free of network requests.
	pub fn connect() -> Result<Self> {
		let base = settings::base_url()?;
		sync_session(&base, &settings::username());
		let mut last_error = None;
		for &is_v2 in versions(&base) {
			let mut client = Self::new(base.clone(), is_v2);
			match client
				.authenticate(false)
				.and_then(|()| client.select_profile())
			{
				Ok(()) => {
					remember_version(is_v2);
					return Ok(client);
				}
				// Auto-detect falls through to the other API version when this
				// one is missing (404) or retired (410).
				Err(err) if is_missing_api(&err) => {
					println!("[silo] connect failed api={}: {err:?}", client.prefix);
					last_error = Some(err);
				}
				Err(err) => return Err(err),
			}
		}
		Err(last_error.unwrap_or_else(|| error!("Could not connect to the Silo server.")))
	}

	/// The client for a direct image request: cached credentials, refreshed
	/// when the access token or the profile's PIN verification has expired.
	pub fn for_images() -> Self {
		let Ok(base) = settings::base_url() else {
			return Self::new(String::new(), false);
		};
		sync_session(&base, &settings::username());
		let is_v2 = stored(DETECTED_VERSION_KEY).as_deref() == Some("v2");
		let mut client = Self::new(base, is_v2);
		if client.authenticate(false).is_ok() && client.select_profile().is_ok() {
			return client;
		}
		// Best effort: use whatever the cache still holds.
		client.token = stored(ACCESS_TOKEN_KEY).unwrap_or_default();
		client.profile_id = stored(PROFILE_ID_KEY).unwrap_or_default();
		client.profile_token = stored(PROFILE_TOKEN_KEY);
		client
	}

	fn url(&self, path: &str) -> String {
		format!("{}{}{}", self.base, self.prefix, path)
	}

	pub fn file_url(&self, content_id: &str, file_id: &str) -> String {
		self.url(&file_path(content_id, file_id))
	}

	fn public_post(&self, path: &str, body: &str) -> Result<Response> {
		Request::post(self.url(path))
			.map_err(|e| error!("Invalid Silo request: {e:?}"))?
			.header("Content-Type", "application/json")
			.header("Accept", "application/json")
			.body(body)
			.send()
			.map_err(|e| error!("Could not reach the Silo server: {e:?}"))
	}

	/// Adds the session's credentials and profile headers to a request.
	pub fn authorize(&self, mut request: Request) -> Request {
		if !self.token.is_empty() {
			let authorization = format!("Bearer {}", self.token);
			request = request.header("Authorization", authorization.as_str());
		}
		if !self.profile_id.is_empty() {
			request = request.header("X-Profile-Id", self.profile_id.as_str());
		}
		if let Some(token) = &self.profile_token {
			request = request.header("X-Profile-Token", token.as_str());
		}
		request
	}

	/// Sends an authenticated request and fails on a non-2xx status. With
	/// `retry`, a `401` signs in again and a `401`/`403` re-selects the profile
	/// (an expired PIN verification), then the request is retried once.
	fn send(
		&mut self,
		method: HttpMethod,
		path: &str,
		customize: &dyn Fn(Request) -> Request,
		retry: bool,
	) -> Result<Response> {
		let mut retry = retry;
		loop {
			let request = Request::new(self.url(path), method)
				.map_err(|e| error!("Invalid Silo request: {e:?}"))?;
			let response = customize(self.authorize(request))
				.send()
				.map_err(|e| error!("Could not reach the Silo server: {e:?}"))?;
			let status = response.status_code();
			if retry && (status == 401 || status == 403) {
				retry = false;
				if status == 401 {
					self.authenticate(true)?;
				}
				self.resolve_profile()?;
				continue;
			}
			if !(200..300).contains(&status) {
				return Err(status_error("Silo request failed", status, &response));
			}
			return Ok(response);
		}
	}

	fn json<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
		self.send(
			HttpMethod::Get,
			path,
			&|request| request.header("Accept", "application/json"),
			true,
		)?
		.get_json_owned()
		.map_err(|e| error!("Unexpected Silo response: {e:?}"))
	}

	fn authenticate(&mut self, force: bool) -> Result<()> {
		if settings::use_api_key() {
			self.token = settings::api_key();
			if self.token.is_empty() {
				bail!("Set a Silo API key in the source settings.");
			}
			return Ok(());
		}
		if !force {
			if let Some(token) = stored(ACCESS_TOKEN_KEY).filter(|_| unexpired(TOKEN_EXPIRY_KEY)) {
				self.token = token;
				return Ok(());
			}
			if self.refresh_session() {
				return Ok(());
			}
		}
		let (username, password) = settings::credentials()
			.ok_or_else(|| error!("Sign in to Silo in the source settings."))?;
		let tokens = self
			.login(&username, &password)?
			.ok_or_else(|| error!("Silo rejected those credentials."))?;
		self.token = tokens.access_token.clone();
		store_tokens(&tokens, true);
		Ok(())
	}

	/// Opens a login session; `None` when Silo rejects the credentials.
	fn login(&self, username: &str, password: &str) -> Result<Option<LoginResponse>> {
		let body = serde_json::to_string(&LoginBody { username, password })
			.map_err(|e| error!("Failed to encode login: {e:?}"))?;
		let response = self.public_post("/auth/login", &body)?;
		match response.status_code() {
			401 | 403 => Ok(None),
			200..=299 => response
				.get_json_owned()
				.map(Some)
				.map_err(|e| error!("Unexpected login response: {e:?}")),
			status => Err(status_error("Silo login failed", status, &response)),
		}
	}

	fn refresh_session(&mut self) -> bool {
		let Some(refresh) = stored(REFRESH_TOKEN_KEY) else {
			return false;
		};
		let Ok(body) = serde_json::to_string(&RefreshBody {
			refresh_token: &refresh,
		}) else {
			return false;
		};
		let Ok(response) = self.public_post("/auth/refresh", &body) else {
			return false;
		};
		let status = response.status_code();
		if status == 401 || status == 403 {
			// The session was revoked; stop retrying it.
			clear(REFRESH_TOKEN_KEY);
		}
		if !(200..300).contains(&status) {
			return false;
		}
		match response.get_json_owned::<LoginResponse>() {
			Ok(tokens) if !tokens.access_token.is_empty() => {
				self.token = tokens.access_token.clone();
				store_tokens(&tokens, false);
				true
			}
			_ => false,
		}
	}

	/// Uses the cached profile while its PIN verification is still valid.
	fn select_profile(&mut self) -> Result<()> {
		let Some(profile_id) = stored(PROFILE_ID_KEY) else {
			return self.resolve_profile();
		};
		let profile_token = stored(PROFILE_TOKEN_KEY);
		if profile_token.is_some() && !unexpired(PROFILE_TOKEN_EXPIRY_KEY) {
			return self.resolve_profile();
		}
		self.profile_id = profile_id;
		self.profile_token = profile_token;
		Ok(())
	}

	/// Picks the configured profile and verifies its PIN. Never retries, so an
	/// invalid credential fails instead of recursing through `send`.
	fn resolve_profile(&mut self) -> Result<()> {
		self.profile_id.clear();
		self.profile_token = None;
		let list: ProfileListResponse = self
			.send(
				HttpMethod::Get,
				"/profiles",
				&|request| request.header("Accept", "application/json"),
				false,
			)?
			.get_json_owned()
			.map_err(|e| error!("Unexpected Silo response: {e:?}"))?;
		let profiles = list.into_vec();
		let hint = settings::profile_hint();
		let selected = if hint.is_empty() {
			profiles.iter().find(|p| p.is_primary).or(profiles.first())
		} else {
			profiles
				.iter()
				.find(|p| p.id.as_string() == hint || p.name.eq_ignore_ascii_case(&hint))
		}
		.ok_or_else(|| match hint.is_empty() {
			true => error!("This Silo account has no profiles."),
			false => error!("Profile '{hint}' was not found on the Silo server."),
		})?;
		let profile_id = selected.id.as_string();

		// API keys skip profile PIN prompts.
		let mut verification = None;
		if selected.has_pin && !settings::use_api_key() {
			let pin = settings::pin();
			if pin.is_empty() {
				bail!(
					"Profile '{}' is PIN protected. Set the PIN in the source settings.",
					selected.name
				);
			}
			let body = serde_json::to_string(&PinBody { pin: &pin })
				.map_err(|e| error!("Failed to encode PIN: {e:?}"))?;
			let path = format!("/profiles/{}/verify-pin", encode_uri_component(&profile_id));
			let response: VerifyPinResponse = self
				.send(
					HttpMethod::Post,
					&path,
					&|request| {
						request
							.header("Accept", "application/json")
							.header("Content-Type", "application/json")
							.body(body.as_str())
					},
					false,
				)?
				.get_json_owned()
				.map_err(|e| error!("Unexpected Silo response: {e:?}"))?;
			if !response.valid {
				bail!("Incorrect PIN for profile '{}'.", selected.name);
			}
			verification = Some(response);
		}

		defaults_set(PROFILE_ID_KEY, DefaultValue::String(profile_id.clone()));
		let token = verification.as_ref().and_then(|v| v.profile_token.clone());
		match &token {
			Some(token) => defaults_set(PROFILE_TOKEN_KEY, DefaultValue::String(token.clone())),
			None => clear(PROFILE_TOKEN_KEY),
		}
		// A token without an expiry is durable until the session ends.
		let expiry = verification
			.and_then(|v| v.expires_at)
			.and_then(|at| at.as_str().and_then(parse_utc))
			.unwrap_or(i64::MAX);
		defaults_set(
			PROFILE_TOKEN_EXPIRY_KEY,
			DefaultValue::String(format!("{expiry}")),
		);
		self.profile_id = profile_id;
		self.profile_token = token;
		Ok(())
	}

	// ----- endpoints -----

	/// Enabled libraries this account can access, filtered to manga/comic
	/// libraries. Silo scans both manga and western comics as `manga` type.
	pub fn manga_libraries(&mut self) -> Result<Vec<Library>> {
		let list: ListOrItems<Library> = self.json("/user/libraries")?;
		Ok(list
			.into_vec()
			.into_iter()
			.filter(|library| library.kind.eq_ignore_ascii_case("manga"))
			.collect())
	}

	pub fn catalog(&mut self, query: &Query) -> Result<CatalogResponse> {
		self.json(&format!("/catalog?{}", query.encode()))
	}

	pub fn item(&mut self, content_id: &str) -> Result<ItemDetail> {
		self.json(&format!(
			"/catalog/items/{}?image_size={}",
			encode_uri_component(content_id),
			encode_uri_component(&self.image_size)
		))
	}

	pub fn catalog_filters(&mut self) -> Result<FiltersResponse> {
		self.json("/catalog/filters?type=manga&skip_technical=true")
	}

	pub fn library_sections(&mut self, library_id: &str) -> Result<SectionResponse> {
		self.json(&format!(
			"/library/{}/sections?image_size={}",
			encode_uri_component(library_id),
			encode_uri_component(&self.image_size)
		))
	}

	/// Fetches a byte range of a chapter archive. `start` is absolute; `end` is
	/// inclusive. A `206` carries the resolved offset and archive length in its
	/// `Content-Range`; a `200` is the whole archive.
	pub fn chapter_range(
		&mut self,
		content_id: &str,
		file_id: &str,
		start: u64,
		end: Option<u64>,
	) -> Result<RangeData> {
		let range = match end {
			Some(end) => format!("bytes={start}-{end}"),
			None => format!("bytes={start}-"),
		};
		let response = self.send(
			HttpMethod::Get,
			&file_path(content_id, file_id),
			&|request| request.header("Range", range.as_str()),
			true,
		)?;
		let status = response.status_code();
		let content_range = response.get_header("Content-Range");
		let data = response
			.get_data()
			.map_err(|e| error!("Failed to read Silo response: {e:?}"))?;
		let (start, total) = if status == 206 {
			parse_content_range(content_range.as_deref(), start)
		} else {
			(0, Some(data.len() as u64))
		};
		Ok(RangeData { data, start, total })
	}

	/// The installation ID of an enabled plugin, from the user plugin list.
	/// Silo lists plugins with user settings or a user navigation route.
	pub fn plugin_installation(&mut self, plugin_id: &str) -> Result<Option<String>> {
		let list: ListOrItems<PluginInstallation> = self.json("/settings/plugins")?;
		Ok(list
			.into_vec()
			.into_iter()
			.find(|installation| installation.plugin_id == plugin_id)
			.map(|installation| installation.id.as_string()))
	}

	pub fn mark_read(&mut self, content_id: &str) -> Result<()> {
		let path = format!("/watched/{}", encode_uri_component(content_id));
		self.send(HttpMethod::Post, &path, &|request| request, true)
			.map(|_| ())
	}
}

/// Verifies credentials for the `BasicLoginHandler` and keeps the resulting
/// session, replacing any cached one.
pub fn validate_login(username: &str, password: &str) -> Result<bool> {
	let base = settings::base_url()?;
	let mut last_error = None;
	for &is_v2 in versions(&base) {
		match Client::new(base.clone(), is_v2).login(username, password) {
			Ok(Some(tokens)) => {
				sync_session(&base, username.trim());
				store_tokens(&tokens, true);
				remember_version(is_v2);
				return Ok(true);
			}
			Ok(None) => return Ok(false),
			Err(err) if is_missing_api(&err) => last_error = Some(err),
			Err(err) => return Err(err),
		}
	}
	Err(last_error.unwrap_or_else(|| error!("Could not reach the Silo server.")))
}

/// The API versions to try, in order (`true` is v2).
fn versions(base: &str) -> &'static [bool] {
	match settings::api_version().as_str() {
		"v2" => &[true],
		"v1" => &[false],
		// A cached v2 detection skips the probe; v1 keeps probing so an
		// upgraded server moves to v2.
		_ if stored(DETECTED_VERSION_KEY).as_deref() == Some("v2") || probe_v2(base) => {
			&[true, false]
		}
		_ => &[false, true],
	}
}

/// Whether the server publishes a real v2 discovery document, not an SPA or
/// WAF page that happens to answer 200.
fn probe_v2(base: &str) -> bool {
	Request::get(format!("{base}/api/v2/system/info"))
		.ok()
		.and_then(|request| request.header("Accept", "application/json").send().ok())
		.filter(|response| response.status_code() == 200)
		.and_then(|response| response.get_json_owned::<SystemInfo>().ok())
		.is_some_and(|info| info.api_major == 2)
}

fn remember_version(is_v2: bool) {
	let version = if is_v2 { "v2" } else { "v1" };
	defaults_set(
		DETECTED_VERSION_KEY,
		DefaultValue::String(String::from(version)),
	);
}

fn status_error(context: &str, status: i32, response: &Response) -> AidokuError {
	let text = response.get_string().unwrap_or_default();
	error!("{context} ({status}): {}", truncate(&text, 200))
}

fn is_missing_api(error: &AidokuError) -> bool {
	matches!(error, AidokuError::Message(message)
		if message.contains("(404)") || message.contains("(410)"))
}

fn file_path(content_id: &str, file_id: &str) -> String {
	format!(
		"/ebooks/{}/files/{}/read",
		encode_uri_component(content_id),
		encode_uri_component(file_id)
	)
}

/// Clears a cached value. An empty string rather than `Null`, which the
/// aidoku test runner cannot store; `stored` treats both as unset.
fn clear(key: &str) {
	defaults_set(key, DefaultValue::String(String::new()));
}

fn stored(key: &str) -> Option<String> {
	defaults_get::<String>(key).filter(|value| !value.is_empty())
}

/// Whether the timestamp stored under `key` is more than a minute away.
fn unexpired(key: &str) -> bool {
	stored(key)
		.and_then(|value| value.parse::<i64>().ok())
		.is_some_and(|expiry| current_date() < expiry.saturating_sub(60))
}

fn store_tokens(tokens: &LoginResponse, new_session: bool) {
	defaults_set(
		ACCESS_TOKEN_KEY,
		DefaultValue::String(tokens.access_token.clone()),
	);
	if let Some(refresh) = &tokens.refresh_token {
		defaults_set(REFRESH_TOKEN_KEY, DefaultValue::String(refresh.clone()));
	}
	// An unknown lifetime is not trusted: the next call refreshes.
	let expiry = tokens
		.expires_in
		.map_or(0, |seconds| current_date().saturating_add(seconds));
	defaults_set(TOKEN_EXPIRY_KEY, DefaultValue::String(format!("{expiry}")));
	// Profile tokens are bound to the login session that verified the PIN.
	if new_session {
		defaults_set(
			PROFILE_TOKEN_EXPIRY_KEY,
			DefaultValue::String(String::from("0")),
		);
	}
}

/// Clears cached session state whose settings changed since it was stored.
fn sync_session(base: &str, username: &str) {
	let api_key = if settings::use_api_key() {
		settings::api_key()
	} else {
		String::new()
	};
	let auth = digest(&[base, username, &api_key]);
	let profile = digest(&[&auth, &settings::profile_hint(), &settings::pin()]);
	reset_if_changed(
		AUTH_SCOPE_KEY,
		auth,
		&[
			ACCESS_TOKEN_KEY,
			REFRESH_TOKEN_KEY,
			TOKEN_EXPIRY_KEY,
			DETECTED_VERSION_KEY,
		],
	);
	reset_if_changed(
		PROFILE_SCOPE_KEY,
		profile,
		&[PROFILE_ID_KEY, PROFILE_TOKEN_KEY, PROFILE_TOKEN_EXPIRY_KEY],
	);
}

fn reset_if_changed(scope_key: &str, scope: String, keys: &[&str]) {
	if stored(scope_key).as_deref() == Some(scope.as_str()) {
		return;
	}
	for key in keys {
		clear(key);
	}
	defaults_set(scope_key, DefaultValue::String(scope));
}

/// A 64-bit FNV-1a digest, so the stored scope never holds a secret.
fn digest(parts: &[&str]) -> String {
	let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
	for part in parts {
		for byte in part.bytes().chain([0]) {
			hash = (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3);
		}
	}
	format!("{hash:016x}")
}

/// Parses the UTC `YYYY-MM-DDTHH:MM:SS` prefix of an RFC 3339 timestamp into
/// Unix seconds. Silo emits these in UTC (`Z`).
pub fn parse_utc(value: &str) -> Option<i64> {
	let field = |range: core::ops::Range<usize>| -> Option<i64> { value.get(range)?.parse().ok() };
	let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
	let seconds = field(11..13)? * 3_600 + field(14..16)? * 60 + field(17..19)?;
	// Days since the Unix epoch from a civil date (Howard Hinnant's algorithm).
	let year = if month <= 2 { year - 1 } else { year };
	let era = year.div_euclid(400);
	let year_of_era = year - era * 400;
	let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
	let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
	Some((era * 146_097 + day_of_era - 719_468) * 86_400 + seconds)
}

fn truncate(value: &str, limit: usize) -> String {
	match value.char_indices().nth(limit) {
		Some((index, _)) => format!("{}…", &value[..index]),
		None => String::from(value),
	}
}

/// A fetched archive byte range.
pub struct RangeData {
	pub data: Vec<u8>,
	/// Absolute offset the returned bytes start at.
	pub start: u64,
	/// The archive's total length, when the server reported it.
	pub total: Option<u64>,
}

/// Parses `bytes <start>-<end>/<total>` into the start offset and total.
fn parse_content_range(header: Option<&str>, fallback_start: u64) -> (u64, Option<u64>) {
	let Some(rest) = header.and_then(|value| value.trim().strip_prefix("bytes ")) else {
		return (fallback_start, None);
	};
	let (range, total) = rest.split_once('/').unwrap_or((rest, "*"));
	let start = range
		.split('-')
		.next()
		.and_then(|part| part.trim().parse().ok())
		.unwrap_or(fallback_start);
	(start, total.trim().parse().ok())
}

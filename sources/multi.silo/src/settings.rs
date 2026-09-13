use aidoku::{Result, alloc::string::String, imports::defaults::defaults_get, prelude::*};

pub const BASE_URL_KEY: &str = "baseUrl";
pub const API_VERSION_KEY: &str = "apiVersion";
pub const AUTH_MODE_KEY: &str = "authMode";
pub const USERNAME_KEY: &str = "credentials.username";
pub const PASSWORD_KEY: &str = "credentials.password";
pub const API_KEY_KEY: &str = "apiKey";
pub const PROFILE_KEY: &str = "profile";
pub const PIN_KEY: &str = "pin";
pub const IMAGE_SIZE_KEY: &str = "imageSize";
pub const MARK_READ_KEY: &str = "markReadOnOpen";
pub const COMIC_PAGES_KEY: &str = "comicPagesPlugin";

/// The configured Silo server base URL, normalized without a trailing slash.
pub fn base_url() -> Result<String> {
	let url = defaults_get::<String>(BASE_URL_KEY).unwrap_or_default();
	let url = url.trim().trim_end_matches('/');
	if url.is_empty() {
		bail!("Silo is not configured. Set your server URL in the source settings.");
	}
	Ok(String::from(url))
}

/// The configured API version: `auto`, `v2`, or `v1`.
pub fn api_version() -> String {
	non_empty(defaults_get::<String>(API_VERSION_KEY)).unwrap_or_else(|| String::from("auto"))
}

/// The configured auth mode: `credentials` or `apiKey`.
pub fn auth_mode() -> String {
	non_empty(defaults_get::<String>(AUTH_MODE_KEY)).unwrap_or_else(|| String::from("credentials"))
}

pub fn credentials() -> Option<(String, String)> {
	let user = defaults_get::<String>(USERNAME_KEY)?;
	let pass = defaults_get::<String>(PASSWORD_KEY).unwrap_or_default();
	if user.is_empty() {
		return None;
	}
	Some((user, pass))
}

pub fn api_key() -> String {
	non_empty(defaults_get::<String>(API_KEY_KEY)).unwrap_or_default()
}

pub fn profile_hint() -> String {
	non_empty(defaults_get::<String>(PROFILE_KEY)).unwrap_or_default()
}

pub fn pin() -> Option<String> {
	non_empty(defaults_get::<String>(PIN_KEY))
}

pub fn image_size() -> String {
	non_empty(defaults_get::<String>(IMAGE_SIZE_KEY)).unwrap_or_else(|| String::from("large"))
}

pub fn mark_read_on_open() -> bool {
	defaults_get::<bool>(MARK_READ_KEY).unwrap_or(true)
}

pub fn comic_pages_plugin() -> String {
	non_empty(defaults_get::<String>(COMIC_PAGES_KEY)).unwrap_or_default()
}

fn non_empty(value: Option<String>) -> Option<String> {
	match value {
		Some(v) if !v.trim().is_empty() => Some(v),
		_ => None,
	}
}

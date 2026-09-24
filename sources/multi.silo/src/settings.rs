use aidoku::{
	Result,
	alloc::string::String,
	imports::defaults::{DefaultValue, defaults_get, defaults_set},
	prelude::*,
};

const BASE_URL_KEY: &str = "baseUrl";
const API_VERSION_KEY: &str = "apiVersion";
const USE_API_KEY_KEY: &str = "useApiKey";
/// Source versions before 11 stored the auth choice in an `authMode` select.
const LEGACY_AUTH_MODE_KEY: &str = "authMode";
const USERNAME_KEY: &str = "credentials.username";
const PASSWORD_KEY: &str = "credentials.password";
const API_KEY_KEY: &str = "apiKey";
const PROFILE_KEY: &str = "profile";
const PIN_KEY: &str = "pin";
const IMAGE_SIZE_KEY: &str = "imageSize";
const MARK_READ_KEY: &str = "markReadOnOpen";
const USE_COMIC_PAGES_KEY: &str = "useComicPages";
const COMIC_PAGES_KEY: &str = "comicPagesPlugin";

/// The configured Silo server base URL, normalized without a trailing slash.
pub fn base_url() -> Result<String> {
	let url = string(BASE_URL_KEY);
	let url = url.trim_end_matches('/');
	if url.is_empty() {
		bail!("Silo is not configured. Set your server URL in the source settings.");
	}
	Ok(String::from(url))
}

/// The configured API version: `auto`, `v2`, or `v1`.
pub fn api_version() -> String {
	or(string(API_VERSION_KEY), "auto")
}

/// Whether the source authenticates with an API key instead of a login.
pub fn use_api_key() -> bool {
	let legacy = string(LEGACY_AUTH_MODE_KEY);
	if !legacy.is_empty() {
		defaults_set(USE_API_KEY_KEY, DefaultValue::Bool(legacy == "apiKey"));
		defaults_set(LEGACY_AUTH_MODE_KEY, DefaultValue::String(String::new()));
	}
	defaults_get::<bool>(USE_API_KEY_KEY).unwrap_or(false)
}

pub fn username() -> String {
	string(USERNAME_KEY)
}

pub fn credentials() -> Option<(String, String)> {
	let user = username();
	(!user.is_empty()).then(|| {
		(
			user,
			defaults_get::<String>(PASSWORD_KEY).unwrap_or_default(),
		)
	})
}

pub fn api_key() -> String {
	string(API_KEY_KEY)
}

pub fn profile_hint() -> String {
	string(PROFILE_KEY)
}

pub fn pin() -> String {
	string(PIN_KEY)
}

pub fn image_size() -> String {
	or(string(IMAGE_SIZE_KEY), "large")
}

pub fn mark_read_on_open() -> bool {
	defaults_get::<bool>(MARK_READ_KEY).unwrap_or(true)
}

pub fn use_comic_pages() -> bool {
	defaults_get::<bool>(USE_COMIC_PAGES_KEY).unwrap_or(true)
}

/// A manually entered Comic Pages installation ID, which overrides detection.
pub fn comic_pages_plugin() -> String {
	string(COMIC_PAGES_KEY)
}

/// A trimmed string setting, empty when unset.
fn string(key: &str) -> String {
	defaults_get::<String>(key)
		.map(|value| String::from(value.trim()))
		.unwrap_or_default()
}

fn or(value: String, fallback: &str) -> String {
	if value.is_empty() {
		String::from(fallback)
	} else {
		value
	}
}

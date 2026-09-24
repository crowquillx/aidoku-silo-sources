//! The subset of Silo's v1 and v2 response shapes the source reads. Every
//! struct defaults missing fields so either API version deserializes.
use aidoku::alloc::{format, string::String, vec::Vec};
use serde::{Deserialize, Deserializer};

/// Deserializes `null` as the type's default (Silo emits `null` for empty
/// arrays on some v1 item responses).
fn null_to_default<'de, D, T>(deserializer: D) -> core::result::Result<T, D::Error>
where
	D: Deserializer<'de>,
	T: Deserialize<'de> + Default,
{
	Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// An identifier that is a JSON string on v2 and a JSON number on v1.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Id {
	Str(String),
	Num(i64),
}

impl Default for Id {
	fn default() -> Self {
		Id::Str(String::new())
	}
}

impl Id {
	pub fn as_string(&self) -> String {
		match self {
			Id::Str(s) => s.clone(),
			Id::Num(n) => format!("{n}"),
		}
	}
}

/// A JSON array on v1 or a `{ "items": [...] }` envelope on v2.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ListOrItems<T> {
	List(Vec<T>),
	Items { items: Vec<T> },
}

impl<T> ListOrItems<T> {
	pub fn into_vec(self) -> Vec<T> {
		match self {
			ListOrItems::List(v) => v,
			ListOrItems::Items { items } => items,
		}
	}
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Library {
	pub id: Id,
	pub name: String,
	#[serde(rename = "type")]
	pub kind: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct LoginResponse {
	pub access_token: String,
	pub refresh_token: Option<String>,
	pub expires_in: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Profile {
	pub id: Id,
	pub name: String,
	pub has_pin: bool,
	pub is_primary: bool,
}

/// v1 returns `{ "profiles": [...] }`, v2 returns `{ "items": [...] }`.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ProfileListResponse {
	profiles: Option<Vec<Profile>>,
	items: Option<Vec<Profile>>,
}

impl ProfileListResponse {
	pub fn into_vec(self) -> Vec<Profile> {
		self.profiles.or(self.items).unwrap_or_default()
	}
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct VerifyPinResponse {
	pub valid: bool,
	pub profile_token: Option<String>,
	/// An RFC 3339 instant, or `null` for a token that does not expire. Kept
	/// untyped so an unexpected shape cannot fail PIN verification.
	pub expires_at: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Item {
	pub content_id: String,
	pub title: String,
	#[serde(deserialize_with = "null_to_default")]
	pub genres: Vec<String>,
	pub overview: Option<String>,
	pub show_status: Option<String>,
	pub poster_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PageInfo {
	pub has_more: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct CatalogResponse {
	#[serde(deserialize_with = "null_to_default")]
	pub items: Vec<Item>,
	/// v1 pagination flag.
	pub has_more: Option<bool>,
	/// v2 pagination envelope.
	pub page: Option<PageInfo>,
}

impl CatalogResponse {
	pub fn has_next_page(&self) -> bool {
		match &self.page {
			Some(page) => page.has_more,
			None => self.has_more.unwrap_or(false),
		}
	}
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Crew {
	pub name: String,
	pub job: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct EbookAuthor {
	pub name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Ebook {
	#[serde(deserialize_with = "null_to_default")]
	pub authors: Vec<EbookAuthor>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct FileVersion {
	pub file_id: Id,
	pub container: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct MangaChapter {
	pub content_id: String,
	pub title: String,
	pub chapter_index: Option<f64>,
	pub volume: Option<String>,
	pub poster_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct MangaExtension {
	#[serde(deserialize_with = "null_to_default")]
	pub chapters: Vec<MangaChapter>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ItemDetail {
	pub content_id: String,
	pub title: String,
	#[serde(deserialize_with = "null_to_default")]
	pub genres: Vec<String>,
	pub overview: Option<String>,
	pub show_status: Option<String>,
	pub poster_url: Option<String>,
	pub backdrop_url: Option<String>,
	#[serde(deserialize_with = "null_to_default")]
	pub crew: Vec<Crew>,
	#[serde(deserialize_with = "null_to_default")]
	pub versions: Vec<FileVersion>,
	pub manga: Option<MangaExtension>,
	pub ebook: Option<Ebook>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Section {
	pub id: String,
	pub title: String,
	#[serde(deserialize_with = "null_to_default")]
	pub items: Vec<Item>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct SectionResponse {
	#[serde(deserialize_with = "null_to_default")]
	pub sections: Vec<Section>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct FiltersResponse {
	#[serde(deserialize_with = "null_to_default")]
	pub genres: Vec<String>,
}

/// One entry of `GET /settings/plugins`: an enabled plugin installation.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PluginInstallation {
	pub id: Id,
	pub plugin_id: String,
}

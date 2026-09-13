use aidoku::alloc::{format, string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// Deserializes `null` as the type's default (Silo emits `null` for empty
/// arrays on some v1 item responses).
fn null_to_default<'de, D, T>(deserializer: D) -> core::result::Result<T, D::Error>
where
	D: serde::Deserializer<'de>,
	T: Deserialize<'de> + Default,
{
	Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// An identifier that is a JSON string on v2 and a JSON number on v1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Library {
	#[serde(default)]
	pub id: Id,
	#[serde(default)]
	pub name: String,
	#[serde(default, rename = "type")]
	pub kind: String,
	#[serde(default)]
	pub sort_order: i32,
	#[serde(default)]
	pub poster_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoginResponse {
	#[serde(default)]
	pub access_token: String,
	#[serde(default)]
	pub refresh_token: Option<String>,
	#[serde(default)]
	pub expires_in: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
	#[serde(default)]
	pub id: Id,
	#[serde(default)]
	pub name: String,
	#[serde(default)]
	pub has_pin: bool,
	#[serde(default)]
	pub is_primary: bool,
}

/// v1 returns `{ "profiles": [...] }`, v2 returns `{ "items": [...] }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileListResponse {
	#[serde(default)]
	pub profiles: Option<Vec<Profile>>,
	#[serde(default)]
	pub items: Option<Vec<Profile>>,
}

impl ProfileListResponse {
	pub fn into_vec(self) -> Vec<Profile> {
		self.profiles.or(self.items).unwrap_or_default()
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerifyPinResponse {
	#[serde(default)]
	pub valid: bool,
	#[serde(default)]
	pub profile_token: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserState {
	#[serde(default)]
	pub played: bool,
	#[serde(default)]
	pub is_favorite: bool,
	#[serde(default)]
	pub in_watchlist: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Item {
	#[serde(default)]
	pub content_id: String,
	#[serde(default, rename = "type")]
	pub kind: String,
	#[serde(default)]
	pub title: String,
	#[serde(default)]
	pub year: Option<i32>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub genres: Vec<String>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub keywords: Vec<String>,
	#[serde(default)]
	pub overview: Option<String>,
	#[serde(default)]
	pub show_status: Option<String>,
	#[serde(default)]
	pub content_rating: Option<String>,
	#[serde(default)]
	pub poster_url: Option<String>,
	#[serde(default)]
	pub backdrop_url: Option<String>,
	#[serde(default)]
	pub rating_imdb: Option<f64>,
	#[serde(default)]
	pub manga_chapter_count: Option<i64>,
	#[serde(default)]
	pub manga_volume_count: Option<i64>,
	#[serde(default)]
	pub user_state: Option<UserState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageInfo {
	#[serde(default)]
	pub has_more: bool,
	pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogResponse {
	#[serde(default, deserialize_with = "null_to_default")]
	pub items: Vec<Item>,
	#[serde(default)]
	pub total: Option<i64>,
	/// v1 pagination flag.
	#[serde(default)]
	pub has_more: Option<bool>,
	/// v2 pagination envelope.
	#[serde(default)]
	pub page: Option<PageInfo>,
}

impl CatalogResponse {
	pub fn has_next_page(&self) -> bool {
		if let Some(page) = &self.page {
			return page.has_more;
		}
		self.has_more.unwrap_or(false)
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Crew {
	#[serde(default)]
	pub name: String,
	#[serde(default)]
	pub job: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cast {
	#[serde(default)]
	pub name: String,
	#[serde(default)]
	pub character: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EbookAuthor {
	#[serde(default)]
	pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ebook {
	#[serde(default)]
	pub authors: Vec<EbookAuthor>,
	#[serde(default)]
	pub publisher: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileVersion {
	#[serde(default)]
	pub file_id: Id,
	#[serde(default)]
	pub file_name: Option<String>,
	#[serde(default)]
	pub file_path: Option<String>,
	#[serde(default)]
	pub container: Option<String>,
	#[serde(default)]
	pub file_size: Option<i64>,
	/// Page count for ebook/manga files.
	#[serde(default)]
	pub duration: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MangaChapter {
	#[serde(default)]
	pub content_id: String,
	#[serde(default)]
	pub title: String,
	#[serde(default)]
	pub chapter_index: Option<f64>,
	#[serde(default)]
	pub volume: Option<String>,
	#[serde(default)]
	pub read: bool,
	#[serde(default)]
	pub progress: Option<f64>,
	#[serde(default)]
	pub poster_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MangaExtension {
	#[serde(default, deserialize_with = "null_to_default")]
	pub chapters: Vec<MangaChapter>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ItemDetail {
	#[serde(default)]
	pub content_id: String,
	#[serde(default, rename = "type")]
	pub kind: String,
	#[serde(default)]
	pub title: String,
	#[serde(default)]
	pub original_title: Option<String>,
	#[serde(default)]
	pub year: Option<i32>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub genres: Vec<String>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub keywords: Vec<String>,
	#[serde(default)]
	pub overview: Option<String>,
	#[serde(default)]
	pub show_status: Option<String>,
	#[serde(default)]
	pub content_rating: Option<String>,
	#[serde(default)]
	pub poster_url: Option<String>,
	#[serde(default)]
	pub backdrop_url: Option<String>,
	#[serde(default)]
	pub rating_imdb: Option<f64>,
	#[serde(default)]
	pub user_rating: Option<f64>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub crew: Vec<Crew>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub cast: Vec<Cast>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub versions: Vec<FileVersion>,
	#[serde(default)]
	pub manga: Option<MangaExtension>,
	#[serde(default)]
	pub ebook: Option<Ebook>,
	#[serde(default)]
	pub series_id: Option<String>,
	#[serde(default)]
	pub series_title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Section {
	#[serde(default)]
	pub id: String,
	#[serde(default)]
	pub section_type: String,
	#[serde(default)]
	pub title: String,
	#[serde(default)]
	pub item_limit: Option<i64>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectionResponse {
	#[serde(default, deserialize_with = "null_to_default")]
	pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FiltersResponse {
	#[serde(default, deserialize_with = "null_to_default")]
	pub genres: Vec<String>,
	#[serde(default, deserialize_with = "null_to_default")]
	pub authors: Vec<String>,
}

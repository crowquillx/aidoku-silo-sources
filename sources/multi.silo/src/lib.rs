#![no_std]
extern crate alloc;

mod client;
mod comic_pages;
mod models;
#[cfg(feature = "cbr-native")]
mod rar;
mod settings;
mod zip;

use aidoku::{
	AlternateCoverProvider, BaseUrlProvider, BasicLoginHandler, Chapter, DeepLinkHandler,
	DeepLinkResult, DynamicFilters, DynamicListings, Filter, FilterValue, Home, HomeComponent,
	HomeComponentValue, HomeLayout, ImageRequestProvider, ImageResponse, Link, LinkValue, Listing,
	ListingKind, ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent,
	PageContext, PageImageProcessor, Result, SelectFilter, Source,
	alloc::{borrow::Cow, format, string::String, vec, vec::Vec},
	imports::{canvas::ImageRef, net::Request},
	prelude::*,
};
use client::{Client, PAGE_SIZE, Query};
use models::{CatalogResponse, Item, ItemDetail, Library, MangaChapter};

/// Page context keys carrying one archive entry's location, so a page can be
/// fetched and inflated independently of the rest of the archive.
const OFF_KEY: &str = "silo_off";
const LEN_KEY: &str = "silo_len";
const METHOD_KEY: &str = "silo_method";
const SIZE_KEY: &str = "silo_usize";

/// The Silo sort field and direction (`true` is descending) behind each
/// option of the `sort` filter in `res/filters.json`.
const SORT_FIELDS: [(&str, bool); 7] = [
	("title", false),
	("title", true),
	("added_at", true),
	("added_at", false),
	("release_date", true),
	("rating_imdb", true),
	("author", false),
];

struct Silo;

impl Source for Silo {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		println!("[silo] search query={query:?} page={page}");
		let mut client = Client::connect()?;
		let params = SearchParams::from_filters(&filters);
		let request = search_query(&client, page, query.as_deref(), &params);
		Ok(to_page_result(client.catalog(&request)?))
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		println!(
			"[silo] manga update key={} details={needs_details} chapters={needs_chapters}",
			manga.key
		);
		let mut client = Client::connect()?;
		let detail = client.item(&manga.key)?;
		if needs_details {
			manga.copy_from(detail_to_manga(&detail, &client.base));
		}
		if manga.title.is_empty() {
			manga.title = detail.title.clone();
		}
		if manga.cover.is_none() {
			manga.cover = detail.poster_url.clone();
		}
		if needs_chapters {
			// Aidoku treats source order as newest-first (its default chapter
			// sort is "source order, descending"). Silo returns chapters
			// oldest-first, so reverse them, otherwise Aidoku's "next chapter"
			// logic starts at the last chapter instead of the first.
			let mut chapters: Vec<Chapter> = detail
				.manga
				.iter()
				.flat_map(|extension| extension.chapters.iter().map(chapter_to_aidoku))
				.collect();
			chapters.reverse();
			manga.chapters = Some(chapters);
		}
		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		println!("[silo] page list chapter={}", chapter.key);
		let mut client = Client::connect()?;
		let detail = client.item(&chapter.key)?;
		let version = detail
			.versions
			.first()
			.ok_or_else(|| error!("This chapter has no readable file on the Silo server."))?;
		let container = version
			.container
			.as_deref()
			.unwrap_or_default()
			.to_ascii_lowercase();
		let file_id = version.file_id.as_string();

		// Decide by content, not extension: a ZIP mislabeled `.cbr` still works.
		// The `206` also reports the archive length in its `Content-Range`.
		let head = client.chapter_range(&chapter.key, &file_id, 0, Some(7))?;
		let total = head
			.total
			.ok_or_else(|| error!("The Silo server did not report the archive size."))?;
		let pages = if is_rar_magic(&head.data) {
			rar_pages(&mut client, &chapter.key, &file_id, total)?
		} else if !is_zip_magic(&head.data) {
			bail!(
				"This chapter is a .{container} file, which this source can't render. Use the Silo web reader for it."
			);
		} else if !matches!(container.as_str(), "" | "cbz" | "cbr" | "rar") {
			bail!("This chapter is a .{container} file, not a CBZ comic archive.");
		} else {
			let entries =
				zip::image_pages(load_entries(&mut client, &chapter.key, &file_id, total)?);
			if entries.is_empty() {
				bail!("The comic archive contains no readable images.");
			}
			archive_pages(&client, &chapter.key, &file_id, &entries, |entry| {
				[
					(OFF_KEY, entry.local_offset),
					(LEN_KEY, entry.comp_size),
					(METHOD_KEY, entry.method as u64),
					(SIZE_KEY, entry.uncomp_size),
				]
			})
		};

		// Aidoku also requests page lists to download chapters, so this marks
		// downloaded chapters too (the setting's subtitle says so).
		if settings::mark_read_on_open() {
			let _ = client.mark_read(&chapter.key);
		}
		Ok(pages)
	}
}

impl ListingProvider for Silo {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		println!("[silo] listing id={} page={page}", listing.id);
		let mut client = Client::connect()?;
		let mut query = Query::new();
		query.add_i64("limit", PAGE_SIZE as i64);
		query.add("image_size", &client.image_size);
		if let Some(rest) = listing.id.strip_prefix("section:") {
			// A home section's own items, in the section's order.
			let (library_id, section_id) = rest
				.split_once(':')
				.ok_or_else(|| error!("Unknown listing {}.", listing.id))?;
			query.add("source", "section");
			query.add("scope", "library");
			query.add("library_id", library_id);
			query.add("section_id", section_id);
		} else {
			query.add("type", "manga");
			if let Some(library_id) = listing.id.strip_prefix("library:") {
				query.add("library_id", library_id);
			}
			add_sort(&mut query, &client, "title", false);
		}
		add_pagination(&mut query, &client, page);
		Ok(to_page_result(client.catalog(&query)?))
	}
}

impl DynamicListings for Silo {
	fn get_dynamic_listings(&self) -> Result<Vec<Listing>> {
		println!("[silo] dynamic listings");
		let libraries = Client::connect()?.manga_libraries()?;
		let mut listings = vec![Listing {
			id: String::from("all"),
			name: String::from("All Manga"),
			kind: ListingKind::List,
		}];
		listings.extend(libraries.iter().map(library_listing));
		Ok(listings)
	}
}

impl Home for Silo {
	fn get_home(&self) -> Result<HomeLayout> {
		println!("[silo] home");
		let mut client = Client::connect()?;
		let libraries = client.manga_libraries()?;
		let mut components = Vec::new();
		if !libraries.is_empty() {
			components.push(HomeComponent {
				title: Some(String::from("Libraries")),
				subtitle: None,
				value: HomeComponentValue::Links(
					libraries
						.iter()
						.map(|library| Link {
							title: library.name.clone(),
							value: Some(LinkValue::Listing(library_listing(library))),
							..Default::default()
						})
						.collect(),
				),
			});
		}

		for library in libraries.iter().take(4) {
			let library_id = library.id.as_string();
			let Ok(response) = client.library_sections(&library_id) else {
				continue;
			};
			for section in response.sections.iter().take(6) {
				if section.items.is_empty() {
					continue;
				}
				components.push(HomeComponent {
					title: Some(section.title.clone()),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: section
							.items
							.iter()
							.map(|item| item_to_manga(item).into())
							.collect(),
						listing: Some(Listing {
							id: format!("section:{library_id}:{}", section.id),
							name: section.title.clone(),
							kind: ListingKind::List,
						}),
					},
				});
			}
		}

		Ok(HomeLayout { components })
	}
}

/// Only the genre list comes from the server; the other filters are static
/// in `res/filters.json`.
impl DynamicFilters for Silo {
	fn get_dynamic_filters(&self) -> Result<Vec<Filter>> {
		let genres = Client::connect()
			.and_then(|mut client| client.catalog_filters())
			.map(|filters| filters.genres)
			.unwrap_or_default();
		if genres.is_empty() {
			return Ok(Vec::new());
		}
		Ok(vec![
			SelectFilter {
				id: Cow::Borrowed("genre"),
				title: Some(Cow::Borrowed("Genre")),
				is_genre: true,
				uses_tag_style: true,
				options: genres.into_iter().map(Cow::Owned).collect(),
				..Default::default()
			}
			.into(),
		])
	}
}

/// The configured server, so Aidoku routes links to it to `DeepLinkHandler`.
impl BaseUrlProvider for Silo {
	fn get_base_url(&self) -> Result<String> {
		settings::base_url()
	}
}

impl DeepLinkHandler for Silo {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let segment = |marker: &str| {
			url.split(marker)
				.nth(1)
				.and_then(|rest| rest.split(['?', '#', '/']).next())
				.filter(|value| !value.is_empty())
				.map(String::from)
		};
		if let Some(key) = segment("/item/") {
			return Ok(Some(DeepLinkResult::Manga { key }));
		}
		Ok(segment("/library/").map(|id| {
			DeepLinkResult::Listing(Listing {
				id: format!("library:{id}"),
				name: id,
				kind: ListingKind::List,
			})
		}))
	}
}

impl BasicLoginHandler for Silo {
	fn handle_basic_login(&self, _key: String, username: String, password: String) -> Result<bool> {
		client::validate_login(&username, &password)
	}
}

impl AlternateCoverProvider for Silo {
	fn get_alternate_covers(&self, manga: Manga) -> Result<Vec<String>> {
		let detail = Client::connect()?.item(&manga.key)?;
		let mut covers: Vec<String> = detail.backdrop_url.into_iter().collect();
		if let Some(poster) = detail.poster_url
			&& !covers.contains(&poster)
		{
			covers.push(poster);
		}
		Ok(covers)
	}
}

impl ImageRequestProvider for Silo {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let get =
			|url: String| Request::get(url).map_err(|e| error!("Invalid image request: {e:?}"));
		let Some(context) = context else {
			return get(url);
		};
		if context.contains_key(comic_pages::MARKER) {
			return comic_pages::image_request(&url, &context, 0);
		}
		let Some(offset) = context_u64(&context, OFF_KEY) else {
			return get(url);
		};
		// Overfetch past the local header (name + extra fields) so the entry's
		// data can be located without a second request.
		let length = context_u64(&context, LEN_KEY).unwrap_or(0);
		let end = offset.saturating_add(30 + 2048).saturating_add(length);
		let range = format!("bytes={offset}-{}", end - 1);
		let client = Client::for_images();
		// Never send this server's credentials to a URL from another server.
		if !url.starts_with(&format!("{}/", client.base)) {
			bail!("The Silo server URL changed. Reopen the chapter.");
		}
		Ok(client.authorize(get(url)?.header("Range", range.as_str())))
	}
}

impl PageImageProcessor for Silo {
	fn process_page_image(
		&self,
		response: ImageResponse,
		context: Option<PageContext>,
	) -> Result<ImageRef> {
		let context = context.unwrap_or_default();
		let bytes = response.image.data();
		let page = decode_page(response.code, &bytes, &context)?;
		Ok(ImageRef::new(&page))
	}
}

register_source!(
	Silo,
	ListingProvider,
	Home,
	DynamicListings,
	DynamicFilters,
	BaseUrlProvider,
	DeepLinkHandler,
	BasicLoginHandler,
	AlternateCoverProvider,
	ImageRequestProvider,
	PageImageProcessor
);

// ----- helpers -----

/// Builds one page per archive entry. Each page URL is unique so Aidoku
/// caches pages separately, and its context locates the entry's bytes.
fn archive_pages<T, const N: usize>(
	client: &Client,
	content_id: &str,
	file_id: &str,
	entries: &[T],
	fields: impl Fn(&T) -> [(&'static str, u64); N],
) -> Vec<Page> {
	let url = client.file_url(content_id, file_id);
	entries
		.iter()
		.enumerate()
		.map(|(index, entry)| {
			let context = fields(entry)
				.into_iter()
				.map(|(key, value)| (String::from(key), format!("{value}")))
				.collect();
			Page {
				content: PageContent::url_context(format!("{url}?aidoku_page={index}"), context),
				..Default::default()
			}
		})
		.collect()
}

/// Pages for a RAR archive: extracted on the server by the Comic Pages plugin
/// when configured, otherwise decoded on the device. The device path downloads
/// the archive once and returns every page as an image, so reading makes no
/// further requests (Aidoku spills image pages to disk, and downloads save
/// them as PNG).
fn rar_pages(
	client: &mut Client,
	content_id: &str,
	file_id: &str,
	total: u64,
) -> Result<Vec<Page>> {
	if let Some(installation) = comic_pages::installation(client) {
		return comic_pages::pages(client, &installation, content_id, file_id);
	}
	#[cfg(feature = "cbr-native")]
	{
		if total == 0 || total > rar::MAX_ARCHIVE_BYTES {
			bail!("RAR archive is empty or exceeds the 16 MiB limit.");
		}
		let archive = client.chapter_range(content_id, file_id, 0, Some(total - 1))?;
		if archive.data.len() as u64 != total {
			bail!("RAR archive response was truncated.");
		}
		let images = rar::decode_pages(&archive.data, ImageRef::new)?;
		Ok(images
			.into_iter()
			.map(|image| Page {
				content: PageContent::image(image),
				..Default::default()
			})
			.collect())
	}
	#[cfg(not(feature = "cbr-native"))]
	{
		let _ = total;
		bail!(
			"CBR/RAR comic archives aren't supported. Convert this file to CBZ or read it in the Silo web reader."
		);
	}
}

/// Reads the ZIP central directory over range requests and returns its entries.
fn load_entries(
	client: &mut Client,
	content_id: &str,
	file_id: &str,
	total: u64,
) -> Result<Vec<zip::Entry>> {
	let tail_start = total - total.min(zip::TAIL_BYTES);
	let tail = client.chapter_range(content_id, file_id, tail_start, None)?;
	let directory = zip::read_directory(&tail.data)?;
	if directory.cd_offset >= tail.start {
		return zip::parse_entries(&tail.data, tail.start, &directory);
	}
	let directory_end = (directory.cd_offset + directory.cd_size).saturating_sub(1);
	let data = client.chapter_range(
		content_id,
		file_id,
		directory.cd_offset,
		Some(directory_end),
	)?;
	zip::parse_entries(&data.data, data.start, &directory)
}

/// Turns a fetched CBZ or plugin page response into the page's image bytes. A
/// `200` means the server ignored the range and returned the whole archive.
fn decode_page(code: u16, data: &[u8], context: &PageContext) -> Result<Vec<u8>> {
	if context.contains_key(comic_pages::MARKER) {
		return comic_pages::decode(code, data, context);
	}
	if !matches!(code, 200 | 206) {
		bail!("Silo returned HTTP {code} for this page. Reopen the chapter.");
	}
	let offset = context_u64(context, OFF_KEY).ok_or_else(|| error!("Missing page metadata."))?;
	let length = context_u64(context, LEN_KEY).ok_or_else(|| error!("Missing page metadata."))?;
	let method = context_u64(context, METHOD_KEY).unwrap_or(0) as u16;
	let size = context_u64(context, SIZE_KEY).unwrap_or(0);
	if code == 200 {
		zip::extract_entry(data, offset, method, length, size)
	} else {
		zip::extract_local(data, method, length, size)
	}
}

fn context_u64(context: &PageContext, key: &str) -> Option<u64> {
	context.get(key).and_then(|value| value.parse::<u64>().ok())
}

fn is_zip_magic(data: &[u8]) -> bool {
	data.starts_with(&[0x50, 0x4b, 0x03, 0x04])
		|| data.starts_with(&[0x50, 0x4b, 0x05, 0x06])
		|| data.starts_with(&[0x50, 0x4b, 0x07, 0x08])
}

fn is_rar_magic(data: &[u8]) -> bool {
	data.starts_with(b"Rar!\x1a\x07\x00") || data.starts_with(b"Rar!\x1a\x07\x01\x00")
}

#[derive(Default)]
struct SearchParams {
	author: Option<String>,
	genre: Option<String>,
	year_from: Option<i64>,
	year_to: Option<i64>,
	/// A Silo sort field and whether it is descending.
	sort: Option<(&'static str, bool)>,
}

impl SearchParams {
	fn from_filters(filters: &[FilterValue]) -> Self {
		let mut params = Self::default();
		for filter in filters {
			match filter {
				FilterValue::Text { id, value } if id == "author" && !value.trim().is_empty() => {
					params.author = Some(value.trim().into());
				}
				FilterValue::Select { id, value } if id == "genre" && !value.is_empty() => {
					params.genre = Some(value.clone());
				}
				FilterValue::Range { id, from, to } if id == "year" => {
					params.year_from = from.filter(|v| *v > 0.0).map(|v| v as i64);
					params.year_to = to.filter(|v| *v > 0.0).map(|v| v as i64);
				}
				FilterValue::Sort { id, index, .. } if id == "sort" => {
					params.sort = SORT_FIELDS.get(*index as usize).copied();
				}
				_ => {}
			}
		}
		params
	}
}

/// Adds a sort in the API's grammar: v2 takes `-field` for descending and
/// has no `order` parameter; v1 takes `sort` plus `order`.
fn add_sort(query: &mut Query, client: &Client, field: &str, descending: bool) {
	if client.is_v2 {
		query.add(
			"sort",
			&format!("{}{field}", if descending { "-" } else { "" }),
		);
	} else {
		query.add("sort", field);
		query.add("order", if descending { "desc" } else { "asc" });
	}
}

/// Adds an `author is <name>` rule: v2 takes the rule groups as one JSON
/// query value, v1 takes them as bracketed keys.
fn add_author(query: &mut Query, client: &Client, author: &str) {
	if client.is_v2 {
		let groups = serde_json::json!([{
			"match": "all",
			"rules": [{ "field": "author", "op": "is", "value": author }],
		}]);
		query.add("groups", &format!("{groups}"));
	} else {
		query.add("groups[0][match]", "all");
		query.add("groups[0][rules][0][field]", "author");
		query.add("groups[0][rules][0][op]", "is");
		query.add("groups[0][rules][0][value]", author);
	}
}

fn add_pagination(query: &mut Query, client: &Client, page: i32) {
	let offset = (page.max(1) - 1) as i64 * PAGE_SIZE as i64;
	query.add_i64(if client.is_v2 { "seek" } else { "offset" }, offset);
}

fn search_query(client: &Client, page: i32, search: Option<&str>, params: &SearchParams) -> Query {
	let mut query = Query::new();
	query.add("type", "manga");
	query.add_i64("limit", PAGE_SIZE as i64);
	query.add("image_size", &client.image_size);
	let search = search.map(str::trim).filter(|s| !s.is_empty());
	if let Some(search) = search {
		query.add("q", search);
	}
	if let Some(author) = &params.author {
		add_author(&mut query, client, author);
	}
	if let Some(genre) = &params.genre {
		query.add("genre", genre);
	}
	if let Some(year) = params.year_from {
		query.add_i64("year_min", year);
	}
	if let Some(year) = params.year_to {
		query.add_i64("year_max", year);
	}
	// Without an explicit sort, a search keeps the server's relevance order.
	match params.sort {
		Some((field, descending)) => add_sort(&mut query, client, field, descending),
		None if search.is_none() => add_sort(&mut query, client, "added_at", true),
		None => {}
	}
	add_pagination(&mut query, client, page);
	query
}

fn library_listing(library: &Library) -> Listing {
	Listing {
		id: format!("library:{}", library.id.as_string()),
		name: library.name.clone(),
		kind: ListingKind::List,
	}
}

fn to_page_result(response: CatalogResponse) -> MangaPageResult {
	MangaPageResult {
		has_next_page: response.has_next_page(),
		entries: response.items.iter().map(item_to_manga).collect(),
	}
}

fn tags(genres: &[String]) -> Option<Vec<String>> {
	(!genres.is_empty()).then(|| genres.to_vec())
}

fn item_to_manga(item: &Item) -> Manga {
	Manga {
		key: item.content_id.clone(),
		title: item.title.clone(),
		cover: item.poster_url.clone(),
		description: item.overview.clone().filter(|text| !text.is_empty()),
		tags: tags(&item.genres),
		status: status_from(item.show_status.as_deref()),
		..Default::default()
	}
}

fn detail_to_manga(detail: &ItemDetail, base: &str) -> Manga {
	let mut authors: Vec<String> = detail
		.crew
		.iter()
		.filter(|credit| credit.job.to_ascii_lowercase().contains("author"))
		.map(|credit| credit.name.clone())
		.filter(|name| !name.is_empty())
		.collect();
	if authors.is_empty()
		&& let Some(ebook) = &detail.ebook
	{
		authors = ebook
			.authors
			.iter()
			.map(|author| author.name.clone())
			.filter(|name| !name.is_empty())
			.collect();
	}

	Manga {
		key: detail.content_id.clone(),
		title: detail.title.clone(),
		cover: detail.poster_url.clone(),
		authors: (!authors.is_empty()).then_some(authors),
		description: detail.overview.clone().filter(|text| !text.is_empty()),
		url: Some(format!("{base}/item/{}", detail.content_id)),
		tags: tags(&detail.genres),
		status: status_from(detail.show_status.as_deref()),
		..Default::default()
	}
}

fn chapter_to_aidoku(chapter: &MangaChapter) -> Chapter {
	Chapter {
		key: chapter.content_id.clone(),
		title: Some(chapter.title.clone()).filter(|title| !title.is_empty()),
		chapter_number: chapter.chapter_index.map(|value| value as f32),
		volume_number: chapter.volume.as_deref().and_then(parse_volume),
		thumbnail: chapter.poster_url.clone(),
		..Default::default()
	}
}

/// The first run of digits in a volume label (`"Vol. 3"` is `3`).
fn parse_volume(volume: &str) -> Option<f32> {
	let start = volume.find(|c: char| c.is_ascii_digit())?;
	let digits = &volume[start..];
	let end = digits
		.find(|c: char| !c.is_ascii_digit())
		.unwrap_or(digits.len());
	digits[..end].parse().ok()
}

fn status_from(show_status: Option<&str>) -> MangaStatus {
	match show_status
		.unwrap_or_default()
		.to_ascii_lowercase()
		.as_str()
	{
		"ongoing" | "continuing" | "returning series" | "in production" => MangaStatus::Ongoing,
		"completed" | "complete" | "ended" | "finished" | "released" => MangaStatus::Completed,
		"cancelled" | "canceled" | "canceled/ended" => MangaStatus::Cancelled,
		"hiatus" | "on hiatus" => MangaStatus::Hiatus,
		_ => MangaStatus::Unknown,
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use aidoku::imports::defaults::{DefaultValue, defaults_set};
	use aidoku_test::aidoku_test;

	fn configure() {
		defaults_set(
			"baseUrl",
			DefaultValue::String(String::from("https://juniper.nightbyte.cc")),
		);
		defaults_set("apiVersion", DefaultValue::String(String::from("auto")));
		defaults_set("useApiKey", DefaultValue::Bool(false));
		defaults_set(
			"credentials.username",
			DefaultValue::String(String::from("test")),
		);
		defaults_set(
			"credentials.password",
			DefaultValue::String(String::from("test")),
		);
		defaults_set("imageSize", DefaultValue::String(String::from("large")));
		defaults_set("markReadOnOpen", DefaultValue::Bool(false));
	}

	// ---- ZIP unit tests (no network) ----

	fn push_u16(out: &mut Vec<u8>, value: u16) {
		out.extend_from_slice(&value.to_le_bytes());
	}

	fn push_u32(out: &mut Vec<u8>, value: u32) {
		out.extend_from_slice(&value.to_le_bytes());
	}

	/// Builds a stored (uncompressed) ZIP archive for parser tests.
	fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
		let mut out = Vec::new();
		let mut offsets = Vec::new();
		for (name, data) in files {
			offsets.push(out.len() as u32);
			push_u32(&mut out, 0x0403_4b50);
			push_u16(&mut out, 20);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u32(&mut out, 0);
			push_u32(&mut out, data.len() as u32);
			push_u32(&mut out, data.len() as u32);
			push_u16(&mut out, name.len() as u16);
			push_u16(&mut out, 0);
			out.extend_from_slice(name.as_bytes());
			out.extend_from_slice(data);
		}
		let cd_offset = out.len() as u32;
		for (index, (name, data)) in files.iter().enumerate() {
			push_u32(&mut out, 0x0201_4b50);
			push_u16(&mut out, 20);
			push_u16(&mut out, 20);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u32(&mut out, 0);
			push_u32(&mut out, data.len() as u32);
			push_u32(&mut out, data.len() as u32);
			push_u16(&mut out, name.len() as u16);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u16(&mut out, 0);
			push_u32(&mut out, 0);
			push_u32(&mut out, offsets[index]);
			out.extend_from_slice(name.as_bytes());
		}
		let cd_size = out.len() as u32 - cd_offset;
		push_u32(&mut out, 0x0605_4b50);
		push_u16(&mut out, 0);
		push_u16(&mut out, 0);
		push_u16(&mut out, files.len() as u16);
		push_u16(&mut out, files.len() as u16);
		push_u32(&mut out, cd_size);
		push_u32(&mut out, cd_offset);
		push_u16(&mut out, 0);
		out
	}

	#[aidoku_test]
	fn test_unit_zip_natural_order_and_filtering() {
		let archive = stored_zip(&[
			("__MACOSX/._page1.png", b"junk"),
			("page10.png", b"ten"),
			("page2.png", b"two"),
			("Notes.txt", b"skip me"),
			("page1.png", b"one"),
		]);
		let directory = zip::read_directory(&archive).unwrap();
		let entries = zip::parse_entries(&archive, 0, &directory).unwrap();
		let pages = zip::image_pages(entries);
		assert_eq!(pages.len(), 3);
		let names: Vec<&str> = pages.iter().map(|entry| entry.name.as_str()).collect();
		assert_eq!(names, ["page1.png", "page2.png", "page10.png"]);
		let decoded: Vec<Vec<u8>> = pages
			.iter()
			.map(|entry| {
				zip::extract_entry(
					&archive,
					entry.local_offset,
					entry.method,
					entry.comp_size,
					entry.uncomp_size,
				)
				.unwrap()
			})
			.collect();
		assert_eq!(decoded[0], b"one");
		assert_eq!(decoded[1], b"two");
		assert_eq!(decoded[2], b"ten");
	}

	#[aidoku_test]
	fn test_unit_zip_deflate_entry() {
		let mut archive = Vec::new();
		let payload = b"a deflated page image payload";
		let compressed = miniz_oxide::deflate::compress_to_vec(payload, 6);
		let name = "001.webp";
		let offset = 0u32;
		push_u32(&mut archive, 0x0403_4b50);
		push_u16(&mut archive, 20);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 8);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 0);
		push_u32(&mut archive, 0);
		push_u32(&mut archive, compressed.len() as u32);
		push_u32(&mut archive, payload.len() as u32);
		push_u16(&mut archive, name.len() as u16);
		push_u16(&mut archive, 0);
		archive.extend_from_slice(name.as_bytes());
		archive.extend_from_slice(&compressed);
		let cd_offset = archive.len() as u32;
		push_u32(&mut archive, 0x0201_4b50);
		push_u16(&mut archive, 20);
		push_u16(&mut archive, 20);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 8);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 0);
		push_u32(&mut archive, 0);
		push_u32(&mut archive, compressed.len() as u32);
		push_u32(&mut archive, payload.len() as u32);
		push_u16(&mut archive, name.len() as u16);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 0);
		push_u32(&mut archive, 0);
		push_u32(&mut archive, offset);
		archive.extend_from_slice(name.as_bytes());
		let cd_size = archive.len() as u32 - cd_offset;
		push_u32(&mut archive, 0x0605_4b50);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 0);
		push_u16(&mut archive, 1);
		push_u16(&mut archive, 1);
		push_u32(&mut archive, cd_size);
		push_u32(&mut archive, cd_offset);
		push_u16(&mut archive, 0);

		let directory = zip::read_directory(&archive).unwrap();
		let entries = zip::parse_entries(&archive, 0, &directory).unwrap();
		let pages = zip::image_pages(entries);
		assert_eq!(pages.len(), 1);
		let decoded = zip::extract_local(
			&archive,
			pages[0].method,
			pages[0].comp_size,
			pages[0].uncomp_size,
		)
		.unwrap();
		assert_eq!(decoded, payload);
	}

	#[aidoku_test]
	fn test_unit_natural_cmp() {
		use core::cmp::Ordering;
		assert_eq!(zip::natural_cmp("page2.png", "page10.png"), Ordering::Less);
		assert_eq!(zip::natural_cmp("002-003.webp", "004.webp"), Ordering::Less);
		assert_eq!(zip::natural_cmp("a/9.png", "a/10.png"), Ordering::Less);
	}

	#[aidoku_test]
	fn test_unit_magic_detection() {
		assert!(is_zip_magic(&[0x50, 0x4b, 0x03, 0x04, 0, 0]));
		assert!(is_zip_magic(&[0x50, 0x4b, 0x05, 0x06]));
		assert!(is_rar_magic(b"Rar!\x1a\x07\x00xxxx"));
		assert!(is_rar_magic(b"Rar!\x1a\x07\x01\x00xxx"));
		assert!(!is_zip_magic(b"Rar!\x1a\x07\x00"));
		assert!(!is_rar_magic(&[0x50, 0x4b, 0x03, 0x04]));
		assert!(!is_zip_magic(b"%PDF-1.7"));
	}

	#[cfg(feature = "cbr-native")]
	fn rar_fixture(which: usize) -> &'static [u8] {
		let fixtures: [&[u8]; 4] = [
			include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/rar40-normal.cbr"
			))
			.as_slice(),
			include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/rar40-solid.cbr"
			))
			.as_slice(),
			include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/rar50-normal.cbr"
			))
			.as_slice(),
			include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/rar50-solid.cbr"
			))
			.as_slice(),
		];
		fixtures[which]
	}

	#[cfg(feature = "cbr-native")]
	#[aidoku_test]
	fn test_unit_rar_fixtures_and_resource_guards() {
		let page = |name: &str| match name {
			"page1" => include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/page1.png"
			))
			.as_slice(),
			"page2" => include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/page2.png"
			))
			.as_slice(),
			_ => include_bytes!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../crates/cbr-native/fixtures/page10.png"
			))
			.as_slice(),
		};
		for which in 0..4 {
			let archive = rar_fixture(which);
			let pages = rar::list_pages(archive).unwrap();
			assert_eq!(
				pages
					.iter()
					.map(|page| page.name.as_str())
					.collect::<Vec<_>>(),
				["page1.png", "page2.png", "page10.png"]
			);
			assert_eq!(
				pages.iter().map(|page| page.index).collect::<Vec<_>>(),
				[2, 1, 0]
			);
			// One pass decodes every page, returned in reading order.
			let decoded = rar::decode_pages(archive, <[u8]>::to_vec).unwrap();
			assert_eq!(decoded, [page("page1"), page("page2"), page("page10")]);
		}

		let archive = rar_fixture(0);
		assert!(rar::list_pages(&archive[..archive.len() / 2]).is_err());
		assert!(rar::decode_pages(&archive[..archive.len() - 1], <[u8]>::to_vec).is_err());
		let oversized = vec![0; rar::MAX_ARCHIVE_BYTES as usize + 1];
		assert!(rar::list_pages(&oversized).is_err());
	}

	// ---- Live server tests against the Silo test instance ----

	#[aidoku_test]
	fn test_live_listings_and_browse() {
		configure();
		let source = Silo::new();
		let listings = source.get_dynamic_listings().unwrap();
		assert!(!listings.is_empty());
		assert!(listings.iter().any(|listing| listing.id == "all"));

		let results = source.get_search_manga_list(None, 1, Vec::new()).unwrap();
		assert!(!results.entries.is_empty());

		let manga = results.entries[0].clone();
		let detail = source.get_manga_update(manga, true, true).unwrap();
		assert!(!detail.title.is_empty());
		assert!(
			detail
				.chapters
				.as_ref()
				.map(|c| !c.is_empty())
				.unwrap_or(false)
		);
	}

	/// Opens a chapter's first page through the same lazy path the app uses and
	/// returns the decoded image bytes.
	fn decode_first_page(chapter_key: &str) -> Vec<u8> {
		let source = Silo::new();
		let pages = source
			.get_page_list(
				Manga::default(),
				Chapter {
					key: String::from(chapter_key),
					..Default::default()
				},
			)
			.unwrap();
		assert!(!pages.is_empty());
		let context = match &pages[0].content {
			PageContent::Url(_, Some(context)) => context.clone(),
			_ => panic!("expected a url page with a context"),
		};
		let mut client = Client::connect().unwrap();
		let detail = client.item(chapter_key).unwrap();
		let file_id = detail.versions[0].file_id.as_string();
		let offset = context_u64(&context, OFF_KEY).unwrap();
		let fetched = client
			.chapter_range(chapter_key, &file_id, offset, None)
			.unwrap();
		let code = if fetched.start == offset { 206 } else { 200 };
		decode_page(code, &fetched.data, &context).unwrap()
	}

	#[aidoku_test]
	fn test_live_page_list() {
		configure();
		let bytes = decode_first_page("141065514771283970");
		assert!(!bytes.is_empty());
	}

	#[aidoku_test]
	fn test_live_home() {
		configure();
		let layout = Silo::new().get_home().unwrap();
		assert!(!layout.components.is_empty());
	}

	#[aidoku_test]
	fn test_live_dynamic_filters() {
		configure();
		let filters = Silo::new().get_dynamic_filters().unwrap();
		assert!(!filters.is_empty());
	}

	#[aidoku_test]
	fn test_live_listing_provider() {
		configure();
		let listing = Listing {
			id: String::from("all"),
			name: String::from("All Manga"),
			kind: ListingKind::List,
		};
		let result = Silo::new().get_manga_list(listing, 1).unwrap();
		assert!(!result.entries.is_empty());
	}

	#[aidoku_test]
	fn test_live_details_only() {
		configure();
		let manga = Manga {
			key: String::from("141023278834647042"),
			..Default::default()
		};
		let updated = Silo::new().get_manga_update(manga, true, false).unwrap();
		assert!(!updated.title.is_empty());
		assert!(updated.chapters.is_none());
	}

	#[aidoku_test]
	fn test_live_chapters_only() {
		configure();
		let manga = Manga {
			key: String::from("141023278834647042"),
			title: String::from("placeholder"),
			..Default::default()
		};
		let updated = Silo::new().get_manga_update(manga, false, true).unwrap();
		let chapters = updated.chapters.unwrap_or_default();
		assert!(!chapters.is_empty());
		// Aidoku expects newest-first: the first chapter number must be the
		// highest, otherwise its "next chapter" logic starts at the wrong end.
		let first = chapters.first().and_then(|c| c.chapter_number);
		let last = chapters.last().and_then(|c| c.chapter_number);
		assert!(first > last, "chapters should be newest-first");
	}

	#[aidoku_test]
	fn test_live_basic_login() {
		configure();
		let source = Silo::new();
		assert!(
			source
				.handle_basic_login(
					String::from("credentials"),
					String::from("test"),
					String::from("test")
				)
				.unwrap()
		);
		assert!(
			!source
				.handle_basic_login(
					String::from("credentials"),
					String::from("test"),
					String::from("wrong-password")
				)
				.unwrap()
		);
	}

	#[aidoku_test]
	fn test_live_image_request_provider() {
		configure();
		let source = Silo::new();
		let pages = source
			.get_page_list(
				Manga::default(),
				Chapter {
					key: String::from("141065514771283970"),
					..Default::default()
				},
			)
			.unwrap();
		let (url, context) = match &pages[0].content {
			PageContent::Url(url, Some(context)) => (url.clone(), context.clone()),
			_ => panic!("expected a url page with a context"),
		};
		let response = source
			.get_image_request(url, Some(context.clone()))
			.unwrap()
			.send()
			.unwrap();
		let code = response.status_code() as u16;
		let data = response.get_data().unwrap();
		let page = decode_page(code, &data, &context).unwrap();
		assert!(!page.is_empty());
	}

	#[aidoku_test]
	fn test_live_cover_image_request() {
		configure();
		let source = Silo::new();
		let result = source.get_search_manga_list(None, 1, Vec::new()).unwrap();
		let cover = result
			.entries
			.iter()
			.find_map(|manga| manga.cover.clone())
			.unwrap();
		let response = source
			.get_image_request(cover, None)
			.unwrap()
			.send()
			.unwrap();
		let code = response.status_code();
		assert!((200..400).contains(&code), "cover request returned {code}");
	}

	#[aidoku_test]
	fn test_live_alternate_covers_and_deep_link() {
		configure();
		let source = Silo::new();
		let manga = Manga {
			key: String::from("141023278834647042"),
			..Default::default()
		};
		let covers = source.get_alternate_covers(manga).unwrap();
		assert!(!covers.is_empty());
		let deep = source
			.handle_deep_link(String::from(
				"https://silo.example.com/item/141023278834647042",
			))
			.unwrap();
		assert!(deep.is_some());
	}

	fn configure_v2_mock() {
		defaults_set(
			"baseUrl",
			DefaultValue::String(String::from("http://127.0.0.1:8799")),
		);
		defaults_set("apiVersion", DefaultValue::String(String::from("v2")));
		defaults_set("useApiKey", DefaultValue::Bool(false));
		defaults_set("useComicPages", DefaultValue::Bool(true));
		for key in ["apiKey", "profile", "pin", "comicPagesPlugin"] {
			defaults_set(key, DefaultValue::String(String::new()));
		}
		defaults_set(
			"credentials.username",
			DefaultValue::String(String::from("mock")),
		);
		defaults_set(
			"credentials.password",
			DefaultValue::String(String::from("mock")),
		);
		defaults_set("imageSize", DefaultValue::String(String::from("large")));
		defaults_set("markReadOnOpen", DefaultValue::Bool(false));
	}

	fn configure_v2_api_key_mock() {
		configure_v2_mock();
		defaults_set("useApiKey", DefaultValue::Bool(true));
		defaults_set("apiKey", DefaultValue::String(String::from("mock-api-key")));
	}

	/// Exercises the v2 code paths (envelopes, string file ids, `seek`
	/// pagination, sort and rule grammar) against `tests/mock_silo_v2.py`.
	/// Run with: `python3 tests/mock_silo_v2.py & cargo test -- --ignored test_mock_`
	#[aidoku_test]
	#[ignore]
	fn test_mock_v2_browse_and_cbz() {
		configure_v2_mock();
		let source = Silo::new();

		let listings = source.get_dynamic_listings().unwrap();
		assert!(listings.iter().any(|listing| listing.id == "library:12"));
		assert!(!listings.iter().any(|listing| listing.id == "library:7"));

		let page_one = source.get_search_manga_list(None, 1, Vec::new()).unwrap();
		assert_eq!(page_one.entries.len(), 2);
		assert!(page_one.has_next_page);

		let page_two = source.get_search_manga_list(None, 2, Vec::new()).unwrap();
		assert_eq!(page_two.entries.len(), 1);
		assert!(!page_two.has_next_page);

		// Descending sorts use v2's `-field` grammar; the mock rejects `order`.
		let sort = |index| FilterValue::Sort {
			id: String::from("sort"),
			index,
			ascending: false,
		};
		let z_to_a = source
			.get_search_manga_list(None, 1, vec![sort(1)])
			.unwrap();
		assert_eq!(z_to_a.entries[0].title, "Mock Manga Two");
		let searched = source
			.get_search_manga_list(Some(String::from("mock")), 1, Vec::new())
			.unwrap();
		assert_eq!(searched.entries.len(), 2);

		// The author rule travels as v2's JSON `groups` value.
		let by_author = source
			.get_search_manga_list(
				None,
				1,
				vec![FilterValue::Text {
					id: String::from("author"),
					value: String::from("Mock Author"),
				}],
			)
			.unwrap();
		assert_eq!(by_author.entries.len(), 1);

		// Home sections link to their own items.
		let home = source.get_home().unwrap();
		let section = home
			.components
			.iter()
			.find_map(|component| match &component.value {
				HomeComponentValue::Scroller { listing, .. } => listing.clone(),
				_ => None,
			})
			.unwrap();
		assert_eq!(section.id, "section:12:recent");
		assert_eq!(source.get_manga_list(section, 1).unwrap().entries.len(), 1);

		let manga = source
			.get_manga_update(page_one.entries[0].clone(), true, true)
			.unwrap();
		assert_eq!(manga.authors.as_ref().map(|a| a.len()), Some(1));
		assert_eq!(manga.chapters.as_ref().map(|c| c.len()), Some(2));

		let chapter = Chapter {
			key: String::from("c1"),
			..Default::default()
		};
		let pages = source.get_page_list(Manga::default(), chapter).unwrap();
		assert_eq!(pages.len(), 2);
		assert!(matches!(&pages[0].content, PageContent::Url(_, Some(_))));
		assert!(!decode_first_page("c1").is_empty());
	}

	#[cfg(feature = "cbr-native")]
	#[aidoku_test]
	#[ignore]
	fn test_mock_rar_pages_are_decoded_images() {
		configure_v2_api_key_mock();
		defaults_set("useComicPages", DefaultValue::Bool(false));
		let pages = Silo::new()
			.get_page_list(
				Manga::default(),
				Chapter {
					key: String::from("cbr1"),
					..Default::default()
				},
			)
			.unwrap();
		assert_eq!(pages.len(), 3);
		assert!(
			pages
				.iter()
				.all(|page| matches!(page.content, PageContent::Image(_)))
		);
	}

	#[aidoku_test]
	#[ignore]
	fn test_mock_comic_pages_plugin_chunks_and_credentials() {
		// No installation ID: the source finds the plugin in Silo's list.
		configure_v2_api_key_mock();
		let source = Silo::new();
		let pages = source
			.get_page_list(
				Manga::default(),
				Chapter {
					key: String::from("cbr1"),
					..Default::default()
				},
			)
			.unwrap();
		assert_eq!(pages.len(), 1);
		let PageContent::Url(url, Some(context)) = &pages[0].content else {
			panic!("expected plugin page");
		};
		assert!(!url.contains("mock-api-key"));
		assert!(
			context
				.values()
				.all(|value| !value.contains("mock-api-key"))
		);
		let response = source
			.get_image_request(url.clone(), Some(context.clone()))
			.unwrap()
			.send()
			.unwrap();
		assert_eq!(response.status_code(), 200);
		let first_chunk = response.get_data().unwrap();
		assert_eq!(first_chunk.len(), 1_048_576);
		let decoded = decode_page(200, &first_chunk, context).unwrap();
		assert_eq!(
			decoded.len() as u64,
			context_u64(context, "silo_plugin_size").unwrap()
		);
		assert!(decoded.len() > 3_000_000);
		assert!(decoded.starts_with(b"\x89PNG\r\n\x1a\n"));
		assert!(decoded.ends_with(b"\x00\x00\x00\x00IEND\xaeB`\x82"));
		assert!(decode_page(403, &first_chunk, context).is_err());
		assert!(decode_page(200, &first_chunk[..100], context).is_err());
		assert!(
			source
				.get_image_request(
					String::from("https://other.invalid/page"),
					Some(context.clone())
				)
				.is_err()
		);
		defaults_set(
			"profileId",
			DefaultValue::String(String::from("different-profile")),
		);
		assert!(
			source
				.get_image_request(url.clone(), Some(context.clone()))
				.is_err()
		);
	}

	#[aidoku_test]
	fn test_unit_query_grammar() {
		let params = SearchParams {
			sort: Some(("added_at", true)),
			author: Some(String::from("Jo")),
			..Default::default()
		};
		let v2 = search_query(&Client::new(String::new(), true), 2, None, &params).encode();
		assert!(v2.contains("sort=-added_at"));
		assert!(!v2.contains("order="));
		assert!(v2.contains("groups=%5B%7B"));
		assert!(v2.contains("seek=30"));
		let v1 = search_query(&Client::new(String::new(), false), 1, None, &params).encode();
		assert!(v1.contains("sort=added_at&order=desc"));
		assert!(v1.contains("groups[0][rules][0][value]=Jo"));
		// A search without an explicit sort keeps the server's relevance order.
		let search = search_query(
			&Client::new(String::new(), true),
			1,
			Some(" mock "),
			&SearchParams::default(),
		)
		.encode();
		assert!(search.contains("q=mock"));
		assert!(!search.contains("sort="));
		let blank = search_query(
			&Client::new(String::new(), true),
			1,
			Some("  "),
			&SearchParams::default(),
		)
		.encode();
		assert!(!blank.contains("q=") && blank.contains("sort=-added_at"));
	}

	#[aidoku_test]
	fn test_unit_parsers() {
		assert_eq!(
			client::parse_utc("2026-01-02T15:04:05.000Z"),
			Some(1_767_366_245)
		);
		assert_eq!(client::parse_utc("1970-01-01T00:00:00Z"), Some(0));
		assert_eq!(client::parse_utc("garbage"), None);
		assert_eq!(parse_volume("Vol. 12 part 3"), Some(12.0));
		assert_eq!(parse_volume("Special"), None);
		let link = |url: &str| Silo::new().handle_deep_link(String::from(url)).unwrap();
		assert!(matches!(
			link("https://silo.test/item/42?x=1"),
			Some(DeepLinkResult::Manga { key }) if key == "42"
		));
		assert!(matches!(
			link("https://silo.test/library/7"),
			Some(DeepLinkResult::Listing(Listing { id, .. })) if id == "library:7"
		));
		assert!(link("https://silo.test/settings").is_none());
	}

	#[aidoku_test]
	fn test_unit_auth_mode_migration() {
		defaults_set("useApiKey", DefaultValue::Bool(false));
		defaults_set("authMode", DefaultValue::String(String::from("apiKey")));
		assert!(settings::use_api_key());
		// The legacy value is consumed, so the switch wins afterwards.
		defaults_set("useApiKey", DefaultValue::Bool(false));
		assert!(!settings::use_api_key());
	}

	/// A rejected API key must fail after one retry instead of recursing
	/// through re-authentication until the stack overflows.
	#[aidoku_test]
	#[ignore]
	fn test_mock_invalid_api_key_fails_fast() {
		configure_v2_api_key_mock();
		defaults_set("apiKey", DefaultValue::String(String::from("bad-key")));
		let error = Client::connect().err().expect("a bad key must not connect");
		assert!(format!("{error:?}").contains("(401)"));
	}

	#[aidoku_test]
	#[ignore]
	fn test_mock_login_keeps_session_and_scopes_it() {
		configure_v2_mock();
		let source = Silo::new();
		let login = |password: &str| {
			source.handle_basic_login(
				String::from("credentials"),
				String::from("mock"),
				String::from(password),
			)
		};
		assert!(!login("wrong").unwrap());
		assert!(login("mock").unwrap());
		assert_eq!(
			aidoku::imports::defaults::defaults_get::<String>("accessToken").as_deref(),
			Some("acc")
		);
		// Another account must not reuse this session.
		defaults_set(
			"credentials.username",
			DefaultValue::String(String::from("someone-else")),
		);
		assert!(Client::connect().is_err());
		assert_eq!(
			aidoku::imports::defaults::defaults_get::<String>("accessToken").as_deref(),
			Some("")
		);
	}

	#[aidoku_test]
	#[ignore]
	fn test_mock_pin_profile() {
		configure_v2_mock();
		defaults_set("profile", DefaultValue::String(String::from("Locked")));
		defaults_set("pin", DefaultValue::String(String::from("0000")));
		assert!(Client::connect().is_err());
		defaults_set("pin", DefaultValue::String(String::from("1234")));
		let client = Client::connect().unwrap();
		assert_eq!(client.profile_id, "p2");
		assert_eq!(client.profile_token.as_deref(), Some("pvt"));
		// Page image requests reuse the verified profile.
		let images = Client::for_images();
		assert_eq!(images.profile_token.as_deref(), Some("pvt"));
		let listings = Silo::new().get_dynamic_listings().unwrap();
		assert!(listings.iter().any(|listing| listing.id == "library:12"));
	}
}

#![no_std]
extern crate alloc;

mod client;
mod models;
mod settings;
mod zip;

use aidoku::{
    AlternateCoverProvider, BasicLoginHandler, Chapter, DeepLinkHandler, DeepLinkResult,
    DynamicFilters, DynamicListings, Filter, FilterValue, Home, HomeComponent, HomeComponentValue,
    HomeLayout, Link, LinkValue, Listing, ListingKind, ListingProvider, Manga, MangaPageResult,
    MangaStatus, Page, PageContent, RangeFilter, Result, SelectFilter, SortFilter, Source,
    TextFilter,
    alloc::{borrow::Cow, format, string::String, vec, vec::Vec},
    imports::canvas::ImageRef,
    prelude::*,
};
use client::{Client, PAGE_SIZE, Query};
use models::{CatalogResponse, Item, ItemDetail, MangaChapter, SectionResponse};

/// Sort options presented to the user, each mapping to a Silo sort field and
/// order. `can_ascend` is false so the direction is explicit per option.
const SORT_LABELS: [&str; 7] = [
    "Title (A–Z)",
    "Title (Z–A)",
    "Recently Added",
    "Oldest Added",
    "Newest Released",
    "Highest Rated",
    "Author (A–Z)",
];

const SORT_FIELDS: [(&str, &str); 7] = [
    ("title", "asc"),
    ("title", "desc"),
    ("added_at", "desc"),
    ("added_at", "asc"),
    ("release_date", "desc"),
    ("rating_imdb", "desc"),
    ("author", "asc"),
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
        let mut client = Client::connect()?;
        let params = SearchParams::from_filters(&filters);
        let request = build_catalog_query(&mut client, page, query.as_deref(), &params, None);
        let response = client.catalog(&request)?;
        Ok(to_page_result(response))
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
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
            let chapters = detail
                .manga
                .as_ref()
                .map(|extension| extension.chapters.iter().map(chapter_to_aidoku).collect())
                .unwrap_or_default();
            manga.chapters = Some(chapters);
        }
        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let mut client = Client::connect()?;
        let detail = client.item(&chapter.key)?;
        let version = detail
            .versions
            .first()
            .ok_or_else(|| error!("This chapter has no readable file on the Silo server."))?;
        let container = version
            .container
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match container.as_str() {
            "" | "cbz" => {}
            "cbr" | "rar" => bail!(
                "CBR/RAR comic archives aren't supported. Convert this file to CBZ or read it in the Silo web reader."
            ),
            other => bail!(
                "This chapter is a .{other} file, which this source can't render. Use the Silo web reader for it."
            ),
        }

        let file_id = version.file_id.as_string();
        let data = client.chapter_archive(&chapter.key, &file_id)?;
        let page_images = zip::comic_pages(&data)?;

        if settings::mark_read_on_open() {
            let _ = client.mark_read(&chapter.key);
        }

        let mut pages = Vec::with_capacity(page_images.len());
        for bytes in page_images {
            pages.push(Page {
                content: PageContent::image(ImageRef::new(&bytes)),
                ..Default::default()
            });
        }
        Ok(pages)
    }
}

impl ListingProvider for Silo {
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
        let mut client = Client::connect()?;
        let library_id = listing.id.strip_prefix("library:").map(String::from);
        let mut query = Query::new();
        query.add("type", "manga");
        query.add_i64("limit", PAGE_SIZE as i64);
        query.add("image_size", &client.image_size);
        if let Some(id) = &library_id {
            query.add("library_id", id);
        }
        query.add("sort", "title");
        query.add("order", "asc");
        add_pagination(&mut query, &client, page);
        let response = client.catalog(&query)?;
        Ok(to_page_result(response))
    }
}

impl DynamicListings for Silo {
    fn get_dynamic_listings(&self) -> Result<Vec<Listing>> {
        let mut client = Client::connect()?;
        let libraries = client.manga_libraries()?;
        let mut listings = vec![Listing {
            id: String::from("all"),
            name: String::from("All Manga"),
            kind: ListingKind::List,
        }];
        for library in libraries {
            listings.push(Listing {
                id: format!("library:{}", library.id.as_string()),
                name: library.name,
                kind: ListingKind::List,
            });
        }
        Ok(listings)
    }
}

impl Home for Silo {
    fn get_home(&self) -> Result<HomeLayout> {
        let mut client = Client::connect()?;
        let libraries = client.manga_libraries()?;
        let mut components = Vec::new();

        let mut library_links = Vec::new();
        for library in &libraries {
            library_links.push(Link {
                title: library.name.clone(),
                value: Some(LinkValue::Listing(Listing {
                    id: format!("library:{}", library.id.as_string()),
                    name: library.name.clone(),
                    kind: ListingKind::List,
                })),
                ..Default::default()
            });
        }
        if !library_links.is_empty() {
            components.push(HomeComponent {
                title: Some(String::from("Libraries")),
                subtitle: None,
                value: HomeComponentValue::Links(library_links),
            });
        }

        for library in libraries.iter().take(4) {
            let Ok(response) = client.library_sections(&library.id.as_string()) else {
                continue;
            };
            add_section_components(&mut components, &response, library);
        }

        Ok(HomeLayout { components })
    }
}

impl DynamicFilters for Silo {
    fn get_dynamic_filters(&self) -> Result<Vec<Filter>> {
        let mut client = Client::connect()?;
        let mut filters: Vec<Filter> = vec![
            TextFilter {
                id: Cow::Borrowed("author"),
                title: Some(Cow::Borrowed("Author")),
                placeholder: Some(Cow::Borrowed("Filter by author")),
                ..Default::default()
            }
            .into(),
        ];

        if let Ok(server_filters) = client.catalog_filters(None)
            && !server_filters.genres.is_empty()
        {
            filters.push(
                SelectFilter {
                    id: Cow::Borrowed("genre"),
                    title: Some(Cow::Borrowed("Genre")),
                    is_genre: true,
                    uses_tag_style: true,
                    options: server_filters
                        .genres
                        .iter()
                        .map(|genre| Cow::Owned(genre.clone()))
                        .collect(),
                    ..Default::default()
                }
                .into(),
            );
        }

        filters.push(
            RangeFilter {
                id: Cow::Borrowed("year"),
                title: Some(Cow::Borrowed("Year")),
                min: Some(1900.0),
                max: Some(2100.0),
                decimal: false,
                ..Default::default()
            }
            .into(),
        );
        filters.push(
            SortFilter {
                id: Cow::Borrowed("sort"),
                title: Some(Cow::Borrowed("Sort")),
                can_ascend: false,
                options: SORT_LABELS
                    .iter()
                    .map(|label| Cow::Borrowed(*label))
                    .collect(),
                ..Default::default()
            }
            .into(),
        );

        Ok(filters)
    }
}

impl DeepLinkHandler for Silo {
    fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
        if let Some(rest) = url.split("/item/").nth(1) {
            let key = rest.split(['?', '#', '/']).next().unwrap_or("");
            if !key.is_empty() {
                return Ok(Some(DeepLinkResult::Manga {
                    key: String::from(key),
                }));
            }
        }
        if let Some(rest) = url.split("/library/").nth(1) {
            let id = rest.split(['?', '#', '/']).next().unwrap_or("");
            if !id.is_empty() {
                return Ok(Some(DeepLinkResult::Listing(Listing {
                    id: format!("library:{id}"),
                    name: String::from(id),
                    kind: ListingKind::List,
                })));
            }
        }
        Ok(None)
    }
}

impl BasicLoginHandler for Silo {
    fn handle_basic_login(&self, _key: String, username: String, password: String) -> Result<bool> {
        let base = settings::base_url()?;
        let is_v2 = client::probe_version(&base);
        client::validate_credentials(&base, is_v2, &username, &password)
    }
}

impl AlternateCoverProvider for Silo {
    fn get_alternate_covers(&self, manga: Manga) -> Result<Vec<String>> {
        let mut client = Client::connect()?;
        let detail = client.item(&manga.key)?;
        let mut covers = Vec::new();
        if let Some(backdrop) = detail.backdrop_url {
            covers.push(backdrop);
        }
        if let Some(poster) = detail.poster_url
            && !covers.contains(&poster)
        {
            covers.push(poster);
        }
        Ok(covers)
    }
}

// ----- helpers -----

struct SearchParams {
    author: Option<String>,
    genre: Option<String>,
    year_from: Option<i64>,
    year_to: Option<i64>,
    sort: Option<(String, String)>,
}

impl SearchParams {
    fn from_filters(filters: &[FilterValue]) -> Self {
        let mut params = Self {
            author: None,
            genre: None,
            year_from: None,
            year_to: None,
            sort: None,
        };
        for filter in filters {
            match filter {
                FilterValue::Text { id, value } => {
                    if id == "author" && !value.trim().is_empty() {
                        params.author = Some(value.trim().into());
                    }
                }
                FilterValue::Select { id, value } => {
                    if id == "genre" && !value.is_empty() {
                        params.genre = Some(value.clone());
                    }
                }
                FilterValue::Range { id, from, to } => {
                    if id == "year" {
                        params.year_from = from.filter(|v| *v > 0.0).map(|v| v as i64);
                        params.year_to = to.filter(|v| *v > 0.0).map(|v| v as i64);
                    }
                }
                FilterValue::Sort { id, index, .. } => {
                    if id == "sort"
                        && let Some((field, order)) = SORT_FIELDS.get(*index as usize)
                    {
                        params.sort = Some((String::from(*field), String::from(*order)));
                    }
                }
                _ => {}
            }
        }
        params
    }
}

fn add_pagination(query: &mut Query, client: &Client, page: i32) {
    let offset = (page.max(1) - 1) as i64 * PAGE_SIZE as i64;
    if client.is_v2 {
        query.add_i64("seek", offset);
    } else {
        query.add_i64("offset", offset);
    }
}

fn build_catalog_query(
    client: &mut Client,
    page: i32,
    search: Option<&str>,
    params: &SearchParams,
    library_id: Option<&str>,
) -> Query {
    let mut query = Query::new();
    query.add("type", "manga");
    query.add_i64("limit", PAGE_SIZE as i64);
    query.add("image_size", &client.image_size);
    if let Some(id) = library_id {
        query.add("library_id", id);
    }
    if let Some(search) = search.filter(|s| !s.trim().is_empty()) {
        query.add("source", "query");
        query.add("q", search.trim());
    }
    if let Some(author) = &params.author {
        query.add("groups[0][match]", "all");
        query.add("groups[0][rules][0][field]", "author");
        query.add("groups[0][rules][0][op]", "is");
        query.add("groups[0][rules][0][value]", author);
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
    match &params.sort {
        Some((field, order)) => {
            query.add("sort", field);
            query.add("order", order);
        }
        None => {
            if search.is_some() {
                query.add("sort", "relevance");
            } else {
                query.add("sort", "added_at");
                query.add("order", "desc");
            }
        }
    }
    add_pagination(&mut query, client, page);
    query
}

fn add_section_components(
    components: &mut Vec<HomeComponent>,
    response: &SectionResponse,
    library: &models::Library,
) {
    let listing = Some(Listing {
        id: format!("library:{}", library.id.as_string()),
        name: library.name.clone(),
        kind: ListingKind::List,
    });
    for section in response.sections.iter().take(6) {
        let entries: Vec<Link> = section
            .items
            .iter()
            .map(|item| item_to_manga(item).into())
            .collect();
        if entries.is_empty() {
            continue;
        }
        components.push(HomeComponent {
            title: Some(section.title.clone()),
            subtitle: None,
            value: HomeComponentValue::Scroller {
                entries,
                listing: listing.clone(),
            },
        });
    }
}

fn to_page_result(response: CatalogResponse) -> MangaPageResult {
    let has_next_page = response.has_next_page();
    MangaPageResult {
        entries: response.items.iter().map(item_to_manga).collect(),
        has_next_page,
    }
}

fn item_to_manga(item: &Item) -> Manga {
    Manga {
        key: item.content_id.clone(),
        title: item.title.clone(),
        cover: item.poster_url.clone(),
        description: item.overview.clone().filter(|text| !text.is_empty()),
        tags: if item.genres.is_empty() {
            None
        } else {
            Some(item.genres.clone())
        },
        status: status_from(item.show_status.as_deref().unwrap_or("")),
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
        tags: if detail.genres.is_empty() {
            None
        } else {
            Some(detail.genres.clone())
        },
        status: status_from(detail.show_status.as_deref().unwrap_or("")),
        ..Default::default()
    }
}

fn chapter_to_aidoku(chapter: &MangaChapter) -> Chapter {
    Chapter {
        key: chapter.content_id.clone(),
        title: Some(chapter.title.clone()).filter(|title| !title.is_empty()),
        chapter_number: chapter.chapter_index.map(|value| value as f32),
        volume_number: parse_volume(&chapter.volume),
        thumbnail: chapter.poster_url.clone(),
        ..Default::default()
    }
}

fn parse_volume(volume: &Option<String>) -> Option<f32> {
    let volume = volume.as_ref()?;
    let mut digits = String::new();
    let mut started = false;
    for character in volume.chars() {
        if character.is_ascii_digit() {
            started = true;
            digits.push(character);
        } else if started {
            break;
        }
    }
    digits.parse::<f32>().ok()
}

fn status_from(show_status: &str) -> MangaStatus {
    match show_status.to_ascii_lowercase().as_str() {
        "ongoing" | "continuing" | "returning series" | "in production" => MangaStatus::Ongoing,
        "completed" | "complete" | "ended" | "finished" | "released" => MangaStatus::Completed,
        "cancelled" | "canceled" | "canceled/ended" => MangaStatus::Cancelled,
        "hiatus" | "on hiatus" => MangaStatus::Hiatus,
        _ => MangaStatus::Unknown,
    }
}

register_source!(
    Silo,
    ListingProvider,
    Home,
    DynamicListings,
    DynamicFilters,
    DeepLinkHandler,
    BasicLoginHandler,
    AlternateCoverProvider
);

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
        defaults_set(
            "authMode",
            DefaultValue::String(String::from("credentials")),
        );
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
    fn test_zip_natural_order_and_filtering() {
        let archive = stored_zip(&[
            ("__MACOSX/._page1.png", b"junk"),
            ("page10.png", b"ten"),
            ("page2.png", b"two"),
            ("Notes.txt", b"skip me"),
            ("page1.png", b"one"),
        ]);
        let pages = zip::comic_pages(&archive).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], b"one");
        assert_eq!(pages[1], b"two");
        assert_eq!(pages[2], b"ten");
    }

    #[aidoku_test]
    fn test_zip_deflate_entry() {
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

        let pages = zip::comic_pages(&archive).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0], payload);
    }

    #[aidoku_test]
    fn test_natural_cmp() {
        use core::cmp::Ordering;
        assert_eq!(zip::natural_cmp("page2.png", "page10.png"), Ordering::Less);
        assert_eq!(zip::natural_cmp("002-003.webp", "004.webp"), Ordering::Less);
        assert_eq!(zip::natural_cmp("a/9.png", "a/10.png"), Ordering::Less);
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

    #[aidoku_test]
    fn test_live_page_list() {
        configure();
        let source = Silo::new();
        let chapter = Chapter {
            key: String::from("141065514771283970"),
            ..Default::default()
        };
        let pages = source.get_page_list(Manga::default(), chapter).unwrap();
        assert!(!pages.is_empty());
        assert!(matches!(pages[0].content, PageContent::Image(_)));
    }

    fn configure_v2_mock() {
        defaults_set(
            "baseUrl",
            DefaultValue::String(String::from("http://127.0.0.1:8799")),
        );
        defaults_set("apiVersion", DefaultValue::String(String::from("v2")));
        defaults_set(
            "authMode",
            DefaultValue::String(String::from("credentials")),
        );
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

    /// Exercises the v2 code paths (envelopes, string file ids, `seek`
    /// pagination) against `tests/mock_silo_v2.py`. Run with:
    /// `python3 tests/mock_silo_v2.py & cargo test -- --ignored`
    #[aidoku_test]
    #[ignore]
    fn test_v2_against_mock() {
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
        assert!(matches!(pages[0].content, PageContent::Image(_)));
    }
}

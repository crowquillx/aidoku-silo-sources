//! Optional server extraction through the separately installed Comic Pages plugin.
use aidoku::{
	Page, PageContent, PageContext, Result,
	alloc::{format, string::String, vec::Vec},
	helpers::uri::encode_uri_component,
	imports::net::Request,
	prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{Client, client, settings};

pub const MARKER: &str = "silo_plugin";
const CHUNK_BYTES: usize = 1_048_576;
const MAX_PAGE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Serialize)]
struct ReadRequest<'a> {
	token: &'a str,
	profile_id: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	profile_token: Option<&'a str>,
	content_id: &'a str,
	file_id: &'a str,
	api_version: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	cache_key: Option<&'a str>,
	offset: u64,
}

#[derive(Deserialize)]
struct PageList {
	cache_key: String,
	chunk_bytes: usize,
	pages: Vec<PageInfo>,
}

#[derive(Deserialize)]
struct PageInfo {
	size: u64,
}

fn identifier(value: &str) -> bool {
	!value.is_empty()
		&& value.len() <= 128
		&& value
			.bytes()
			.all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn plugin_base(base: &str, installation: &str, api_version: &str) -> Result<String> {
	if !identifier(installation) {
		bail!("Set the Comic Pages plugin installation ID in source settings.");
	}
	match api_version {
		"v1" => Ok(format!("{base}/api/v1/plugins/{installation}/v1")),
		"v2" => Ok(format!(
			"{base}/api/v2/plugin-content/plugins/{installation}/v1"
		)),
		_ => bail!("Invalid Comic Pages API version."),
	}
}

fn request(url: &str, body: &ReadRequest<'_>) -> Result<Request> {
	if body.token.is_empty() || body.profile_id.is_empty() {
		bail!("Sign in to Silo and reopen the chapter.");
	}
	let json = serde_json::to_vec(body).map_err(|_| error!("Could not encode page request."))?;
	let authorization = format!("Bearer {}", body.token);
	let mut request = Request::post(url)
		.map_err(|_| error!("Invalid Comic Pages URL."))?
		.header("Authorization", authorization.as_str())
		.header("X-Profile-Id", body.profile_id)
		.header("Content-Type", "application/json")
		.body(json);
	if let Some(token) = body.profile_token {
		request = request.header("X-Profile-Token", token);
	}
	Ok(request)
}

pub fn pages(client: &Client, content_id: &str, file_id: &str) -> Result<Vec<Page>> {
	let installation = settings::comic_pages_plugin();
	let api_version = if client.is_v2 { "v2" } else { "v1" };
	let base = plugin_base(&client.base, &installation, api_version)?;
	let url = format!("{base}/pages");
	let body = ReadRequest {
		token: &client.token,
		profile_id: &client.profile_id,
		profile_token: client.profile_token.as_deref(),
		content_id,
		file_id,
		api_version,
		cache_key: None,
		offset: 0,
	};
	for _ in 0..45 {
		let response = request(&url, &body)?
			.send()
			.map_err(|_| error!("Could not reach the Comic Pages plugin."))?;
		let status = response.status_code();
		if status == 202 {
			continue;
		}
		if status != 200 {
			bail!(
				"Comic Pages returned HTTP {status}. Check the plugin installation and chapter access."
			);
		}
		let data = response
			.get_data()
			.map_err(|_| error!("Could not read plugin page list."))?;
		let list: PageList =
			serde_json::from_slice(&data).map_err(|_| error!("Invalid Comic Pages response."))?;
		if list.chunk_bytes != CHUNK_BYTES
			|| list.pages.is_empty()
			|| list.pages.len() > 2048
			|| list.cache_key.len() != 64
			|| !list.cache_key.bytes().all(|c| c.is_ascii_hexdigit())
		{
			bail!("Unsupported Comic Pages response.");
		}
		let mut pages = Vec::with_capacity(list.pages.len());
		for (index, page) in list.pages.into_iter().enumerate() {
			if page.size == 0 || page.size > MAX_PAGE_BYTES {
				bail!("Comic Pages image exceeds the 32 MiB page limit.");
			}
			let mut context = PageContext::new();
			for (key, value) in [
				(MARKER, installation.clone()),
				("silo_plugin_api", String::from(api_version)),
				("silo_plugin_content", String::from(content_id)),
				("silo_plugin_file", String::from(file_id)),
				("silo_plugin_profile", client.profile_id.clone()),
				("silo_plugin_cache", list.cache_key.clone()),
				("silo_plugin_page", format!("{index}")),
				("silo_plugin_size", format!("{}", page.size)),
			] {
				context.insert(String::from(key), value);
			}
			pages.push(Page {
				content: PageContent::url_context(
					format!(
						"{base}/page/{index}?revision={}&reader={}",
						list.cache_key,
						encode_uri_component(&client.profile_id)
					),
					context,
				),
				..Default::default()
			});
		}
		return Ok(pages);
	}
	bail!("Comic Pages is still preparing this archive. Reopen the chapter shortly.");
}

fn value<'a>(context: &'a PageContext, key: &str) -> Result<&'a str> {
	context
		.get(key)
		.map(String::as_str)
		.ok_or_else(|| error!("Missing Comic Pages metadata."))
}

fn page_url(context: &PageContext) -> Result<String> {
	let installation = value(context, MARKER)?;
	if installation != settings::comic_pages_plugin() {
		bail!("Comic Pages settings changed. Reopen the chapter.");
	}
	let base = plugin_base(
		&settings::base_url()?,
		installation,
		value(context, "silo_plugin_api")?,
	)?;
	let index: usize = value(context, "silo_plugin_page")?
		.parse()
		.map_err(|_| error!("Invalid Comic Pages index."))?;
	if index >= 2048 {
		bail!("Invalid Comic Pages index.");
	}
	let cache_key = value(context, "silo_plugin_cache")?;
	if cache_key.len() != 64 || !cache_key.bytes().all(|c| c.is_ascii_hexdigit()) {
		bail!("Invalid Comic Pages cache key.");
	}
	Ok(format!(
		"{base}/page/{index}?revision={cache_key}&reader={}",
		encode_uri_component(value(context, "silo_plugin_profile")?)
	))
}

pub fn image_request(url: &str, context: &PageContext, offset: u64) -> Result<Request> {
	if url != page_url(context)? {
		bail!("Comic Pages URL does not match this Silo server. Reopen the chapter.");
	}
	let auth = client::ensure_image_auth();
	if auth.profile_id != value(context, "silo_plugin_profile")? {
		bail!("Silo profile changed. Reopen the chapter.");
	}
	let token = if settings::auth_mode() == "apiKey" {
		settings::api_key()
	} else {
		auth.token.unwrap_or_default()
	};
	request(
		url,
		&ReadRequest {
			token: &token,
			profile_id: &auth.profile_id,
			profile_token: auth.profile_token.as_deref(),
			content_id: value(context, "silo_plugin_content")?,
			file_id: value(context, "silo_plugin_file")?,
			api_version: value(context, "silo_plugin_api")?,
			cache_key: Some(value(context, "silo_plugin_cache")?),
			offset,
		},
	)
}

pub fn decode(code: u16, first_chunk: &[u8], context: &PageContext) -> Result<Vec<u8>> {
	let total: u64 = value(context, "silo_plugin_size")?
		.parse()
		.map_err(|_| error!("Invalid Comic Pages image size."))?;
	if total == 0 || total > MAX_PAGE_BYTES {
		bail!("Comic Pages image exceeds the 32 MiB page limit.");
	}
	if code != 200 || first_chunk.len() != (total as usize).min(CHUNK_BYTES) {
		bail!("Comic Pages image is unavailable or truncated. Reopen the chapter.");
	}
	let mut bytes = Vec::with_capacity(total as usize);
	bytes.extend_from_slice(first_chunk);
	let url = page_url(context)?;
	while bytes.len() < total as usize {
		let response = image_request(&url, context, bytes.len() as u64)?
			.send()
			.map_err(|_| error!("Could not fetch Comic Pages image chunk."))?;
		if response.status_code() != 200 {
			bail!("Comic Pages image changed or access expired. Reopen the chapter.");
		}
		let chunk = response
			.get_data()
			.map_err(|_| error!("Could not read image chunk."))?;
		if chunk.len() != (total as usize - bytes.len()).min(CHUNK_BYTES) {
			bail!("Comic Pages image chunk was truncated.");
		}
		bytes.extend_from_slice(&chunk);
	}
	Ok(bytes)
}

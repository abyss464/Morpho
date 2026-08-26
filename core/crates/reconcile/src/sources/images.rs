//! Image providers and the WebP encoder.
//!
//! Two families, and the difference between them is the whole of ruling #18:
//!
//! * **keyed stock libraries** — Unsplash, Pexels, Pixabay. A provider without
//!   a key is not queried at all; it is *disabled*, which the rules treat as
//!   waived (README Part 4 §"任务生命周期").
//! * **keyless open libraries** — Wikimedia Commons and Openverse. No
//!   credentials exist to be missing, so they are always enabled, and they are
//!   what makes a machine with no stock-photo account still produce real
//!   pictures instead of falling straight through to SDXL. Both carry per-file
//!   licence metadata, which is recorded on the candidate.
//!
//! Every provider gets its own dispatcher lane. Every downloaded photo is
//! decoded, fitted into the 768×576 box from README Part 5 and re-encoded as
//! WebP before it ever reaches the content-addressed store, whatever it came
//! from — so the library only ever holds bytes the app can use directly.

use serde::Deserialize;

use morpho_domain::error::TaskError;
use morpho_domain::types::ImageSource;

use crate::config::SourcesConfig;
use crate::sources::http;

/// Candidates requested per word from a keyed stock provider.
pub const RESULTS_PER_WORD: usize = 3;
/// Candidates kept per word from a keyless provider (ruling #18).
///
/// One more than the stock libraries, because an open collection is noisier:
/// the extra candidate gives the scorer something to reject.
pub const KEYLESS_RESULTS_PER_WORD: usize = 4;
/// Openverse is asked for more than it will keep, so that results filtered out
/// locally do not leave the word empty-handed.
const OPENVERSE_PAGE_SIZE: usize = 8;
/// Thumbnail width requested from Wikimedia Commons. Comfortably above the
/// 768-wide target box, so the fit is always a downscale.
const COMMONS_THUMB_WIDTH: u32 = 960;
/// Target box (README Part 5: WebP 768×576 q80).
pub const TARGET_WIDTH: u32 = 768;
pub const TARGET_HEIGHT: u32 = 576;
/// WebP quality. 80 is the budget's assumption.
pub const WEBP_QUALITY: f32 = 80.0;
/// Refuse absurd downloads before decoding them.
const MAX_DOWNLOAD_BYTES: usize = 24 * 1024 * 1024;
/// File extensions the decoder can actually handle. Commons is full of SVG,
/// TIFF, PDF and video in the same namespace, and a format the encoder cannot
/// read is better skipped than downloaded and thrown away.
const DECODABLE: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// One photo a provider offered, before download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoRef {
    pub source: ImageSource,
    /// Provider's own id, stored in `source_ref`.
    pub source_ref: String,
    pub download_url: String,
    pub license: Option<String>,
}

/// A downloaded, re-encoded photo ready for the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Search one provider.
pub async fn search(
    client: &reqwest::Client,
    config: &SourcesConfig,
    source: ImageSource,
    query: &str,
) -> Result<Vec<PhotoRef>, TaskError> {
    let encoded = http::encode_query(query);
    let context = format!("{source} search {query}");

    // The keyless half of ruling #18 first: nothing to look up, nothing to fail.
    match source {
        ImageSource::Wikimedia => {
            return search_wikimedia(client, config, &encoded, &context).await
        }
        ImageSource::Openverse => {
            return search_openverse(client, config, &encoded, &context).await
        }
        _ => {}
    }

    let Some(key) = config.image_key(source) else {
        // Callers check `enabled_image_sources` first; reaching here means a
        // key vanished between derivation and execution.
        return Err(TaskError::permanent(format!(
            "{source} has no API key configured"
        )));
    };

    match source {
        ImageSource::Unsplash => {
            let url = format!(
                "https://api.unsplash.com/search/photos?query={encoded}&per_page={RESULTS_PER_WORD}&orientation=landscape&content_filter=high"
            );
            let body: UnsplashResponse = http::get_json(
                client,
                &url,
                &[("Authorization", format!("Client-ID {key}"))],
                &context,
            )
            .await?;
            Ok(body
                .results
                .into_iter()
                .filter_map(|photo| {
                    Some(PhotoRef {
                        source,
                        source_ref: format!("unsplash:{}", photo.id),
                        download_url: photo.urls.regular.or(photo.urls.full)?,
                        license: Some(format!(
                            "Unsplash License; photo by {}",
                            photo.user.map(|u| u.name).unwrap_or_default()
                        )),
                    })
                })
                .collect())
        }
        ImageSource::Pexels => {
            let url = format!(
                "https://api.pexels.com/v1/search?query={encoded}&per_page={RESULTS_PER_WORD}&orientation=landscape"
            );
            let body: PexelsResponse = http::get_json(
                client,
                &url,
                &[("Authorization", key.to_string())],
                &context,
            )
            .await?;
            Ok(body
                .photos
                .into_iter()
                .filter_map(|photo| {
                    Some(PhotoRef {
                        source,
                        source_ref: format!("pexels:{}", photo.id),
                        download_url: photo.src.large.or(photo.src.original)?,
                        license: Some(format!(
                            "Pexels License; photo by {}",
                            photo.photographer.unwrap_or_default()
                        )),
                    })
                })
                .collect())
        }
        ImageSource::Pixabay => {
            let url = format!(
                "https://pixabay.com/api/?key={key}&q={encoded}&image_type=photo&orientation=horizontal&safesearch=true&per_page={}",
                RESULTS_PER_WORD.max(3)
            );
            let body: PixabayResponse = http::get_json(client, &url, &[], &context).await?;
            Ok(body
                .hits
                .into_iter()
                .take(RESULTS_PER_WORD)
                .filter_map(|hit| {
                    Some(PhotoRef {
                        source,
                        source_ref: format!("pixabay:{}", hit.id),
                        download_url: hit.large_image_url.or(hit.web_format_url)?,
                        license: Some(format!(
                            "Pixabay Content License; by {}",
                            hit.user.unwrap_or_default()
                        )),
                    })
                })
                .collect())
        }
        ImageSource::Wikimedia | ImageSource::Openverse => unreachable!("handled above"),
        ImageSource::Sdxl | ImageSource::Manual => Err(TaskError::permanent(format!(
            "{source} is not a searchable image provider"
        ))),
    }
}

/// Search Wikimedia Commons.
///
/// `generator=search` over namespace 6 (`File:`) runs the same full-text search
/// as `list=search` but feeds the hits straight into `prop=imageinfo`, so one
/// request yields both the ranking and every file's URL, size and licence.
/// `iiurlwidth` asks the thumbnailer for a 960-wide rendering, which is what
/// gets downloaded — pulling the originals would mean multi-megabyte camera
/// files for a picture that ends up 768 pixels wide.
async fn search_wikimedia(
    client: &reqwest::Client,
    config: &SourcesConfig,
    encoded_query: &str,
    context: &str,
) -> Result<Vec<PhotoRef>, TaskError> {
    let url = format!(
        "{}?action=query&format=json&formatversion=2\
         &generator=search&gsrsearch={encoded_query}&gsrnamespace=6&gsrlimit={}\
         &prop=imageinfo&iiprop=url%7Csize%7Cextmetadata\
         &iiextmetadatafilter=LicenseShortName%7CUsageTerms%7CArtist%7CLicense\
         &iiurlwidth={COMMONS_THUMB_WIDTH}",
        config.wikimedia_url.trim_end_matches('/'),
        // Over-fetch: the namespace holds plenty of SVG and video that the
        // extension filter below will throw away.
        KEYLESS_RESULTS_PER_WORD * 2,
    );
    let body: CommonsResponse = http::get_json(client, &url, &[], context).await?;
    Ok(body
        .query
        .pages
        .into_iter()
        .filter(|page| is_decodable(&page.title))
        .filter_map(|page| {
            let info = page.imageinfo.into_iter().next()?;
            let meta = &info.extmetadata;
            Some(PhotoRef {
                source: ImageSource::Wikimedia,
                source_ref: format!("wikimedia:{}", page.title),
                download_url: info.thumburl.or(info.url)?,
                license: Some(attribution(
                    meta.license_short_name
                        .as_ref()
                        .or(meta.usage_terms.as_ref())
                        .or(meta.license.as_ref())
                        .map(|field| field.value.as_str()),
                    meta.artist.as_ref().map(|field| field.value.as_str()),
                    "Wikimedia Commons",
                )),
            })
        })
        .take(KEYLESS_RESULTS_PER_WORD)
        .collect())
}

/// Search Openverse.
///
/// `license_type=commercial,modification` is the aggregator's own filter for
/// "reusable and remixable", which is the only kind of picture that may ship
/// inside a release bundle.
async fn search_openverse(
    client: &reqwest::Client,
    config: &SourcesConfig,
    encoded_query: &str,
    context: &str,
) -> Result<Vec<PhotoRef>, TaskError> {
    let url = format!(
        "{}?q={encoded_query}&license_type=commercial,modification&page_size={OPENVERSE_PAGE_SIZE}",
        config.openverse_url.trim_end_matches('/')
    );
    let body: OpenverseResponse = http::get_json(client, &url, &[], context).await?;
    Ok(body
        .results
        .into_iter()
        .filter_map(|hit| {
            let download_url = hit.url.or(hit.thumbnail)?;
            if !is_decodable(&download_url) {
                return None;
            }
            let license = match (hit.license.as_deref(), hit.license_version.as_deref()) {
                (Some(name), Some(version)) => {
                    Some(format!("CC {} {version}", name.to_uppercase()))
                }
                (Some(name), None) => Some(format!("CC {}", name.to_uppercase())),
                _ => None,
            };
            Some(PhotoRef {
                source: ImageSource::Openverse,
                source_ref: format!("openverse:{}", hit.id),
                download_url,
                license: Some(attribution(
                    license.as_deref(),
                    hit.creator.as_deref(),
                    "Openverse",
                )),
            })
        })
        .take(KEYLESS_RESULTS_PER_WORD)
        .collect())
}

/// One human-readable attribution line, stored in `image_candidates.license`.
///
/// The console renders this verbatim next to the picture, and the exporter
/// carries it into the bundle, so it has to stand on its own: what the licence
/// is, who made it, where it came from.
fn attribution(license: Option<&str>, author: Option<&str>, provider: &str) -> String {
    let license = license
        .map(strip_markup)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "licence unstated".to_string());
    match author.map(strip_markup).filter(|value| !value.is_empty()) {
        Some(author) => format!("{license}; by {author} ({provider})"),
        None => format!("{license} ({provider})"),
    }
}

/// Reduce an extmetadata field to plain text.
///
/// Commons stores `Artist` as rendered HTML — an anchor to the uploader's user
/// page, sometimes wrapped in a `<span>` — because it is meant for a web page.
/// The console is not one, so the tags come out and the entities go back.
fn strip_markup(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut depth = 0usize;
    for ch in value.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Does this file name or URL end in something the decoder can read?
fn is_decodable(name: &str) -> bool {
    let path = name.split(['?', '#']).next().unwrap_or(name);
    let Some((_, extension)) = path.rsplit_once('.') else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    DECODABLE.contains(&extension.as_str())
}

/// Download one photo and re-encode it for the library.
pub async fn download(
    client: &reqwest::Client,
    photo: &PhotoRef,
) -> Result<EncodedImage, TaskError> {
    let bytes = http::get_bytes(
        client,
        &photo.download_url,
        &[],
        &format!("{} download {}", photo.source, photo.source_ref),
    )
    .await?;
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        return Err(TaskError::permanent(format!(
            "{} returned {} bytes, over the {MAX_DOWNLOAD_BYTES}-byte limit",
            photo.source,
            bytes.len()
        )));
    }
    encode(&bytes)
}

/// Decode arbitrary image bytes, fit them into the target box and encode WebP.
///
/// "Fit" means preserve the aspect ratio and never upscale: an image smaller
/// than the box keeps its own size rather than being stretched into blur. The
/// scorer already penalises low resolution, so the honest thing here is to keep
/// the pixels that exist.
pub fn encode(bytes: &[u8]) -> Result<EncodedImage, TaskError> {
    let decoded = image::load_from_memory(bytes)
        // Undecodable bytes will not become decodable on a retry.
        .map_err(|err| TaskError::permanent(format!("cannot decode image: {err}")))?;

    let fitted = if decoded.width() > TARGET_WIDTH || decoded.height() > TARGET_HEIGHT {
        decoded.resize(
            TARGET_WIDTH,
            TARGET_HEIGHT,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        decoded
    };

    let rgb = fitted.to_rgb8();
    let (width, height) = (rgb.width(), rgb.height());
    if width == 0 || height == 0 {
        return Err(TaskError::permanent("image decoded to zero pixels"));
    }

    let encoder = webp::Encoder::from_rgb(rgb.as_raw(), width, height);
    let encoded = encoder.encode(WEBP_QUALITY);
    Ok(EncodedImage {
        bytes: encoded.to_vec(),
        width,
        height,
    })
}

// --- Provider response shapes ------------------------------------------------

#[derive(Debug, Deserialize)]
struct UnsplashResponse {
    #[serde(default)]
    results: Vec<UnsplashPhoto>,
}

#[derive(Debug, Deserialize)]
struct UnsplashPhoto {
    #[serde(default)]
    id: String,
    #[serde(default)]
    urls: UnsplashUrls,
    #[serde(default)]
    user: Option<UnsplashUser>,
}

#[derive(Debug, Default, Deserialize)]
struct UnsplashUrls {
    #[serde(default)]
    regular: Option<String>,
    #[serde(default)]
    full: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UnsplashUser {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct PexelsResponse {
    #[serde(default)]
    photos: Vec<PexelsPhoto>,
}

#[derive(Debug, Deserialize)]
struct PexelsPhoto {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    photographer: Option<String>,
    #[serde(default)]
    src: PexelsSrc,
}

#[derive(Debug, Default, Deserialize)]
struct PexelsSrc {
    #[serde(default)]
    large: Option<String>,
    #[serde(default)]
    original: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PixabayResponse {
    #[serde(default)]
    hits: Vec<PixabayHit>,
}

#[derive(Debug, Deserialize)]
struct PixabayHit {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    user: Option<String>,
    #[serde(rename = "largeImageURL", default)]
    large_image_url: Option<String>,
    #[serde(rename = "webformatURL", default)]
    web_format_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CommonsResponse {
    #[serde(default)]
    query: CommonsQuery,
}

#[derive(Debug, Default, Deserialize)]
struct CommonsQuery {
    /// `formatversion=2` turns this from a pageid-keyed object into an array,
    /// which is the only reason this parses without a custom visitor.
    #[serde(default)]
    pages: Vec<CommonsPage>,
}

#[derive(Debug, Deserialize)]
struct CommonsPage {
    #[serde(default)]
    title: String,
    #[serde(default)]
    imageinfo: Vec<CommonsImageInfo>,
}

#[derive(Debug, Deserialize)]
struct CommonsImageInfo {
    #[serde(default)]
    thumburl: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    extmetadata: CommonsExtMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct CommonsExtMetadata {
    #[serde(rename = "LicenseShortName", default)]
    license_short_name: Option<MetadataField>,
    #[serde(rename = "UsageTerms", default)]
    usage_terms: Option<MetadataField>,
    #[serde(rename = "License", default)]
    license: Option<MetadataField>,
    #[serde(rename = "Artist", default)]
    artist: Option<MetadataField>,
}

#[derive(Debug, Deserialize)]
struct MetadataField {
    #[serde(default)]
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct OpenverseResponse {
    #[serde(default)]
    results: Vec<OpenverseHit>,
}

#[derive(Debug, Deserialize)]
struct OpenverseHit {
    #[serde(default)]
    id: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    creator: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    license_version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut buffer = image::RgbImage::new(width, height);
        for (x, y, pixel) in buffer.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(buffer)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn oversized_images_are_fitted_into_the_target_box() {
        let encoded = encode(&png(2400, 1600)).unwrap();
        assert!(encoded.width <= TARGET_WIDTH);
        assert!(encoded.height <= TARGET_HEIGHT);
        // 3:2 input fitted into a 4:3 box is width-limited.
        assert_eq!(encoded.width, TARGET_WIDTH);
    }

    #[test]
    fn aspect_ratio_is_preserved() {
        let encoded = encode(&png(2000, 1000)).unwrap();
        let ratio = encoded.width as f64 / encoded.height as f64;
        assert!((ratio - 2.0).abs() < 0.02, "{encoded:?}");
    }

    #[test]
    fn small_images_are_never_upscaled() {
        let encoded = encode(&png(320, 240)).unwrap();
        assert_eq!((encoded.width, encoded.height), (320, 240));
    }

    #[test]
    fn output_is_a_webp_file() {
        let encoded = encode(&png(1000, 800)).unwrap();
        assert_eq!(&encoded.bytes[0..4], b"RIFF");
        assert_eq!(&encoded.bytes[8..12], b"WEBP");
    }

    #[test]
    fn encoding_is_deterministic_so_the_same_photo_dedupes() {
        let source = png(1200, 900);
        assert_eq!(encode(&source).unwrap(), encode(&source).unwrap());
    }

    #[test]
    fn the_result_decodes_again() {
        let encoded = encode(&png(1200, 900)).unwrap();
        let round_trip = image::load_from_memory(&encoded.bytes).unwrap();
        assert_eq!(round_trip.width(), encoded.width);
        assert_eq!(round_trip.height(), encoded.height);
    }

    #[test]
    fn undecodable_bytes_are_permanent_not_retried() {
        let err = encode(b"this is not an image").unwrap_err();
        assert_eq!(err.kind(), morpho_domain::error::ErrorKind::Permanent);
    }

    #[test]
    fn unsplash_responses_map_onto_photo_refs() {
        let body = r#"{"results":[{"id":"abc123","urls":{"regular":"https://img/r.jpg",
                       "full":"https://img/f.jpg"},"user":{"name":"Ada"}}]}"#;
        let parsed: UnsplashResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].id, "abc123");
        assert_eq!(
            parsed.results[0].urls.regular.as_deref(),
            Some("https://img/r.jpg")
        );
    }

    #[test]
    fn pexels_and_pixabay_responses_parse() {
        let pexels: PexelsResponse = serde_json::from_str(
            r#"{"photos":[{"id":7,"photographer":"Bo","src":{"large":"https://p/l.jpg"}}]}"#,
        )
        .unwrap();
        assert_eq!(pexels.photos[0].id, 7);
        assert_eq!(
            pexels.photos[0].src.large.as_deref(),
            Some("https://p/l.jpg")
        );

        let pixabay: PixabayResponse = serde_json::from_str(
            r#"{"total":1,"hits":[{"id":9,"user":"Cy","largeImageURL":"https://x/l.jpg",
                "webformatURL":"https://x/w.jpg"}]}"#,
        )
        .unwrap();
        assert_eq!(pixabay.hits[0].id, 9);
        assert_eq!(
            pixabay.hits[0].large_image_url.as_deref(),
            Some("https://x/l.jpg")
        );
    }

    #[test]
    fn an_empty_provider_response_is_an_empty_result_not_an_error() {
        let parsed: UnsplashResponse = serde_json::from_str(r#"{"results":[]}"#).unwrap();
        assert!(parsed.results.is_empty());
        let parsed: PixabayResponse = serde_json::from_str(r#"{"total":0,"hits":[]}"#).unwrap();
        assert!(parsed.hits.is_empty());
    }

    #[tokio::test]
    async fn searching_a_keyless_provider_is_permanent() {
        let config = SourcesConfig::default();
        let client = http::build_client(&config).unwrap();
        let err = search(&client, &config, ImageSource::Unsplash, "serene")
            .await
            .unwrap_err();
        assert_eq!(err.kind(), morpho_domain::error::ErrorKind::Permanent);
        assert!(err.message().contains("no API key"));
    }

    #[tokio::test]
    async fn generated_and_manual_sources_are_not_searchable() {
        let config = SourcesConfig {
            unsplash_access_key: Some("k".into()),
            ..SourcesConfig::default()
        };
        let client = http::build_client(&config).unwrap();
        for source in [ImageSource::Sdxl, ImageSource::Manual] {
            let err = search(&client, &config, source, "serene")
                .await
                .unwrap_err();
            assert_eq!(err.kind(), morpho_domain::error::ErrorKind::Permanent);
        }
    }

    // -- keyless providers (ruling #18) ------------------------------------

    /// Trimmed from a live Commons `generator=search` response.
    const COMMONS: &str = r#"{
      "batchcomplete": true,
      "query": {"pages": [
        {"pageid": 49266573, "ns": 6, "title": "File:LakeSerene2.jpg",
         "imageinfo": [{
            "size": 1306832, "width": 1474, "height": 1964,
            "thumburl": "https://upload.wikimedia.org/w/thumb/2d/LakeSerene2.jpg/960px-LakeSerene2.jpg",
            "thumbwidth": 960, "thumbheight": 1279,
            "url": "https://upload.wikimedia.org/w/2d/LakeSerene2.jpg",
            "descriptionurl": "https://commons.wikimedia.org/wiki/File:LakeSerene2.jpg",
            "extmetadata": {
              "Artist": {"value": "<a href=\"//commons.wikimedia.org/wiki/User:Danust\" class=\"new\">Dan &amp; Ust</a>", "source": "commons-desc-page"},
              "LicenseShortName": {"value": "CC BY-SA 4.0", "source": "commons-desc-page"},
              "UsageTerms": {"value": "Creative Commons Attribution-Share Alike 4.0"},
              "License": {"value": "cc-by-sa-4.0"}
            }}]},
        {"pageid": 111, "ns": 6, "title": "File:Serenity diagram.svg",
         "imageinfo": [{"url": "https://upload.wikimedia.org/w/aa/Serenity_diagram.svg",
                        "extmetadata": {}}]},
        {"pageid": 222, "ns": 6, "title": "File:Quiet water.png",
         "imageinfo": [{"url": "https://upload.wikimedia.org/w/bb/Quiet_water.png",
                        "extmetadata": {}}]}
      ]}
    }"#;

    /// Trimmed from a live Openverse `/v1/images/` response.
    const OPENVERSE: &str = r#"{
      "result_count": 240, "page_count": 80, "page_size": 3, "page": 1,
      "results": [
        {"id": "b806336a-71eb-408f-8ee3-72d27d1d1823",
         "title": "Lovely serene night scene",
         "url": "https://live.staticflickr.com/2833/32577135193_2331616d15_b.jpg",
         "creator": "PiktourUK", "license": "by", "license_version": "2.0",
         "provider": "flickr", "width": 1024, "height": 485,
         "thumbnail": "https://api.openverse.org/v1/images/b806336a/thumb/",
         "tags": [{"name": "boats"}], "unstable__sensitivity": []},
        {"id": "no-extension", "url": "https://example.test/render?id=7",
         "creator": "Ada", "license": "cc0", "license_version": "1.0"}
      ]
    }"#;

    #[test]
    fn commons_pages_become_photo_refs_with_attribution() {
        let parsed: CommonsResponse = serde_json::from_str(COMMONS).unwrap();
        assert_eq!(parsed.query.pages.len(), 3);
        let page = &parsed.query.pages[0];
        assert_eq!(page.title, "File:LakeSerene2.jpg");
        let info = &page.imageinfo[0];
        assert!(info.thumburl.as_deref().unwrap().contains("960px"));
        assert_eq!(
            info.extmetadata.license_short_name.as_ref().unwrap().value,
            "CC BY-SA 4.0"
        );
    }

    #[test]
    fn commons_attribution_is_plain_text() {
        let parsed: CommonsResponse = serde_json::from_str(COMMONS).unwrap();
        let meta = &parsed.query.pages[0].imageinfo[0].extmetadata;
        let line = attribution(
            meta.license_short_name.as_ref().map(|f| f.value.as_str()),
            meta.artist.as_ref().map(|f| f.value.as_str()),
            "Wikimedia Commons",
        );
        assert_eq!(line, "CC BY-SA 4.0; by Dan & Ust (Wikimedia Commons)");
    }

    #[test]
    fn undecodable_commons_formats_are_skipped() {
        // Namespace 6 is full of SVG, TIFF and video; the decoder reads none.
        assert!(!is_decodable("File:Serenity diagram.svg"));
        assert!(!is_decodable("File:Scan.tif"));
        assert!(!is_decodable("File:Clip.ogv"));
        assert!(is_decodable("File:LakeSerene2.jpg"));
        assert!(is_decodable("File:Quiet water.PNG"));
        assert!(is_decodable("https://host/a/b.jpeg?width=960&x=1"));
        assert!(!is_decodable("https://example.test/render?id=7"));
        assert!(!is_decodable("no-dot-at-all"));
    }

    #[test]
    fn openverse_hits_become_photo_refs() {
        let parsed: OpenverseResponse = serde_json::from_str(OPENVERSE).unwrap();
        assert_eq!(parsed.results.len(), 2);
        let hit = &parsed.results[0];
        assert_eq!(hit.id, "b806336a-71eb-408f-8ee3-72d27d1d1823");
        assert_eq!(hit.creator.as_deref(), Some("PiktourUK"));
        assert_eq!(
            attribution(
                Some(&format!(
                    "CC {} {}",
                    hit.license.as_deref().unwrap().to_uppercase(),
                    hit.license_version.as_deref().unwrap()
                )),
                hit.creator.as_deref(),
                "Openverse",
            ),
            "CC BY 2.0; by PiktourUK (Openverse)"
        );
    }

    #[test]
    fn a_missing_licence_is_stated_rather_than_implied() {
        assert_eq!(
            attribution(None, None, "Openverse"),
            "licence unstated (Openverse)"
        );
        assert_eq!(
            attribution(Some("   "), Some("Ada"), "Wikimedia Commons"),
            "licence unstated; by Ada (Wikimedia Commons)"
        );
        assert_eq!(
            attribution(Some("CC0 1.0"), None, "Openverse"),
            "CC0 1.0 (Openverse)"
        );
    }

    #[test]
    fn markup_is_reduced_to_text() {
        assert_eq!(
            strip_markup("<a href=\"//x\" class=\"new\">Jane&nbsp;Doe</a>"),
            "Jane Doe"
        );
        assert_eq!(strip_markup("<span>A  &amp;  B</span>"), "A & B");
        assert_eq!(strip_markup("plain"), "plain");
        assert_eq!(strip_markup("<b></b>"), "");
    }

    #[tokio::test]
    async fn the_keyless_providers_never_ask_for_a_key() {
        // The whole point of ruling #18: a default config, with no credentials
        // anywhere, still has two live image sources.
        let config = SourcesConfig::default();
        assert!(config.image_key(ImageSource::Wikimedia).is_none());
        assert!(config.image_key(ImageSource::Openverse).is_none());
        assert!(config
            .enabled_image_sources()
            .contains(&ImageSource::Wikimedia));
        assert!(config
            .enabled_image_sources()
            .contains(&ImageSource::Openverse));
    }

    #[test]
    fn an_empty_keyless_response_is_an_empty_result_not_an_error() {
        let parsed: CommonsResponse = serde_json::from_str(r#"{"batchcomplete":true}"#).unwrap();
        assert!(parsed.query.pages.is_empty());
        let parsed: OpenverseResponse =
            serde_json::from_str(r#"{"result_count":0,"results":[]}"#).unwrap();
        assert!(parsed.results.is_empty());
    }
}

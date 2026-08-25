//! Stock-photo providers and the WebP encoder.
//!
//! Three independent APIs (Unsplash, Pexels, Pixabay), each on its own
//! dispatcher lane and each with its own key. A provider without a key is not
//! queried at all — it is *disabled*, which the rules treat as waived so the
//! SDXL fallback can fire (README Part 4 §"任务生命周期").
//!
//! Every downloaded photo is decoded, fitted into the 768×576 box from README
//! Part 5 and re-encoded as WebP before it ever reaches the content-addressed
//! store, so the library only ever holds bytes the app can use directly.

use serde::Deserialize;

use morpho_domain::error::TaskError;
use morpho_domain::types::ImageSource;

use crate::config::SourcesConfig;
use crate::sources::http;

/// Candidates requested per word per provider.
pub const RESULTS_PER_WORD: usize = 3;
/// Target box (README Part 5: WebP 768×576 q80).
pub const TARGET_WIDTH: u32 = 768;
pub const TARGET_HEIGHT: u32 = 576;
/// WebP quality. 80 is the budget's assumption.
pub const WEBP_QUALITY: f32 = 80.0;
/// Refuse absurd downloads before decoding them.
const MAX_DOWNLOAD_BYTES: usize = 24 * 1024 * 1024;

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
    let Some(key) = config.image_key(source) else {
        // Callers check `enabled_image_sources` first; reaching here means a
        // key vanished between derivation and execution.
        return Err(TaskError::permanent(format!(
            "{source} has no API key configured"
        )));
    };
    let encoded = encode_query(query);
    let context = format!("{source} search {query}");

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
        ImageSource::Sdxl | ImageSource::Manual => Err(TaskError::permanent(format!(
            "{source} is not a stock-photo provider"
        ))),
    }
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

fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
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

    #[test]
    fn query_encoding_is_form_style() {
        assert_eq!(encode_query("serene"), "serene");
        assert_eq!(encode_query("ad hoc"), "ad+hoc");
        assert_eq!(encode_query("a&b"), "a%26b");
    }
}

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
//! Wikimedia is searched two ways: by file name in the `File:` namespace, and —
//! only when that comes up short — by the lead image of the word's English
//! Wikipedia article. The second strategy exists because abstract words own no
//! file named after them but often own an article that has already decided how
//! to picture them. Either way the asset is a Commons file and its licence is
//! read from Commons, so nothing enters the library unattributed.
//!
//! Some words survive all of that with nothing, and for them the keyless
//! providers are asked a second time on looser terms — Openverse without its
//! licence filter, both of them with a query widened by the primary gloss's
//! content words. A second pass differs from the first only in what it asks
//! for; it records the same provider, the licence the result actually states,
//! and a note in `source_ref` saying which pass found it, which is what keeps
//! the scorer able to rank it below a first-pass hit.
//!
//! Every provider gets its own dispatcher lane. Every downloaded photo is
//! decoded, fitted into the 768×576 box from README Part 5 and re-encoded as
//! WebP before it ever reaches the content-addressed store, whatever it came
//! from — so the library only ever holds bytes the app can use directly.

use serde::Deserialize;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::types::ImageSource;

use crate::config::SourcesConfig;
use crate::score::ImageStrategy;
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
/// Shortest edge an article's lead image may declare, in pixels.
///
/// The `originalimage` field carries flags, coats of arms and wordmark logos
/// through the same slot as photographs. Those arrive as small SVGs, which the
/// decodable-format filter already refuses, but the raster ones need a floor —
/// and the summary states the size, so it costs no request to apply it.
const MIN_LEAD_IMAGE_EDGE: u32 = 96;
/// Weight below which a Commons SVG is an icon rather than a diagram.
///
/// A vector lead image reaches the library as Commons' own PNG rendering, so
/// the format is not the problem — the subject is. Logos, wordmarks and flag
/// icons are a handful of shapes and land under a kilobyte; a drawn diagram is
/// hundreds of paths and lands well above ten.
const SVG_MIN_BYTES: u64 = 10 * 1024;
/// Prefix every Commons-hosted asset shares. Anything else — a file uploaded to
/// the language wiki itself, an external mirror — has no Commons file page and
/// therefore no licence this code can read.
const COMMONS_UPLOAD_PREFIX: &str = "wikipedia/commons/";
/// Recorded in `source_ref` so a candidate says which strategy found it.
const ARTICLE_LEAD: &str = "article-lead";

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
///
/// `lemma` is the bare word; `query` is the same word widened with the gloss's
/// content words. Most providers only ever see the query — Wikimedia also needs
/// the lemma, because an article title is a word, not a search phrase.
///
/// `strategy` selects the pass. The first one is [`ImageStrategy::Strict`] and
/// is what every word gets; the others are retries for a word the strict passes
/// left with nothing, and only the keyless providers have them.
pub async fn search(
    client: &reqwest::Client,
    config: &SourcesConfig,
    source: ImageSource,
    lemma: &str,
    query: &str,
    strategy: ImageStrategy,
) -> Result<Vec<PhotoRef>, TaskError> {
    let encoded = http::encode_query(query);
    let context = format!("{source} search {query}");

    // The keyless half of ruling #18 first: nothing to look up, nothing to fail.
    match source {
        ImageSource::Wikimedia => {
            return search_wikimedia(client, config, lemma, &encoded, &context, strategy).await
        }
        ImageSource::Openverse => {
            return search_openverse(client, config, &encoded, &context, strategy).await
        }
        _ => {}
    }

    // A stock library has one pass and no second one; a job asking for another
    // is a rule and executor disagreeing, which no retry fixes.
    if strategy != ImageStrategy::Strict {
        return Err(TaskError::permanent(format!(
            "{source} has no {strategy:?} pass"
        )));
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

/// Search Wikimedia, in two strategies.
///
/// The `File:` namespace is the first and the better one when it answers: a
/// file named after the word is usually a picture of the word. It answers well
/// for concrete nouns and thinly for the abstract and the common — a live pass
/// left several hundred words, `desire` and `manner` and `instance` among them,
/// with no candidate at all.
///
/// Many of those words do own an English Wikipedia article, and the article's
/// lead image is a considered editorial choice about how to depict the concept.
/// So when the namespace search comes up short of the per-word cap, the article
/// is asked as well. A word the first strategy already filled costs no extra
/// request, and the search phrase is not reused for it: an article is found by
/// its title, which is the bare word, not by the gloss-widened query.
///
/// The widened second pass runs the namespace search alone. The article lead is
/// found by title, and the title is the bare lemma whatever the query says — so
/// a second lookup would fetch the same summary and reach the same picture the
/// strict pass already decided about, for the price of two more requests.
async fn search_wikimedia(
    client: &reqwest::Client,
    config: &SourcesConfig,
    lemma: &str,
    encoded_query: &str,
    context: &str,
    strategy: ImageStrategy,
) -> Result<Vec<PhotoRef>, TaskError> {
    let mut photos = search_commons_files(client, config, encoded_query, context, strategy).await?;
    if strategy == ImageStrategy::Strict && photos.len() < KEYLESS_RESULTS_PER_WORD {
        match article_lead(client, config, lemma, context).await {
            Ok(Some(photo)) => {
                // The article's picture is often also the top namespace hit.
                if !photos
                    .iter()
                    .any(|existing| existing.download_url == photo.download_url)
                {
                    photos.push(photo);
                }
            }
            Ok(None) => {}
            // No article, a disambiguation page, a picture that is not on
            // Commons: the word has no lead image, and whatever the namespace
            // search found still stands.
            Err(err) if err.kind() == ErrorKind::Permanent => {
                tracing::debug!(lemma, error = %err, "no usable wikipedia lead image");
            }
            // A parked lane or an unreachable host is the caller's problem, the
            // same way it is for a photo download.
            Err(err) => return Err(err),
        }
    }
    Ok(photos)
}

/// Strategy one: full-text search over the Commons `File:` namespace.
///
/// `generator=search` over namespace 6 (`File:`) runs the same full-text search
/// as `list=search` but feeds the hits straight into `prop=imageinfo`, so one
/// request yields both the ranking and every file's URL, size and licence.
/// `iiurlwidth` asks the thumbnailer for a 960-wide rendering, which is what
/// gets downloaded — pulling the originals would mean multi-megabyte camera
/// files for a picture that ends up 768 pixels wide.
async fn search_commons_files(
    client: &reqwest::Client,
    config: &SourcesConfig,
    encoded_query: &str,
    context: &str,
    strategy: ImageStrategy,
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
                source_ref: annotate(&format!("wikimedia:{}", page.title), &[strategy.note()]),
                download_url: info.thumburl.or(info.url)?,
                license: Some(attribution(
                    commons_license(meta).as_deref(),
                    meta.artist.as_ref().map(|field| field.value.as_str()),
                    "Wikimedia Commons",
                )),
            })
        })
        .take(KEYLESS_RESULTS_PER_WORD)
        .collect())
}

/// Strategy two: the lead image of the English Wikipedia article for the word.
///
/// Two requests, because the summary and the licence live in different places.
/// The REST summary says *which* picture the article leads with and nothing at
/// all about its terms; the terms are on the Commons file page, reached by
/// reading the file name back out of the asset URL. A picture whose file page
/// cannot be found, or whose extmetadata states no licence, is dropped — an
/// image with no readable terms may not ship in a release bundle, and guessing
/// is not an option here.
async fn article_lead(
    client: &reqwest::Client,
    config: &SourcesConfig,
    lemma: &str,
    context: &str,
) -> Result<Option<PhotoRef>, TaskError> {
    // A REST path segment, not a search phrase: MediaWiki titles take an
    // underscore where a multi-word lemma has a space.
    let title = http::encode_query(&lemma.trim().replace(' ', "_"));
    if title.is_empty() {
        return Ok(None);
    }
    let summary: WikipediaSummary = http::get_json(
        client,
        &format!("{}/{title}", config.wikipedia_url.trim_end_matches('/')),
        &[],
        context,
    )
    .await?;

    let Some(image) = lead_image(&summary) else {
        return Ok(None);
    };
    let Some(file) = commons_file_name(&image.source) else {
        return Ok(None);
    };
    let Some(page) = commons_file_page(client, config, &file, context).await? else {
        return Ok(None);
    };
    Ok(licensed_candidate(page, &[Some(ARTICLE_LEAD)]))
}

/// Attach provenance notes to a `source_ref`.
///
/// The notes ride in one trailing parenthesised group — `wikimedia:File:A.jpg
/// (article-lead)`, `openverse:abc (relaxed-license)` — which is the form the
/// scorer reads back to tell a first-pass candidate from a second-pass one, and
/// the form the console already renders. A pass with nothing to declare leaves
/// the reference exactly as the first wave wrote it.
fn annotate(base: &str, notes: &[Option<&str>]) -> String {
    let notes: Vec<&str> = notes
        .iter()
        .filter_map(|note| note.filter(|value| !value.is_empty()))
        .collect();
    if notes.is_empty() {
        base.to_string()
    } else {
        format!("{base} ({})", notes.join(", "))
    }
}

/// The picture an article leads with, if it is one worth looking up.
///
/// `originalimage` is the article's actual lead image; `thumbnail` is a
/// rendering of the same file, and it is only consulted when the original is
/// absent — falling back from a rejected original to its rendering would undo
/// the rejection, since the two are the same picture at different sizes.
fn lead_image(summary: &WikipediaSummary) -> Option<&SummaryImage> {
    // A disambiguation page's picture is the ambiguity icon: it depicts the
    // fact that the word is ambiguous, which teaches nothing about the word.
    if summary.kind.as_deref() == Some("disambiguation") {
        return None;
    }
    let image = summary
        .originalimage
        .as_ref()
        .or(summary.thumbnail.as_ref())?;
    is_depiction(image).then_some(image)
}

/// Is this lead image worth a second request?
fn is_depiction(image: &SummaryImage) -> bool {
    // A vector is admitted here and judged by weight on the file page: an SVG
    // is either a drawn diagram, which is a fine illustration, or a wordmark,
    // which is not, and the summary says nothing that tells them apart.
    if !is_decodable(&image.source) && !is_vector(&image.source) {
        return false;
    }
    // Only a stated dimension can disqualify; a summary that omits the size
    // gets the benefit of the doubt and is filtered on decode instead.
    let too_small = |edge: Option<u32>| edge.is_some_and(|value| value < MIN_LEAD_IMAGE_EDGE);
    !too_small(image.width) && !too_small(image.height)
}

/// Is this file name an SVG?
fn is_vector(name: &str) -> bool {
    let path = name.split(['?', '#']).next().unwrap_or(name);
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("svg"))
}

/// The Commons file name behind an `upload.wikimedia.org` asset URL.
///
/// Two shapes come out of the summary endpoint:
///
/// ```text
/// https://upload.wikimedia.org/wikipedia/commons/3/3f/Lake_Serene.jpg
/// https://upload.wikimedia.org/wikipedia/commons/thumb/3/3f/Lake_Serene.jpg/320px-Lake_Serene.jpg
/// ```
///
/// `wikipedia/en/…` — a file uploaded to the language wiki rather than to
/// Commons, which is where non-free logos live — has no Commons file page, so
/// no licence, so no candidate.
fn commons_file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let rest = path
        .strip_prefix("https://upload.wikimedia.org/")
        .or_else(|| path.strip_prefix("http://upload.wikimedia.org/"))?
        .strip_prefix(COMMONS_UPLOAD_PREFIX)?;

    let mut segments: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if segments.first() == Some(&"thumb") {
        segments.remove(0);
        // The trailing segment is the rendition (`320px-Name.jpg`, or
        // `langde-320px-Name.svg.png`); the file name is the one before it.
        segments.pop()?;
    }
    // What remains is `<hash1>/<hash2>/<name>`.
    if segments.len() != 3 {
        return None;
    }
    // Underscores in a MediaWiki title are spaces, and the API accepts either;
    // spaces are what the file page itself reports, so `source_ref` matches the
    // titles the namespace search produces.
    let name = percent_decode(segments[2]).replace('_', " ");
    // The rendition may have been the only usable thing in the URL: a TIFF or
    // a video frame renders to a JPEG thumbnail, and neither is a file this
    // strategy should be reaching for.
    (!name.is_empty() && (is_decodable(&name) || is_vector(&name))).then_some(name)
}

/// One Commons file page, by exact title.
async fn commons_file_page(
    client: &reqwest::Client,
    config: &SourcesConfig,
    file: &str,
    context: &str,
) -> Result<Option<CommonsPage>, TaskError> {
    let url = format!(
        "{}?action=query&format=json&formatversion=2\
         &titles=File%3A{}&prop=imageinfo&iiprop=url%7Csize%7Cextmetadata\
         &iiextmetadatafilter=LicenseShortName%7CUsageTerms%7CArtist%7CLicense\
         &iiurlwidth={COMMONS_THUMB_WIDTH}",
        config.wikimedia_url.trim_end_matches('/'),
        http::encode_query(file),
    );
    let body: CommonsResponse = http::get_json(client, &url, &[], context).await?;
    Ok(body.query.pages.into_iter().next())
}

/// A Commons page as a candidate, refused unless its licence is on record.
///
/// Stricter than the namespace search, which keeps a file whose extmetadata is
/// silent and says so in the attribution line. Here the picture was reached
/// sideways, through an article, so "licence unstated" would be a claim about a
/// file this code never looked up properly rather than a fact about the file.
fn licensed_candidate(page: CommonsPage, notes: &[Option<&str>]) -> Option<PhotoRef> {
    let title = page.title.clone();
    let info = page.imageinfo.into_iter().next()?;
    if is_vector(&title) {
        // Commons renders SVG to PNG for every thumbnail, so a drawn diagram
        // is perfectly usable even though the source file is not decodable.
        // A wordmark or a flag icon is not usable, and weight is what tells
        // the two apart: a diagram carries hundreds of paths, a logo a dozen
        // shapes. Measured on Commons, `DNA simple2.svg` is 27 kB and
        // `Commons-logo.svg` is under 1 kB.
        if info.size.unwrap_or(0) < SVG_MIN_BYTES {
            return None;
        }
    } else if !is_decodable(&title) {
        return None;
    }
    let meta = &info.extmetadata;
    let license = commons_license(meta)?;
    Some(PhotoRef {
        source: ImageSource::Wikimedia,
        // Same shape the namespace search records, plus which strategy found
        // it, so a console reviewer can tell the two apart.
        source_ref: annotate(&format!("wikimedia:{title}"), notes),
        // The 960-wide rendering of the very file the article leads with: the
        // original is the right picture and the wrong number of bytes. The
        // bytes that get fetched must be decodable whatever the file is, which
        // for a vector means the rendered thumbnail and nothing else.
        download_url: info.thumburl.or(info.url).filter(|url| is_decodable(url))?,
        license: Some(attribution(
            Some(&license),
            meta.artist.as_ref().map(|field| field.value.as_str()),
            "Wikimedia Commons",
        )),
    })
}

/// The licence a Commons file states, as plain text, if it states one.
fn commons_license(meta: &CommonsExtMetadata) -> Option<String> {
    meta.license_short_name
        .as_ref()
        .or(meta.usage_terms.as_ref())
        .or(meta.license.as_ref())
        .map(|field| strip_markup(&field.value))
        .filter(|value| !value.is_empty())
}

/// Undo percent-encoding in a URL path segment.
///
/// Commons asset URLs carry the file name encoded — `Caf%C3%A9_noir.jpg` — and
/// the API wants the decoded title back. Invalid escapes are left alone rather
/// than dropped, so a malformed URL degrades into a title that simply does not
/// match instead of into a different file.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            let decoded = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok());
            if let Some(byte) = decoded {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Search Openverse.
///
/// `license_type=commercial,modification` is the aggregator's own filter for
/// "reusable and remixable", and it is what the strict pass asks for.
///
/// The relaxed pass drops the parameter entirely, which widens the answer to
/// every Creative Commons licence Openverse indexes — NonCommercial and
/// NoDerivatives included. That is a real change in what a candidate *is*, so
/// nothing about it is inferred: each result already states its own licence and
/// version in the payload, and that is what gets recorded, verbatim, on the
/// candidate. A word that reaches this pass has already come up empty
/// everywhere else, and a picture whose terms are recorded honestly is a
/// decision a human can make later — one that was never fetched is not.
async fn search_openverse(
    client: &reqwest::Client,
    config: &SourcesConfig,
    encoded_query: &str,
    context: &str,
    strategy: ImageStrategy,
) -> Result<Vec<PhotoRef>, TaskError> {
    let license_filter = match strategy {
        ImageStrategy::RelaxedLicense => "",
        _ => "&license_type=commercial,modification",
    };
    let url = format!(
        "{}?q={encoded_query}{license_filter}&page_size={OPENVERSE_PAGE_SIZE}",
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
                source_ref: annotate(&format!("openverse:{}", hit.id), &[strategy.note()]),
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

/// The `/page/summary/{title}` payload, reduced to the two fields that matter.
///
/// Deliberately not the whole thing: the summary also carries the extract, the
/// coordinates and a dozen link forms, none of which this strategy uses, and
/// none of which should be able to break parsing when the REST API adds more.
#[derive(Debug, Default, Deserialize)]
struct WikipediaSummary {
    /// `standard`, `disambiguation`, `mainpage`, `no-extract`.
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    originalimage: Option<SummaryImage>,
    #[serde(default)]
    thumbnail: Option<SummaryImage>,
}

#[derive(Debug, Default, Deserialize)]
struct SummaryImage {
    #[serde(default)]
    source: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CommonsImageInfo {
    #[serde(default)]
    thumburl: Option<String>,
    #[serde(default)]
    url: Option<String>,
    /// Bytes of the source file, which is how a vector icon is told from a
    /// vector diagram.
    #[serde(default)]
    size: Option<u64>,
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
        let err = search(
            &client,
            &config,
            ImageSource::Unsplash,
            "serene",
            "serene calm",
            ImageStrategy::Strict,
        )
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
            let err = search(
                &client,
                &config,
                source,
                "serene",
                "serene calm",
                ImageStrategy::Strict,
            )
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

    // -- strategy two: the article lead image -------------------------------

    #[test]
    fn a_commons_asset_url_yields_its_file_name() {
        assert_eq!(
            commons_file_name(
                "https://upload.wikimedia.org/wikipedia/commons/3/3f/Lake_Serene.jpg"
            )
            .as_deref(),
            Some("Lake Serene.jpg")
        );
        // The thumbnail form: the rendition segment is dropped, not read.
        assert_eq!(
            commons_file_name(
                "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3f/Lake_Serene.jpg/320px-Lake_Serene.jpg"
            )
            .as_deref(),
            Some("Lake Serene.jpg")
        );
        // Percent-escapes come back as the title the API expects.
        assert_eq!(
            commons_file_name(
                "https://upload.wikimedia.org/wikipedia/commons/a/ab/Caf%C3%A9_noir.jpg"
            )
            .as_deref(),
            Some("Café noir.jpg")
        );
        assert_eq!(
            commons_file_name(
                "https://upload.wikimedia.org/wikipedia/commons/a/ab/Desire.png?download=1"
            )
            .as_deref(),
            Some("Desire.png")
        );
    }

    #[test]
    fn a_file_that_is_not_on_commons_has_no_licence_to_read() {
        // Locally uploaded to the language wiki: no Commons file page, so
        // nothing to attribute, so no candidate.
        assert!(commons_file_name(
            "https://upload.wikimedia.org/wikipedia/en/4/44/Company_wordmark.png"
        )
        .is_none());
        assert!(commons_file_name("https://example.test/photo.jpg").is_none());
        assert!(commons_file_name("https://upload.wikimedia.org/wikipedia/commons/").is_none());
        // A rasterised thumbnail names the file it was rendered from, not the
        // rendition: a TIFF or a video frame is not something this strategy
        // should reach for, whatever the rendition's extension claims.
        assert!(commons_file_name(
            "https://upload.wikimedia.org/wikipedia/commons/thumb/1/12/Scan.tif/64px-Scan.tif.jpg"
        )
        .is_none());
        // An SVG does come through, to be judged by weight on the file page.
        assert_eq!(
            commons_file_name(
                "https://upload.wikimedia.org/wikipedia/commons/thumb/f/fe/DNA_simple2.svg/330px-DNA_simple2.svg.png"
            )
            .as_deref(),
            Some("DNA simple2.svg")
        );
    }

    #[test]
    fn percent_escapes_decode_and_bad_ones_survive() {
        assert_eq!(percent_decode("Caf%C3%A9"), "Café");
        assert_eq!(percent_decode("plain_name.jpg"), "plain_name.jpg");
        assert_eq!(percent_decode("100%25"), "100%");
        // Truncated or non-hex escapes stay as written rather than vanishing.
        assert_eq!(percent_decode("a%"), "a%");
        assert_eq!(percent_decode("a%zz"), "a%zz");
    }

    /// A live `/page/summary/Function` response, trimmed.
    const SUMMARY: &str = r#"{
      "type": "standard", "title": "Function (mathematics)",
      "thumbnail": {"source": "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3b/Function_machine2.svg/320px-Function_machine2.svg.png",
                    "width": 320, "height": 213},
      "originalimage": {"source": "https://upload.wikimedia.org/wikipedia/commons/3/3f/Lake_Serene.jpg",
                        "width": 1474, "height": 1964},
      "extract": "In mathematics, a function ..."
    }"#;

    #[test]
    fn the_original_is_preferred_over_the_thumbnail() {
        let summary: WikipediaSummary = serde_json::from_str(SUMMARY).unwrap();
        let image = lead_image(&summary).unwrap();
        assert!(image.source.ends_with("Lake_Serene.jpg"), "{image:?}");
    }

    #[test]
    fn a_disambiguation_page_offers_no_depiction() {
        let summary: WikipediaSummary = serde_json::from_str(
            r#"{"type":"disambiguation","title":"Instance",
                "originalimage":{"source":"https://upload.wikimedia.org/wikipedia/commons/3/3f/Disambig.jpg",
                                 "width":900,"height":900}}"#,
        )
        .unwrap();
        assert!(lead_image(&summary).is_none());
    }

    #[test]
    fn a_rejected_original_is_not_downgraded_to_its_own_thumbnail() {
        // The original is a scan the decoder cannot read; its thumbnail is a
        // JPEG of the same scan. Falling back would undo the rejection.
        let summary: WikipediaSummary = serde_json::from_str(
            r#"{"type":"standard",
                "originalimage":{"source":"https://upload.wikimedia.org/wikipedia/commons/1/12/Scan.tif",
                                 "width":2048,"height":1536},
                "thumbnail":{"source":"https://upload.wikimedia.org/wikipedia/commons/thumb/1/12/Scan.tif/320px-Scan.tif.jpg",
                             "width":320,"height":240}}"#,
        )
        .unwrap();
        assert!(lead_image(&summary).is_none());
    }

    #[test]
    fn a_vector_lead_is_carried_to_the_file_page_to_be_weighed() {
        // Nothing in the summary separates a drawn diagram from a wordmark, so
        // the decision is deferred rather than guessed.
        let summary: WikipediaSummary = serde_json::from_str(
            r#"{"type":"standard","title":"Structure",
                "thumbnail":{"source":"https://upload.wikimedia.org/wikipedia/commons/thumb/f/fe/DNA_simple2.svg/330px-DNA_simple2.svg.png",
                             "width":330,"height":582}}"#,
        )
        .unwrap();
        assert!(lead_image(&summary).is_some());

        let file_page = |bytes: u64| -> CommonsPage {
            serde_json::from_str(&format!(
                r#"{{"title": "File:DNA simple2.svg", "imageinfo": [{{
                     "size": {bytes},
                     "thumburl": "https://upload.wikimedia.org/w/thumb/DNA_simple2.svg/960px-DNA_simple2.svg.png",
                     "url": "https://upload.wikimedia.org/w/DNA_simple2.svg",
                     "extmetadata": {{"LicenseShortName": {{"value": "Public domain"}}}}}}]}}"#
            ))
            .unwrap()
        };

        // A real diagram: kept, and fetched as Commons' own PNG rendering
        // rather than as the SVG the decoder cannot read.
        let photo = licensed_candidate(file_page(26_978), &[Some(ARTICLE_LEAD)]).unwrap();
        assert!(photo.download_url.ends_with(".png"), "{photo:?}");
        assert_eq!(
            photo.source_ref,
            "wikimedia:File:DNA simple2.svg (article-lead)"
        );
        // A wordmark at the weight of `Commons-logo.svg`: refused.
        assert!(licensed_candidate(file_page(932), &[Some(ARTICLE_LEAD)]).is_none());
        // Weight unstated is treated as weightless, which is the safe way
        // round for a filter that exists to keep logos out.
        let unweighed: CommonsPage = serde_json::from_str(
            r#"{"title": "File:Logo.svg", "imageinfo": [{"thumburl": "https://x/960px-Logo.svg.png",
                "extmetadata": {"LicenseShortName": {"value": "CC BY-SA 3.0"}}}]}"#,
        )
        .unwrap();
        assert!(licensed_candidate(unweighed, &[Some(ARTICLE_LEAD)]).is_none());
    }

    #[test]
    fn a_vector_with_no_rendering_is_never_fetched_raw() {
        // Without a thumbnail there is nothing decodable to download, and the
        // raw SVG must not be substituted for it.
        let page: CommonsPage = serde_json::from_str(
            r#"{"title": "File:Diagram.svg", "imageinfo": [{"size": 40000,
                "url": "https://upload.wikimedia.org/w/Diagram.svg",
                "extmetadata": {"LicenseShortName": {"value": "CC0 1.0"}}}]}"#,
        )
        .unwrap();
        assert!(licensed_candidate(page, &[Some(ARTICLE_LEAD)]).is_none());
    }

    #[test]
    fn a_tiny_icon_is_not_a_picture_of_the_word() {
        let summary: WikipediaSummary = serde_json::from_str(
            r#"{"type":"standard",
                "originalimage":{"source":"https://upload.wikimedia.org/wikipedia/commons/1/12/Flag.png",
                                 "width":40,"height":24}}"#,
        )
        .unwrap();
        assert!(lead_image(&summary).is_none());
        // A summary that states no size gets the benefit of the doubt; the
        // decoder is the next filter.
        let summary: WikipediaSummary = serde_json::from_str(
            r#"{"type":"standard",
                "originalimage":{"source":"https://upload.wikimedia.org/wikipedia/commons/1/12/Manner.jpg"}}"#,
        )
        .unwrap();
        assert!(lead_image(&summary).is_some());
        // An article with no picture at all.
        let summary: WikipediaSummary =
            serde_json::from_str(r#"{"type":"standard","title":"Manner"}"#).unwrap();
        assert!(lead_image(&summary).is_none());
    }

    #[test]
    fn a_lead_image_records_the_real_licence_and_its_strategy() {
        let page: CommonsPage = serde_json::from_str(
            r#"{"pageid": 1, "ns": 6, "title": "File:Lake Serene.jpg",
                "imageinfo": [{"thumburl": "https://upload.wikimedia.org/w/thumb/960px-Lake_Serene.jpg",
                               "url": "https://upload.wikimedia.org/w/Lake_Serene.jpg",
                               "extmetadata": {"LicenseShortName": {"value": "CC BY-SA 4.0"},
                                               "Artist": {"value": "<a href=\"//x\">Dan &amp; Ust</a>"}}}]}"#,
        )
        .unwrap();
        let photo = licensed_candidate(page, &[Some(ARTICLE_LEAD)]).unwrap();
        assert_eq!(
            photo.source_ref,
            "wikimedia:File:Lake Serene.jpg (article-lead)"
        );
        assert_eq!(
            photo.license.as_deref(),
            Some("CC BY-SA 4.0; by Dan & Ust (Wikimedia Commons)")
        );
        assert!(photo.download_url.contains("960px"));
        assert_eq!(photo.source, ImageSource::Wikimedia);
    }

    #[test]
    fn a_file_page_with_no_stated_licence_produces_no_candidate() {
        // No licence, no candidate — an unattributable picture cannot ship in
        // a release bundle, and "licence unstated" would be a guess here.
        for imageinfo in [
            r#"[{"url": "https://upload.wikimedia.org/w/A.jpg", "extmetadata": {}}]"#,
            r#"[{"url": "https://upload.wikimedia.org/w/A.jpg",
                 "extmetadata": {"LicenseShortName": {"value": "  "}}}]"#,
            "[]",
        ] {
            let page: CommonsPage = serde_json::from_str(&format!(
                r#"{{"title": "File:A.jpg", "imageinfo": {imageinfo}}}"#
            ))
            .unwrap();
            assert!(
                licensed_candidate(page, &[Some(ARTICLE_LEAD)]).is_none(),
                "{imageinfo}"
            );
        }
        // A missing page carries no imageinfo at all.
        let page: CommonsPage =
            serde_json::from_str(r#"{"title": "File:Nope.jpg", "missing": true}"#).unwrap();
        assert!(licensed_candidate(page, &[Some(ARTICLE_LEAD)]).is_none());
    }

    // -- the two strategies together, over loopback -------------------------

    /// Four decodable namespace hits: the per-word cap, met by strategy one.
    const COMMONS_FULL: &str = r#"{"query": {"pages": [
        {"title": "File:One.jpg", "imageinfo": [{"thumburl": "https://host/1.jpg", "extmetadata": {}}]},
        {"title": "File:Two.jpg", "imageinfo": [{"thumburl": "https://host/2.jpg", "extmetadata": {}}]},
        {"title": "File:Three.jpg", "imageinfo": [{"thumburl": "https://host/3.jpg", "extmetadata": {}}]},
        {"title": "File:Four.jpg", "imageinfo": [{"thumburl": "https://host/4.jpg", "extmetadata": {}}]}
    ]}}"#;

    /// The Commons file page for the article's lead image.
    const FILE_PAGE: &str = r#"{"query": {"pages": [
        {"pageid": 42, "ns": 6, "title": "File:Lake Serene.jpg",
         "imageinfo": [{"thumburl": "https://upload.wikimedia.org/w/thumb/960px-Lake_Serene.jpg",
                        "url": "https://upload.wikimedia.org/w/Lake_Serene.jpg",
                        "extmetadata": {"LicenseShortName": {"value": "CC BY-SA 4.0"},
                                        "Artist": {"value": "Ada"}}}]}
    ]}}"#;

    /// A loopback HTTP server that answers canned JSON.
    ///
    /// The sources are plain `reqwest` against configurable base URLs, so this
    /// is enough to exercise the two-request path end to end — which request
    /// was made, and which was not — without a mocking dependency and without
    /// touching the network.
    struct Mock {
        base: String,
        requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        _task: tokio::task::JoinHandle<()>,
    }

    impl Mock {
        /// Every request line the server saw, in order.
        fn seen(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }

        fn asked_for(&self, needle: &str) -> bool {
            self.seen().iter().any(|line| line.contains(needle))
        }

        /// A config whose keyless endpoints all point here.
        fn config(&self) -> SourcesConfig {
            SourcesConfig {
                wikimedia_url: crate::config::WikimediaUrl(format!("{}/w/api.php", self.base)),
                wikipedia_url: crate::config::WikipediaUrl(format!("{}/summary", self.base)),
                openverse_url: crate::config::OpenverseUrl(format!("{}/v1/images/", self.base)),
                ..SourcesConfig::default()
            }
        }
    }

    /// Serve `routes` — the first entry whose needle appears in the request
    /// line wins; anything unmatched is a 404, which is what the real REST
    /// endpoint answers for a word with no article.
    async fn mock(routes: &'static [(&'static str, &'static str)]) -> Mock {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = std::sync::Arc::clone(&requests);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let log = std::sync::Arc::clone(&log);
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buffer = vec![0u8; 8192];
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    let line = String::from_utf8_lossy(&buffer[..read])
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    log.lock().unwrap().push(line.clone());
                    let response = match routes
                        .iter()
                        .find(|(needle, _)| line.contains(needle))
                        .map(|(_, body)| *body)
                    {
                        Some(body) => format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                             content-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        ),
                        None => "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\
                                 connection: close\r\n\r\n"
                            .to_string(),
                    };
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Mock {
            base,
            requests,
            _task: task,
        }
    }

    async fn wikimedia(server: &Mock, lemma: &str) -> Vec<PhotoRef> {
        search_as(
            server,
            ImageSource::Wikimedia,
            lemma,
            lemma,
            ImageStrategy::Strict,
        )
        .await
    }

    /// Run one pass of one provider against the loopback server.
    async fn search_as(
        server: &Mock,
        source: ImageSource,
        lemma: &str,
        query: &str,
        strategy: ImageStrategy,
    ) -> Vec<PhotoRef> {
        let config = server.config();
        let client = http::build_client(&config).unwrap();
        search(&client, &config, source, lemma, query, strategy)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_word_the_namespace_search_fills_costs_no_extra_request() {
        let server = mock(&[
            ("generator=search", COMMONS_FULL),
            ("/summary/", SUMMARY),
            ("titles=File", FILE_PAGE),
        ])
        .await;
        let photos = wikimedia(&server, "lake").await;
        assert_eq!(photos.len(), KEYLESS_RESULTS_PER_WORD);
        assert!(
            !server.asked_for("/summary/"),
            "strategy two ran anyway: {:?}",
            server.seen()
        );
    }

    #[tokio::test]
    async fn a_short_namespace_result_is_topped_up_from_the_article() {
        // COMMONS holds three pages, one of them an SVG: two survive, which is
        // under the cap of four.
        let server = mock(&[
            ("generator=search", COMMONS),
            ("/summary/", SUMMARY),
            ("titles=File", FILE_PAGE),
        ])
        .await;
        let photos = wikimedia(&server, "function").await;
        assert_eq!(photos.len(), 3, "{photos:?}");

        let lead = photos.last().unwrap();
        assert_eq!(
            lead.source_ref,
            "wikimedia:File:Lake Serene.jpg (article-lead)"
        );
        assert_eq!(
            lead.license.as_deref(),
            Some("CC BY-SA 4.0; by Ada (Wikimedia Commons)")
        );
        assert!(lead.download_url.contains("960px"), "{lead:?}");
        // The word, not the gloss-widened query, and the file page is asked
        // for by exact title.
        assert!(server.asked_for("/summary/function"), "{:?}", server.seen());
        assert!(
            server.asked_for("titles=File%3ALake+Serene.jpg"),
            "{:?}",
            server.seen()
        );
    }

    #[tokio::test]
    async fn a_word_with_no_article_keeps_what_commons_gave_it() {
        // The summary 404s, which is a permanent error, and must not take the
        // namespace hits down with it.
        let server = mock(&[("generator=search", COMMONS)]).await;
        let photos = wikimedia(&server, "manner").await;
        assert_eq!(photos.len(), 2, "{photos:?}");
        assert!(server.asked_for("/summary/manner"));
    }

    #[tokio::test]
    async fn a_disambiguation_article_never_reaches_the_file_page() {
        let server = mock(&[
            ("generator=search", COMMONS),
            (
                "/summary/",
                r#"{"type":"disambiguation","title":"Instance",
                    "originalimage":{"source":"https://upload.wikimedia.org/wikipedia/commons/3/3f/Disambig.jpg",
                                     "width":900,"height":900}}"#,
            ),
            ("titles=File", FILE_PAGE),
        ])
        .await;
        let photos = wikimedia(&server, "instance").await;
        assert_eq!(photos.len(), 2, "{photos:?}");
        assert!(!server.asked_for("titles=File"), "{:?}", server.seen());
    }

    #[tokio::test]
    async fn a_lead_image_hosted_outside_commons_is_not_a_candidate() {
        let server = mock(&[
            ("generator=search", COMMONS),
            (
                "/summary/",
                r#"{"type":"standard","title":"Strength",
                    "originalimage":{"source":"https://upload.wikimedia.org/wikipedia/en/4/44/Fair_use.jpg",
                                     "width":900,"height":900}}"#,
            ),
            ("titles=File", FILE_PAGE),
        ])
        .await;
        let photos = wikimedia(&server, "strength").await;
        assert_eq!(photos.len(), 2, "{photos:?}");
        assert!(!server.asked_for("titles=File"), "{:?}", server.seen());
    }

    #[tokio::test]
    async fn a_lead_image_whose_file_page_states_no_licence_is_dropped() {
        let server = mock(&[
            ("generator=search", COMMONS),
            ("/summary/", SUMMARY),
            (
                "titles=File",
                r#"{"query": {"pages": [
                    {"title": "File:Lake Serene.jpg",
                     "imageinfo": [{"thumburl": "https://upload.wikimedia.org/w/thumb/960px-Lake_Serene.jpg",
                                    "extmetadata": {}}]}
                ]}}"#,
            ),
        ])
        .await;
        let photos = wikimedia(&server, "desire").await;
        assert_eq!(photos.len(), 2, "{photos:?}");
        assert!(photos
            .iter()
            .all(|photo| !photo.source_ref.contains(ARTICLE_LEAD)));
    }

    #[tokio::test]
    async fn the_article_lead_is_not_recorded_twice() {
        // The namespace search already found the very file the article leads
        // with; one candidate, not two.
        let server = mock(&[
            (
                "generator=search",
                r#"{"query": {"pages": [
                    {"title": "File:Lake Serene.jpg",
                     "imageinfo": [{"thumburl": "https://upload.wikimedia.org/w/thumb/960px-Lake_Serene.jpg",
                                    "extmetadata": {"LicenseShortName": {"value": "CC BY-SA 4.0"}}}]}
                ]}}"#,
            ),
            ("/summary/", SUMMARY),
            ("titles=File", FILE_PAGE),
        ])
        .await;
        let photos = wikimedia(&server, "serene").await;
        assert_eq!(photos.len(), 1, "{photos:?}");
        assert_eq!(photos[0].source_ref, "wikimedia:File:Lake Serene.jpg");
    }

    // -- the second passes ---------------------------------------------------

    #[test]
    fn a_source_ref_carries_only_the_notes_it_has() {
        assert_eq!(annotate("openverse:abc", &[None]), "openverse:abc");
        assert_eq!(
            annotate("openverse:abc", &[Some("relaxed-license")]),
            "openverse:abc (relaxed-license)"
        );
        assert_eq!(
            annotate("wikimedia:File:A.jpg", &[Some("article-lead"), None]),
            "wikimedia:File:A.jpg (article-lead)"
        );
        assert_eq!(
            annotate(
                "wikimedia:File:A.jpg",
                &[Some("article-lead"), Some("widened-query")]
            ),
            "wikimedia:File:A.jpg (article-lead, widened-query)"
        );
        // And what it produces is what the scorer reads back.
        assert_eq!(
            ImageStrategy::from_source_ref(Some(&annotate(
                "openverse:abc",
                &[ImageStrategy::RelaxedLicense.note()]
            ))),
            ImageStrategy::RelaxedLicense
        );
    }

    /// The relaxed pass is defined by what it stops asking for.
    #[tokio::test]
    async fn the_relaxed_pass_drops_the_licence_filter() {
        let server = mock(&[("/v1/images", OPENVERSE)]).await;
        let photos = search_as(
            &server,
            ImageSource::Openverse,
            "desire",
            "desire",
            ImageStrategy::RelaxedLicense,
        )
        .await;

        assert!(
            !server.asked_for("license_type"),
            "the filter survived: {:?}",
            server.seen()
        );
        assert!(server.asked_for("q=desire"), "{:?}", server.seen());
        assert_eq!(photos.len(), 1, "{photos:?}");
        assert_eq!(
            photos[0].source_ref,
            "openverse:b806336a-71eb-408f-8ee3-72d27d1d1823 (relaxed-license)"
        );
        // The licence is the one the payload stated, not a claim about what the
        // dropped filter would have allowed.
        assert_eq!(
            photos[0].license.as_deref(),
            Some("CC BY 2.0; by PiktourUK (Openverse)")
        );
        assert_eq!(photos[0].source, ImageSource::Openverse);
    }

    /// A licence the release bundle may not ship still arrives with its terms
    /// on the record, which is the only reason relaxing the filter is safe.
    #[tokio::test]
    async fn a_noncommercial_result_records_the_licence_it_actually_has() {
        let server = mock(&[(
            "/v1/images",
            r#"{"results":[{"id":"nc-1","url":"https://host/pic.jpg",
                "creator":"Ada","license":"by-nc-nd","license_version":"4.0"}]}"#,
        )])
        .await;
        let photos = search_as(
            &server,
            ImageSource::Openverse,
            "manner",
            "manner",
            ImageStrategy::RelaxedLicense,
        )
        .await;
        assert_eq!(
            photos[0].license.as_deref(),
            Some("CC BY-NC-ND 4.0; by Ada (Openverse)")
        );
    }

    #[tokio::test]
    async fn the_strict_and_widened_openverse_passes_keep_the_filter() {
        for strategy in [ImageStrategy::Strict, ImageStrategy::WidenedQuery] {
            let server = mock(&[("/v1/images", OPENVERSE)]).await;
            let photos = search_as(
                &server,
                ImageSource::Openverse,
                "desire",
                "desire wish longing",
                strategy,
            )
            .await;
            assert!(
                server.asked_for("license_type=commercial%2Cmodification")
                    || server.asked_for("license_type=commercial,modification"),
                "{strategy:?} lost the filter: {:?}",
                server.seen()
            );
            let expected = match strategy {
                ImageStrategy::WidenedQuery => {
                    "openverse:b806336a-71eb-408f-8ee3-72d27d1d1823 (widened-query)"
                }
                _ => "openverse:b806336a-71eb-408f-8ee3-72d27d1d1823",
            };
            assert_eq!(photos[0].source_ref, expected);
        }
    }

    /// The widened Wikimedia pass searches the namespace with the new query and
    /// stops there: the article is found by title, and the title has not
    /// changed since the strict pass looked it up.
    #[tokio::test]
    async fn the_widened_wikimedia_pass_never_asks_the_article_again() {
        let server = mock(&[
            ("generator=search", COMMONS),
            ("/summary/", SUMMARY),
            ("titles=File", FILE_PAGE),
        ])
        .await;
        let photos = search_as(
            &server,
            ImageSource::Wikimedia,
            "desire",
            "desire wish longing",
            ImageStrategy::WidenedQuery,
        )
        .await;

        assert_eq!(photos.len(), 2, "{photos:?}");
        assert!(
            !server.asked_for("/summary/"),
            "the article was asked twice: {:?}",
            server.seen()
        );
        assert!(
            server.asked_for("gsrsearch=desire+wish+longing"),
            "{:?}",
            server.seen()
        );
        for photo in &photos {
            assert!(
                photo.source_ref.ends_with("(widened-query)"),
                "{photo:?} does not declare its pass"
            );
        }
    }

    /// A stock library has one pass. Asking it for another is a wiring bug, and
    /// a retry would only ask again.
    #[tokio::test]
    async fn a_keyed_provider_has_no_second_pass() {
        let config = SourcesConfig {
            unsplash_access_key: Some("k".into()),
            ..SourcesConfig::default()
        };
        let client = http::build_client(&config).unwrap();
        for strategy in [ImageStrategy::RelaxedLicense, ImageStrategy::WidenedQuery] {
            let err = search(
                &client,
                &config,
                ImageSource::Unsplash,
                "serene",
                "serene calm",
                strategy,
            )
            .await
            .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Permanent);
        }
    }
}

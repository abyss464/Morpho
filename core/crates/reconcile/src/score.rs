//! Candidate scoring (README Part 3 §"选择语义", rule 1).
//!
//! Scoring inputs are: readability against the live lexicon, with out-of-scope
//! tokens gating the total down as well as shading the component; a length
//! window, source priors, how common a sense is,
//! part-of-speech / primary-sense match, and resolution for images. Everything
//! is a pure function of already-materialized inputs, so a `scorer_ver` bump is
//! the only thing that ever invalidates a score.
//!
//! Scores live in `[0, 1]`. The breakdown is stored verbatim in `score_detail`
//! so the console can explain a choice without re-deriving it.

use serde::Serialize;

use morpho_domain::canon::fold_lemma;
use morpho_domain::types::{DefinitionSource, ExampleSource, ImageSource};
use morpho_domain::version::SCORER_ALGO_VER;

/// Hysteresis margin: an alternative must beat the incumbent by this much
/// before automatic selection switches (README rule 3). Without it, two
/// candidates a thousandth apart would trade the slot on every pass.
pub const HYSTERESIS_DELTA: f64 = 0.05;

/// Marker written into `image_candidates.source_ref` by the relaxed-licence
/// second pass over Openverse.
pub const RELAXED_LICENSE: &str = "relaxed-license";
/// Marker written into `image_candidates.source_ref` by the gloss-widened
/// second pass over a keyless provider.
pub const WIDENED_QUERY: &str = "widened-query";

/// Leading word of the note a scene generation writes: `(scene scene/1)`.
///
/// A scene candidate is an SDXL candidate whose prompt described the word's own
/// slot-1 sentence rather than the bare concept. It is *not* a strategy in the
/// [`ImageStrategy`] sense — it changes nothing about how the candidate scores,
/// because [`score_image`] only ever looks at resolution, primary-sense match
/// and the strategy penalty, none of which the scene note touches. The note
/// exists so the deriving rule can tell whether a word already holds a scene
/// image made under the current template, and so the duplicate-image pressure
/// can leave those candidates alone.
pub const SCENE_NOTE: &str = "scene";

/// How much a second-pass candidate gives up against a first-pass one.
///
/// It has to exceed [`HYSTERESIS_DELTA`], or a second-pass picture sitting in
/// the image slot would be immune to the strict hit that arrives later: the
/// selector only switches past the margin, so a penalty of a hundredth would
/// merely reorder the ranking without ever moving the slot.
///
/// It is subtracted rather than folded into the weighted sum on purpose. Every
/// candidate already in the library came from a strict pass, so a subtraction
/// that is zero for `Strict` leaves all of their scores bit-for-bit identical —
/// which is what makes this change safe without a `scorer_ver` bump and the
/// full rescore that comes with one.
pub const STRATEGY_PENALTY: f64 = 0.10;

/// The penalty is useless at or below the switching margin: see above.
const _: () = assert!(STRATEGY_PENALTY > HYSTERESIS_DELTA);

/// How much a picture gives up for being the one another word already shows.
///
/// Media is content-addressed, so two words that search for the same idea come
/// back with the same `file_hash` — and a question renders the word beside its
/// three fixed distractors, which makes two identical option images an
/// unanswerable card. This is the pressure that pulls the second-best picture
/// into the slot when the best one is spoken for.
///
/// It clears [`HYSTERESIS_DELTA`] for the same reason [`STRATEGY_PENALTY`] does,
/// and here the reason is sharper: two candidates of equal merit, one duplicated
/// and one not, must actually *move* the slot rather than merely reorder behind
/// it. A penalty inside the margin would rank the unique picture first and leave
/// the duplicate sitting in the selection forever.
pub const DUPLICATE_IMAGE_PENALTY: f64 = 0.10;

/// Same reasoning as the strategy penalty, enforced the same way.
const _: () = assert!(DUPLICATE_IMAGE_PENALTY > HYSTERESIS_DELTA);

/// Which search strategy produced an image candidate.
///
/// A word with no candidate after the first pass is searched again on looser
/// terms — Openverse without its licence filter, the keyless providers with a
/// query widened by the gloss's content words. Those hits are real pictures and
/// worth having, and they are still weaker evidence than a hit on the word
/// itself under a licence the bundle may ship: the licence is looser, or the
/// query drifted away from the word. So they are ranked below.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageStrategy {
    /// The first pass: exact terms, publishable licence only.
    #[default]
    Strict,
    /// Openverse again, without `license_type` — every CC licence, NC and ND
    /// included. The licence the result actually carries is recorded verbatim.
    RelaxedLicense,
    /// A keyless provider again, asked for the word plus the content words of
    /// its primary gloss.
    WidenedQuery,
}

impl ImageStrategy {
    /// The note this strategy leaves in `source_ref`, or `None` for the first
    /// pass, which annotates nothing.
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::Strict => None,
            Self::RelaxedLicense => Some(RELAXED_LICENSE),
            Self::WidenedQuery => Some(WIDENED_QUERY),
        }
    }

    /// Read the strategy back off a stored `source_ref`.
    ///
    /// Only the trailing parenthesised note is consulted, never the whole
    /// string: a Commons file may legitimately be *called*
    /// `File:Relaxed-license terms.jpg`, and that is a title, not a provenance
    /// claim.
    pub fn from_source_ref(source_ref: Option<&str>) -> Self {
        let Some(notes) = source_ref.and_then(trailing_note) else {
            return Self::Strict;
        };
        for note in notes.split(',').map(str::trim) {
            if note == RELAXED_LICENSE {
                return Self::RelaxedLicense;
            }
            if note == WIDENED_QUERY {
                return Self::WidenedQuery;
            }
        }
        Self::Strict
    }

    /// Marks deducted from the weighted total.
    pub const fn penalty(self) -> f64 {
        match self {
            Self::Strict => 0.0,
            Self::RelaxedLicense | Self::WidenedQuery => STRATEGY_PENALTY,
        }
    }
}

/// The contents of a trailing `(…)` group, if the string ends in one.
fn trailing_note(source_ref: &str) -> Option<&str> {
    let inner = source_ref.trim_end().strip_suffix(')')?;
    let open = inner.rfind('(')?;
    Some(&inner[open + 1..])
}

/// The scene-prompt version a candidate was generated under, if it is a scene
/// candidate at all.
///
/// Read only from the trailing parenthesised note, for the same reason
/// [`ImageStrategy::from_source_ref`] is: a Commons file may be *called*
/// `File:Scene at dawn.jpg`, and a title is not a provenance claim.
pub fn scene_prompt_ver(source_ref: Option<&str>) -> Option<&str> {
    let notes = source_ref.and_then(trailing_note)?;
    notes.split(',').map(str::trim).find_map(|note| {
        let rest = note.strip_prefix(SCENE_NOTE)?;
        let ver = rest.trim_start();
        // `(scene)` with no version is not a version claim, and `(scenery)`
        // is not a scene note at all.
        (ver.len() < rest.len() && !ver.is_empty()).then_some(ver)
    })
}

/// Was this candidate generated from an example sentence?
pub fn is_scene_image(source_ref: Option<&str>) -> bool {
    scene_prompt_ver(source_ref).is_some()
}

/// How a definition's tokens land against the live lexicon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenCoverage {
    /// Tokens resolving to a base word.
    pub base: usize,
    /// Tokens resolving to a target or auxiliary word.
    pub in_scope: usize,
    /// Tokens resolving to nothing in the lexicon.
    pub out_of_scope: usize,
}

impl TokenCoverage {
    pub fn total(&self) -> usize {
        self.base + self.in_scope + self.out_of_scope
    }

    /// `1.0` when every token is known, falling fast as out-of-scope tokens
    /// appear. A definition the learner cannot read is worthless however
    /// elegant it is, so the penalty is three times the token's weight and the
    /// result is clamped at zero.
    pub fn readability(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let penalty = 3.0 * self.out_of_scope as f64 / total as f64;
        (1.0 - penalty).clamp(0.0, 1.0)
    }
}

/// Triangular preference for a token count inside `[low, high]`.
fn length_window(count: usize, low: usize, high: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    if count >= low && count <= high {
        return 1.0;
    }
    if count < low {
        return count as f64 / low as f64;
    }
    // Beyond the window, decay towards zero over another window's width.
    let overshoot = (count - high) as f64;
    (1.0 - overshoot / (high.max(1) as f64)).clamp(0.0, 1.0)
}

const fn definition_prior(source: DefinitionSource) -> f64 {
    // README: manual > llm_rewrite > freedict > wordnet.
    match source {
        DefinitionSource::Manual => 1.0,
        DefinitionSource::LlmRewrite => 0.85,
        DefinitionSource::Freedict => 0.70,
        DefinitionSource::Wordnet => 0.55,
    }
}

const fn example_prior(source: ExampleSource) -> f64 {
    // Ruling #18: manual > exam_corpus > freedict > tatoeba > llm.
    //
    // An exam sentence was written for exactly this purpose; a dictionary's own
    // usage line was written to illustrate the sense it sits under; a Tatoeba
    // sentence merely contains the word; a generated one merely looks like it
    // does. The ordering follows how much the sentence was chosen *for the
    // word*, which is what the mode-1 card needs.
    match source {
        ExampleSource::Manual => 1.0,
        ExampleSource::ExamCorpus => 0.85,
        ExampleSource::Freedict => 0.72,
        ExampleSource::Tatoeba => 0.64,
        ExampleSource::Llm => 0.55,
    }
}

/// A score plus the breakdown that justifies it.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub score: f64,
    pub detail: ScoreDetail,
}

impl Scored {
    pub fn detail_json(&self) -> String {
        serde_json::to_string(&self.detail).unwrap_or_else(|_| "{}".to_string())
    }

    pub const fn scorer_ver() -> &'static str {
        SCORER_ALGO_VER
    }
}

/// `score_detail` payload. Every field is a `[0, 1]` component except `total`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScoreDetail {
    pub total: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_prior: Option<f64>,
    /// `1.0` for a clean definition, [`SELF_REFERENCE_FACTOR`] for one that
    /// uses its own headword — a multiplier, not a component, so the total is
    /// not the weighted sum of the fields above it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_reference: Option<f64>,
    /// What [`out_of_scope_factor`] returned for this candidate's unreadable
    /// tokens — the second multiplier, alongside `self_reference`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_scope_factor: Option<f64>,
    /// Where this sense sits in its source's list, 1-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sense_rank: Option<usize>,
    /// What that position is worth. See [`sense_rank_prior`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sense_prior: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_match: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_scope_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    /// Marks a second-pass image candidate gave up. Absent — not zero — on a
    /// first-pass candidate, so the breakdown of everything the strict passes
    /// produced is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_penalty: Option<f64>,
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

/// What is left of a definition that uses the word it defines.
///
/// A circular gloss teaches nothing — "resource: to supply with resources" is
/// the card telling the learner to already know the answer — so it is a
/// multiplier rather than a component: whatever else a self-referential
/// candidate has going for it, any clean sibling in the same slot outranks it
/// by a mile. It is not zero on purpose. A word whose every candidate is
/// circular still gets *a* definition, because an imperfect sense beats an
/// empty slot; the penalty only has to guarantee it is the last resort.
pub const SELF_REFERENCE_FACTOR: f64 = 0.15;

/// What is left of a definition that leans on a word outside the lexicon.
///
/// An out-of-scope token is not a blemish on an otherwise fine gloss — it is a
/// hole the learner falls through, and it drags a second word into the
/// dependency closure to boot. Grading it inside `readability` made it
/// *comparable* to the other components: at weight 0.40 one bad token in ten
/// costs about 0.12, which two sense ranks under [`SENSE_WEIGHT`] pay for
/// outright, so `scorer/2` happily traded a clean gloss for a commoner sense
/// that nobody could read and reopened the whole out-of-scope queue.
///
/// So it multiplies, exactly like [`SELF_REFERENCE_FACTOR`], and for the same
/// reason: any clean sibling in the slot wins, and an unreadable candidate only
/// takes a slot no clean candidate can fill. Two factors rather than one so
/// that among candidates that are *all* unreadable, fewer holes still wins.
pub const fn out_of_scope_factor(out_of_scope: usize) -> f64 {
    match out_of_scope {
        0 => 1.0,
        1 => 0.25,
        _ => 0.10,
    }
}

/// How fast a sense's weight falls as it moves down its dictionary's list.
///
/// Both sources order senses by how common they are — WordNet by tagged corpus
/// frequency, the Free Dictionary editorially — so position in the list is the
/// only frequency evidence available in-process. One step costs about a
/// quarter of the component; by sense five it is worth 40% of sense one.
pub const SENSE_RANK_DECAY: f64 = 0.35;

/// Weight of the sense-commonality component in a definition's score.
///
/// It has to clear [`HYSTERESIS_DELTA`] across a couple of ranks or it would
/// reorder the candidate list without ever moving a slot: rank 1 against rank 3
/// is worth `0.25 * (1.0 - 0.588) ≈ 0.10`, twice the margin.
const SENSE_WEIGHT: f64 = 0.25;

/// Two ranks apart must actually move a slot, not merely reorder behind it.
const _: () = assert!(SENSE_WEIGHT * 0.4 > HYSTERESIS_DELTA);

/// How much a sense is worth for sitting at 1-based `rank` in its dictionary.
///
/// `None` — a manual or rewritten candidate, which has no list to sit in — is
/// full marks: somebody chose it deliberately, which is stronger evidence than
/// any position in a list.
pub fn sense_rank_prior(rank: Option<usize>) -> f64 {
    match rank {
        None | Some(0) | Some(1) => 1.0,
        Some(rank) => 1.0 / (1.0 + SENSE_RANK_DECAY * (rank - 1) as f64),
    }
}

/// Whether `text` uses `lemma` itself, in any of the inflections English forms
/// by suffixation.
///
/// Lemmatizing the definition would be the exact answer, and the cached
/// extraction does exactly that — but it is a *separate* artifact that lands
/// after the candidate does, so scoring it means scoring whatever the
/// extraction happened to hold at the time. Reading the text directly makes the
/// check a pure function of the candidate, which is what the `scorer_ver`
/// contract promises. Generating the forms rather than stripping suffixes off
/// the text also avoids the reverse error: "resourceful" is not "resource".
pub fn is_self_referential(lemma: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    inflections(lemma)
        .iter()
        .any(|form| contains_whole_word(&text, form))
}

/// The lemma plus the inflections regular English suffixation produces.
///
/// Irregulars ("go/went", "child/children") are deliberately absent: the point
/// is to catch a gloss that repeats its own headword, and a dictionary that
/// defines "go" as "went somewhere" is not the failure mode this exists for.
/// Guessing wider would start punishing definitions that merely rhyme.
fn inflections(lemma: &str) -> Vec<String> {
    let lemma = fold_lemma(lemma);
    if lemma.is_empty() {
        return Vec::new();
    }
    let mut forms = vec![
        lemma.clone(),
        format!("{lemma}s"),
        format!("{lemma}es"),
        format!("{lemma}ed"),
        format!("{lemma}d"),
        format!("{lemma}ing"),
    ];
    let chars: Vec<char> = lemma.chars().collect();
    let last = chars[chars.len() - 1];
    // "carry" → "carries", "carried".
    if last == 'y' && chars.len() > 1 && !is_vowel(chars[chars.len() - 2]) {
        let stem: String = chars[..chars.len() - 1].iter().collect();
        forms.push(format!("{stem}ies"));
        forms.push(format!("{stem}ied"));
    }
    // "charge" → "charging", "charged" (the bare +d is already above).
    if last == 'e' {
        let stem: String = chars[..chars.len() - 1].iter().collect();
        forms.push(format!("{stem}ing"));
        forms.push(format!("{stem}ed"));
    }
    // "plan" → "planning", "planned": a final consonant after a single vowel
    // after a consonant doubles. 'w', 'x' and 'y' never do.
    if chars.len() >= 3
        && !is_vowel(last)
        && !matches!(last, 'w' | 'x' | 'y')
        && is_vowel(chars[chars.len() - 2])
        && !is_vowel(chars[chars.len() - 3])
    {
        forms.push(format!("{lemma}{last}ing"));
        forms.push(format!("{lemma}{last}ed"));
    }
    forms
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Does `needle` occur in `haystack` bounded by non-word characters?
///
/// Both sides are already lowercase. A "word character" here is anything
/// alphanumeric, so "resourceful" does not contain "resource" and "re-source"
/// does.
fn contains_whole_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let mut from = 0usize;
    while let Some(offset) = haystack[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_word_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        // Advance by one character, not one byte: the haystack is arbitrary
        // UTF-8 and slicing mid-character would panic.
        from = start
            + haystack[start..]
                .chars()
                .next()
                .map_or(needle.len(), char::len_utf8);
        if from >= haystack.len() {
            break;
        }
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b >= 0x80
}

/// What a definition candidate looks like to the scorer.
#[derive(Debug, Clone)]
pub struct DefinitionFacts {
    pub source: DefinitionSource,
    pub coverage: TokenCoverage,
    /// True when the definition uses the very word it defines, in any
    /// inflection. See [`is_self_referential`].
    pub self_referential: bool,
    /// 1-based position of this sense in its source's list for this word and
    /// part of speech, or `None` when there is no list (manual, rewrite).
    pub sense_rank: Option<usize>,
}

/// Score one definition candidate.
///
/// A definition exists to be *read*, so readability leads; how common the sense
/// is comes next, because a rare sense of a common word is a card the learner
/// will never need; the length window and the source prior break the remaining
/// ties. A circular gloss and an unreadable one are both multiplied down rather
/// than docked, so either can only ever win a slot nothing cleaner can fill.
/// `readability` survives as a component to grade token density *among* clean
/// candidates — with no out-of-scope tokens it is simply 1.0, and the ranking
/// falls to the rest.
pub fn score_definition(facts: &DefinitionFacts) -> Scored {
    let readability = facts.coverage.readability();
    // 4–14 tokens: long enough to disambiguate, short enough to hold.
    let length = length_window(facts.coverage.total(), 4, 14);
    let prior = definition_prior(facts.source);
    let sense = sense_rank_prior(facts.sense_rank);
    let self_reference = if facts.self_referential {
        SELF_REFERENCE_FACTOR
    } else {
        1.0
    };
    let out_of_scope = out_of_scope_factor(facts.coverage.out_of_scope);

    let merit = 0.40 * readability + 0.14 * length + 0.21 * prior + SENSE_WEIGHT * sense;
    let total = clamp(merit * self_reference * out_of_scope);
    Scored {
        score: total,
        detail: ScoreDetail {
            total,
            readability: Some(readability),
            length: Some(length),
            source_prior: Some(prior),
            sense_rank: facts.sense_rank,
            sense_prior: Some(sense),
            self_reference: Some(self_reference),
            out_of_scope_factor: Some(out_of_scope),
            out_of_scope_tokens: Some(facts.coverage.out_of_scope),
            token_count: Some(facts.coverage.total()),
            ..ScoreDetail::default()
        },
    }
}

/// What an example candidate looks like to the scorer.
#[derive(Debug, Clone)]
pub struct ExampleFacts {
    pub source: ExampleSource,
    pub coverage: TokenCoverage,
    /// The highlight range resolves to the target word inside the sentence.
    pub highlight_valid: bool,
}

/// Score one example candidate.
///
/// Sentences run longer than definitions, and a broken highlight makes the
/// mode-1 card unusable, so it is scored as a hard component rather than a
/// nudge.
pub fn score_example(facts: &ExampleFacts) -> Scored {
    let readability = facts.coverage.readability();
    let length = length_window(facts.coverage.total(), 6, 22);
    let prior = example_prior(facts.source);
    let highlight = if facts.highlight_valid { 1.0 } else { 0.0 };

    let total = 0.35 * readability + 0.20 * length + 0.20 * prior + 0.25 * highlight;
    Scored {
        score: clamp(total),
        detail: ScoreDetail {
            total: clamp(total),
            readability: Some(readability),
            length: Some(length),
            source_prior: Some(prior),
            highlight: Some(highlight),
            out_of_scope_tokens: Some(facts.coverage.out_of_scope),
            token_count: Some(facts.coverage.total()),
            ..ScoreDetail::default()
        },
    }
}

/// What an image candidate looks like to the scorer.
#[derive(Debug, Clone)]
pub struct ImageFacts {
    pub source: ImageSource,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// The candidate's `pos` hint matches the word's primary sense (or it
    /// carries no hint at all, which is neutral).
    pub pos_matches_primary: bool,
    /// Which pass found it. Read back from `source_ref`.
    pub strategy: ImageStrategy,
}

/// Target render size (README Part 5: WebP 768×576).
pub const TARGET_WIDTH: i64 = 768;
pub const TARGET_HEIGHT: i64 = 576;

/// Weight of the resolution component in an image's score.
///
/// `scorer/4` drops the source prior: a stock photo, a Commons hit and an SDXL
/// generation are judged purely on the quality evidence available for *this*
/// candidate — how well it fills the target box and whether it matches the
/// word's primary sense — never on which provider produced it. The two
/// remaining components are scaled up from their old 0.35/0.20 split
/// (`0.35 + 0.20 = 0.55`) so they still sum to 1.0, in the same proportion they
/// already carried.
pub const IMAGE_RESOLUTION_WEIGHT: f64 = 0.35 / 0.55;
/// Weight of the primary-sense-match component. See [`IMAGE_RESOLUTION_WEIGHT`].
pub const IMAGE_POS_MATCH_WEIGHT: f64 = 0.20 / 0.55;

/// Score one image candidate.
///
/// Origin is deliberately not an input: a manual upload, a keyed-stock hit, a
/// keyless-provider hit and an SDXL generation are scored identically once
/// their resolution and primary-sense match are equal. Quality speaks for
/// itself; where a picture came from does not.
pub fn score_image(facts: &ImageFacts) -> Scored {
    let resolution = match (facts.width, facts.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            // Full marks at or above the target box; a downscale is free, an
            // upscale is visible.
            let ratio = (w as f64 / TARGET_WIDTH as f64).min(h as f64 / TARGET_HEIGHT as f64);
            ratio.clamp(0.0, 1.0)
        }
        // Unknown dimensions are treated as adequate, not as a fault: the
        // encoder already fitted the bytes to the target box.
        _ => 0.8,
    };
    let pos_match = if facts.pos_matches_primary { 1.0 } else { 0.7 };
    let penalty = facts.strategy.penalty();

    let total = IMAGE_RESOLUTION_WEIGHT * resolution + IMAGE_POS_MATCH_WEIGHT * pos_match - penalty;
    Scored {
        score: clamp(total),
        detail: ScoreDetail {
            total: clamp(total),
            resolution: Some(resolution),
            pos_match: Some(pos_match),
            strategy_penalty: (penalty > 0.0).then_some(penalty),
            ..ScoreDetail::default()
        },
    }
}

fn clamp(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

/// The score automatic selection ranks an image candidate by.
///
/// [`score_image`] is a pure function of the candidate and is cached in
/// `auto_score` under a `scorer_ver`; whether a picture is *also* somebody
/// else's is a property of the selection table, which changes every time a slot
/// moves. Folding it into the stored score would make every selection invalidate
/// scores across the whole lexicon and rescore a live database in circles. So it
/// is subtracted here, at ranking time, from a value nothing persists.
///
/// The result is deliberately not clamped into `[0, 1]`: it is a comparison key,
/// and flooring it at zero would let two weak candidates tie where the
/// preference is real.
pub fn image_selection_score(auto_score: f64, duplicate: bool) -> f64 {
    if duplicate {
        auto_score - DUPLICATE_IMAGE_PENALTY
    } else {
        auto_score
    }
}

/// Should automatic selection move a slot from `current` to `challenger`?
///
/// README rule 3: an `auto`, unpinned slot only switches when the challenger
/// beats the incumbent by more than the hysteresis margin. An empty slot takes
/// the best candidate outright (rule 2).
pub fn should_switch(current: Option<f64>, challenger: f64) -> bool {
    match current {
        None => true,
        Some(incumbent) => challenger > incumbent + HYSTERESIS_DELTA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(base: usize, in_scope: usize, oos: usize) -> TokenCoverage {
        TokenCoverage {
            base,
            in_scope,
            out_of_scope: oos,
        }
    }

    #[test]
    fn readability_is_perfect_when_every_token_is_known() {
        assert_eq!(coverage(4, 2, 0).readability(), 1.0);
    }

    #[test]
    fn out_of_scope_tokens_are_punished_hard() {
        let clean = coverage(9, 1, 0).readability();
        let one_bad = coverage(9, 0, 1).readability();
        assert!(one_bad < clean);
        // One bad token in ten already costs 30% of the readability score.
        assert!((one_bad - 0.7).abs() < 1e-9);
        // Four in ten wipes it out entirely rather than going negative.
        assert_eq!(coverage(6, 0, 4).readability(), 0.0);
    }

    #[test]
    fn empty_coverage_scores_zero_rather_than_dividing_by_zero() {
        assert_eq!(coverage(0, 0, 0).readability(), 0.0);
        assert!(coverage(0, 0, 0).total() == 0);
    }

    #[test]
    fn length_window_peaks_inside_the_band() {
        assert_eq!(length_window(8, 4, 14), 1.0);
        assert_eq!(length_window(4, 4, 14), 1.0);
        assert_eq!(length_window(14, 4, 14), 1.0);
        assert!(length_window(2, 4, 14) < 1.0);
        assert!(length_window(20, 4, 14) < 1.0);
        assert_eq!(length_window(0, 4, 14), 0.0);
        assert_eq!(length_window(100, 4, 14), 0.0);
    }

    #[test]
    fn definition_source_priors_follow_the_whitepaper() {
        let facts = |source| DefinitionFacts {
            source,
            coverage: coverage(6, 2, 0),
            self_referential: false,
            sense_rank: Some(1),
        };
        let manual = score_definition(&facts(DefinitionSource::Manual)).score;
        let rewrite = score_definition(&facts(DefinitionSource::LlmRewrite)).score;
        let freedict = score_definition(&facts(DefinitionSource::Freedict)).score;
        let wordnet = score_definition(&facts(DefinitionSource::Wordnet)).score;
        assert!(manual > rewrite && rewrite > freedict && freedict > wordnet);
    }

    #[test]
    fn readability_outweighs_the_source_prior() {
        // A WordNet gloss everyone can read beats an unreadable manual one.
        let readable_wordnet = score_definition(&DefinitionFacts {
            source: DefinitionSource::Wordnet,
            coverage: coverage(8, 2, 0),
            self_referential: false,
            sense_rank: Some(1),
        });
        let unreadable_manual = score_definition(&DefinitionFacts {
            source: DefinitionSource::Manual,
            coverage: coverage(5, 2, 3),
            self_referential: false,
            sense_rank: Some(1),
        });
        assert!(readable_wordnet.score > unreadable_manual.score);
    }

    #[test]
    fn a_self_referential_definition_is_penalized() {
        let base = DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 0),
            self_referential: false,
            sense_rank: Some(1),
        };
        let mut circular = base.clone();
        circular.self_referential = true;
        assert!(score_definition(&base).score > score_definition(&circular).score);
    }

    // -- self-reference, inflection-aware ----------------------------------

    #[test]
    fn a_definition_that_repeats_its_headword_is_caught_in_any_inflection() {
        for (lemma, text) in [
            ("resource", "To supply with resources."),
            ("resource", "A resource of some kind."),
            ("attorney", "To work as a legal attorney."),
            ("charge", "The act of charging something."),
            ("charge", "Having been charged already."),
            ("carry", "One who carries a load."),
            ("carry", "Carried from place to place."),
            ("plan", "Planning done in advance."),
            ("plan", "A thing that was planned."),
            ("study", "Studies of a subject."),
            ("Brick", "TO BUILD WITH BRICKS."),
        ] {
            assert!(
                is_self_referential(lemma, text),
                "{lemma:?} should be found in {text:?}"
            );
        }
    }

    /// The reverse error the generated-forms approach exists to avoid: a longer
    /// word that merely starts with the lemma is a different word.
    #[test]
    fn a_word_that_merely_contains_the_lemma_is_not_self_reference() {
        for (lemma, text) in [
            ("resource", "Full of resourcefulness and wit."),
            ("art", "A part of the whole."),
            ("man", "Able to manage a household."),
            ("cat", "A large catalogue of names."),
            ("plan", "A flat surface; a plane."),
            ("charge", "Someone in a large chariot."),
        ] {
            assert!(
                !is_self_referential(lemma, text),
                "{lemma:?} should not be found in {text:?}"
            );
        }
    }

    #[test]
    fn punctuation_and_hyphens_still_bound_a_whole_word() {
        assert!(is_self_referential("case", "(law) A case, in short."));
        assert!(is_self_referential("source", "To re-source a component."));
        assert!(is_self_referential("bare", "Bare."));
        assert!(!is_self_referential("", "anything at all"));
        assert!(!is_self_referential("bare", ""));
    }

    /// The haystack is arbitrary UTF-8, so the scan must never slice a
    /// character in half.
    #[test]
    fn a_non_ascii_definition_is_scanned_without_panicking() {
        assert!(is_self_referential("cafe", "A café is a cafe of sorts."));
        assert!(!is_self_referential("cafe", "Naïve façade — 咖啡馆."));
    }

    #[test]
    fn a_circular_gloss_loses_to_any_clean_sibling() {
        // The worst clean candidate in a slot still beats the best circular
        // one: that is what makes the factor a veto rather than a nudge.
        let circular_best = score_definition(&DefinitionFacts {
            source: DefinitionSource::Manual,
            coverage: coverage(8, 2, 0),
            self_referential: true,
            sense_rank: Some(1),
        });
        // Worst on every count a clean candidate is allowed to be worst on:
        // the weakest source, a sense far down the list, and a gloss too short
        // for the length window.
        let clean_worst = score_definition(&DefinitionFacts {
            source: DefinitionSource::Wordnet,
            coverage: coverage(2, 0, 0),
            self_referential: false,
            sense_rank: Some(9),
        });
        assert!(
            clean_worst.score > circular_best.score,
            "{clean_worst:?} vs {circular_best:?}"
        );
        assert!(should_switch(Some(circular_best.score), clean_worst.score));
    }

    // -- out-of-scope tokens, as a gate -------------------------------------

    /// The `scorer/2` regression this factor exists for: the sense prior is
    /// worth about 0.10 across a couple of ranks and one bad token in ten only
    /// cost about 0.12 of readability, so a commoner sense the learner cannot
    /// read outranked a clean rarer one — and every such trade reopened the
    /// out-of-scope queue and cascaded through the dependency closure.
    #[test]
    fn a_clean_later_sense_beats_a_common_one_with_an_unreadable_token() {
        let common_but_unreadable = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(8, 1, 1),
            self_referential: false,
            sense_rank: Some(1),
        });
        let clean_third = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(7, 3, 0),
            self_referential: false,
            sense_rank: Some(3),
        });
        assert!(
            clean_third.score > common_but_unreadable.score,
            "{clean_third:?} vs {common_but_unreadable:?}"
        );
        // And by enough to take the slot, not merely to sort above it.
        assert!(should_switch(
            Some(common_but_unreadable.score),
            clean_third.score
        ));
    }

    /// One unreadable token in a long gloss is still a hole. Diluting it across
    /// more tokens is what let the additive form hide it.
    #[test]
    fn a_single_bad_token_cannot_be_diluted_by_a_longer_definition() {
        let clean = score_definition(&DefinitionFacts {
            source: DefinitionSource::Wordnet,
            coverage: coverage(4, 0, 0),
            self_referential: false,
            sense_rank: Some(7),
        });
        for total in [6usize, 10, 14, 20, 40] {
            let long_and_bad = score_definition(&DefinitionFacts {
                source: DefinitionSource::Manual,
                coverage: coverage(total - 1, 0, 1),
                self_referential: false,
                sense_rank: Some(1),
            });
            assert!(
                clean.score > long_and_bad.score,
                "{total} tokens: {clean:?} vs {long_and_bad:?}"
            );
        }
    }

    /// …but it is a gate, not a rejection: a word whose every candidate leans
    /// on an unknown word still gets a definition rather than an empty slot,
    /// and among those the one with fewer holes wins.
    #[test]
    fn an_unreadable_candidate_still_wins_a_slot_nothing_cleaner_can_fill() {
        let one_bad = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(8, 1, 1),
            self_referential: false,
            sense_rank: Some(1),
        });
        let two_bad = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(7, 1, 2),
            self_referential: false,
            sense_rank: Some(1),
        });
        assert!(one_bad.score > 0.0);
        assert!(one_bad.score > two_bad.score);
        assert!(two_bad.score > 0.0);
        assert_eq!(one_bad.detail.out_of_scope_factor, Some(0.25));
        assert_eq!(two_bad.detail.out_of_scope_factor, Some(0.10));
    }

    /// The factor is flat past two: beyond that the candidate is already last
    /// resort and only the other components need to separate them.
    #[test]
    fn the_out_of_scope_factor_is_a_clean_one_and_two_step() {
        assert_eq!(out_of_scope_factor(0), 1.0);
        assert_eq!(out_of_scope_factor(1), 0.25);
        for oos in 2..12 {
            assert_eq!(out_of_scope_factor(oos), 0.10);
        }
    }

    /// A clean candidate is scored exactly as `scorer/2` scored it — the factor
    /// only ever takes marks away from a candidate that has a hole in it.
    #[test]
    fn a_clean_definition_is_untouched_by_the_gate() {
        let scored = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 0),
            self_referential: false,
            sense_rank: Some(2),
        });
        assert_eq!(scored.detail.out_of_scope_factor, Some(1.0));
        let merit = 0.40 * 1.0
            + 0.14 * length_window(8, 4, 14)
            + 0.21 * definition_prior(DefinitionSource::Freedict)
            + SENSE_WEIGHT * sense_rank_prior(Some(2));
        assert!((scored.score - merit).abs() < 1e-12);
    }

    /// …and it is still worth more than nothing, so a word whose every
    /// candidate is circular keeps a definition instead of an empty slot.
    #[test]
    fn a_circular_gloss_is_still_a_last_resort_rather_than_a_rejection() {
        let scored = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 0),
            self_referential: true,
            sense_rank: Some(1),
        });
        assert!(scored.score > 0.0);
        assert_eq!(scored.detail.self_reference, Some(SELF_REFERENCE_FACTOR));
    }

    // -- sense commonality --------------------------------------------------

    #[test]
    fn the_sense_rank_prior_decays_and_never_reaches_zero() {
        assert_eq!(sense_rank_prior(None), 1.0);
        assert_eq!(sense_rank_prior(Some(1)), 1.0);
        // A zero rank is a caller that does not know; treat it as the first.
        assert_eq!(sense_rank_prior(Some(0)), 1.0);
        for rank in 2..12 {
            let here = sense_rank_prior(Some(rank));
            assert!(here < sense_rank_prior(Some(rank - 1)));
            assert!(here > 0.0);
        }
        assert!((sense_rank_prior(Some(2)) - 1.0 / 1.35).abs() < 1e-12);
        assert!((sense_rank_prior(Some(5)) - 1.0 / 2.4).abs() < 1e-12);
    }

    /// "charm": the common noun sense sits first in the dictionary and "to make
    /// music upon" sits well down the verb list. Equal on every other count,
    /// the first sense has to win by more than the switching margin or the
    /// obscure one would keep the slot it already holds.
    #[test]
    fn an_earlier_sense_takes_the_slot_off_a_later_one() {
        let sense = |rank| {
            score_definition(&DefinitionFacts {
                source: DefinitionSource::Freedict,
                coverage: coverage(8, 2, 0),
                self_referential: false,
                sense_rank: Some(rank),
            })
            .score
        };
        assert!(sense(1) > sense(2));
        assert!(should_switch(Some(sense(3)), sense(1)));
        assert!(should_switch(Some(sense(5)), sense(1)));
    }

    /// The prior is evidence, not a veto: a first sense nobody can read still
    /// loses to a readable later one.
    #[test]
    fn readability_still_outweighs_the_sense_prior() {
        let unreadable_first = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(4, 0, 4),
            self_referential: false,
            sense_rank: Some(1),
        });
        let readable_fourth = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 0),
            self_referential: false,
            sense_rank: Some(4),
        });
        assert!(readable_fourth.score > unreadable_first.score);
    }

    #[test]
    fn the_breakdown_names_the_rank_it_used() {
        let scored = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 0),
            self_referential: false,
            sense_rank: Some(3),
        });
        let json: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert_eq!(json["sense_rank"], 3);
        assert!((json["sense_prior"].as_f64().unwrap() - sense_rank_prior(Some(3))).abs() < 1e-12);
        // A candidate with no list to sit in says so by omission.
        let manual = score_definition(&DefinitionFacts {
            source: DefinitionSource::Manual,
            coverage: coverage(6, 2, 0),
            self_referential: false,
            sense_rank: None,
        });
        let json: serde_json::Value = serde_json::from_str(&manual.detail_json()).unwrap();
        assert!(json.get("sense_rank").is_none());
        assert_eq!(json["sense_prior"], 1.0);
    }

    #[test]
    fn a_broken_highlight_sinks_an_example() {
        let good = score_example(&ExampleFacts {
            source: ExampleSource::ExamCorpus,
            coverage: coverage(10, 2, 0),
            highlight_valid: true,
        });
        let broken = score_example(&ExampleFacts {
            source: ExampleSource::ExamCorpus,
            coverage: coverage(10, 2, 0),
            highlight_valid: false,
        });
        assert!(good.score - broken.score >= 0.24);
    }

    /// Ruling #18's example ordering, end to end.
    #[test]
    fn example_source_priors_follow_the_ruling() {
        let facts = |source| ExampleFacts {
            source,
            coverage: coverage(10, 2, 0),
            highlight_valid: true,
        };
        let ranked: Vec<f64> = [
            ExampleSource::Manual,
            ExampleSource::ExamCorpus,
            ExampleSource::Freedict,
            ExampleSource::Tatoeba,
            ExampleSource::Llm,
        ]
        .into_iter()
        .map(|source| score_example(&facts(source)).score)
        .collect();
        assert!(
            ranked.windows(2).all(|pair| pair[0] > pair[1]),
            "manual > exam_corpus > freedict > tatoeba > llm, got {ranked:?}"
        );
    }

    /// `scorer/4`: origin carries no weight at all. Every source ties once
    /// resolution and primary-sense match are equal — manual, keyed stock,
    /// keyless providers and SDXL alike.
    #[test]
    fn image_sources_no_longer_bias_the_score() {
        let facts = |source| ImageFacts {
            source,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        };
        let scores: Vec<f64> = [
            ImageSource::Manual,
            ImageSource::Unsplash,
            ImageSource::Pexels,
            ImageSource::Pixabay,
            ImageSource::Wikimedia,
            ImageSource::Openverse,
            ImageSource::Sdxl,
        ]
        .into_iter()
        .map(|source| score_image(&facts(source)).score)
        .collect();
        assert!(
            scores
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 1e-12),
            "no source should outrank another on origin alone, got {scores:?}"
        );
    }

    /// A generated picture at the same resolution and primary-sense match as a
    /// real photograph now ties it: quality, not provenance, drives the score.
    #[test]
    fn a_generated_image_ties_a_real_photo_at_equal_quality() {
        let commons = score_image(&ImageFacts {
            source: ImageSource::Wikimedia,
            width: Some(768),
            height: Some(576),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        let generated = score_image(&ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(768),
            height: Some(576),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        assert!((commons.score - generated.score).abs() < 1e-12);

        // But a sharper photo still outranks a lower-resolution generation —
        // resolution is real evidence, source is not.
        let sharper_commons = score_image(&ImageFacts {
            source: ImageSource::Wikimedia,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        let soft_generated = score_image(&ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(384),
            height: Some(288),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        assert!(sharper_commons.score > soft_generated.score);
    }

    #[test]
    fn resolution_below_the_target_box_costs_marks() {
        let big = score_image(&ImageFacts {
            source: ImageSource::Pexels,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        let small = score_image(&ImageFacts {
            source: ImageSource::Pexels,
            width: Some(320),
            height: Some(240),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        assert!(big.score > small.score);
        assert_eq!(big.detail.resolution, Some(1.0));
    }

    #[test]
    fn every_score_stays_inside_the_unit_interval() {
        for oos in 0..8 {
            let scored = score_definition(&DefinitionFacts {
                source: DefinitionSource::Manual,
                coverage: coverage(8, 0, oos),
                self_referential: oos % 2 == 0,
                sense_rank: Some(oos),
            });
            assert!((0.0..=1.0).contains(&scored.score), "{scored:?}");
        }
        let weird = score_image(&ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(-5),
            height: Some(0),
            pos_matches_primary: false,
            strategy: ImageStrategy::Strict,
        });
        assert!((0.0..=1.0).contains(&weird.score));
    }

    #[test]
    fn detail_json_round_trips_and_carries_the_total() {
        let scored = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 1),
            self_referential: false,
            sense_rank: Some(2),
        });
        let parsed: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert!((parsed["total"].as_f64().unwrap() - scored.score).abs() < 1e-9);
        assert_eq!(parsed["out_of_scope_tokens"], 1);
        assert!(
            parsed.get("resolution").is_none(),
            "unused fields are omitted"
        );
    }

    // -- second-pass strategies --------------------------------------------

    fn keyless(strategy: ImageStrategy) -> ImageFacts {
        ImageFacts {
            source: ImageSource::Openverse,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy,
        }
    }

    #[test]
    fn a_second_pass_candidate_ranks_below_the_strict_pass() {
        let strict = score_image(&keyless(ImageStrategy::Strict)).score;
        for strategy in [ImageStrategy::RelaxedLicense, ImageStrategy::WidenedQuery] {
            let relaxed = score_image(&keyless(strategy)).score;
            assert!(
                relaxed < strict,
                "{strategy:?} scored {relaxed} vs {strict}"
            );
        }
    }

    /// The point of the penalty: a strict hit arriving later must be able to
    /// take the slot off a second-pass incumbent, which means clearing the
    /// hysteresis margin rather than merely outranking it.
    #[test]
    fn a_later_strict_hit_displaces_a_second_pass_incumbent() {
        let incumbent = score_image(&keyless(ImageStrategy::WidenedQuery)).score;
        let challenger = score_image(&keyless(ImageStrategy::Strict)).score;
        assert!(should_switch(Some(incumbent), challenger));
    }

    /// A strict-pass hit at full resolution and a primary-sense match takes
    /// full marks: `scorer/4` has nothing left to dock once the source prior is
    /// gone and neither of the remaining components is imperfect.
    #[test]
    fn a_strict_candidate_at_full_quality_scores_full_marks() {
        let facts = keyless(ImageStrategy::Strict);
        let scored = score_image(&facts);
        assert!((scored.score - 1.0).abs() < 1e-9, "{scored:?}");
        assert_eq!(scored.detail.strategy_penalty, None);
        let json: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert!(json.get("strategy_penalty").is_none());
    }

    #[test]
    fn a_second_pass_candidate_says_so_in_its_breakdown() {
        let scored = score_image(&keyless(ImageStrategy::RelaxedLicense));
        assert_eq!(scored.detail.strategy_penalty, Some(STRATEGY_PENALTY));
        let json: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert!((json["strategy_penalty"].as_f64().unwrap() - STRATEGY_PENALTY).abs() < 1e-9);
        assert!((json["total"].as_f64().unwrap() - scored.score).abs() < 1e-9);
    }

    #[test]
    fn the_strategy_is_read_back_off_the_source_ref() {
        assert_eq!(
            ImageStrategy::from_source_ref(Some("openverse:abc (relaxed-license)")),
            ImageStrategy::RelaxedLicense
        );
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Lake.jpg (widened-query)")),
            ImageStrategy::WidenedQuery
        );
        // The first pass annotates nothing, and the article-lead note is a
        // first-pass strategy that must not be penalised.
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Lake.jpg")),
            ImageStrategy::Strict
        );
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Lake.jpg (article-lead)")),
            ImageStrategy::Strict
        );
        assert_eq!(ImageStrategy::from_source_ref(None), ImageStrategy::Strict);
        // Composed notes still resolve.
        assert_eq!(
            ImageStrategy::from_source_ref(Some(
                "wikimedia:File:A.jpg (article-lead, widened-query)"
            )),
            ImageStrategy::WidenedQuery
        );
    }

    /// A file whose *title* contains a strategy word is a title, not a claim.
    #[test]
    fn only_the_trailing_note_is_read_as_provenance() {
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Relaxed-license terms.jpg")),
            ImageStrategy::Strict
        );
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Widened-query (diagram).png")),
            ImageStrategy::Strict
        );
    }

    // -- scene candidates ---------------------------------------------------

    fn generated(source_ref: &str) -> ImageFacts {
        ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(768),
            height: Some(576),
            pos_matches_primary: true,
            strategy: ImageStrategy::from_source_ref(Some(source_ref)),
        }
    }

    #[test]
    fn the_scene_version_is_read_back_off_the_source_ref() {
        assert_eq!(
            scene_prompt_ver(Some("sdxl:1234 (scene scene/1)")),
            Some("scene/1")
        );
        assert_eq!(
            scene_prompt_ver(Some("sdxl:1234 (scene scene/2)")),
            Some("scene/2")
        );
        // A bare-concept generation carries a JSON blob and no note at all.
        assert_eq!(
            scene_prompt_ver(Some(r#"{"prompt":"a clear photographic scene","seed":7}"#)),
            None
        );
        assert_eq!(scene_prompt_ver(None), None);
    }

    /// A note has to actually be one. `(scene)` claims no version and
    /// `(scenery)` is a different word.
    #[test]
    fn a_note_that_is_not_a_version_claim_is_not_read_as_one() {
        assert_eq!(scene_prompt_ver(Some("sdxl:1 (scene)")), None);
        assert_eq!(scene_prompt_ver(Some("sdxl:1 (scenery)")), None);
        assert_eq!(scene_prompt_ver(Some("sdxl:1 (scene )")), None);
        // And a title is a title, exactly as it is for the strategies.
        assert_eq!(
            scene_prompt_ver(Some("wikimedia:File:Scene scene/1 at dawn.jpg")),
            None
        );
        assert!(!is_scene_image(Some("wikimedia:File:A scene.jpg")));
        assert!(is_scene_image(Some("sdxl:1 (scene scene/1)")));
    }

    /// The note is bookkeeping, not merit. Scene mode changes what a generated
    /// picture *depicts*; it does not change where a generated picture ranks.
    #[test]
    fn a_scene_candidate_scores_exactly_like_any_other_generated_one() {
        let scene = score_image(&generated("sdxl:1234 (scene scene/1)"));
        let bare = score_image(&generated(r#"{"prompt":"...","seed":1234}"#));
        assert_eq!(scene.score, bare.score);
        assert_eq!(scene.detail.strategy_penalty, None);
        // 7/11 * 1.0 + 4/11 * 1.0 — full resolution, primary-sense match, no
        // strategy penalty, and (`scorer/4`) no source prior to dock it either.
        assert!((scene.score - 1.0).abs() < 1e-9, "{scene:?}");
    }

    /// `scorer/4`: a generated picture ties a library photograph of identical
    /// quality — origin no longer separates them.
    #[test]
    fn a_scene_candidate_ties_a_library_photograph_of_equal_quality() {
        let scene = score_image(&generated("sdxl:1234 (scene scene/1)")).score;
        for source in [
            ImageSource::Manual,
            ImageSource::Unsplash,
            ImageSource::Pexels,
            ImageSource::Pixabay,
            ImageSource::Wikimedia,
            ImageSource::Openverse,
        ] {
            let library_same_quality = score_image(&ImageFacts {
                source,
                width: Some(768),
                height: Some(576),
                pos_matches_primary: true,
                strategy: ImageStrategy::Strict,
            })
            .score;
            assert!(
                (library_same_quality - scene).abs() < 1e-9,
                "{source} scored {library_same_quality} vs {scene}"
            );
        }
    }

    /// Quality is still real evidence, only origin stopped being one: a
    /// full-resolution library photograph still outranks a soft, half-size
    /// generation, and by enough to take the slot back.
    #[test]
    fn a_sharper_library_photograph_still_outranks_a_soft_generation() {
        let soft_scene = score_image(&ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(384),
            height: Some(288),
            pos_matches_primary: true,
            strategy: ImageStrategy::from_source_ref(Some("sdxl:1234 (scene scene/1)")),
        })
        .score;
        for source in [
            ImageSource::Manual,
            ImageSource::Unsplash,
            ImageSource::Pexels,
            ImageSource::Pixabay,
            ImageSource::Wikimedia,
            ImageSource::Openverse,
        ] {
            let sharp_library = score_image(&ImageFacts {
                source,
                width: Some(1600),
                height: Some(1200),
                pos_matches_primary: true,
                strategy: ImageStrategy::Strict,
            })
            .score;
            assert!(
                should_switch(Some(soft_scene), sharp_library),
                "{source} at full resolution could not take the slot off a half-size generation"
            );
        }
    }

    // -- global image uniqueness -------------------------------------------

    #[test]
    fn a_picture_another_word_already_shows_ranks_below_a_fresh_one() {
        let taken = image_selection_score(0.865, true);
        let free = image_selection_score(0.865, false);
        assert!(taken < free);
        assert!((free - taken - DUPLICATE_IMAGE_PENALTY).abs() < 1e-9);
    }

    /// The ordering the whole gate rests on: penalty > margin, so a candidate of
    /// equal merit whose hash is free actually takes the slot instead of merely
    /// ranking above the duplicate that sits in it.
    #[test]
    fn the_duplicate_penalty_clears_the_switching_margin() {
        const { assert!(DUPLICATE_IMAGE_PENALTY > HYSTERESIS_DELTA) };
        let incumbent = image_selection_score(0.865, true);
        let challenger = image_selection_score(0.865, false);
        assert!(should_switch(Some(incumbent), challenger));
        // And one hundredth of a mark would not have: the compile-time
        // assertion above is what keeps that from being tuned into the code.
        assert!(!should_switch(Some(0.865 - 0.01), 0.865));
    }

    /// Nothing moves when no candidate is spoken for — the pool scores exactly
    /// as it did before the penalty existed, bit for bit.
    #[test]
    fn a_pool_with_no_shared_hash_is_scored_exactly_as_before() {
        for raw in [0.0, 0.3, 0.865, 1.0] {
            assert_eq!(image_selection_score(raw, false), raw);
        }
        let incumbent = image_selection_score(0.90, false);
        let challenger = image_selection_score(0.88, false);
        assert!(!should_switch(Some(incumbent), challenger));
    }

    /// Two duplicates are still ranked against each other on merit, which is why
    /// the effective score is a comparison key rather than a clamped `[0, 1]`
    /// score.
    #[test]
    fn two_duplicates_keep_their_relative_order_even_at_the_bottom() {
        let better = image_selection_score(0.06, true);
        let worse = image_selection_score(0.03, true);
        assert!(better > worse);
        assert!(worse < 0.0, "a comparison key may go negative");
    }

    #[test]
    fn a_duplicate_still_loses_to_a_far_better_picture_of_its_own_kind() {
        // Merit is not overruled: a strict, full-resolution, primary-sense-
        // matching hit that happens to be shared still beats a widened keyless
        // one that is neither full resolution nor a primary-sense match, even
        // after the duplicate penalty lands.
        let stock = image_selection_score(
            score_image(&ImageFacts {
                source: ImageSource::Unsplash,
                width: Some(1600),
                height: Some(1200),
                pos_matches_primary: true,
                strategy: ImageStrategy::Strict,
            })
            .score,
            true,
        );
        let widened = image_selection_score(
            score_image(&ImageFacts {
                source: ImageSource::Openverse,
                width: Some(1600),
                height: Some(1200),
                pos_matches_primary: false,
                strategy: ImageStrategy::WidenedQuery,
            })
            .score,
            false,
        );
        assert!(stock > widened);
    }

    #[test]
    fn hysteresis_keeps_a_near_tie_from_oscillating() {
        assert!(should_switch(None, 0.1), "an empty slot takes anything");
        assert!(!should_switch(Some(0.80), 0.82));
        assert!(
            !should_switch(Some(0.80), 0.85),
            "exactly delta is not enough"
        );
        assert!(should_switch(Some(0.80), 0.86));
        assert!(!should_switch(Some(0.80), 0.50));
    }
}

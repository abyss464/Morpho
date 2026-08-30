//! Router assembly for `docs/contracts/admin-api.md`.
//!
//! Every endpoint in the contract is live; nothing answers `501` any more.

pub mod read;
pub mod write;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::AppState;

pub fn api_router(state: AppState) -> Router {
    Router::new()
        // -- Dashboard & observability -----------------------------------
        .route("/dashboard", get(read::dashboard))
        .route("/events", get(read::events))
        .route("/jobs", get(read::jobs))
        .route("/stream", get(read::stream))
        // -- Gallery -------------------------------------------------------
        .route("/gallery", get(read::gallery))
        // -- Words --------------------------------------------------------
        .route("/words", get(read::words).post(write::create_word))
        .route("/words/{id}", get(read::word_detail))
        .route(
            "/words/{id}/gloss",
            post(write::set_gloss).delete(write::clear_gloss),
        )
        // -- Candidates & selections --------------------------------------
        .route("/candidates/definition", post(write::mint_definition))
        .route("/candidates/example", post(write::mint_example))
        .route("/candidates/image", post(write::upload_image))
        // Tag query + bulk lever (#54); literal segments, declared before the
        // `{kind}` wildcard routes below.
        .route("/candidates/by-tag", get(read::candidates_by_tag))
        .route("/candidates/bulk-reject", post(write::bulk_reject_by_tag))
        // Before the `{kind}` wildcard for the same reason the two definition
        // sub-routes are: a literal segment must be declared first.
        .route(
            "/candidates/example/{cand_id}",
            delete(write::purge_example),
        )
        .route(
            "/candidates/{kind}/{cand_id}/reject",
            post(write::reject_candidate),
        )
        .route("/candidates/{kind}/{cand_id}/tags", post(write::assign_tag))
        .route(
            "/candidates/{kind}/{cand_id}/tags/{category}",
            delete(write::unassign_tag),
        )
        // -- Tag vocabulary (#54) -----------------------------------------
        .route("/tags", get(read::list_tags).post(write::create_tag))
        .route(
            "/tags/categories",
            get(read::tag_categories).post(write::create_tag_category),
        )
        .route("/tags/{category}/{value}", delete(write::delete_tag))
        // The two fixed definition sub-routes must be declared before the
        // `{kind}` wildcard, or `definition` would swallow them.
        .route("/selections/definition/primary", post(write::set_primary))
        .route("/selections/definition/enabled", post(write::set_enabled))
        .route(
            "/selections/{kind}/approve",
            post(write::approve).delete(write::unapprove),
        )
        .route("/selections/{kind}", post(write::set_selection))
        // -- Distractors ---------------------------------------------------
        .route(
            "/distractors/rebind-violations",
            post(write::rebind_violations),
        )
        // -- OOV queue -----------------------------------------------------
        .route("/oov", get(read::oov))
        .route("/oov/{lemma}/resolve", post(write::resolve_oov))
        // -- Dead letters --------------------------------------------------
        .route("/dead-letters", get(read::dead_letters))
        .route("/dead-letters/retry", post(write::retry_dead_letter))
        .route("/dead-letters/waive", post(write::waive_dead_letter))
        // -- Plan & releases -----------------------------------------------
        .route("/plan", get(read::plan))
        .route("/plan/groups/{seq}", get(read::plan_group))
        .route("/releases", get(read::releases))
        .route("/releases/preview", get(read::release_preview))
        .route("/releases/export", post(write::export_release))
        .route("/releases/publish", post(write::publish_release))
        // -- Media ----------------------------------------------------------
        .route("/media/{file_hash}", get(read::media))
        .with_state(state)
}

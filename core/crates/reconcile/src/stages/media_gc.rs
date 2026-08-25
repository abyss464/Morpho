//! Media garbage collection — the marking half.
//!
//! README Part 3 §"媒体 GC": the reference set is candidate references ∪ TTS
//! references ∪ **release manifest** references. Anything outside it gets
//! `gc_eligible_at = now + 14 days`; anything that comes back inside has its
//! stamp cleared.
//!
//! This wave stops there. Nothing is deleted and nothing is moved to the trash,
//! by design: the grace period has to actually elapse before a sweep can be
//! trusted, and a release manifest pins its files forever, so the cost of
//! waiting is a few megabytes and the cost of being wrong is a broken APK.

use morpho_domain::event::Actor;
use morpho_domain::time::format_ts;
use morpho_store::error::Result;
use morpho_store::ops::MarkMediaGc;
use morpho_store::{queries, Store, WriteOp, WriteResult};

/// Grace period before an unreferenced file could be collected.
pub const MEDIA_GC_GRACE: chrono::Duration = chrono::Duration::days(14);

/// Mark unreferenced media and un-mark media that came back into use.
/// Returns the number of files newly marked.
pub async fn sweep_media(store: &Store) -> Result<usize> {
    let (registry, referenced) = store
        .read(|conn| {
            Ok((
                queries::media_registry(conn)?,
                queries::referenced_media(conn)?,
            ))
        })
        .await?;

    let referenced_set: std::collections::HashSet<String> = referenced.into_iter().collect();
    let mut unreferenced = Vec::new();
    let mut back_in_use = Vec::new();
    for (file_hash, gc_eligible_at) in registry {
        match (
            referenced_set.contains(&file_hash),
            gc_eligible_at.is_some(),
        ) {
            (false, false) => unreferenced.push(file_hash),
            (true, true) => back_in_use.push(file_hash),
            _ => {}
        }
    }

    if unreferenced.is_empty() && back_in_use.is_empty() {
        return Ok(0);
    }

    let eligible_at = format_ts(chrono::Utc::now() + MEDIA_GC_GRACE);
    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::MarkMediaGc(MarkMediaGc {
                unreferenced,
                eligible_at,
                referenced: back_in_use,
            }),
        )
        .await?;
    Ok(match outcome.result {
        WriteResult::MediaGc { marked, .. } => marked,
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grace_period_matches_the_whitepaper() {
        assert_eq!(MEDIA_GC_GRACE.num_days(), 14);
    }
}

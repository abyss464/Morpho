---
file: admin-ui/src/features/gallery/useImageFlags.ts
---

# Gallery image triage flags

Client-only localStorage hook for the gallery's flag/review/resolve workflow. Tracks two sets of word IDs: `flagged` (needs attention) and `needsRegen` (image should be regenerated). State syncs across browser tabs via the `storage` event. Provides `flag`, `unflag`, `markNeedsRegen`, `clear`, boolean checks, and aggregate counts. No server round-trip -- purely a local worklist marker.

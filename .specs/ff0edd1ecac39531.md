---
file: admin-ui/src/api/endpoints.ts
---

# API endpoint functions

One async function per row of `docs/contracts/admin-api.md`. Every response passes through `mappers.ts` before reaching the UI layer. All word-scoped mutations return the refreshed `WordDetail` because a single write cascades through selection, approval, readiness, and blockers.

## Dashboard & observability
- `getDashboard(signal?)` -- GET /dashboard
- `getEvents(query?, signal?)` -- GET /events, paginated audit log newest-first
- `getJobs(signal?)` -- GET /jobs, live queue snapshot

## Words
- `listWords(query?, signal?)` -- GET /words, paginated with role/ready/blocker/group/q filters
- `listGallery(query?, signal?)` -- GET /gallery, selected-image overview for visual review
- `getWord(wordId, signal?)` -- GET /words/{id}, full detail with slots
- `createWord(body)` -- POST /words

## Candidates & selections
- `mintDefinitionCandidate(body)` / `mintExampleCandidate(body)` / `uploadImageCandidate({word_id, file})` -- POST /candidates/{kind}
- `rejectCandidate(kind, candId)` -- POST /candidates/{kind}/{cand_id}/reject
- `overrideSelection(kind, body)` -- POST /selections/{kind}
- `approveSelection(kind, body)` / `unapproveSelection(kind, body)` -- POST/DELETE /selections/{kind}/approve
- `setPrimarySense(body)` -- POST /selections/definition/primary
- `setSenseEnabled(body)` -- POST /selections/definition/enabled

## OOV queue
- `listOov(query?, signal?)` -- GET /oov, paginated
- `resolveOov(lemma, body)` -- POST /oov/{lemma}/resolve

## Dead letters
- `listDeadLetters(query?, signal?)` -- GET /dead-letters, paginated
- `retryDeadLetter(body)` / `waiveDeadLetter(body)` -- POST /dead-letters/retry|waive

## Plan & releases
- `getPlan(signal?)` -- GET /plan
- `getPlanGroup(seq, signal?)` -- GET /plan/groups/{seq}
- `listReleases(signal?)` -- GET /releases, paginated
- `getReleasePreview(signal?)` -- GET /releases/preview, holdback report
- `exportRelease(body?)` -- POST /releases/export, 409 with gate failures on validation fail

## Constraints
- Do not call `fetch` or `request()` from outside this file; all API access goes through these functions.
- Every mutation returns updated domain objects, not raw wire data.

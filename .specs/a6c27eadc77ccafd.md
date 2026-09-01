---
file: admin-ui/src/features/gallery/GalleryPage.tsx
---

Image gallery page, supports browsing, filtering, and triaging word database images.

## Export

- **GalleryPage** — Receives `GalleryPageProps`. Provides two display modes: grid and review, supporting infinite scroll loading. Can filter by image source and review status, and sort by CLIP similarity. When switching to the flagged or needs_regen triage view, displays the local mark list managed by `useImageFlags` (data stored in localStorage).
- **GalleryPageProps** — Contains the current search status object and search change callback functions. The parent component controls the gallery's filter conditions through these props.
- **GallerySearch** — Search parameter structure, fields include: source (image source), approved (review status), q (keyword), sort (sorting method), view (view mode).
- **GalleryViewMode** — View mode, takes values `"flagged"` or `"needs_regen"`, used for switching triage views.

## Constraints

- Search status is owned by the parent component and passed in via props; do not create an independent search status inside GalleryPage.
- Mark data is stored in localStorage, valid only in the current browser, and not synced to the backend.
- The view field in GallerySearch only accepts values defined by GalleryViewMode; it cannot be arbitrarily extended.

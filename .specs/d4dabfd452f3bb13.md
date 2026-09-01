---
file: app/app/src/main/kotlin/dev/morpho/data/content/EtymologySegments.kt
---

# Word Source Segment Codec

Processes the JSON array in the `words.etymology_segments` column of `release.db` (e.g., `["bene","vol","ent"]`). This is the sole data source for the word source segmentation chip; when the column is null or has an invalid format, the interface degrades to plain-text word source display.

## parse(raw) -> string list
Decodes the column value into a segment list. Returns an empty list when the input is null, blank, malformed, or a non-array — no exceptions are thrown, it simply degrades silently.

## encode(segments) -> JSON string or null
Encodes the segment list back into a JSON array string. Blank segments are automatically filtered; returns null if the list is empty after cleanup. Note that this method does not write to `release.db` — it exists purely to validate round-trip consistency of encoding/decoding in tests.

#!/usr/bin/env bash
# One-shot release pipeline for Morpho.
#
# Automates OPERATIONS.md §5 (cutting a release) and §6 (refreshing the app):
# health check, bulk approve, preview gate, export, copy release.db, sync media,
# patch ReleaseDatabaseTest.kt, specbook refresh, build + test, git commit.
#
# Usage:
#   ops/release.sh           interactive (pauses on specbook interface changes)
#   ops/release.sh --yes     auto-confirm specbook interface changes

set -euo pipefail

REPO="$(git rev-parse --show-toplevel)"
cd "$REPO"

API="http://127.0.0.1:30012"
YES=0
for arg in "$@"; do
    case "$arg" in
        --yes) YES=1 ;;
        *) echo "unknown flag: $arg"; exit 1 ;;
    esac
done

# Format a number with Kotlin-style underscore separators (every 3 digits).
kt_num() {
    python3 -c "print(f'{$1:_}')"
}

# ------------------------------------------------------------------ 1. health check

echo "==> health check"
if ! curl -sf "${API}/api/dashboard" >/dev/null; then
    echo "FAIL: engine not reachable at ${API}/api/dashboard"
    echo "Start it with: docker compose up -d"
    exit 1
fi
echo "    engine healthy"

# ------------------------------------------------------------------ 2. bulk approve

echo "==> bulk approve"
MORPHO_API="$API" python3 ops/bulk_approve.py

# ------------------------------------------------------------------ 3. preview gate

echo "==> preview gate"
PREVIEW_JSON="$(curl -sf "${API}/api/releases/preview")"
GATE_FAILURES="$(echo "$PREVIEW_JSON" | jq -r '.gate_failures // []')"
GATE_COUNT="$(echo "$GATE_FAILURES" | jq 'length')"

if [ "$GATE_COUNT" -ne 0 ]; then
    echo "FAIL: preview gate has $GATE_COUNT failure(s):"
    echo "$GATE_FAILURES" | jq -r '.[] | "  \(.gate): \(.message)"'
    exit 1
fi
echo "    gate clean"

# ------------------------------------------------------------------ 4. export

echo "==> export"
curl -sf -X POST "${API}/api/releases/export" \
    -H 'Content-Type: application/json' \
    -H 'X-Morpho-User: release-script' \
    -d '{}' >/dev/null

# Find the newest export directory.
EXPORT_DIR="$(find data/releases -maxdepth 1 -type d -name 'export-*' | sort | tail -1)"
if [ -z "$EXPORT_DIR" ] || [ ! -f "$EXPORT_DIR/release.db" ]; then
    echo "FAIL: no export directory found after export"
    exit 1
fi
echo "    exported to $EXPORT_DIR"

# ------------------------------------------------------------------ 5. copy release.db

echo "==> copy release.db"
cp "$EXPORT_DIR/release.db" app/app/src/main/assets/release.db
echo "    done"

# ------------------------------------------------------------------ 6. sync media

echo "==> sync media"
MEDIA_TARGET="app/content_media/src/main/assets/content_media"
mkdir -p "$MEDIA_TARGET/img" "$MEDIA_TARGET/audio"

# rsync exported media into target.
rsync -a "$EXPORT_DIR/img/" "$MEDIA_TARGET/img/"
rsync -a "$EXPORT_DIR/audio/" "$MEDIA_TARGET/audio/"

# Build the set of media files that should exist according to manifest (the manifest
# also lists release.db itself, which is copied separately).
MANIFEST="$EXPORT_DIR/manifest.json"
MANIFEST_LIST="$(mktemp)"
jq -r '.files[].path' "$MANIFEST" | grep -E '^(img|audio)/' | sort > "$MANIFEST_LIST"

# Build sorted list of files currently on disk (relative to MEDIA_TARGET).
DISK_LIST="$(mktemp)"
(cd "$MEDIA_TARGET" && find img audio -type f) | sort > "$DISK_LIST"

# Trash files not in manifest (gio trash, never rm).
TRASHED=0
while IFS= read -r rel; do
    gio trash -- "$MEDIA_TARGET/$rel"
    TRASHED=$((TRASHED + 1))
done < <(comm -23 "$DISK_LIST" "$MANIFEST_LIST")

# Verify: every manifest entry exists on disk.
MISSING=0
while IFS= read -r path; do
    if [ ! -f "$MEDIA_TARGET/$path" ]; then
        echo "FAIL: missing media file: $path"
        MISSING=$((MISSING + 1))
    fi
done < "$MANIFEST_LIST"

# Verify: no extra files remain after trash.
DISK_AFTER="$(mktemp)"
(cd "$MEDIA_TARGET" && find img audio -type f) | sort > "$DISK_AFTER"
EXTRA="$(comm -23 "$DISK_AFTER" "$MANIFEST_LIST" | wc -l)"

rm -f "$MANIFEST_LIST" "$DISK_LIST" "$DISK_AFTER"

if [ "$MISSING" -ne 0 ] || [ "$EXTRA" -ne 0 ]; then
    echo "FAIL: media sync verification failed (missing=$MISSING extra=$EXTRA)"
    exit 1
fi
echo "    synced ($TRASHED trashed)"

# ------------------------------------------------------------------ 7. patch ReleaseDatabaseTest.kt

echo "==> patch ReleaseDatabaseTest.kt"
RELEASE_DB="$EXPORT_DIR/release.db"
TEST_FILE="app/app/src/test/kotlin/dev/morpho/data/db/ReleaseDatabaseTest.kt"

CONTENT_VERSION="$(sqlite3 "$RELEASE_DB" "SELECT value FROM meta WHERE key='content_version'")"
WORDS="$(sqlite3 "$RELEASE_DB" "SELECT COUNT(*) FROM words")"
SENSES="$(sqlite3 "$RELEASE_DB" "SELECT COUNT(*) FROM senses")"
EXAMPLES="$(sqlite3 "$RELEASE_DB" "SELECT COUNT(*) FROM examples")"
DISTRACTORS="$(sqlite3 "$RELEASE_DB" "SELECT COUNT(*) FROM distractors")"
GLOSS_ANCHORS="$(sqlite3 "$RELEASE_DB" "SELECT COUNT(*) FROM gloss_anchors")"
GROUPS="$(sqlite3 "$RELEASE_DB" "SELECT COUNT(*) FROM groups")"

KT_WORDS="$(kt_num "$WORDS")"
KT_SENSES="$(kt_num "$SENSES")"
KT_EXAMPLES="$(kt_num "$EXAMPLES")"
KT_DISTRACTORS="$(kt_num "$DISTRACTORS")"
KT_GLOSS_ANCHORS="$(kt_num "$GLOSS_ANCHORS")"
KT_GROUPS="$(kt_num "$GROUPS")"

# Patch content_version string.
sed -i -E "s/\"[0-9]{4}\.[0-9]{2}\.[0-9]{2}\+[0-9a-f]+\"/\"${CONTENT_VERSION}\"/" "$TEST_FILE"

# Patch count assertions: assertEquals(N_NNNL, ... db.xxxQueries
sed -i -E "s/assertEquals\([0-9_]+L,(\s*db\.wordsQueries)/assertEquals(${KT_WORDS}L,\1/" "$TEST_FILE"
sed -i -E "s/assertEquals\([0-9_]+L,(\s*db\.sensesQueries)/assertEquals(${KT_SENSES}L,\1/" "$TEST_FILE"
sed -i -E "s/assertEquals\([0-9_]+L,(\s*db\.examplesQueries)/assertEquals(${KT_EXAMPLES}L,\1/" "$TEST_FILE"
sed -i -E "s/assertEquals\([0-9_]+L,(\s*db\.distractorsQueries)/assertEquals(${KT_DISTRACTORS}L,\1/" "$TEST_FILE"
sed -i -E "s/assertEquals\([0-9_]+L,(\s*db\.glossAnchorsQueries)/assertEquals(${KT_GLOSS_ANCHORS}L,\1/" "$TEST_FILE"
sed -i -E "s/assertEquals\([0-9_]+L,(\s*db\.groupsQueries)/assertEquals(${KT_GROUPS}L,\1/" "$TEST_FILE"

# Patch learning_order density: assertEquals(N_NNN, plan.size) and (1..N_NNN).toList()
sed -i -E "s/assertEquals\([0-9_]+, plan\.size\)/assertEquals(${KT_WORDS}, plan.size)/" "$TEST_FILE"
sed -i -E "s/\(1\.\.[0-9_]+\)\.toList\(\)/(1..${KT_WORDS}).toList()/" "$TEST_FILE"

# Patch gloss_anchors index.size assertion: assertEquals(NNN, index.size)
sed -i -E "s/assertEquals\([0-9_]+, index\.size\)/assertEquals(${KT_GLOSS_ANCHORS}, index.size)/" "$TEST_FILE"

# Patch highlight check count: assertEquals(N_NNN, checked)
sed -i -E "s/assertEquals\([0-9_]+, checked\)/assertEquals(${KT_EXAMPLES}, checked)/" "$TEST_FILE"

echo "    version=$CONTENT_VERSION words=$WORDS senses=$SENSES examples=$EXAMPLES"
echo "    distractors=$DISTRACTORS gloss_anchors=$GLOSS_ANCHORS groups=$GROUPS"

# ------------------------------------------------------------------ 8. specbook refresh

echo "==> specbook refresh"
python3 ops/specbook.py refresh --all

SPECBOOK_UNTRACKED="$(git ls-files --others --exclude-standard .specs/ docs/specbook/ | head -1)"
if git diff --quiet -- .specs/ docs/specbook/ SPECBOOK.md && [ -z "$SPECBOOK_UNTRACKED" ]; then
    echo "    no specbook changes"
else
    # Check if any diff is more than just hash updates.
    SPECBOOK_DIFF="$(git diff -- docs/specbook/ SPECBOOK.md .specs/)"
    # Content lines (not file headers or empty): anything starting with +/- that
    # isn't a diff header (+++/---) or a blank line.
    CONTENT_LINES="$(echo "$SPECBOOK_DIFF" \
        | grep '^[+-]' \
        | grep -v '^[+-][+-][+-]' \
        | grep -v '^[+-]$' \
        || true)"

    # Hash-only changes look like: lines containing only a 16-char hex filename,
    # or "file: ..." frontmatter, or "N specs" count lines, or spec-index entries.
    NONHASH="$(echo "$CONTENT_LINES" \
        | grep -vE '^[+-](file:|---$)' \
        | grep -vE '^[+-]\s*$' \
        | grep -vE '^[+-]- \[' \
        | grep -vE '^[+-][0-9]+ specs' \
        | grep -vE '^[+-]\S+\.md' \
        || true)"

    if [ -n "$NONHASH" ] && [ "$YES" -eq 0 ]; then
        echo "    specbook has interface changes:"
        git diff --stat -- docs/specbook/ SPECBOOK.md .specs/
        echo ""
        git diff -- docs/specbook/ SPECBOOK.md .specs/
        echo ""
        read -rp "    Continue? [y/N] " REPLY
        if [[ ! "$REPLY" =~ ^[Yy]$ ]]; then
            echo "    Aborted by user."
            exit 1
        fi
    else
        echo "    specbook updated (hash-only changes)"
    fi
fi

# ------------------------------------------------------------------ 9. build + test

echo "==> build + test"
(cd app && ./gradlew :domain:test :app:testFatApkDebugUnitTest \
    :app:assembleFatApkDebug :app:assembleFatApkRelease)

# ------------------------------------------------------------------ 10. git commit

echo "==> git commit"
COMMIT_MSG="ops: cut release ${CONTENT_VERSION}"

# Stage the files we changed.
git add app/app/src/main/assets/release.db
git add "$TEST_FILE"
# Stage specbook changes if any (includes untracked new specs).
git add .specs/ docs/specbook/ SPECBOOK.md

git commit -m "$COMMIT_MSG"

# ------------------------------------------------------------------ 11. summary

MEDIA_COUNT="$(jq '.media_count' "$MANIFEST")"
APK="app/app/build/outputs/apk/fatApk/debug/app-fatApk-debug.apk"

echo ""
echo "=== release complete ==="
echo "  version:     $CONTENT_VERSION"
echo "  words:       $WORDS"
echo "  media files: $MEDIA_COUNT"
echo "  APK:         $APK"
if [ -f "$APK" ]; then
    APK_SIZE="$(du -h "$APK" | cut -f1)"
    echo "  APK size:    $APK_SIZE"
fi

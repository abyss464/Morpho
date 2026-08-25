-- ============================================================================
-- Morpho user database contract (user.db — read/write, app private storage)
-- Owned by: app/. Progress is per-word ONLY: never store group_id or
-- learning_order here (groups are ephemeral presentation, re-cut per release).
-- Progress rows are NEVER deleted; absent words simply drop out via joins
-- against release.db. Included in Android Auto Backup (checkpoint WAL first).
-- ============================================================================

CREATE TABLE learning_progress (
    word_id       INTEGER PRIMARY KEY,
    current_mode  INTEGER NOT NULL DEFAULT 1 CHECK (current_mode IN (1,2,3)),
    rounds_passed INTEGER NOT NULL DEFAULT 0,
    status        TEXT NOT NULL DEFAULT 'learning' CHECK (status IN ('learning','learned'))
);

CREATE TABLE fsrs_cards (
    word_id        INTEGER PRIMARY KEY,
    due            TEXT NOT NULL,
    stability      REAL NOT NULL,
    difficulty     REAL NOT NULL,
    elapsed_days   INTEGER NOT NULL DEFAULT 0,
    scheduled_days INTEGER NOT NULL DEFAULT 0,
    reps           INTEGER NOT NULL DEFAULT 0,
    lapses         INTEGER NOT NULL DEFAULT 0,
    state          INTEGER NOT NULL DEFAULT 0,   -- FSRS v5 state enum
    last_review    TEXT
);

CREATE TABLE daily_stats (
    date         TEXT PRIMARY KEY,   -- YYYY-MM-DD local
    new_learned  INTEGER NOT NULL DEFAULT 0,
    reviewed     INTEGER NOT NULL DEFAULT 0,
    correct_rate REAL
);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,   -- content_version, schema_ver, daily_goal
    value TEXT NOT NULL
);

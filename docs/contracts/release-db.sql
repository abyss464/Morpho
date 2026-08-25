-- ============================================================================
-- Morpho release database contract (release.db — read-only, shipped in APK)
-- Produced by: morphod export. Consumed by: app/ (SQLDelight .sq files must
-- mirror this DDL exactly).
--
-- Contains ONLY words that passed readiness gating AND the dependency-closed
-- cut, ordered by the frozen plan. Base words, candidates, selection metadata,
-- hashes-as-columns, jobs, events are all stripped.
-- Media referenced by content-addressed filename: img/{hash}.webp, audio/{hash}.ogg
-- Export determinism: page_size=4096, journal_mode=DELETE, inserts ordered by
-- primary key, VACUUM at end, no timestamps in rows.
-- ============================================================================

CREATE TABLE words (
    word_id         INTEGER PRIMARY KEY,   -- stable across releases; user progress keys on it
    word            TEXT NOT NULL,
    phonetic        TEXT,
    frequency_rank  INTEGER,
    role            TEXT NOT NULL CHECK (role IN ('target','auxiliary')),
    group_id        INTEGER NOT NULL REFERENCES groups(group_id),
    learning_order  INTEGER NOT NULL,
    etymology       TEXT,
    image_file      TEXT NOT NULL,         -- img/{hash}.webp
    word_audio_file TEXT NOT NULL          -- audio/{hash}.ogg
);
CREATE INDEX ix_words_order ON words(learning_order);
CREATE INDEX ix_words_group ON words(group_id);

CREATE TABLE senses (
    sense_id       INTEGER PRIMARY KEY,
    word_id        INTEGER NOT NULL REFERENCES words(word_id),
    pos            TEXT NOT NULL,
    definition     TEXT NOT NULL,
    is_primary     INTEGER NOT NULL,       -- exactly one per word; drives all quiz options
    def_audio_file TEXT NOT NULL
);
CREATE INDEX ix_senses_word ON senses(word_id);

CREATE TABLE examples (
    example_id    INTEGER PRIMARY KEY,
    word_id       INTEGER NOT NULL REFERENCES words(word_id),
    display_order INTEGER NOT NULL,        -- 1 = mode-1 sentence; 2-3 detail page
    sentence      TEXT NOT NULL,
    hl_start      INTEGER NOT NULL,        -- UTF-8 byte offsets
    hl_end        INTEGER NOT NULL,
    ex_audio_file TEXT NOT NULL
);
CREATE INDEX ix_examples_word ON examples(word_id);

CREATE TABLE groups (
    group_id    INTEGER PRIMARY KEY,
    group_order INTEGER NOT NULL,
    group_type  TEXT NOT NULL CHECK (group_type IN ('scc','root','semantic','fill'))
);

CREATE TABLE distractors (
    word_id            INTEGER NOT NULL REFERENCES words(word_id),
    rank               INTEGER NOT NULL CHECK (rank IN (1,2,3)),
    distractor_word_id INTEGER NOT NULL REFERENCES words(word_id),
    PRIMARY KEY (word_id, rank)
);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,   -- content_version, plan_id, exported_at, schema_ver
    value TEXT NOT NULL
);

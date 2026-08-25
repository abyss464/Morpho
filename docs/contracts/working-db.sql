-- ============================================================================
-- Morpho working database contract (data/working.db)
-- Owned by: core/ (morphod). admin-ui and adapters never touch this file
-- directly; they go through the admin API / adapter protocol.
-- Design rationale: README Part 3. This file is the normative DDL.
--
-- Runtime PRAGMAs (set by morphod, not stored here):
--   journal_mode=WAL, synchronous=NORMAL, foreign_keys=ON, busy_timeout=5000
--
-- Conventions:
--   * All hashes are lowercase-hex blake3 over canonicalized input
--     (NFC, trimmed, internal whitespace collapsed, case preserved).
--   * All timestamps are UTC ISO-8601 TEXT: strftime('%Y-%m-%dT%H:%M:%fZ','now').
--   * Every hash mixes in the producing code's algo_version.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- Core lexicon
-- ----------------------------------------------------------------------------
CREATE TABLE words (
    word_id          INTEGER PRIMARY KEY,
    lemma            TEXT NOT NULL UNIQUE COLLATE NOCASE,
    role             TEXT NOT NULL CHECK (role IN ('target','base','auxiliary')),
    aux_status       TEXT CHECK (aux_status IN ('active','retired')),
    phonetic         TEXT,
    frequency_rank   INTEGER,
    etymology        TEXT,
    etymology_source TEXT CHECK (etymology_source IN ('wiktionary','morfessor','manual')),
    -- Derived caches, reconciler-owned (recomputed inline every pass):
    ready            INTEGER NOT NULL DEFAULT 0,
    blockers         TEXT NOT NULL DEFAULT '[]',   -- JSON array of blocker codes
    created_by       TEXT NOT NULL DEFAULT 'import'
                     CHECK (created_by IN ('import','promotion','manual')),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK ((role = 'auxiliary') = (aux_status IS NOT NULL))
);

CREATE VIEW active_words AS
SELECT * FROM words
WHERE role = 'target' OR (role = 'auxiliary' AND aux_status = 'active');

-- ----------------------------------------------------------------------------
-- Candidates & selections: definitions
-- ----------------------------------------------------------------------------
CREATE TABLE definition_candidates (
    def_cand_id    INTEGER PRIMARY KEY,
    word_id        INTEGER NOT NULL REFERENCES words(word_id),
    pos            TEXT NOT NULL,             -- noun/verb/adj/adv/prep/conj/interj/phrase
    text           TEXT NOT NULL,             -- IMMUTABLE once inserted
    text_hash      TEXT NOT NULL,
    source         TEXT NOT NULL CHECK (source IN ('freedict','wordnet','llm_rewrite','manual')),
    source_ref     TEXT,
    parent_cand_id INTEGER REFERENCES definition_candidates(def_cand_id),
    status         TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','rejected')),
    auto_score     REAL,
    score_detail   TEXT,                      -- JSON breakdown
    scorer_ver     TEXT,
    created_by     TEXT NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (word_id, pos, text_hash)
);
CREATE INDEX ix_defcand_word ON definition_candidates(word_id);

CREATE TABLE definition_selections (
    word_id        INTEGER NOT NULL REFERENCES words(word_id),
    pos            TEXT NOT NULL,
    def_cand_id    INTEGER NOT NULL REFERENCES definition_candidates(def_cand_id),
    is_primary     INTEGER NOT NULL DEFAULT 0,
    enabled        INTEGER NOT NULL DEFAULT 1,
    selected_by    TEXT NOT NULL CHECK (selected_by IN ('auto','human')),
    pinned         INTEGER NOT NULL DEFAULT 0,
    approved       INTEGER NOT NULL DEFAULT 0,
    approved_hash  TEXT,
    approved_by    TEXT,
    approved_at    TEXT,
    selection_rev  INTEGER NOT NULL DEFAULT 1,
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (word_id, pos)
);
CREATE UNIQUE INDEX ux_defsel_primary ON definition_selections(word_id) WHERE is_primary = 1;

-- ----------------------------------------------------------------------------
-- Candidates & selections: examples
-- ----------------------------------------------------------------------------
CREATE TABLE example_candidates (
    ex_cand_id   INTEGER PRIMARY KEY,
    word_id      INTEGER NOT NULL REFERENCES words(word_id),
    text         TEXT NOT NULL,               -- IMMUTABLE
    text_hash    TEXT NOT NULL,
    hl_start     INTEGER NOT NULL,            -- byte offsets into text (UTF-8)
    hl_end       INTEGER NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('exam_corpus','llm','manual')),
    source_ref   TEXT,
    status       TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','rejected')),
    auto_score   REAL, score_detail TEXT, scorer_ver TEXT,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (word_id, text_hash)
);
CREATE INDEX ix_excand_word ON example_candidates(word_id);

CREATE TABLE example_selections (
    word_id       INTEGER NOT NULL REFERENCES words(word_id),
    slot          INTEGER NOT NULL CHECK (slot BETWEEN 1 AND 3),  -- slot 1 = mode-1 sentence
    ex_cand_id    INTEGER NOT NULL REFERENCES example_candidates(ex_cand_id),
    selected_by   TEXT NOT NULL CHECK (selected_by IN ('auto','human')),
    pinned        INTEGER NOT NULL DEFAULT 0,
    approved      INTEGER NOT NULL DEFAULT 0,
    approved_hash TEXT, approved_by TEXT, approved_at TEXT,
    selection_rev INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (word_id, slot),
    UNIQUE (word_id, ex_cand_id)
);

-- ----------------------------------------------------------------------------
-- Candidates & selections: images
-- ----------------------------------------------------------------------------
CREATE TABLE image_candidates (
    img_cand_id  INTEGER PRIMARY KEY,
    word_id      INTEGER NOT NULL REFERENCES words(word_id),
    pos          TEXT,                        -- optional sense hint
    file_hash    TEXT NOT NULL REFERENCES media_files(file_hash),
    width        INTEGER, height INTEGER,
    source       TEXT NOT NULL CHECK (source IN ('unsplash','pexels','pixabay','sdxl','manual')),
    source_ref   TEXT,                        -- photo id / {prompt,seed,model} JSON / note
    license      TEXT,
    query_used   TEXT,
    status       TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','rejected')),
    auto_score   REAL, score_detail TEXT, scorer_ver TEXT,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (word_id, file_hash)
);
CREATE INDEX ix_imgcand_word ON image_candidates(word_id);

CREATE TABLE image_selections (
    word_id       INTEGER PRIMARY KEY REFERENCES words(word_id),  -- exactly one live image
    img_cand_id   INTEGER NOT NULL REFERENCES image_candidates(img_cand_id),
    selected_by   TEXT NOT NULL CHECK (selected_by IN ('auto','human')),
    pinned        INTEGER NOT NULL DEFAULT 0,
    approved      INTEGER NOT NULL DEFAULT 0,
    approved_hash TEXT, approved_by TEXT, approved_at TEXT,
    selection_rev INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ----------------------------------------------------------------------------
-- Media registry (content-addressed store: data/media/{hash[:2]}/{hash}.{webp|ogg})
-- ----------------------------------------------------------------------------
CREATE TABLE media_files (
    file_hash      TEXT PRIMARY KEY,
    kind           TEXT NOT NULL CHECK (kind IN ('image','audio')),
    rel_path       TEXT NOT NULL,
    bytes          INTEGER NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    gc_eligible_at TEXT                       -- NULL while referenced
);

-- ----------------------------------------------------------------------------
-- Derived: tokenization (cached per immutable candidate) + classification views
-- ----------------------------------------------------------------------------
CREATE TABLE def_extractions (
    def_cand_id    INTEGER PRIMARY KEY REFERENCES definition_candidates(def_cand_id),
    input_hash     TEXT NOT NULL,             -- blake3(text_hash || tokenizer_ver || lemmatizer_ver)
    tokenizer_ver  TEXT NOT NULL,
    lemmatizer_ver TEXT NOT NULL,
    extracted_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE def_tokens (
    def_cand_id  INTEGER NOT NULL REFERENCES definition_candidates(def_cand_id),
    position     INTEGER NOT NULL,
    surface      TEXT NOT NULL,
    lemma        TEXT NOT NULL COLLATE NOCASE,
    PRIMARY KEY (def_cand_id, position)
);
CREATE INDEX ix_deftok_lemma ON def_tokens(lemma);

-- Dependency edges of SELECTED definitions only. Pure view: never stale.
CREATE VIEW def_dependencies AS
SELECT DISTINCT ds.word_id, ds.pos, ds.def_cand_id, w.word_id AS depends_on_word_id
FROM definition_selections ds
JOIN def_tokens t ON t.def_cand_id = ds.def_cand_id
JOIN words w ON w.lemma = t.lemma AND w.role IN ('target','auxiliary')
WHERE ds.enabled = 1 AND w.word_id <> ds.word_id;

-- Out-of-scope tokens in currently selected definitions. Pure view.
CREATE VIEW oos_occurrences AS
SELECT t.lemma AS oos_lemma, ds.word_id, ds.def_cand_id, COUNT(*) AS hits
FROM definition_selections ds
JOIN def_tokens t ON t.def_cand_id = ds.def_cand_id
LEFT JOIN words w ON w.lemma = t.lemma
WHERE ds.enabled = 1 AND w.word_id IS NULL
GROUP BY t.lemma, ds.word_id, ds.def_cand_id;

-- Human-facing queue; reconciler syncs it against oos_occurrences every cycle.
CREATE TABLE oos_queue (
    oos_lemma  TEXT PRIMARY KEY COLLATE NOCASE,
    status     TEXT NOT NULL DEFAULT 'open'
               CHECK (status IN ('open','resolved_rewrite','resolved_promote','auto_closed')),
    first_seen TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    resolved_by TEXT, resolved_at TEXT, notes TEXT
);

-- ----------------------------------------------------------------------------
-- Derived: TTS (content-addressed by what was synthesized)
-- ----------------------------------------------------------------------------
CREATE TABLE tts_assets (
    tts_id      INTEGER PRIMARY KEY,
    input_hash  TEXT NOT NULL UNIQUE,  -- blake3(canonical(text)||voice||engine||engine_ver||params)
    text        TEXT NOT NULL,
    text_hash   TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('word','definition','example')),
    voice       TEXT NOT NULL,
    engine      TEXT NOT NULL,
    engine_ver  TEXT NOT NULL,
    params_json TEXT NOT NULL,
    file_hash   TEXT REFERENCES media_files(file_hash),
    duration_ms INTEGER,
    status      TEXT NOT NULL CHECK (status IN ('ready','failed')),
    built_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX ix_tts_text ON tts_assets(text_hash);

-- Desired TTS texts (engine combines rows with current voice/params config).
CREATE VIEW tts_desired AS
SELECT 'word' AS kind, w.lemma AS text FROM active_words w
UNION
SELECT 'definition', dc.text
FROM definition_selections ds
JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
JOIN active_words w ON w.word_id = ds.word_id
WHERE ds.enabled = 1
UNION
SELECT 'example', ec.text
FROM example_selections es
JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
JOIN active_words w ON w.word_id = es.word_id;

-- ----------------------------------------------------------------------------
-- Derived: learning plan (versioned global artifact)
-- ----------------------------------------------------------------------------
CREATE TABLE plan_artifacts (
    plan_id     INTEGER PRIMARY KEY,
    input_hash  TEXT NOT NULL,
    algo_ver    TEXT NOT NULL,
    params_json TEXT NOT NULL,
    is_current  INTEGER NOT NULL DEFAULT 0,
    built_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    stats_json  TEXT
);
CREATE UNIQUE INDEX ux_plan_current ON plan_artifacts(is_current) WHERE is_current = 1;

CREATE TABLE plan_groups (
    plan_id    INTEGER NOT NULL REFERENCES plan_artifacts(plan_id),
    group_seq  INTEGER NOT NULL,
    group_type TEXT NOT NULL CHECK (group_type IN ('scc','root','semantic','fill')),
    PRIMARY KEY (plan_id, group_seq)
);

CREATE TABLE plan_words (
    plan_id        INTEGER NOT NULL REFERENCES plan_artifacts(plan_id),
    word_id        INTEGER NOT NULL REFERENCES words(word_id),
    learning_order INTEGER NOT NULL,
    group_seq      INTEGER NOT NULL,
    PRIMARY KEY (plan_id, word_id)
);

-- ----------------------------------------------------------------------------
-- Distractors: bind once, never recompute (product rule)
-- ----------------------------------------------------------------------------
CREATE TABLE distractors (
    word_id            INTEGER NOT NULL REFERENCES words(word_id),
    rank               INTEGER NOT NULL CHECK (rank IN (1,2,3)),
    distractor_word_id INTEGER NOT NULL REFERENCES words(word_id),
    algo_ver           TEXT NOT NULL,
    bound_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    bound_by           TEXT NOT NULL DEFAULT 'auto',
    PRIMARY KEY (word_id, rank),
    UNIQUE (word_id, distractor_word_id),
    CHECK (word_id <> distractor_word_id)
);
CREATE INDEX ix_distractor_ref ON distractors(distractor_word_id);

-- Auxiliary liveness (pure view; reconciler flips aux_status from it)
CREATE VIEW aux_liveness AS
SELECT w.word_id,
       EXISTS (SELECT 1 FROM def_dependencies d WHERE d.depends_on_word_id = w.word_id)
    OR EXISTS (SELECT 1 FROM distractors x WHERE x.distractor_word_id = w.word_id)
       AS is_live
FROM words w WHERE w.role = 'auxiliary';

-- ----------------------------------------------------------------------------
-- Orchestration state (queue itself is derived & in-memory; see README Part 4)
-- ----------------------------------------------------------------------------
CREATE TABLE job_state (
    kind          TEXT NOT NULL,
    subject_type  TEXT NOT NULL,   -- word | def_candidate | tts_input | global
    subject_id    TEXT NOT NULL,
    rate_key      TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('backoff','dead','waived')),
    attempts      INTEGER NOT NULL DEFAULT 0,
    next_retry_at TEXT,
    last_error    TEXT,
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (kind, subject_type, subject_id)
);

CREATE TABLE source_fetch (
    kind         TEXT NOT NULL,    -- definitions | examples | etymology | images
    word_id      INTEGER NOT NULL REFERENCES words(word_id),
    source       TEXT NOT NULL,
    fetched_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    result_count INTEGER NOT NULL,
    PRIMARY KEY (kind, word_id, source)
);

CREATE TABLE rate_limits (
    rate_key        TEXT PRIMARY KEY,
    max_concurrency INTEGER NOT NULL,
    refill_per_min  REAL NOT NULL,
    burst           INTEGER NOT NULL
);

-- Append-only audit log
CREATE TABLE events (
    event_id    INTEGER PRIMARY KEY,
    ts          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    actor       TEXT NOT NULL,      -- reconciler | worker:<kind> | admin:<user>
    entity_type TEXT NOT NULL,
    entity_id   TEXT NOT NULL,
    action      TEXT NOT NULL,
    detail      TEXT               -- JSON before/after snapshot
);
CREATE INDEX ix_events_entity ON events(entity_type, entity_id);
CREATE INDEX ix_events_ts ON events(ts);

-- ----------------------------------------------------------------------------
-- Releases
-- ----------------------------------------------------------------------------
CREATE TABLE releases (
    release_id   INTEGER PRIMARY KEY,
    version      TEXT NOT NULL UNIQUE,       -- YYYY.MM.DD+<manifest-hash-8>
    plan_id      INTEGER NOT NULL REFERENCES plan_artifacts(plan_id),
    input_hash   TEXT NOT NULL,
    db_file_hash TEXT NOT NULL,
    exported_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    exported_by  TEXT NOT NULL,
    notes        TEXT
);

CREATE TABLE release_manifests (
    release_id INTEGER NOT NULL REFERENCES releases(release_id),
    file_hash  TEXT NOT NULL REFERENCES media_files(file_hash),
    PRIMARY KEY (release_id, file_hash)
);

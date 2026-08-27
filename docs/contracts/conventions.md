# Engineering Conventions

Read README.md (the whitepaper) first — it is the design authority. These conventions govern implementation. If a needed decision is not covered here or in the contracts, pick the standard option and report it in your final summary; do NOT invent new cross-subsystem interfaces.

## Ownership boundaries

| Directory | Owner agent | May also read | Must never touch |
|---|---|---|---|
| `core/` | core | docs/, adapters/ (protocol) | app/, admin-ui/ |
| `admin-ui/` | admin | docs/ | core/, app/, adapters/ |
| `adapters/` | adapters | docs/ | core/, app/, admin-ui/ |
| `app/` | app | docs/ | core/, admin-ui/, adapters/ |

No agent runs git commands — the conductor owns version control. No agent edits README.md or docs/contracts/* — propose changes in your summary instead. Temp files go to the session scratchpad, never into the repo.

## All code, comments, identifiers, commit-facing text: English.

## Rust (core/)

- Edition 2021+, stable toolchain. Binary crate `morphod` in a cargo workspace (`core/Cargo.toml` workspace root; split lib crates when natural: `store`, `domain`, `reconcile`, `api`).
- Stack: tokio, axum, rusqlite (bundled), reqwest (rustls), serde, thiserror (libraries) + anyhow (binary edges), tracing + tracing-subscriber, blake3, governor, clap.
- Store discipline per README Part 4: ONE writer task owning the sole write connection, mpsc of typed `WriteOp` with oneshot acks; read pool of read-only connections; every commit publishes on the broadcast change bus. No other code path opens the DB for writing.
- Schema: embed `docs/contracts/working-db.sql` via `include_str!` and apply on fresh DB; `user_version` pragma for future migrations.
- Errors at adapter/HTTP edges use the taxonomy `Permanent | Transient | RateLimited{until}`.
- `cargo fmt` clean, `cargo clippy -- -D warnings` clean, unit tests colocated `#[cfg(test)]`, integration tests in `tests/` against in-memory/temp DBs. No network in tests.

## TypeScript (admin-ui/)

- Vite + React 18 + TypeScript strict. UI library: **Ant Design 5** + `@ant-design/icons`; charts: ECharts via `echarts-for-react`.
- Data: TanStack Query v5 (all server state; no Redux), TanStack Router (file-based routes), typed API client in `src/api/` mirroring `docs/contracts/admin-api.md` exactly.
- Mocks: MSW v2 with realistic fixtures under `src/mocks/`; dev server runs fully against mocks (`VITE_API_MOCK=1`), real mode proxies `/api` to morphod.
- ESLint + Prettier defaults; feature-folder layout `src/features/{dashboard,words,oov,deadletters,releases}`.
- Package manager: pnpm if available, else npm. Commit lockfile.

## Python (adapters/)

- uv-managed; Python 3.12; one package per adapter + `adapters/common` for the stdin/stdout envelope helper. `ruff` clean. Protocol tests mock external calls; no network in tests.

## Kotlin (app/)

- Kotlin 2.x, AGP 8.x, Compose BOM (Material 3), minSdk 26, targetSdk 35. Version catalog `gradle/libs.versions.toml`.
- Libraries: SQLDelight 2.x (both DBs; `.sq` mirrors docs/contracts/*.sql), Media3 ExoPlayer, Coil 3, lottie-compose, kotlinx-coroutines, kotlinx-serialization. No Hilt — manual DI via an `AppContainer` (app is small; keep it explicit).
- Architecture per README Part 6: `ui/` (Compose screens + design system), `domain/` (LearningEngine, ReviewScheduler, ProgressTracker — pure Kotlin, unit-testable, no Android imports), `data/` (ContentStore, repositories, AudioPlayer, ImageLoader).
- Design system per `docs/contracts/app-design.md`. All UI text English (the product is English-immersion); strings in resources from day one.
- If the Android SDK is absent on this machine, scaffold everything (including Gradle wrapper) and report the build as unverified — do not install SDKs.

## Testing bar (all subsystems)

Every pure-logic module ships with unit tests (Rust: hashing/canonicalization, selection rules, graph algorithms; Kotlin: LearningEngine transitions, FSRS math; TS: API client mappers; Python: protocol envelope). UI layers: compile + render smoke tests are enough for wave 1.

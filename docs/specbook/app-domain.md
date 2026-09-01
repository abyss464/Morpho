# app/domain

10 specs.


============================================================
app/domain/src/main/kotlin/dev/morpho/domain/content/
============================================================


--- GlossIndex.kt ---

# definition annotation index

Build a lookup index from the `gloss_anchors` table. Used to find word items with Chinese annotations in English definition text and mark them. Matching rules: full word matching (won't light up "rites" in favourites), case-insensitive, no word stem reduction (published data already contains base forms). Apostrophes and hyphens are allowed inside words (mother-in-law counts as one word).

## GlossAnchor

An annotation anchor row/line: word ID, English word item, Chinese annotation.

## GlossMatch

A matching result: start and end character positions in the scanned text (left-closed, right-open), original word item, Chinese annotation. Provides a `contains` method to determine whether a character position is within this match range.

## GlossIndex

### size / isEmpty
The number of word items in the index, and whether it is empty.

### gloss(token) -> Chinese annotation or empty
Query the annotation of a single word item. Case-insensitive. Returns empty when the word is not in the published data.

### scan(text) -> match list
Scan a piece of text and return all hit annotated word items, ordered by reading order, non-overlapping. Returns an empty list with no extra memory allocation when there are no hits.

### build methods
- GlossIndex.EMPTY: empty index constant.
- GlossIndex.of(anchors): build an index from a set of GlossAnchor. For duplicate rows/lines of the same word item with different casing, take the one with the smallest word ID. Returns EMPTY for an empty collection.


============================================================
app/domain/src/main/kotlin/dev/morpho/domain/learning/
============================================================


--- LearningEngine.kt ---

# Learning Engine

The core of session building and the three-round mode ladder. Due review takes priority over new words; new words are strictly taken from those not yet mastered, following learning_order. Word groups are only a presentation unit; the engine does not hold a "current group" pointer — each session re-scans from learning_order, so regrouping across content versions transitions seamlessly.

## buildSession(plan, progress, dueReviewIds, newWordQuota, config?) -> SessionPlan

Builds the day's learning plan. Filters out already-mastered words from the full course plan, takes up to newWordQuota new words in learning order, and slices them into learning units along word group boundaries. The returned SessionPlan contains a review word ID list and a learning unit list.

## startSession(plan, progress, config?) -> LearningSessionState

Creates runtime status from the plan and automatically enters the first unit. Marks complete directly when the plan is empty.

## submitAnswer(state, correct) -> AnswerResult

Submits the current question's answer and returns the updated status and result description. Rules:
- Wrong: keeps the current mode, burns the round; the same question reappears until correct. Retry-correct does not count toward valid passes.
- First-time correct: mode advances one level (1->2->3, capped), passing one round.
- All three rounds passed: the word is marked as already mastered.
- When a round ends, active words are shuffled into the next round.
- When a unit ends, incomplete words carry over to the next unit.

The returned AnswerResult contains the new status, result description (whether correct, whether advanced, whether mastered, whether round/unit/session ended), and the progress update list needing persistence.

## abandon(state) -> LearningSessionState

Abandons the current session, retaining progress already passed, and marks the session complete.

## Auxiliary Types

- **SessionConfig**: session configuration — daily goal, unit minimum/maximum word count (15-20), required rounds (3), session seed.
- **LearningUnit**: a learning unit — index, word group ID, word ID list.
- **SessionPlan**: session plan — review word ID list and learning unit list. Provides new word count, review count, and whether it is empty.
- **WordCard**: a word's runtime status within a unit — word ID, current mode, rounds already passed, whether failed this round, whether it is a carried-over word, whether already retired.
- **UnitRuntime**: unit runtime — current round's question order, round number, cursor, attempt count.
- **Question**: current question — word ID, mode, round number, unit index, position within round, round size, attempt count. Provides optionSeed for shuffling options.
- **SessionStats**: session statistics — answer count, first-time correct/wrong count, advancement count, mastery count, review count, review correct count. Provides first-attempt accuracy.
- **AnswerOutcome**: description of a single answer's outcome — whether correct, whether first attempt, whether retry needed, whether advanced, new mode, whether mastered, whether round/unit/session ended, whether to show the detail page.
- **AnswerResult**: return value of submitAnswer — new status, outcome description, progress rows/lines list needing persistence.
- **LearningSessionState**: complete session status. Provides currentQuestion and unitProgress (status of all words in the current unit, carried-over words listed first).


--- OptionAssembler.kt ---

# Option Assembler

For each quiz and review question, build a set of four answer options. Distractors are already bound to the content at build time; the engine only determines the ordering. The order is deterministically computed from the question identity, so the layout stays the same on wrong retries.

## assemble(answerWordId, distractorIds, seed) -> list of four word IDs

Merge the correct answer with the three bound distractors, shuffle deterministically using the seed, and return. Requires exactly three distractors, and the correct answer must not appear among them.

## answerIndex(options, answerWordId) -> index of the correct option

Find the position of the correct answer in the list returned by assemble. Throws an error if the answer is not in the list.


============================================================
app/domain/src/main/kotlin/dev/morpho/domain/model/
============================================================


--- Content.kt ---

# Content Model (read-only)

A mirror of the released `release.db` database. The client must never modify this data — it is generated by `morphod export` and distributed with the APK.

## WordRole enum

The role of a word within the course: TARGET (target word) or AUXILIARY (support word). Provides conversion to and from database strings.

## GroupType enum

The type of a word group: SCC / ROOT / SEMANTIC / FILL. Provides conversion to and from database strings.

## Word

Complete static information for a word. Contains: word ID, spelling, pronunciation, frequency rank, role, word group ID it belongs to, learning order, word source description, word source segment JSON (raw column values, decoded by the presentation layer), image file name, and word audio file name. Media file names are content-addressed (`img/{hash}.webp`, `audio/{hash}.ogg`) and are resolved to actual bytes via the ContentStore.

## Sense

A sense. Contains: sense ID, word ID it belongs to, part of speech, definition, whether it is the primary sense, and definition audio file name.

## Example

An example. Contains: example ID, word ID it belongs to, display order, sentence text, highlight start/end positions (UTF-8 byte offsets), example audio file name, and an optional example image file name.

### highlightCharRange()

Converts the UTF-8 byte offsets from the release contract into a Kotlin character range, used for slicing the sentence string for highlighting. Returns an empty range when the offsets are out of bounds.

## WordGroup

A word group. Contains: word group ID, group order, and group type.

## WordBundle

A word plus all subsidiary data needed by quiz and detail pages, loaded at once. Contains: the Word, the full list of senses, the full list of examples, and the word ID list of the three distractors.

### primarySense

Returns the primary sense; if none is marked as primary, returns the first one. Throws an error if the word has no senses.

### mode1Example

The example with the smallest display order, i.e., the sentence used for mode 1.

### detailExamples

All examples sorted by display order, used on the detail page.

## PlanWord

Lightweight plan rows/lines containing only word ID, word group ID, and learning order. Used when building a learning session to avoid loading full WordBundle objects.

## ContentMetaKeys

Constants for the `meta` table key names defined by the release contract: CONTENT_VERSION, PLAN_ID, EXPORTED_AT, SCHEMA_VER.


--- Progress.kt ---

# User Progress Model

Mirror of `user.db`. Progress is keyed by word ID — word groups are re-divided with version changes, so groups are never used as keys.

## LearningStatus enum

Learning status: LEARNING (in progress) or LEARNED (already mastered). Provides mutual conversion between the two and the database string.

## LearnMode enum

Learning mode, divided into three levels (1 = sentence + image, 2 = word + image + definition, 3 = word + plain-text definition).

### promoted()
Returns the previous-level mode. If already at the highest level (3), it remains unchanged.

### fromLevel(level)
Gets the mode by number; invalid numbers fall back to mode 1.

## LearningProgress

A word's learning progress row: word ID, current mode, rounds already completed, learning status.

## CardState enum

FSRS v5 card status: NEW(0) / LEARNING(1) / REVIEW(2) / RELEARNING(3). Persisted as integer encoding.

## FsrsCard

The complete status of a spaced repetition card: word ID, due time, stability, difficulty, elapsed days, planned days, review count, lapse count, card status, last review time.

## DailyStats

A day's statistics row: date, new word count, review word count, correct count, total answer count. Precision is stored as counts rather than ratios, so multiple sessions can be summed directly without drift.

### correctRate
Correct rate (0.0 to 1.0), empty when there were no answers that day.

## GreetingPeriod enum

Time periods: MORNING / AFTERNOON / EVENING, used for the homepage greeting.

## ActivityChartStyle enum

Activity chart style: BAR (bar chart) or HEATMAP (heatmap).

## ThemeMode enum

Color scheme: SYSTEM (follow system) / LIGHT / DARK.

### isDark(systemInDark)
Resolves the actual shade to use based on whether the system is currently in dark mode.

## DailyActivity

A day's activity summary: date, learning word count, new word count, review word count. Used for the weekly activity chart.

## HeatmapCell / HeatmapData

Heatmap cell (date + intensity level) and the heatmap's overall data (cell list, week count, maximum activity amount).

## UserMetaKeys

Key name constants to use when writing to `user.db`'s meta, covering content version, schema version, daily goal, sound toggle, haptic toggle, sound effect volume, reduced animation override, last learning date, streak days, activity chart style, and theme mode.

## ProgressDefaults

Default configuration constants: daily goal 50 words, review multiplier 4x, schema version 2, sound effect volume 0.8. The review multiplier is used to limit the review volume per session — the daily new word target is multiplied as the limit, and any excess is deferred rather than dropped.


============================================================
app/domain/src/main/kotlin/dev/morpho/domain/progress/
============================================================


--- ProgressTracker.kt ---

# 进度跟踪器

在 inference UI 上显示所有计数器。分子取自 user.db，分母取自 release.db——当内容更新使 word 量增加时，进度条目自动重新缩放，无需迁移；旧版本中删除的 word 自动停止计入。

## overall(shippedWordIds, progress) -> OverallProgress

计算总体进度。以当前发布版包含的全部 word ID 作为分母，统计已掌握数以及进行中数（有复习轮数但尚未掌握的 word）。旧版本遗留的孤儿 rows/lines 会被忽略。

返回的 OverallProgress 提供：总 word 数、已掌握数、进行中数、剩余数、完成比例（0 到 1，用于首页进度环）。

## today(stats, dailyGoal, dueReviewCount) -> TodayProgress

计算今日进度。从当天统计中取新学 word 数和复习数，结合 daily goal 和待复习数。

返回的 TodayProgress 提供：新学 word 数、daily goal、复习数、待复习数、correct 数、作答总数、correct 率、剩余新 word 数、目标是否达成、完成比例、是否还有任务。

## streak(history, today) -> 连续天数

计算截至今天（或昨天——当天未结束前不算断签）的连续学习天数。历史记录无需排序，只统计有活动的天数。

## greetingPeriod(hour) -> 时段

根据小时数返回问候时段：5-11 点为早上，12-17 点为下午，其余为晚上。

## weeklyActivity(recentStats, today) -> 七天活动列表

返回最近七天（含今天）的每日活动摘要，无记录的天填零。

## heatmapData(recentStats, today, weeks?) -> HeatmapData

生成热力图数据，默认覆盖最近 16 周。从最近一个周一开始，每天计算活动量并映射到 0-4 的强度等级（按最大值的四分位）。

## estimatedDaysRemaining(remainingWords, recentStats) -> 预计剩余天数或空

根据最近（至多 14 天）有新学 word 的日均速度估算剩余天数。已学完返回 0，无数据返回空。

## mergeSession(existing, date, newLearned, reviewed, correctAnswers, totalAnswers) -> DailyStats

将一次会话的结果合并到当天统计中。所有字段均为纯计数累加，多次合并与一次性合并结果相同。


--- SessionBank.kt ---

# Session Watermark

Records the cumulative count of sessions already written to `daily_stats` in the current run. Progress is written to the database in real time as learning happens, rather than waiting until the settlement page — avoiding loss of all records if the user exits midway. The watermark mechanism ensures that repeated writes to the database are only counted once.

## isEmpty

Returns true when all three counters are zero, indicating there are no pending increments to write.

## pending(running) -> SessionBank

Uses the current run's total count minus the watermark to derive the increments not yet written to the database. After a successful write, the watermark is advanced to the current running value; if the total hasn't changed, calling it again will return an empty increment rather than duplicating the write.


============================================================
app/domain/src/main/kotlin/dev/morpho/domain/review/
============================================================


--- Fsrs.kt ---

# FSRS v5 Spaced Repetition

FSRS v5 (Free Spaced Repetition Scheduler) full implementation. Each card carries two latent variables: stability S (days required for memory to decay to 90% retrievability) and difficulty D (inherent difficulty, 1 to 10). Morpho does not use learning/relearning step counts; cards enter review status directly after passing through the learning ladder; no interval fuzzing is applied, all scheduling is deterministic.

## Fsrs constant object

Provides decay constant DECAY, decay factor FACTOR, 19 default weights, stability lower bound 0.1, difficulty range 1-10, maximum interval 36500 days.

## Grade enum

Four score levels: AGAIN(1) / HARD(2) / GOOD(3) / EASY(4). Morpho's binary answer mapping targets AGAIN or GOOD.

## FsrsScheduler(parameters?, desiredRetention?, maximumIntervalDays?)

A stateless scheduler that can specify 19 model weights (defaults built in), target retention rate (default 0.9), and maximum interval days.

### retrievability(elapsedDays, stability) -> retrievability
Computes the memory retrievability after a given number of days, based on the forgetting curve.

### retrievability(card, now) -> retrievability
Computes a card's retrievability at a given moment. Returns 0 for cards that have never been reviewed.

### intervalDays(stability) -> days
Computes the next review interval based on stability and target retention rate, rounded to whole days, minimum 1 day.

### initialStability(grade) -> stability
Initializes stability on first review based on the score.

### initialDifficulty(grade) -> difficulty
Initializes difficulty on first review based on the score.

### nextDifficulty(difficulty, grade) -> difficulty
Computes the next difficulty based on current difficulty and score, with linear damping and mean reversion toward "easy".

### nextRecallStability(difficulty, stability, retrievability, grade) -> stability
Computes new stability after a successful recall (scores 2-4), considering difficulty penalty and ease bonus.

### nextForgetStability(difficulty, stability, retrievability) -> stability
Computes new stability after a lapse (score 1), with a limit ensuring forgetting does not inadvertently increase stability.

### shortTermStability(stability, grade) -> stability
Computes stability for repeated reviews within the same day.

### review(card, grade, now) -> FsrsCard
Executes a review on a card and returns the rescheduled card. Initializes parameters on first review; uses short-term formula for same-day repetitions; runs the full forgetting curve for cross-day reviews. A lapse (AGAIN) puts the card back into relearning status.

### newCard(wordId, now, grade?) -> FsrsCard
Creates the first review card for a word that has just passed through the learning ladder, initialized with a GOOD score by default.

### daysUntilRetention(card, target?) -> days
Computes how many days from the last review until a card decays to the target retention rate. Used for the "Next review in N days" text on the summary page.


--- ReviewScheduler.kt ---

# Review scheduler

Determines what to review today. Interval math is handled by FsrsScheduler; this handles queue construction. All reviews use the image + definition grid uniformly (mode 2 visual).

## ReviewItem

One entry in the review queue, carrying an FsrsCard.

## ReviewScheduler(fsrs?)

Accepts optional custom FsrsScheduler during construction; uses standard parameters by default.

### dueCards(cards, now) -> list of due cards
Filters cards with due time no later than now, sorted from earliest to latest due time.

### buildQueue(cards, now, limit?) -> list of review items
Builds the complete review queue. Optional limit parameter truncates queue length.

### grade(correct) -> Grade
Maps binary answer result to FSRS score: correct = GOOD, incorrect = AGAIN.

### applyAnswer(card, correct, now) -> FsrsCard
Applies answer result to a card, returns the rescheduled card.

### newCardFor(wordId, now) -> FsrsCard
Creates a new review card for the specified word.

### scheduler() -> FsrsScheduler
Returns the internal scheduler instance in use.


============================================================
app/domain/src/main/kotlin/dev/morpho/domain/util/
============================================================


--- DeterministicRandom.kt ---

# Deterministic Random

Used to replace the standard database random number generator, ensuring that the same seed produces exactly the same permutation on any device and on any round/line version.

## SplitMix64(seed)

SplitMix64 pseudorandom number generator.

### nextLong() -> long
Generates the next 64-bit pseudorandom number.

### nextInt(bound) -> int
Generates a uniformly distributed random integer between 0 (inclusive) and bound (exclusive). bound must be positive.

## deterministicShuffled(seed) -> new list

A List extension method. Uses SplitMix64 to drive a Fisher-Yates shuffle, returning a new list that has been deterministically shuffled. The original list is left unchanged. When there are fewer than two elements, returns a copy directly.

## seedOf(vararg parts) -> long

Folds multiple long integers (such as word ID, mode, round number, unit index) into a single seed via the FNV-1a 64-bit hash. Used to map question identity to a stable shuffle seed.

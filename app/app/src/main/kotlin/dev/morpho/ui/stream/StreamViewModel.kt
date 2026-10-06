package dev.morpho.ui.stream

import android.os.SystemClock
import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.data.haptics.HapticPattern
import dev.morpho.data.sound.SfxEvent
import dev.morpho.data.stream.StreamSnapshot
import dev.morpho.data.stream.WordNote
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.model.WordBundle
import dev.morpho.domain.progress.ProgressTracker
import dev.morpho.domain.review.Grade
import dev.morpho.domain.stream.Difficulty
import dev.morpho.domain.stream.Outcome
import dev.morpho.domain.stream.Pieces
import dev.morpho.domain.stream.Puzzle
import dev.morpho.domain.stream.ReviewTask
import dev.morpho.domain.stream.Seeded
import dev.morpho.domain.stream.SpellPuzzle
import dev.morpho.domain.stream.Spelling
import dev.morpho.domain.stream.Step
import dev.morpho.domain.stream.StepKind
import dev.morpho.domain.stream.StepResult
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/** A step's task as the learner works it: rebuilding the meaning, or filling the gap. */
sealed interface TaskState {
    val solved: Boolean
}

/**
 * The explain task (docs/contracts/stream.md §2, §4): the definition with its blanks, filled
 * piece by piece. [filled] holds, per blank, the id of the piece in it or null; filling the
 * last blank checks the tray, and [misses] counts failed checks.
 */
data class RebuildState(
    val puzzle: Puzzle,
    val filled: List<Int?> = List(puzzle.answer.size) { null },
    val checked: Boolean = false,
    val misses: Int = 0,
    val hinted: Boolean = false,
    override val solved: Boolean = false,
) : TaskState {
    fun textOf(id: Int): String = puzzle.pieces.first { it.id == id }.text

    private val answerTexts: List<String> = puzzle.answer.map { Pieces.norm(textOf(it)) }

    /** Whether blank [k] holds a piece with the right text, in [slots] (the current fill by default). */
    fun right(k: Int, slots: List<Int?> = filled): Boolean =
        slots[k]?.let { Pieces.norm(textOf(it)) == answerTexts[k] } ?: false

    /** The first empty blank, the one the next tapped piece fills; -1 when all are full. */
    val nextOpen: Int get() = filled.indexOf(null)

    fun used(id: Int): Boolean = id in filled

    /** The tray was checked and is wrong: misplaced pieces show red. */
    val wrongShown: Boolean get() = checked && null !in filled && !solved

    /** The piece that belongs in blank [k]: the first with its text not already right elsewhere. */
    fun pieceFor(k: Int): Int? = puzzle.pieces.firstOrNull { p ->
        Pieces.norm(p.text) == answerTexts[k] && filled.indices.none { i -> filled[i] == p.id && right(i) }
    }?.id
}

/**
 * The spelling that ends a rebuild review (docs/contracts/stream.md §2): the word's letters
 * as blanks, filled from letter tiles. [filled] holds, per blank, the tile in it or null.
 */
data class SpellState(
    val puzzle: SpellPuzzle,
    val filled: List<Int?> = List(puzzle.answer.size) { null },
    val checked: Boolean = false,
    val misses: Int = 0,
    val hinted: Boolean = false,
    override val solved: Boolean = false,
) : TaskState {
    fun letterOf(id: Int): String = puzzle.tiles.first { it.id == id }.letter

    /** Whether blank [k] holds the right letter, in [slots] (the current fill by default). */
    fun right(k: Int, slots: List<Int?> = filled): Boolean =
        slots[k]?.let { letterOf(it) == puzzle.answer[k] } ?: false

    /** The first empty blank, the one the next tapped tile fills; -1 when all are full. */
    val nextOpen: Int get() = filled.indexOf(null)

    fun used(id: Int): Boolean = id in filled

    /** The word was checked and is wrong: wrong letters show red. */
    val wrongShown: Boolean get() = checked && null !in filled && !solved

    /** The tile that belongs in blank [k]: the first with its letter not already right elsewhere. */
    fun tileFor(k: Int): Int? = puzzle.tiles.firstOrNull { t ->
        t.letter == puzzle.answer[k] && filled.indices.none { i -> filled[i] == t.id && right(i) }
    }?.id

    /** Every blank filled with its right letter, each tile used once. */
    fun answerFill(): List<Int> {
        val taken = mutableSetOf<Int>()
        return puzzle.answer.map { letter ->
            puzzle.tiles.first { it.letter == letter && it.id !in taken }.id.also { taken += it }
        }
    }
}

/** How a rebuild review's two parts went, for its result card. */
data class ReviewParts(val rebuild: Outcome, val spell: Outcome)

/** A rebuild review's first part, carried into its spelling: how it went and how long it took. */
data class RebuiltPart(val outcome: Outcome, val elapsedMs: Long)

/**
 * The use task: the example with the word blanked, and four words to fill it with.
 * [wrong] holds the ids of options already picked wrongly.
 */
data class FillState(
    val gap: Gap,
    val options: List<WordBundle>,
    val answerId: Long,
    val wrong: List<Long> = emptyList(),
    override val solved: Boolean = false,
) : TaskState

/** A review once its task is solved: the rating it earned and what each rating would schedule. */
data class ReviewVerdict(
    val outcome: Outcome,
    val grade: Grade,
    /** Days until the next review, per rating, from the card as it was before this review. */
    val intervals: Map<Grade, Long>,
    /** A rebuild review's two parts; null for a fill-in. */
    val parts: ReviewParts? = null,
)

/** The step on screen. */
data class StepView(
    val step: Step,
    val word: WordBundle,
    /** 1-based unit of the word in learning order. */
    val unit: Int,
    /** `know` for a word already being learned: "Look again" rather than "New word". */
    val again: Boolean,
    val note: WordNote?,
    /** The word's FSRS review count before this review; 0 outside review. */
    val reps: Int,
    val task: TaskState?,
    val pictureShown: Boolean = false,
    /** The task's result once solved; Continue opens when it is set (or always, for `know`). */
    val outcome: Outcome? = null,
    /** A rebuild review's rebuild, once its spelling has begun. */
    val rebuilt: RebuiltPart? = null,
    /** A solved review: shown as its result card. */
    val verdict: ReviewVerdict? = null,
    /** Bumps on every step load, so the screen re-keys its transition. */
    val serial: Int = 0,
)

/** The done screen's figures (docs/contracts/stream.md §7). */
data class DoneSummary(
    val steps: Int,
    val reviewed: Int,
    val reviewedClean: Int,
    val met: Int,
    val metClean: Int,
    /** Reviews due by the end of tomorrow. */
    val tomorrow: Int,
    val newPerDay: Int,
    val streak: Int,
    /** Units finished today, or null when none was. */
    val units: UnitNews? = null,
)

/** What comes after the units finished today. */
enum class UnitEnding {
    /** The next unit is untouched: it starts tomorrow. */
    NEXT_TOMORROW,

    /** The next unit has already begun. */
    NEXT_BEGUN,

    /** The last finished unit was the release's last. */
    LAST_UNIT,
}

/** Units whose last words graduated today, in order, and what follows the last of them. */
data class UnitNews(val finished: List<Int>, val ending: UnitEnding)

data class StreamUiState(
    val done: Int = 0,
    val total: Int = 0,
    val view: StepView? = null,
    val summary: DoneSummary? = null,
    val nowPlaying: String? = null,
)

/**
 * Drives the stream: asks [dev.morpho.domain.stream.StreamEngine] for the next step,
 * loads what the step shows, runs its task, and writes every finished step (cards, the
 * day's counts, the stream state) before choosing the next. The step on screen is saved
 * as the state's `current`, so leaving and coming back resumes on it.
 */
class StreamViewModel(private val container: AppContainer) : ViewModel() {

    private val engine = container.streamEngine
    private val store = container.streamStore

    private val _state = MutableStateFlow(StreamUiState())
    val state: StateFlow<StreamUiState> = _state.asStateFlow()

    private var snapshot: StreamSnapshot? = null
    private var notes: Map<Long, WordNote> = emptyMap()
    private var startedAt = 0L

    /** How long a rebuild review's rebuild took, from the step's start to its solve. */
    private var rebuildMs = 0L
    private var serial = 0
    private var busy = false
    private val skipped = mutableSetOf<Long>()

    private val newPerDay: Int get() = container.settingsRepository.settings.value.dailyGoal

    init {
        viewModelScope.launch {
            container.audioPlayer.nowPlaying.collect { file -> _state.update { it.copy(nowPlaying = file) } }
        }
        viewModelScope.launch {
            notes = store.notes()
            snapshot = store.open(LocalDate.now())
            showNext()
        }
    }

    // ------------------------------------------------------------- choosing

    /** Shows the saved step, or chooses the next one; the done screen when nothing is left. */
    private suspend fun showNext() {
        val snap = snapshot ?: return
        val date = LocalDate.now()
        val now = Instant.now()
        var stream = engine.today(snap.state, date)
        val step = stream.current ?: engine.next(stream, snap.order, snap.cards, newPerDay, now, date)
        if (step == null) {
            snapshot = snap.copy(state = stream)
            showDone()
            return
        }
        if (stream.current != step) {
            stream = stream.copy(current = step)
            store.save(stream)
        }
        snapshot = snap.copy(state = stream)

        val view = runCatching { load(step, snapshot!!) }
            .onFailure { Log.e(TAG, "could not load step $step", it) }
            .getOrNull()
        if (view == null) {
            skip(step)
            return
        }
        startedAt = SystemClock.elapsedRealtime()
        _state.value = _state.value.copy(view = view, summary = null)
        refreshProgress()
        // A fill-in review keeps quiet: hearing the word would answer it.
        when {
            step.kind == StepKind.KNOW -> readAloud(view.word)
            step.kind == StepKind.REVIEW && step.task == ReviewTask.REBUILD ->
                container.audioPlayer.play(view.word.word.wordAudioFile)
        }
    }

    /**
     * A step whose word cannot be loaded (a word a later release dropped) leaves the stream
     * unrated: the word stops being Learning and the next step is chosen. A word that fails
     * twice ends today's stream rather than looping on it.
     */
    private suspend fun skip(step: Step) {
        val snap = snapshot ?: return
        val stream = snap.state.copy(words = snap.state.words - step.wordId, current = null)
        snapshot = snap.copy(state = stream)
        store.save(stream)
        if (!skipped.add(step.wordId)) {
            showDone()
            return
        }
        showNext()
    }

    private suspend fun load(step: Step, snap: StreamSnapshot): StepView? {
        val content = container.contentRepository
        val word = content.bundle(step.wordId) ?: return null
        val stream = snap.state
        val rebuild = step.kind == StepKind.EXPLAIN1 || step.kind == StepKind.EXPLAIN2 ||
            (step.kind == StepKind.REVIEW && step.task == ReviewTask.REBUILD)
        val fill = step.kind == StepKind.USE || (step.kind == StepKind.REVIEW && step.task == ReviewTask.FILL)

        val task: TaskState? = when {
            rebuild -> {
                val easy = step.kind == StepKind.EXPLAIN1
                val poolIds = if (easy) easyPool(step.wordId, snap) else hardPool(step.wordId, snap)
                val pool = content.bundles(poolIds).let { found -> poolIds.mapNotNull { found[it] } }
                RebuildState(Pieces.puzzle(word, pool, if (easy) Difficulty.EASY else Difficulty.HARD))
            }
            fill -> {
                val others = content.bundles(word.distractorIds).let { found ->
                    word.distractorIds.mapNotNull { found[it] }
                }.take(3)
                val options = Seeded(step.wordId + 7).shuffle(listOf(word) + others)
                FillState(gap = gapOf(word), options = options, answerId = step.wordId)
            }
            else -> null
        }
        serial += 1
        return StepView(
            step = step,
            word = word,
            unit = snap.unitOf(step.wordId),
            again = step.kind == StepKind.KNOW && step.wordId in stream.words,
            note = notes[step.wordId],
            reps = snap.cards[step.wordId]?.reps ?: 0,
            task = task,
            serial = serial,
        )
    }

    /** Easy decoys: the words being learned now, plus the eight met most recently. */
    private fun easyPool(wordId: Long, snap: StreamSnapshot): List<Long> {
        val recent = snap.cards.keys.sortedByDescending { snap.positionOf(it) }.take(RECENT_POOL)
        return (snap.state.words.keys + recent).distinct().filter { it != wordId }
    }

    /** Hard decoys: twelve met words, chosen deterministically by the word's id. */
    private fun hardPool(wordId: Long, snap: StreamSnapshot): List<Long> =
        Seeded(wordId).shuffle(snap.cards.keys.sorted().filter { it != wordId }).take(MET_POOL)

    private suspend fun showDone() {
        val snap = snapshot ?: return
        val today = LocalDate.now()
        val endOfTomorrow = today.plusDays(2).atStartOfDay(ZoneId.systemDefault()).toInstant().minusMillis(1)
        val day = snap.state.day
        val summary = DoneSummary(
            steps = day.steps,
            reviewed = day.reviewed,
            reviewedClean = day.reviewedClean,
            met = day.met,
            metClean = day.metClean,
            tomorrow = engine.dueReviews(snap.state, snap.cards, snap.order.toSet(), endOfTomorrow).size,
            newPerDay = newPerDay,
            streak = ProgressTracker.streak(container.progressRepository.recentStats(), today),
            units = unitNews(snap, today),
        )
        val arriving = _state.value.view != null
        _state.value = _state.value.copy(view = null, summary = summary)
        refreshProgress()
        if (arriving) container.playSfx(SfxEvent.REVIEW_DONE)
    }

    /**
     * Units finished today: a unit is finished when one of its words graduated today (its
     * card has one review, made today) and every word in it has a card.
     */
    private fun unitNews(snap: StreamSnapshot, today: LocalDate): UnitNews? {
        val zone = ZoneId.systemDefault()
        val wordsOf = snap.order.chunked(StreamSnapshot.UNIT_SIZE)
        val finished = snap.cards.values
            .filter { it.reps == 1 && it.lastReview?.atZone(zone)?.toLocalDate() == today }
            .filter { snap.positionOf(it.wordId) >= 0 }
            .map { snap.unitOf(it.wordId) }
            .distinct()
            .filter { u -> wordsOf[u - 1].all { it in snap.cards } }
            .sorted()
        val last = finished.lastOrNull() ?: return null
        val ending = when {
            last >= wordsOf.size -> UnitEnding.LAST_UNIT
            wordsOf[last].any { it in snap.cards || it in snap.state.words } -> UnitEnding.NEXT_BEGUN
            else -> UnitEnding.NEXT_TOMORROW
        }
        return UnitNews(finished, ending)
    }

    private fun refreshProgress() {
        val snap = snapshot ?: return
        val (done, total) = engine.progress(snap.state, snap.order, snap.cards, newPerDay, Instant.now(), LocalDate.now())
        _state.update { it.copy(done = done, total = total) }
    }

    // ------------------------------------------------------------- the task

    /** A tapped piece fills the first open blank. */
    fun onPlacePiece(id: Int) {
        val rebuild = currentRebuild() ?: return
        val open = rebuild.nextOpen
        if (rebuild.solved || rebuild.used(id) || open < 0) return
        update(rebuild, rebuild.filled.mapIndexed { k, x -> if (k == open) id else x })
    }

    /** A tapped chip goes back to the bank and reopens its blank. */
    fun onReturnPiece(blank: Int) {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved) return
        update(rebuild, rebuild.filled.mapIndexed { k, x -> if (k == blank) null else x })
    }

    /** Empties every blank, of the rebuild or of the spelling. */
    fun onStartOver() {
        when (val task = _state.value.view?.task) {
            is RebuildState -> if (!task.solved && task.filled.any { it != null }) update(task, task.filled.map { null })
            is SpellState -> if (!task.solved && task.filled.any { it != null }) updateSpell(task, task.filled.map { null })
            else -> Unit
        }
    }

    /** Puts the right piece into the first blank that is open or wrong. Counts as help. */
    fun onShowNextPiece() {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved) return
        val k = rebuild.filled.indices.firstOrNull { !rebuild.right(it) } ?: return
        val piece = rebuild.pieceFor(k) ?: return
        val next = rebuild.filled.mapIndexed { i, x ->
            when {
                i == k -> piece
                x == piece -> null
                else -> x
            }
        }
        update(rebuild.copy(hinted = true), next, usedHint = true)
    }

    /** Fills every blank with the answer, which solves the step as failed. */
    fun onShowAnswer() {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved) return
        update(rebuild.copy(hinted = true), rebuild.puzzle.answer, usedHint = true)
    }

    /** Sets the blanks; filling the last one checks the tray. */
    private fun update(rebuild: RebuildState, next: List<Int?>, usedHint: Boolean = rebuild.hinted) {
        val placed = rebuild.copy(filled = next, checked = false)
        if (null in next) {
            container.playSfx(SfxEvent.TAP)
            setTask(placed)
            return
        }
        if (!next.indices.all { placed.right(it, next) }) {
            feedback(correct = false)
            setTask(placed.copy(checked = true, misses = rebuild.misses + 1))
            return
        }
        // A tray solved with help gets a plain tap, not the correct chime.
        if (usedHint) container.playSfx(SfxEvent.TAP) else feedback(correct = true)
        val outcome = when {
            usedHint || rebuild.misses >= 2 -> Outcome.FAILED
            rebuild.misses == 1 -> Outcome.SHAKY
            else -> Outcome.CLEAN
        }
        setTask(placed.copy(checked = true, solved = true))
        solved(outcome) { view -> container.audioPlayer.play(view.word.primarySense.defAudioFile) }
    }

    // ------------------------------------------------------------- spelling

    /** A tapped tile fills the first open blank of the word. */
    fun onPlaceLetter(id: Int) {
        val spell = currentSpell() ?: return
        val open = spell.nextOpen
        if (spell.solved || spell.used(id) || open < 0) return
        updateSpell(spell, spell.filled.mapIndexed { k, x -> if (k == open) id else x })
    }

    /** A tapped letter goes back to the tiles and reopens its blank. */
    fun onReturnLetter(blank: Int) {
        val spell = currentSpell() ?: return
        if (spell.solved) return
        updateSpell(spell, spell.filled.mapIndexed { k, x -> if (k == blank) null else x })
    }

    /** Puts the right letter into the first blank that is open or wrong. Counts as help. */
    fun onShowNextLetter() {
        val spell = currentSpell() ?: return
        if (spell.solved) return
        val k = spell.filled.indices.firstOrNull { !spell.right(it) } ?: return
        val tile = spell.tileFor(k) ?: return
        val next = spell.filled.mapIndexed { i, x ->
            when {
                i == k -> tile
                x == tile -> null
                else -> x
            }
        }
        updateSpell(spell.copy(hinted = true), next, usedHint = true)
    }

    /** Fills in the whole word, which solves the spelling as failed. */
    fun onShowWord() {
        val spell = currentSpell() ?: return
        if (spell.solved) return
        updateSpell(spell.copy(hinted = true), spell.answerFill(), usedHint = true)
    }

    /** Sets the word's blanks; filling the last one checks the word. */
    private fun updateSpell(spell: SpellState, next: List<Int?>, usedHint: Boolean = spell.hinted) {
        val placed = spell.copy(filled = next, checked = false)
        if (null in next) {
            container.playSfx(SfxEvent.TAP)
            setTask(placed)
            return
        }
        if (!next.indices.all { placed.right(it, next) }) {
            feedback(correct = false)
            setTask(placed.copy(checked = true, misses = spell.misses + 1))
            return
        }
        if (usedHint) container.playSfx(SfxEvent.TAP) else feedback(correct = true)
        val outcome = when {
            usedHint || spell.misses >= 2 -> Outcome.FAILED
            spell.misses == 1 -> Outcome.SHAKY
            else -> Outcome.CLEAN
        }
        setTask(placed.copy(checked = true, solved = true))
        solved(outcome) { view -> container.audioPlayer.play(view.word.word.wordAudioFile) }
    }

    /** "Now spell it": the rebuild review goes on to spelling the word from its meaning. */
    private fun startSpelling(view: StepView) {
        val rebuilt = view.outcome ?: return
        container.playSfx(SfxEvent.TAP)
        container.audioPlayer.stop()
        startedAt = SystemClock.elapsedRealtime()
        setView(
            view.copy(
                task = SpellState(Spelling.puzzle(view.word.word.word, view.word.word.wordId)),
                outcome = null,
                rebuilt = RebuiltPart(rebuilt, rebuildMs),
            ),
        )
    }

    fun onPickWord(wordId: Long) {
        val view = _state.value.view ?: return
        val fill = view.task as? FillState ?: return
        if (fill.solved || wordId in fill.wrong) return
        if (wordId != fill.answerId) {
            feedback(correct = false)
            setTask(fill.copy(wrong = fill.wrong + wordId))
            return
        }
        feedback(correct = true)
        setTask(fill.copy(solved = true))
        val outcome = if (fill.wrong.isEmpty()) Outcome.CLEAN else Outcome.FAILED
        solved(outcome) { v -> container.audioPlayer.play(v.word.cardExample?.exAudioFile) }
    }

    /**
     * A solved task. Learning steps wait for Continue; a review is applied at once and
     * turns into its result card, read aloud in full.
     */
    private fun solved(outcome: Outcome, playSolved: (StepView) -> Unit) {
        val view = _state.value.view ?: return
        val elapsed = SystemClock.elapsedRealtime() - startedAt
        // Learning steps, and a review's rebuild, wait for Continue ("Now spell it").
        if (view.step.kind != StepKind.REVIEW || view.task is RebuildState) {
            if (view.step.kind == StepKind.REVIEW) rebuildMs = elapsed
            setView(view.copy(outcome = outcome))
            playSolved(view)
            return
        }
        // The review is recorded once: after the fill-in, or after spelling, rated from the
        // worse of rebuild and spelling over both parts' time.
        val rebuilt = view.rebuilt
        val total = if (rebuilt != null) worse(rebuilt.outcome, outcome) else outcome
        val parts = rebuilt?.let { ReviewParts(it.outcome, outcome) }
        val snap = snapshot ?: return
        val prev = snap.cards[view.step.wordId] ?: return
        val now = Instant.now()
        viewModelScope.launch {
            val result = finishStep(view.step, total, (rebuilt?.elapsedMs ?: 0) + elapsed)
            val grade = result.state.lastReview?.grade ?: return@launch
            setView(
                view.copy(
                    outcome = total,
                    verdict = ReviewVerdict(total, grade, engine.intervals(prev, now), parts),
                ),
            )
            readAloud(view.word)
        }
    }

    fun onShowPicture() {
        val view = _state.value.view ?: return
        container.playSfx(SfxEvent.TAP)
        setView(view.copy(pictureShown = true))
    }

    // ------------------------------------------------------------- moving on

    fun onContinue() {
        if (busy) return
        val view = _state.value.view ?: return
        if (view.verdict != null) {
            container.playSfx(SfxEvent.TAP)
            busy = true
            viewModelScope.launch {
                showNext()
                busy = false
            }
            return
        }
        if (view.step.kind == StepKind.REVIEW) {
            if (view.task is RebuildState && view.outcome != null) startSpelling(view)
            return
        }
        val outcome = if (view.step.kind == StepKind.KNOW) Outcome.CLEAN else view.outcome ?: return
        busy = true
        viewModelScope.launch {
            finish(view.step, outcome, SystemClock.elapsedRealtime() - startedAt)
            busy = false
        }
    }

    private suspend fun finish(step: Step, outcome: Outcome, elapsedMs: Long) {
        val result = finishStep(step, outcome, elapsedMs)
        if (result.graduated) {
            container.playSfx(SfxEvent.PROMOTE)
            container.hapticsManager.perform(HapticPattern.PROMOTE)
        } else {
            container.playSfx(SfxEvent.TAP)
        }
        showNext()
    }

    /** Applies a finished step and writes it: cards, the day's counts and the stream state. */
    private suspend fun finishStep(step: Step, outcome: Outcome, elapsedMs: Long): StepResult {
        val snap = snapshot ?: error("no stream loaded")
        val date = LocalDate.now()
        val result = engine.complete(snap.state, step, outcome, elapsedMs, snap.cards, Instant.now(), date)
        store.record(result, date)
        snapshot = snap.copy(state = result.state, cards = snap.cards.withCards(result.cards))
        refreshProgress()
        return result
    }

    /** Replaces the derived rating of the review on screen with the learner's choice. */
    fun onRate(grade: Grade) {
        val view = _state.value.view ?: return
        val verdict = view.verdict ?: return
        if (verdict.grade == grade) return
        val snap = snapshot ?: return
        container.playSfx(SfxEvent.TAP)
        val result = engine.override(snap.state, grade, Instant.now())
        snapshot = snap.copy(state = result.state, cards = snap.cards.withCards(result.cards))
        setView(view.copy(verdict = verdict.copy(grade = grade)))
        viewModelScope.launch {
            store.record(StepResult(result.state, result.cards), LocalDate.now())
            refreshProgress()
        }
    }

    fun onSaveNote(text: String) {
        val view = _state.value.view ?: return
        if (text.isBlank()) return
        container.playSfx(SfxEvent.TAP)
        viewModelScope.launch {
            notes = store.saveNote(view.word.word.wordId, text, Instant.now())
            setView(view.copy(note = notes[view.word.word.wordId]))
        }
    }

    /** "Meet 5 more words": raises today's new-word allowance and goes back into the stream. */
    fun onMeetMore() {
        val snap = snapshot ?: return
        container.playSfx(SfxEvent.TAP)
        val stream = engine.today(snap.state, LocalDate.now())
        val raised = stream.copy(day = stream.day.copy(extra = stream.day.extra + MORE_WORDS))
        snapshot = snap.copy(state = raised)
        viewModelScope.launch {
            store.save(raised)
            showNext()
        }
    }

    // ------------------------------------------------------------- audio

    /** The word, its primary definition, then its example, as one run. */
    fun readAloud(word: WordBundle) {
        container.audioPlayer.playSequence(
            listOf(word.word.wordAudioFile, word.primarySense.defAudioFile, word.cardExample?.exAudioFile),
        )
    }

    fun onPlayWord(word: WordBundle) = container.audioPlayer.play(word.word.wordAudioFile)

    fun onPause() {
        container.playSfx(SfxEvent.TAP)
        container.audioPlayer.stop()
    }

    // ------------------------------------------------------------- helpers

    private fun currentRebuild(): RebuildState? = _state.value.view?.task as? RebuildState

    private fun currentSpell(): SpellState? = _state.value.view?.task as? SpellState

    private fun setTask(task: TaskState) {
        val view = _state.value.view ?: return
        setView(view.copy(task = task))
    }

    private fun setView(view: StepView) = _state.update { it.copy(view = view) }

    private fun feedback(correct: Boolean) {
        container.playSfx(if (correct) SfxEvent.CORRECT else SfxEvent.WRONG)
        container.hapticsManager.perform(if (correct) HapticPattern.CORRECT else HapticPattern.WRONG)
    }

    override fun onCleared() {
        container.audioPlayer.stop()
        super.onCleared()
    }

    companion object {
        private const val TAG = "StreamViewModel"
        private const val RECENT_POOL = 8
        private const val MET_POOL = 12
        private const val MORE_WORDS = 5

        fun factory(container: AppContainer) = viewModelFactory {
            initializer { StreamViewModel(container) }
        }
    }
}

/** The worse of two outcomes: failed over shaky over clean. */
private fun worse(a: Outcome, b: Outcome): Outcome = if (a.ordinal >= b.ordinal) a else b

private fun Map<Long, FsrsCard>.withCards(changed: List<FsrsCard>): Map<Long, FsrsCard> =
    if (changed.isEmpty()) this else this + changed.associateBy { it.wordId }

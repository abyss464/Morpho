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
import dev.morpho.domain.stream.ReviewTask
import dev.morpho.domain.stream.Seeded
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
 * The explain task (docs/contracts/stream.md §2): pieces tapped into a tray in order.
 * Placing the last piece checks it; [misses] counts failed checks.
 */
data class RebuildState(
    val puzzle: dev.morpho.domain.stream.Puzzle,
    val placed: List<Int> = emptyList(),
    val checked: Boolean = false,
    val misses: Int = 0,
    val hinted: Boolean = false,
    override val solved: Boolean = false,
) : TaskState {
    fun textOf(id: Int): String = puzzle.pieces.first { it.id == id }.text

    /** Whether the piece in tray slot [k] is the right one for that slot. */
    fun right(k: Int): Boolean = Pieces.norm(textOf(placed[k])) == puzzle.answer[k]

    /** The tray was checked and is wrong: misplaced pieces show red. */
    val wrongShown: Boolean get() = checked && placed.size == puzzle.answer.size && !solved
}

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
)

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
        )
        val arriving = _state.value.view != null
        _state.value = _state.value.copy(view = null, summary = summary)
        refreshProgress()
        if (arriving) container.playSfx(SfxEvent.REVIEW_DONE)
    }

    private fun refreshProgress() {
        val snap = snapshot ?: return
        val (done, total) = engine.progress(snap.state, snap.order, snap.cards, newPerDay, Instant.now(), LocalDate.now())
        _state.update { it.copy(done = done, total = total) }
    }

    // ------------------------------------------------------------- the task

    fun onPlacePiece(id: Int) {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved || id in rebuild.placed) return
        val next = rebuild.placed + id
        if (next.size < rebuild.puzzle.answer.size) container.playSfx(SfxEvent.TAP)
        check(rebuild.copy(placed = next, checked = false), usedHint = rebuild.hinted)
    }

    fun onReturnPiece(id: Int) {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved) return
        container.playSfx(SfxEvent.TAP)
        setTask(rebuild.copy(placed = rebuild.placed - id, checked = false))
    }

    fun onStartOver() {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved) return
        container.playSfx(SfxEvent.TAP)
        setTask(rebuild.copy(placed = emptyList(), checked = false))
    }

    /** Keeps the correct opening pieces and places the next correct one. */
    fun onShowNextPiece() {
        val rebuild = currentRebuild() ?: return
        if (rebuild.solved) return
        var keep = 0
        while (keep < rebuild.placed.size && rebuild.right(keep)) keep += 1
        val head = rebuild.placed.take(keep)
        val piece = rebuild.puzzle.pieces.firstOrNull {
            it.id !in head && Pieces.norm(it.text) == rebuild.puzzle.answer.getOrNull(keep)
        } ?: return
        val next = head + piece.id
        if (next.size < rebuild.puzzle.answer.size) container.playSfx(SfxEvent.TAP)
        check(rebuild.copy(placed = next, checked = false, hinted = true), usedHint = true)
    }

    /** Placing the last piece checks the tray. */
    private fun check(rebuild: RebuildState, usedHint: Boolean) {
        if (rebuild.placed.size != rebuild.puzzle.answer.size) {
            setTask(rebuild)
            return
        }
        val correct = rebuild.placed.indices.all(rebuild::right)
        if (!correct) {
            feedback(correct = false)
            setTask(rebuild.copy(checked = true, misses = rebuild.misses + 1))
            return
        }
        feedback(correct = true)
        val outcome = when {
            usedHint || rebuild.misses >= 2 -> Outcome.FAILED
            rebuild.misses == 1 -> Outcome.SHAKY
            else -> Outcome.CLEAN
        }
        setTask(rebuild.copy(checked = true, solved = true))
        solved(outcome) { view -> container.audioPlayer.play(view.word.primarySense.defAudioFile) }
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
        if (view.step.kind != StepKind.REVIEW) {
            setView(view.copy(outcome = outcome))
            playSolved(view)
            return
        }
        val snap = snapshot ?: return
        val prev = snap.cards[view.step.wordId] ?: return
        val now = Instant.now()
        viewModelScope.launch {
            val result = finishStep(view.step, outcome, SystemClock.elapsedRealtime() - startedAt)
            val grade = result.state.lastReview?.grade ?: return@launch
            setView(view.copy(outcome = outcome, verdict = ReviewVerdict(outcome, grade, engine.intervals(prev, now))))
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
        if (view.step.kind == StepKind.REVIEW) return
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

private fun Map<Long, FsrsCard>.withCards(changed: List<FsrsCard>): Map<Long, FsrsCard> =
    if (changed.isEmpty()) this else this + changed.associateBy { it.wordId }

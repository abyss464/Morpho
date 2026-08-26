package dev.morpho.ui.review

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.data.haptics.HapticPattern
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.di.SessionKind
import dev.morpho.di.SessionResult
import dev.morpho.domain.learning.OptionAssembler
import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.model.ProgressDefaults
import dev.morpho.domain.progress.ProgressTracker
import dev.morpho.domain.review.ReviewItem
import dev.morpho.domain.review.ReviewQuestionType
import dev.morpho.domain.util.seedOf
import dev.morpho.ui.common.toWordDetail
import dev.morpho.ui.designsystem.component.FeedbackSignal
import dev.morpho.ui.designsystem.component.SpellState
import dev.morpho.ui.designsystem.component.TextOption
import dev.morpho.ui.designsystem.component.WordDetail
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate

data class ReviewQuestionUi(
    val wordId: Long,
    val type: ReviewQuestionType,
    val word: String,
    val phonetic: String?,
    val wordAudioFile: String,
    val definition: String,
    val definitionAudioFile: String,
    val pos: String,
    /** Definition-to-word only. */
    val wordOptions: List<TextOption> = emptyList(),
    val correctIndex: Int = 0,
)

data class ReviewUiState(
    val loading: Boolean = true,
    val finished: Boolean = false,
    val empty: Boolean = false,
    val question: ReviewQuestionUi? = null,
    val index: Int = 0,
    val total: Int = 0,
    val selectedIndex: Int? = null,
    val revealed: Boolean = false,
    val spelling: String = "",
    val spellState: SpellState = SpellState.TYPING,
    val feedback: FeedbackSignal = FeedbackSignal.None,
    val detail: WordDetail? = null,
    val nowPlayingFile: String? = null,
)

/**
 * Runs the due-review queue: FSRS decides what is due, [dev.morpho.domain.review.ReviewScheduler]
 * decides how to ask it, and every answer feeds a grade straight back into the card.
 */
class ReviewViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(ReviewUiState())
    val state: StateFlow<ReviewUiState> = _state.asStateFlow()

    private var queue: List<ReviewItem> = emptyList()
    private var cursor = 0
    private var correctCount = 0
    private var answeredCount = 0
    private var advancing = false

    init {
        viewModelScope.launch { start() }
        viewModelScope.launch {
            container.audioPlayer.nowPlaying.collect {
                _state.value = _state.value.copy(nowPlayingFile = it)
            }
        }
    }

    private suspend fun start() {
        val now = Instant.now()
        val cards = container.progressRepository.allCards()
        val goal = container.settingsRepository.settings.value.dailyGoal
        queue = container.reviewScheduler.buildQueue(
            cards = cards,
            now = now,
            daySeed = LocalDate.now().toEpochDay(),
            // Bounded, or a mature deck eventually opens a session nobody can finish.
            limit = goal * ProgressDefaults.REVIEW_SESSION_MULTIPLIER,
        )
        if (queue.isEmpty()) {
            _state.value = ReviewUiState(loading = false, finished = false, empty = true)
            return
        }
        cursor = 0
        render()
    }

    private suspend fun render() {
        val item = queue.getOrNull(cursor)
        if (item == null) {
            finish()
            return
        }
        val ui = buildQuestion(item)
        if (ui == null) {
            // Card points at a word this release does not ship — skip it silently
            // (README Part 6: dropped words leave the review queue via the join).
            cursor++
            render()
            return
        }
        _state.value = _state.value.copy(
            loading = false,
            question = ui,
            index = cursor,
            total = queue.size,
            selectedIndex = null,
            revealed = false,
            spelling = "",
            spellState = SpellState.TYPING,
            feedback = FeedbackSignal.None,
            detail = null,
        )
        if (ui.type == ReviewQuestionType.LISTENING_SPELL) {
            container.audioPlayer.play(ui.wordAudioFile)
        }
        queue.getOrNull(cursor + 1)?.let { next ->
            container.contentRepository.bundle(next.card.wordId)?.let {
                container.audioPlayer.preload(it.word.wordAudioFile)
            }
        }
    }

    private suspend fun buildQuestion(item: ReviewItem): ReviewQuestionUi? {
        val content = container.contentRepository.questionBundles(item.card.wordId) ?: return null
        val answer = content.answer
        val sense = answer.primarySense

        return when (item.type) {
            ReviewQuestionType.LISTENING_SPELL -> ReviewQuestionUi(
                wordId = answer.word.wordId,
                type = item.type,
                word = answer.word.word,
                phonetic = answer.word.phonetic,
                wordAudioFile = answer.word.wordAudioFile,
                definition = sense.definition,
                definitionAudioFile = sense.defAudioFile,
                pos = sense.pos,
            )

            ReviewQuestionType.DEFINITION_TO_WORD -> {
                val optionIds = OptionAssembler.assemble(
                    answerWordId = answer.word.wordId,
                    distractorIds = answer.distractorIds,
                    seed = seedOf(LocalDate.now().toEpochDay(), answer.word.wordId, 7L),
                )
                val byId = content.options.associateBy { it.word.wordId }
                val options = optionIds.mapNotNull { byId[it] }
                if (options.size != OptionAssembler.OPTION_COUNT) return null
                ReviewQuestionUi(
                    wordId = answer.word.wordId,
                    type = item.type,
                    word = answer.word.word,
                    phonetic = answer.word.phonetic,
                    wordAudioFile = answer.word.wordAudioFile,
                    definition = sense.definition,
                    definitionAudioFile = sense.defAudioFile,
                    pos = sense.pos,
                    wordOptions = options.map {
                        TextOption(it.word.wordId, it.word.word, serif = false)
                    },
                    correctIndex = optionIds.indexOf(answer.word.wordId),
                )
            }
        }
    }

    // --------------------------------------------------------------- answers

    fun onOptionSelected(index: Int) {
        if (advancing) return
        val ui = _state.value.question ?: return
        grade(ui, correct = index == ui.correctIndex, selectedIndex = index)
    }

    fun onSpellingChanged(value: String) {
        if (advancing) return
        _state.value = _state.value.copy(spelling = value)
    }

    fun onSpellingSubmitted() {
        if (advancing) return
        val ui = _state.value.question ?: return
        val typed = _state.value.spelling.trim()
        if (typed.isEmpty()) return
        grade(ui, correct = typed.equals(ui.word, ignoreCase = true), selectedIndex = null)
    }

    private fun grade(ui: ReviewQuestionUi, correct: Boolean, selectedIndex: Int?) {
        advancing = true
        answeredCount++
        if (correct) correctCount++

        container.playSfx(if (correct) SfxEvent.CORRECT else SfxEvent.WRONG)
        container.hapticsManager.perform(
            if (correct) HapticPattern.CORRECT else HapticPattern.WRONG,
        )

        _state.value = _state.value.copy(
            selectedIndex = selectedIndex,
            revealed = true,
            spellState = if (correct) SpellState.CORRECT else SpellState.WRONG,
            feedback = if (correct) FeedbackSignal.Correct else FeedbackSignal.Wrong,
        )

        viewModelScope.launch {
            val card: FsrsCard = queue[cursor].card
            val updated = container.reviewScheduler.applyAnswer(card, correct, Instant.now())
            container.progressRepository.upsertCards(listOf(updated))

            delay(FEEDBACK_HOLD_MS)
            _state.value = _state.value.copy(feedback = FeedbackSignal.None)

            if (!correct) {
                // A miss always shows the word in full before moving on.
                container.contentRepository.bundle(ui.wordId)?.let { bundle ->
                    container.audioPlayer.play(bundle.word.wordAudioFile)
                    _state.value = _state.value.copy(detail = bundle.toWordDetail())
                }
                return@launch
            }
            cursor++
            advancing = false
            render()
        }
    }

    fun onDetailDismissed() {
        _state.value = _state.value.copy(detail = null)
        viewModelScope.launch {
            cursor++
            advancing = false
            render()
        }
    }

    fun onPlayAudio(file: String?) = container.audioPlayer.play(file)

    fun onReplayWord() {
        _state.value.question?.let { container.audioPlayer.play(it.wordAudioFile) }
    }

    fun onExit() = container.audioPlayer.stop()

    private suspend fun finish() {
        val today = LocalDate.now()
        val progressRepo = container.progressRepository
        val merged = ProgressTracker.mergeSession(
            existing = progressRepo.statsFor(today),
            date = today,
            newLearned = 0,
            reviewed = answeredCount,
            correctAnswers = correctCount,
            totalAnswers = answeredCount,
        )
        progressRepo.upsertStats(merged)

        val streak = ProgressTracker.streak(progressRepo.recentStats(), today)
        container.sessionResults.publish(
            SessionResult(
                kind = SessionKind.REVIEW,
                newLearned = 0,
                reviewed = answeredCount,
                correctFirstTry = correctCount,
                totalFirstTry = answeredCount,
                streakDays = streak,
                goalMet = false,
            ),
        )
        container.playSfx(SfxEvent.REVIEW_DONE)
        _state.value = _state.value.copy(
            loading = false,
            finished = true,
            question = null,
            detail = null,
        )
    }

    override fun onCleared() {
        container.audioPlayer.stop()
        super.onCleared()
    }

    companion object {
        private const val FEEDBACK_HOLD_MS = 450L

        fun factory(container: AppContainer) = viewModelFactory {
            initializer { ReviewViewModel(container) }
        }
    }
}

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
import dev.morpho.domain.model.WordBundle
import dev.morpho.domain.progress.ProgressTracker
import dev.morpho.domain.review.ReviewItem
import dev.morpho.domain.util.seedOf
import dev.morpho.ui.common.toWordDetail
import dev.morpho.ui.designsystem.component.FeedbackSignal
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.SenseDetail
import dev.morpho.ui.designsystem.component.WordDetail
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate

/** UI model for a single review question — unified mode-2 visual layout. */
data class ReviewQuestionUi(
    val wordId: Long,
    val word: String,
    val phonetic: String?,
    val wordAudioFile: String,
    val imageOptions: List<ImageOption>,
    val correctIndex: Int,
    /** All senses of the answer word, for the wrong-answer retry help card. */
    val senses: List<SenseDetail>,
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
    /** True after a wrong answer: the user must pick the correct option to proceed. */
    val mustRetry: Boolean = false,
    val feedback: FeedbackSignal = FeedbackSignal.None,
    val detail: WordDetail? = null,
    val nowPlayingFile: String? = null,
)

/**
 * Runs the due-review queue using a unified image+definition grid (mode-2 visual)
 * for every card. FSRS decides what is due,
 * [dev.morpho.domain.review.ReviewScheduler] builds the queue, and every answer
 * feeds a grade straight back into the card.
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
            mustRetry = false,
            feedback = FeedbackSignal.None,
            detail = null,
        )
        // Auto-play word pronunciation on entry, same as learning mode 2.
        container.audioPlayer.play(ui.wordAudioFile)
        // Preload the next card's audio.
        queue.getOrNull(cursor + 1)?.let { next ->
            container.contentRepository.bundle(next.card.wordId)?.let {
                container.audioPlayer.preload(it.word.wordAudioFile)
            }
        }
    }

    private suspend fun buildQuestion(item: ReviewItem): ReviewQuestionUi? {
        val content = container.contentRepository.questionBundles(item.card.wordId) ?: return null
        val answer = content.answer

        val optionIds = OptionAssembler.assemble(
            answerWordId = answer.word.wordId,
            distractorIds = answer.distractorIds,
            seed = seedOf(LocalDate.now().toEpochDay(), answer.word.wordId, 7L),
        )
        val byId = content.options.associateBy { it.word.wordId }
        val ordered = optionIds.mapNotNull { byId[it] }
        if (ordered.size != OptionAssembler.OPTION_COUNT) return null

        return ReviewQuestionUi(
            wordId = answer.word.wordId,
            word = answer.word.word,
            phonetic = answer.word.phonetic,
            wordAudioFile = answer.word.wordAudioFile,
            imageOptions = ordered.mapIndexed { index, bundle ->
                bundle.toImageOption(index, ordered.size)
            },
            correctIndex = optionIds.indexOf(answer.word.wordId),
            senses = answer.senses.map { sense ->
                SenseDetail(
                    pos = sense.pos,
                    definition = sense.definition,
                    isPrimary = sense.isPrimary,
                    audioFile = sense.defAudioFile,
                )
            },
        )
    }

    private fun WordBundle.toImageOption(index: Int, total: Int) = ImageOption(
        wordId = word.wordId,
        imageFile = word.imageFile,
        caption = primarySense.definition,
        accessibilityLabel = "Option ${index + 1} of $total: ${primarySense.definition}",
    )

    // --------------------------------------------------------------- answers

    fun onOptionSelected(index: Int) {
        if (advancing) return
        val ui = _state.value.question ?: return
        val correct = index == ui.correctIndex
        val hadRetry = _state.value.mustRetry

        if (!correct) {
            // First-try miss: count it and grade the FSRS card.
            if (!hadRetry) {
                answeredCount++
                viewModelScope.launch {
                    val card: FsrsCard = queue[cursor].card
                    val updated = container.reviewScheduler.applyAnswer(card, false, Instant.now())
                    container.progressRepository.upsertCards(listOf(updated))
                }
            }

            container.playSfx(SfxEvent.WRONG)
            container.hapticsManager.perform(HapticPattern.WRONG)
            // Play word audio on the first miss only.
            if (!hadRetry) {
                container.audioPlayer.play(ui.wordAudioFile)
            }
            _state.value = _state.value.copy(
                selectedIndex = index,
                revealed = true,
                mustRetry = true,
                feedback = FeedbackSignal.Wrong,
            )
            viewModelScope.launch {
                delay(container.tokenDurations.flash.toLong())
                _state.value = _state.value.copy(feedback = FeedbackSignal.None)
            }
            return
        }

        // Correct tap.
        if (!hadRetry) {
            answeredCount++
            correctCount++
            viewModelScope.launch {
                val card: FsrsCard = queue[cursor].card
                val updated = container.reviewScheduler.applyAnswer(card, true, Instant.now())
                container.progressRepository.upsertCards(listOf(updated))
            }
        }

        container.playSfx(SfxEvent.CORRECT)
        container.hapticsManager.perform(HapticPattern.CORRECT)

        _state.value = _state.value.copy(
            selectedIndex = index,
            revealed = true,
            mustRetry = false,
            feedback = FeedbackSignal.Correct,
        )

        advancing = true
        viewModelScope.launch {
            delay(FEEDBACK_HOLD_MS)
            _state.value = _state.value.copy(feedback = FeedbackSignal.None)

            if (hadRetry) {
                // Wrong-then-correct: show detail sheet before advancing.
                container.contentRepository.bundle(ui.wordId)?.let { bundle ->
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

/** Durations live in the Compose token layer; the view model needs the raw numbers. */
private val AppContainer.tokenDurations: dev.morpho.ui.designsystem.token.Durations
    get() = dev.morpho.ui.designsystem.token.Durations()

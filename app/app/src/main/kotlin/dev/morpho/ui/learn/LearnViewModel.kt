package dev.morpho.ui.learn

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.data.haptics.HapticPattern
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.di.SessionKind
import dev.morpho.di.SessionResult
import dev.morpho.domain.learning.AnswerOutcome
import dev.morpho.domain.learning.LearningEngine
import dev.morpho.domain.learning.LearningSessionState
import dev.morpho.domain.learning.OptionAssembler
import dev.morpho.domain.learning.Question
import dev.morpho.domain.learning.SessionConfig
import dev.morpho.domain.model.LearnMode
import dev.morpho.domain.model.WordBundle
import dev.morpho.domain.progress.ProgressTracker
import dev.morpho.ui.common.toWordDetail
import dev.morpho.ui.designsystem.component.FeedbackSignal
import dev.morpho.ui.designsystem.component.GroupSegmentState
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.TextOption
import dev.morpho.ui.designsystem.component.WordDetail
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate

/** Everything the Learn screen renders for the current question. */
data class QuestionUi(
    val wordId: Long,
    val mode: LearnMode,
    val word: String,
    val phonetic: String?,
    val wordAudioFile: String,
    val sentence: String? = null,
    val highlight: IntRange = IntRange.EMPTY,
    val sentenceAudioFile: String? = null,
    val imageOptions: List<ImageOption> = emptyList(),
    val textOptions: List<TextOption> = emptyList(),
    val correctIndex: Int,
    val primaryDefinition: String,
)

data class LearnUiState(
    val loading: Boolean = true,
    val finished: Boolean = false,
    val empty: Boolean = false,
    val question: QuestionUi? = null,
    val selectedIndex: Int? = null,
    val revealed: Boolean = false,
    val mustRetry: Boolean = false,
    val feedback: FeedbackSignal = FeedbackSignal.None,
    val detail: WordDetail? = null,
    val detailContinueLabel: String? = null,
    val round: Int = 1,
    val roundTotal: Int = 3,
    val unitIndex: Int = 0,
    val unitCount: Int = 0,
    val groupSegments: List<GroupSegmentState> = emptyList(),
    val nowPlayingFile: String? = null,
)

/**
 * Drives a learning session: asks [LearningEngine] what to show next, loads the
 * content for it, and turns each answer into progress writes plus the sound, haptic
 * and motion signals the design contract specifies.
 */
class LearnViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(LearnUiState())
    val state: StateFlow<LearnUiState> = _state.asStateFlow()

    private var session: LearningSessionState? = null
    private var config: SessionConfig = SessionConfig()
    private var advancing = false

    init {
        viewModelScope.launch { startSession() }
        viewModelScope.launch {
            container.audioPlayer.nowPlaying.collect { file ->
                _state.value = _state.value.copy(nowPlayingFile = file)
            }
        }
    }

    private suspend fun startSession() {
        val settings = container.settingsRepository.settings.value
        val content = container.contentRepository
        val progressRepo = container.progressRepository

        val today = LocalDate.now()
        val doneToday = progressRepo.statsFor(today)?.newLearned ?: 0
        val quota = (settings.dailyGoal - doneToday).coerceAtLeast(0)

        config = SessionConfig(
            dailyGoal = settings.dailyGoal,
            // One session seed per calendar day keeps shuffles stable if the user
            // leaves and comes back, while still varying day to day.
            sessionSeed = today.toEpochDay(),
        )

        val progress = progressRepo.progressMap()
        val plan = LearningEngine.buildSession(
            plan = content.planWords(),
            progress = progress,
            dueReviewIds = emptyList(),
            newWordQuota = if (quota == 0) settings.dailyGoal else quota,
            config = config,
        )

        if (plan.units.isEmpty()) {
            _state.value = LearnUiState(loading = false, finished = true, empty = true)
            return
        }

        session = LearningEngine.startSession(plan, progress, config)
        renderCurrent()
    }

    // ------------------------------------------------------------- rendering

    private suspend fun renderCurrent() {
        val state = session ?: return
        val question = state.currentQuestion
        if (question == null) {
            finish()
            return
        }
        val ui = buildQuestionUi(question) ?: run {
            // Content is missing for this word (should be impossible in a valid
            // release). Skip it rather than trapping the user.
            session = LearningEngine.submitAnswer(state, correct = true).state
            renderCurrent()
            return
        }

        _state.value = _state.value.copy(
            loading = false,
            finished = false,
            question = ui,
            selectedIndex = null,
            revealed = false,
            mustRetry = false,
            feedback = FeedbackSignal.None,
            detail = null,
            round = question.round,
            roundTotal = config.roundsRequired,
            unitIndex = question.unitIndex,
            unitCount = state.units.size,
            groupSegments = segmentsFor(state),
        )

        // Modes 2 and 3 lead with the word; mode 1 waits until the answer is revealed
        // so the picture, not the voice, carries the question.
        if (ui.mode != LearnMode.SENTENCE_IMAGE) {
            container.audioPlayer.play(ui.wordAudioFile)
        }
        preloadNext(state)
    }

    private suspend fun buildQuestionUi(question: Question): QuestionUi? {
        val content = container.contentRepository.questionBundles(question.wordId) ?: return null
        val answer = content.answer
        val optionIds = OptionAssembler.assemble(
            answerWordId = question.wordId,
            distractorIds = answer.distractorIds,
            seed = question.optionSeed(config.sessionSeed),
        )
        val byId = content.options.associateBy { it.word.wordId }
        val ordered = optionIds.mapNotNull { byId[it] }
        if (ordered.size != OptionAssembler.OPTION_COUNT) return null
        val correctIndex = optionIds.indexOf(question.wordId)
        val example = answer.mode1Example

        return QuestionUi(
            wordId = question.wordId,
            mode = question.mode,
            word = answer.word.word,
            phonetic = answer.word.phonetic,
            wordAudioFile = answer.word.wordAudioFile,
            sentence = example?.sentence,
            highlight = example?.highlightCharRange() ?: IntRange.EMPTY,
            sentenceAudioFile = example?.exAudioFile,
            imageOptions = if (question.mode == LearnMode.WORD_TEXT_DEF) {
                emptyList()
            } else {
                ordered.mapIndexed { index, bundle -> bundle.toImageOption(index, ordered.size, question.mode) }
            },
            textOptions = if (question.mode == LearnMode.WORD_TEXT_DEF) {
                ordered.map { TextOption(it.word.wordId, it.primarySense.definition, it.primarySense.pos) }
            } else {
                emptyList()
            },
            correctIndex = correctIndex,
            primaryDefinition = answer.primarySense.definition,
        )
    }

    private fun WordBundle.toImageOption(index: Int, total: Int, mode: LearnMode) = ImageOption(
        wordId = word.wordId,
        imageFile = word.imageFile,
        caption = if (mode == LearnMode.WORD_IMAGE_DEF) primarySense.definition else null,
        accessibilityLabel = if (mode == LearnMode.WORD_IMAGE_DEF) {
            "Option ${index + 1} of $total: ${primarySense.definition}"
        } else {
            "Option ${index + 1} of $total"
        },
    )

    private fun segmentsFor(state: LearningSessionState): List<GroupSegmentState> {
        val currentId = state.currentQuestion?.wordId
        return state.unitProgress().map { card ->
            when {
                card.retired -> GroupSegmentState.PASSED
                card.wordId == currentId -> GroupSegmentState.IN_PROGRESS
                card.carried -> GroupSegmentState.CARRIED
                card.roundsPassed > 0 -> GroupSegmentState.IN_PROGRESS
                else -> GroupSegmentState.PENDING
            }
        }
    }

    private suspend fun preloadNext(state: LearningSessionState) {
        val runtime = state.current ?: return
        val nextId = runtime.order.getOrNull(runtime.cursor + 1) ?: return
        val bundle = container.contentRepository.bundle(nextId) ?: return
        val card = runtime.cards[nextId]
        val file = if (card?.mode == LearnMode.SENTENCE_IMAGE) {
            bundle.mode1Example?.exAudioFile
        } else {
            bundle.word.wordAudioFile
        }
        container.audioPlayer.preload(file)
    }

    // --------------------------------------------------------------- answers

    fun onOptionSelected(index: Int) {
        if (advancing) return
        val ui = _state.value.question ?: return
        val current = session ?: return
        val correct = index == ui.correctIndex
        val hadRetry = _state.value.mustRetry

        val result = LearningEngine.submitAnswer(current, correct)
        session = result.state
        viewModelScope.launch {
            container.progressRepository.upsertProgress(result.progressUpdates)
        }

        if (!correct) {
            container.playSfx(SfxEvent.WRONG)
            container.hapticsManager.perform(HapticPattern.WRONG)
            _state.value = _state.value.copy(
                selectedIndex = index,
                revealed = true,
                mustRetry = true,
                feedback = FeedbackSignal.Wrong,
                groupSegments = segmentsFor(result.state),
            )
            viewModelScope.launch {
                delay(container.tokenDurations.flash.toLong())
                _state.value = _state.value.copy(feedback = FeedbackSignal.None)
            }
            return
        }

        onCorrect(index, ui, result.outcome, hadRetry)
    }

    private fun onCorrect(
        index: Int,
        ui: QuestionUi,
        outcome: AnswerOutcome,
        hadRetry: Boolean,
    ) {
        container.playSfx(if (outcome.promoted) SfxEvent.PROMOTE else SfxEvent.CORRECT)
        container.hapticsManager.perform(
            if (outcome.promoted) HapticPattern.PROMOTE else HapticPattern.CORRECT,
        )

        _state.value = _state.value.copy(
            selectedIndex = index,
            revealed = true,
            mustRetry = false,
            feedback = when {
                outcome.promoted -> FeedbackSignal.Promoted(outcome.newMode.level)
                else -> FeedbackSignal.Correct
            },
        )

        // Mode 1 reveals the sentence audio only after the answer is in.
        if (ui.mode == LearnMode.SENTENCE_IMAGE) {
            container.audioPlayer.play(ui.sentenceAudioFile ?: ui.wordAudioFile)
        }

        advancing = true
        viewModelScope.launch {
            delay(container.tokenDurations.correct.toLong())
            _state.value = _state.value.copy(feedback = FeedbackSignal.None)

            when {
                // The word graduated, or the user needed a retry: show the detail sheet.
                // README Part 1: "必须选对后查看详情页".
                outcome.wordLearned || hadRetry -> showDetail(ui.wordId, outcome)

                outcome.unitCompleted -> celebrateGroup(outcome)
                else -> {
                    advancing = false
                    renderCurrent()
                }
            }
        }
    }

    private suspend fun showDetail(wordId: Long, outcome: AnswerOutcome) {
        val bundle = container.contentRepository.bundle(wordId)
        if (bundle == null) {
            advancing = false
            renderCurrent()
            return
        }
        pendingOutcome = outcome
        _state.value = _state.value.copy(
            detail = bundle.toWordDetail(),
            detailContinueLabel = "Continue",
        )
    }

    private var pendingOutcome: AnswerOutcome? = null

    fun onDetailDismissed() {
        val outcome = pendingOutcome
        pendingOutcome = null
        _state.value = _state.value.copy(detail = null)
        viewModelScope.launch {
            if (outcome?.unitCompleted == true) {
                celebrateGroup(outcome)
            } else {
                advancing = false
                renderCurrent()
            }
        }
    }

    private suspend fun celebrateGroup(outcome: AnswerOutcome) {
        container.playSfx(SfxEvent.GROUP_COMPLETE)
        container.hapticsManager.perform(HapticPattern.GROUP_COMPLETE)
        _state.value = _state.value.copy(
            feedback = FeedbackSignal.GroupComplete("Group complete"),
        )
        if (outcome.sessionCompleted) {
            // The celebration overlay drives onCelebrationFinished, which finishes.
            return
        }
    }

    fun onCelebrationFinished() {
        _state.value = _state.value.copy(feedback = FeedbackSignal.None)
        viewModelScope.launch {
            advancing = false
            renderCurrent()
        }
    }

    fun onPlayAudio(file: String?) {
        container.audioPlayer.play(file)
    }

    fun onExit() {
        session?.let { session = LearningEngine.abandon(it) }
        container.audioPlayer.stop()
    }

    // ---------------------------------------------------------------- finish

    private suspend fun finish() {
        val state = session ?: return
        val progressRepo = container.progressRepository
        val today = LocalDate.now()
        val now = Instant.now()

        // Every graduated word gets its first FSRS card, dated from this moment.
        val newCards = state.learnedThisSession
            .filter { progressRepo.card(it) == null }
            .map { container.reviewScheduler.newCardFor(it, now) }
        progressRepo.upsertCards(newCards)

        val merged = ProgressTracker.mergeSession(
            existing = progressRepo.statsFor(today),
            date = today,
            newLearned = state.stats.learned,
            reviewed = 0,
            correctAnswers = state.stats.correctFirstTry,
            totalAnswers = state.stats.firstTryTotal,
        )
        progressRepo.upsertStats(merged)

        val streak = ProgressTracker.streak(progressRepo.recentStats(), today)
        val goal = container.settingsRepository.settings.value.dailyGoal
        container.sessionResults.publish(
            SessionResult(
                kind = SessionKind.LEARNING,
                newLearned = state.stats.learned,
                reviewed = 0,
                correctFirstTry = state.stats.correctFirstTry,
                totalFirstTry = state.stats.firstTryTotal,
                streakDays = streak,
                goalMet = merged.newLearned >= goal,
            ),
        )
        if (streak > 0) container.playSfx(SfxEvent.STREAK)

        _state.value = _state.value.copy(
            loading = false,
            finished = true,
            question = null,
            detail = null,
            feedback = FeedbackSignal.None,
        )
    }

    override fun onCleared() {
        container.audioPlayer.stop()
        super.onCleared()
    }

    companion object {
        fun factory(container: AppContainer) = viewModelFactory {
            initializer { LearnViewModel(container) }
        }
    }
}

/** Durations live in the Compose token layer; the view model needs the raw numbers. */
private val AppContainer.tokenDurations: dev.morpho.ui.designsystem.token.Durations
    get() = dev.morpho.ui.designsystem.token.Durations()

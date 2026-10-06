package dev.morpho.ui.word

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.R
import dev.morpho.data.sound.SfxEvent
import dev.morpho.data.stream.StreamSnapshot
import dev.morpho.data.stream.WordNote
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.WordBundle
import dev.morpho.ui.designsystem.component.MorphoLoader
import dev.morpho.ui.designsystem.theme.MorphoTheme
import dev.morpho.ui.stream.StageMark
import dev.morpho.ui.stream.WordCard
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** A looked-up word: its card, where it sits, and whether it is in review. */
data class LookedUp(
    val word: WordBundle,
    val unit: Int,
    val inReview: Boolean,
    val note: WordNote?,
)

/** Loads one word for reading. Reads only: looking a word up never changes the stream. */
class WordViewModel(private val container: AppContainer, private val wordId: Long) : ViewModel() {

    private val _word = MutableStateFlow<LookedUp?>(null)
    val word: StateFlow<LookedUp?> = _word.asStateFlow()

    init {
        viewModelScope.launch {
            val bundle = container.contentRepository.bundle(wordId) ?: return@launch
            val position = container.contentRepository.wordIndex().indexOfFirst { it.wordId == wordId }
            _word.value = LookedUp(
                word = bundle,
                unit = position.coerceAtLeast(0) / StreamSnapshot.UNIT_SIZE + 1,
                inReview = container.progressRepository.allCards().any { it.wordId == wordId },
                note = container.streamStore.notes()[wordId],
            )
        }
    }

    fun readAloud(word: WordBundle) {
        container.audioPlayer.playSequence(
            listOf(word.word.wordAudioFile, word.primarySense.defAudioFile, word.cardExample?.exAudioFile),
        )
    }

    companion object {
        fun factory(container: AppContainer, wordId: Long) = viewModelFactory {
            initializer { WordViewModel(container, wordId) }
        }
    }
}

/**
 * A word opened from "Look up a word" (docs/contracts/stream.md §7): the full word card,
 * read only, with a way back to Today.
 */
@Composable
fun WordScreen(
    container: AppContainer,
    wordId: Long,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: WordViewModel = viewModel(key = "word-$wordId", factory = WordViewModel.factory(container, wordId))
    val looked by viewModel.word.collectAsStateWithLifecycle()
    val nowPlaying by container.audioPlayer.nowPlaying.collectAsStateWithLifecycle()
    DisposableEffect(Unit) { onDispose { container.audioPlayer.stop() } }

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        IconButton(
            onClick = {
                container.playSfx(SfxEvent.TAP)
                onBack()
            },
            modifier = Modifier.padding(start = MorphoTheme.spacing.xxs),
        ) {
            Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = stringResource(R.string.action_back))
        }
        val shown = looked
        if (shown == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { MorphoLoader() }
            return@Column
        }
        WordCard(
            word = shown.word,
            mark = if (shown.inReview) StageMark.REVIEW else StageMark.NEW,
            label = if (shown.inReview) {
                stringResource(R.string.word_in_review, shown.unit)
            } else {
                stringResource(R.string.today_unit, shown.unit)
            },
            note = shown.note,
            playing = nowPlaying == shown.word.word.wordAudioFile,
            onPlay = { viewModel.readAloud(shown.word) },
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = MorphoTheme.spacing.screenGutter)
                .padding(bottom = MorphoTheme.spacing.xl),
        )
    }
}

package dev.morpho.ui.designsystem.component

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** Result state of a spelling attempt. */
enum class SpellState { TYPING, CORRECT, WRONG }

/**
 * Letter-box input for the listening-spell review question.
 *
 * One box per letter of the target word. A hidden [BasicTextField] captures the
 * keyboard so the visible boxes stay purely presentational; each newly typed
 * character fades and lifts into place, and a wrong answer shakes the whole row and
 * reveals the correct spelling letter by letter.
 */
@Composable
fun SpellInput(
    target: String,
    value: String,
    onValueChange: (String) -> Unit,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
    state: SpellState = SpellState.TYPING,
    enabled: Boolean = true,
    revealAnswer: Boolean = false,
) {
    val tokens = MorphoTheme.tokens
    val accents = MorphoTheme.accents
    val reducedMotion = MorphoTheme.reducedMotion
    val focusRequester = remember { FocusRequester() }
    val amplitude = with(LocalDensity.current) { tokens.sizes.shakeAmplitude.toPx() }
    val shake = remember { floatAnimatable() }

    LaunchedEffect(state) {
        if (state == SpellState.WRONG && !reducedMotion) {
            shake.runShake(amplitude, tokens.durations.shake)
        }
    }
    LaunchedEffect(enabled) {
        if (enabled) runCatching { focusRequester.requestFocus() }
    }

    Column(
        modifier = modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
    ) {
        FlowRow(
            modifier = Modifier
                .fillMaxWidth()
                .graphicsLayer { translationX = shake.value }
                .semantics {
                    contentDescription = "Spell the word you heard, ${target.length} letters"
                },
            horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xxs, Alignment.CenterHorizontally),
            verticalArrangement = Arrangement.spacedBy(tokens.spacing.xxs),
        ) {
            target.forEachIndexed { index, expected ->
                val typed = value.getOrNull(index)
                val shown = when {
                    revealAnswer && typed == null -> expected
                    revealAnswer && typed?.equals(expected, ignoreCase = true) != true -> expected
                    else -> typed
                }
                LetterBox(
                    char = shown,
                    index = index,
                    state = state,
                    revealed = revealAnswer,
                    matches = typed?.equals(expected, ignoreCase = true) == true,
                    cursor = enabled && state == SpellState.TYPING && index == value.length,
                    borderColor = when (state) {
                        SpellState.CORRECT -> MaterialTheme.colorScheme.primary
                        SpellState.WRONG -> accents.wrong
                        SpellState.TYPING -> MaterialTheme.colorScheme.outlineVariant
                    },
                )
            }
        }

        // The real input: invisible, but the sole owner of the IME.
        BasicTextField(
            value = remember(value) { TextFieldValue(value, androidx.compose.ui.text.TextRange(value.length)) },
            onValueChange = { field ->
                val filtered = field.text.filter { it.isLetter() || it == '-' || it == '\'' }
                onValueChange(filtered.take(target.length))
            },
            enabled = enabled,
            singleLine = true,
            keyboardOptions = KeyboardOptions(
                keyboardType = KeyboardType.Ascii,
                imeAction = ImeAction.Done,
                autoCorrectEnabled = false,
                capitalization = KeyboardCapitalization.None,
            ),
            keyboardActions = KeyboardActions(onDone = { onSubmit() }),
            cursorBrush = androidx.compose.ui.graphics.SolidColor(Color.Transparent),
            textStyle = MorphoTheme.reading.spellLetter.copy(color = Color.Transparent),
            modifier = Modifier
                .size(1.dp)
                .alpha(0f)
                .focusRequester(focusRequester)
                .focusable(enabled),
        )
    }
}

@Composable
private fun LetterBox(
    char: Char?,
    index: Int,
    state: SpellState,
    revealed: Boolean,
    matches: Boolean,
    cursor: Boolean,
    borderColor: Color,
) {
    val tokens = MorphoTheme.tokens
    val accents = MorphoTheme.accents
    val appear by animateFloatAsState(
        targetValue = if (char != null) 1f else 0f,
        animationSpec = tween(
            durationMillis = tokens.durations.fade,
            delayMillis = if (revealed) index * 40 else 0,
            easing = tokens.easings.standard,
        ),
        label = "letter-appear",
    )
    val textColor = when {
        state == SpellState.CORRECT -> MaterialTheme.colorScheme.primary
        revealed && !matches -> accents.wrong
        else -> MaterialTheme.colorScheme.onSurface
    }
    Box(
        modifier = Modifier
            .width(tokens.sizes.spellBox)
            .height(tokens.sizes.spellBoxTall)
            .clip(tokens.radii.shapeXs)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .border(
                width = if (cursor) 2.dp else 1.dp,
                color = if (cursor) MaterialTheme.colorScheme.primary else borderColor,
                shape = tokens.radii.shapeXs,
            ),
        contentAlignment = Alignment.Center,
    ) {
        if (char != null) {
            Text(
                text = char.lowercaseChar().toString(),
                style = MorphoTheme.reading.spellLetter,
                color = textColor,
                textAlign = TextAlign.Center,
                modifier = Modifier.graphicsLayer {
                    alpha = appear
                    translationY = (1f - appear) * 8f
                },
            )
        }
    }
}

@ThemePreviews
@Composable
private fun SpellInputPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.lg)) {
            SpellInput(
                target = "benevolent",
                value = "bene",
                onValueChange = {},
                onSubmit = {},
                enabled = false,
            )
            SpellInput(
                target = "benevolent",
                value = "benevolent",
                onValueChange = {},
                onSubmit = {},
                state = SpellState.CORRECT,
                enabled = false,
            )
            SpellInput(
                target = "benevolent",
                value = "benevalent",
                onValueChange = {},
                onSubmit = {},
                state = SpellState.WRONG,
                revealAnswer = true,
                enabled = false,
            )
        }
    }
}

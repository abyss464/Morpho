package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import dev.morpho.domain.content.GlossIndex
import dev.morpho.domain.content.GlossMatch
import dev.morpho.ui.designsystem.theme.MorphoTheme
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * The release's `gloss_anchors` table, in whatever form the current screen has it.
 *
 * Defaults to [GlossIndex.EMPTY] so every `@Preview` and every component test renders
 * plain English with no wiring — exactly like [LocalContentImageRenderer].
 */
val LocalGlossIndex = staticCompositionLocalOf { GlossIndex.EMPTY }

/**
 * How a glossed token gives up its Chinese.
 *
 * The distinction exists for one reason: a quiz option is a single answer button, and a
 * tap landing on a word inside it must never be ambiguous about whether it selected the
 * option. So option text takes [LongPress] — the tap keeps its one meaning, and the
 * gloss lives on a gesture the answer flow does not use. Everywhere the text is not
 * itself an answer (detail-sheet definitions, example sentences) [Tap] is right, because
 * a hint you have to discover by long-pressing is a hint nobody finds.
 */
enum class GlossTrigger { Tap, LongPress }

/**
 * English text whose [gloss_anchors][LocalGlossIndex] tokens are subtly marked and
 * reveal a Chinese gloss on demand.
 *
 * The tint plus underline is deliberately quiet — the reader should be able to run
 * straight past it, because the product's whole premise is that the English carries the
 * meaning and Chinese is a grounding modality of last resort, no different from the
 * picture. Nothing appears on screen until it is asked for.
 *
 * @param onPlainTap what a tap that missed every anchor should do — play the sentence,
 *   select the option, or nothing at all.
 * @param pressInteractionSource the enclosing card's interaction source. Text with a
 *   gesture detector on it swallows the pointer, so without this the card would stop
 *   showing its press animation wherever the words are.
 */
@Composable
fun GlossedText(
    text: AnnotatedString,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    trigger: GlossTrigger = GlossTrigger.Tap,
    enabled: Boolean = true,
    onPlainTap: (() -> Unit)? = null,
    pressInteractionSource: MutableInteractionSource? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
    textAlign: TextAlign? = null,
) {
    val index = LocalGlossIndex.current
    val anchorTint = MaterialTheme.colorScheme.tertiary
    val matches = remember(text, index) { index.scan(text.text) }
    val annotated = remember(text, matches, anchorTint) {
        if (matches.isEmpty()) text else text.withAnchorMarks(matches, anchorTint)
    }

    // Nothing to reveal and nothing to intercept: fall back to a plain Text so the
    // overwhelming majority of definitions cost exactly what they did before.
    if (matches.isEmpty() && onPlainTap == null) {
        Text(
            text = annotated,
            style = style,
            color = color,
            modifier = modifier,
            maxLines = maxLines,
            overflow = overflow,
            textAlign = textAlign,
        )
        return
    }

    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    var shown by remember { mutableStateOf<ShownGloss?>(null) }

    // Callers build this lambda inline (`onPlainTap = { onSelect(index) }`), so its
    // identity changes on every recomposition — and recompositions come thick and fast
    // during the answer reveal. Keying the gesture detector on it would restart the
    // detector mid-press and swallow the long press that was already underway.
    val plainTap by rememberUpdatedState(onPlainTap)

    fun anchorAt(position: Offset): ShownGloss? {
        val result = layout ?: return null
        if (matches.isEmpty()) return null
        val offset = result.getOffsetForPosition(position)
        val match = matches.firstOrNull { offset in it } ?: return null
        return ShownGloss(match, result.boundsOf(match))
    }

    Box(modifier) {
        Text(
            text = annotated,
            style = style,
            color = color,
            maxLines = maxLines,
            overflow = overflow,
            textAlign = textAlign,
            onTextLayout = { layout = it },
            modifier = Modifier.pointerInput(matches, trigger, enabled) {
                detectTapGestures(
                    onPress = { position ->
                        val source = pressInteractionSource
                        if (source == null || !enabled) {
                            tryAwaitRelease()
                        } else {
                            val press = PressInteraction.Press(position)
                            source.emit(press)
                            val released = tryAwaitRelease()
                            source.emit(
                                if (released) {
                                    PressInteraction.Release(press)
                                } else {
                                    PressInteraction.Cancel(press)
                                },
                            )
                        }
                    },
                    onLongPress = if (trigger == GlossTrigger.LongPress) {
                        { position -> anchorAt(position)?.let { shown = it } }
                    } else {
                        null
                    },
                    onTap = { position ->
                        val hit = if (trigger == GlossTrigger.Tap) anchorAt(position) else null
                        when {
                            hit != null -> shown = hit
                            enabled -> plainTap?.invoke()
                        }
                    },
                )
            },
        )

        shown?.let { gloss ->
            GlossPopover(gloss = gloss, onDismiss = { shown = null })
        }
    }
}

/** [String] overload for the common case of unstyled text. */
@Composable
fun GlossedText(
    text: String,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    trigger: GlossTrigger = GlossTrigger.Tap,
    enabled: Boolean = true,
    onPlainTap: (() -> Unit)? = null,
    pressInteractionSource: MutableInteractionSource? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
    textAlign: TextAlign? = null,
) = GlossedText(
    text = remember(text) { AnnotatedString(text) },
    style = style,
    color = color,
    modifier = modifier,
    trigger = trigger,
    enabled = enabled,
    onPlainTap = onPlainTap,
    pressInteractionSource = pressInteractionSource,
    maxLines = maxLines,
    overflow = overflow,
    textAlign = textAlign,
)

/** The token the user asked about, plus where it sits inside its text block. */
private data class ShownGloss(val match: GlossMatch, val bounds: Rect)

/**
 * Plain-tooltip styling from M3: inverse surface, extra-small corner, one line of
 * content. It reads as chrome floating over the page rather than as part of the
 * definition, which is the point — the English underneath stays the text.
 */
@Composable
private fun GlossPopover(gloss: ShownGloss, onDismiss: () -> Unit) {
    val gap = with(androidx.compose.ui.platform.LocalDensity.current) { GLOSS_POPOVER_GAP.roundToPx() }
    Popup(
        popupPositionProvider = remember(gloss.bounds, gap) {
            GlossPositionProvider(gloss.bounds, gap)
        },
        onDismissRequest = onDismiss,
        properties = PopupProperties(
            focusable = true,
            dismissOnBackPress = true,
            dismissOnClickOutside = true,
        ),
    ) {
        Surface(
            color = MaterialTheme.colorScheme.inverseSurface,
            contentColor = MaterialTheme.colorScheme.inverseOnSurface,
            shape = MorphoTheme.radii.shapeXs,
            tonalElevation = MorphoTheme.elevations.floating,
            shadowElevation = MorphoTheme.elevations.floating,
        ) {
            Column(
                modifier = Modifier
                    .widthIn(max = GLOSS_POPOVER_MAX_WIDTH)
                    .padding(
                        horizontal = MorphoTheme.spacing.sm,
                        vertical = MorphoTheme.spacing.xs,
                    ),
            ) {
                Text(
                    text = gloss.match.lemma,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.inverseOnSurface.copy(alpha = 0.7f),
                )
                Text(
                    text = gloss.match.gloss,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
}

/**
 * Centres the popover on the token and puts it above, flipping below when the token is
 * near the top of the window. Coordinates arrive in window space, so [anchor] — which is
 * local to the text block — is offset by the block's own position.
 */
private class GlossPositionProvider(
    private val anchor: Rect,
    private val gap: Int,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val centreX = anchorBounds.left + anchor.center.x.roundToInt()
        val x = (centreX - popupContentSize.width / 2)
            .coerceIn(0, max(0, windowSize.width - popupContentSize.width))

        val above = anchorBounds.top + anchor.top.roundToInt() - popupContentSize.height - gap
        val below = anchorBounds.top + anchor.bottom.roundToInt() + gap
        val y = if (above >= 0) {
            above
        } else {
            below.coerceIn(0, max(0, windowSize.height - popupContentSize.height))
        }
        return IntOffset(x, y)
    }
}

/**
 * Marks each anchor with a tint and a hairline underline, on top of whatever styling
 * the caller already applied (the mode-1 highlight pill, for one).
 */
private fun AnnotatedString.withAnchorMarks(
    matches: List<GlossMatch>,
    tint: Color,
): AnnotatedString = buildAnnotatedString {
    append(this@withAnchorMarks)
    matches.forEach { match ->
        addStyle(
            SpanStyle(color = tint, textDecoration = TextDecoration.Underline),
            match.start,
            match.endExclusive,
        )
    }
}

/** The token's box, or its first line's box when the token wrapped across lines. */
private fun TextLayoutResult.boundsOf(match: GlossMatch): Rect {
    val first = getBoundingBox(match.start.coerceIn(0, layoutInput.text.length - 1))
    val lastIndex = (match.endExclusive - 1).coerceIn(0, layoutInput.text.length - 1)
    val last = getBoundingBox(lastIndex)
    return if (getLineForOffset(match.start) == getLineForOffset(lastIndex)) {
        Rect(first.left, first.top, last.right, last.bottom)
    } else {
        first
    }
}

private val GLOSS_POPOVER_GAP: Dp = 6.dp
private val GLOSS_POPOVER_MAX_WIDTH: Dp = 220.dp

// ------------------------------------------------------------------ previews

/** A stand-in index so previews show the affordance without opening release.db. */
internal val previewGlossIndex = GlossIndex.of(
    listOf(
        dev.morpho.domain.content.GlossAnchor(1, "pasture", "牧场"),
        dev.morpho.domain.content.GlossAnchor(2, "intricate", "错综复杂的"),
        dev.morpho.domain.content.GlossAnchor(3, "thwart", "挫败"),
    ),
)

@ThemePreviews
@Composable
private fun GlossedTextPreview() {
    androidx.compose.runtime.CompositionLocalProvider(LocalGlossIndex provides previewGlossIndex) {
        PreviewBox {
            Column(
                verticalArrangement = androidx.compose.foundation.layout.Arrangement
                    .spacedBy(MorphoTheme.spacing.md),
            ) {
                GlossedText(
                    text = "an intricate network of paths crossing the open pasture",
                    style = MorphoTheme.reading.definition,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                GlossedText(
                    text = "a plan meant to thwart a rival before it can begin",
                    style = MorphoTheme.reading.definitionOption,
                    color = MaterialTheme.colorScheme.onSurface,
                    trigger = GlossTrigger.LongPress,
                    onPlainTap = {},
                )
                GlossedText(
                    text = "nothing here is anchored, so this renders as plain English",
                    style = MorphoTheme.reading.definition,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

package dev.morpho.ui.designsystem.component

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
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextDecoration
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
 * English text whose [gloss_anchors][LocalGlossIndex] tokens are subtly marked and
 * reveal a Chinese gloss when tapped.
 *
 * The tint plus underline is deliberately quiet — the reader should be able to run
 * straight past it, because the product's whole premise is that the English carries the
 * meaning and Chinese is a grounding modality of last resort, no different from the
 * picture. Nothing appears on screen until it is asked for.
 *
 * @param underline a span drawn with a 2dp rule in [underlineColor] beneath it — the
 *   headword inside an example sentence.
 */
@Composable
fun GlossedText(
    text: AnnotatedString,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    underline: IntRange = IntRange.EMPTY,
    underlineColor: Color = Color.Unspecified,
) {
    val index = LocalGlossIndex.current
    val anchorTint = MaterialTheme.colorScheme.tertiary
    val matches = remember(text, index) { index.scan(text.text) }

    // Nothing to reveal or draw: a plain Text, so most definitions cost nothing extra.
    if (matches.isEmpty() && underline.isEmpty()) {
        Text(text = text, style = style, color = color, modifier = modifier)
        return
    }

    val annotated = remember(text, matches, anchorTint) {
        if (matches.isEmpty()) text else text.withAnchorMarks(matches, anchorTint)
    }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    var shown by remember { mutableStateOf<ShownGloss?>(null) }
    val rule = with(androidx.compose.ui.platform.LocalDensity.current) { UNDERLINE_WIDTH.toPx() }

    Box(modifier) {
        Text(
            text = annotated,
            style = style,
            color = color,
            onTextLayout = { layout = it },
            modifier = Modifier
                .drawBehind {
                    val result = layout ?: return@drawBehind
                    result.lineBoxesOf(underline).forEach { box ->
                        drawRect(
                            color = underlineColor,
                            topLeft = androidx.compose.ui.geometry.Offset(box.left, box.bottom + rule),
                            size = androidx.compose.ui.geometry.Size(box.width, rule),
                        )
                    }
                }
                .pointerInput(matches) {
                    if (matches.isEmpty()) return@pointerInput
                    detectTapGestures { position ->
                        val result = layout ?: return@detectTapGestures
                        val offset = result.getOffsetForPosition(position)
                        matches.firstOrNull { offset in it }?.let { shown = ShownGloss(it, result.boundsOf(it)) }
                    }
                },
        )

        shown?.let { gloss ->
            GlossPopover(gloss = gloss, onDismiss = { shown = null })
        }
    }
}

/**
 * One box per line that [range] covers, from its first to its last character on that
 * line; each box's bottom is that line's baseline.
 */
private fun TextLayoutResult.lineBoxesOf(range: IntRange): List<Rect> {
    if (range.isEmpty()) return emptyList()
    val last = layoutInput.text.length - 1
    if (last < 0) return emptyList()
    return (range.first.coerceIn(0, last)..range.last.coerceIn(0, last))
        .groupBy { getLineForOffset(it) }
        .map { (line, offsets) ->
            val first = getBoundingBox(offsets.first())
            val end = getBoundingBox(offsets.last())
            Rect(first.left, first.top, end.right, getLineBaseline(line))
        }
}

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
 * the caller already applied (the bold headword, for one).
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
private val UNDERLINE_WIDTH: Dp = 2.dp
private val GLOSS_POPOVER_MAX_WIDTH: Dp = 220.dp

// ------------------------------------------------------------------ previews

/** A stand-in index so previews show the affordance without opening release.db. */
private val previewGlossIndex = GlossIndex.of(
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
                    text = AnnotatedString("an intricate network of paths crossing the open pasture"),
                    style = MorphoTheme.reading.definition,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                GlossedText(
                    text = AnnotatedString("nothing here is anchored, so this renders as plain English"),
                    style = MorphoTheme.reading.sentence,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoSectionLabel
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The app icon's progress row, promoted from decoration to a component.
 *
 * The icon carries four marks under the "M": a mist-blue square, a copper diamond, then
 * two parchment diamonds. Read left to right they are *not started → learning →
 * mastered*, which is the app's whole model in four shapes. Everything in this file is
 * built from those two marks and nothing else.
 */

// ------------------------------------------------------------------ primitives

/** Proportions lifted from `ic_launcher_foreground.xml`: the diamond outsizes the square. */
private const val SQUARE_RATIO = 0.62f
private const val DIAMOND_RATIO = 1.0f
private const val SQUARE_CORNER_RATIO = 0.16f

private fun DrawScope.drawMotifSquare(center: Offset, cell: Float, color: Color) {
    val side = cell * SQUARE_RATIO
    drawRoundRect(
        color = color,
        topLeft = Offset(center.x - side / 2f, center.y - side / 2f),
        size = Size(side, side),
        cornerRadius = CornerRadius(side * SQUARE_CORNER_RATIO),
    )
}

private fun DrawScope.drawMotifDiamond(
    center: Offset,
    cell: Float,
    color: Color,
    /** Non-null draws an outline of this width instead of a solid mark. */
    strokeWidth: Float? = null,
) {
    val half = cell * DIAMOND_RATIO / 2f
    val path = Path().apply {
        moveTo(center.x, center.y - half)
        lineTo(center.x + half, center.y)
        lineTo(center.x, center.y + half)
        lineTo(center.x - half, center.y)
        close()
    }
    drawPath(
        path = path,
        color = color,
        style = if (strokeWidth == null) {
            androidx.compose.ui.graphics.drawscope.Fill
        } else {
            Stroke(width = strokeWidth)
        },
    )
}

/** A single icon mark, for inline use next to a label. */
@Composable
fun MorphoMark(
    kind: MarkKind,
    color: Color,
    modifier: Modifier = Modifier,
    size: Dp = 10.dp,
) {
    Canvas(modifier.size(size)) {
        val center = Offset(this.size.width / 2f, this.size.height / 2f)
        when (kind) {
            MarkKind.SQUARE -> drawMotifSquare(center, this.size.minDimension, color)
            MarkKind.DIAMOND -> drawMotifDiamond(center, this.size.minDimension, color)
        }
    }
}

enum class MarkKind { SQUARE, DIAMOND }

/**
 * A slim rail for a long arc — the whole vocabulary, not one day. Milestones ride on
 * the rail as diamonds: copper once passed, an outline while still ahead.
 */
@Composable
fun MotifMilestoneRail(
    fraction: Float,
    modifier: Modifier = Modifier,
    milestones: List<Float> = listOf(0.25f, 0.5f, 0.75f),
    contentDescription: String? = null,
) {
    val accents = MorphoTheme.accents
    val railColor = accents.ringTrack
    val fillColor = MaterialTheme.colorScheme.primary
    val reachedColor = accents.motifActive
    val aheadColor = accents.motifBase
    val safeFraction = fraction.coerceIn(0f, 1f)

    Canvas(
        modifier = modifier
            .fillMaxWidth()
            .height(16.dp)
            .then(
                if (contentDescription == null) {
                    Modifier
                } else {
                    Modifier.clearAndSetSemantics { this.contentDescription = contentDescription }
                },
            ),
    ) {
        val railHeight = 4.dp.toPx()
        val railY = (size.height - railHeight) / 2f
        val corner = CornerRadius(railHeight / 2f)
        val diamondCell = 12.dp.toPx()
        // Keep the end diamonds inside the canvas.
        val inset = diamondCell / 2f
        val railWidth = size.width - inset * 2f

        drawRoundRect(
            color = railColor,
            topLeft = Offset(inset, railY),
            size = Size(railWidth, railHeight),
            cornerRadius = corner,
        )
        val fillWidth = railWidth * safeFraction
        if (fillWidth > 0f) {
            drawRoundRect(
                color = fillColor,
                topLeft = Offset(inset, railY),
                size = Size(fillWidth, railHeight),
                cornerRadius = corner,
            )
        }

        val centerY = size.height / 2f
        milestones.forEach { milestone ->
            val center = Offset(inset + railWidth * milestone.coerceIn(0f, 1f), centerY)
            if (safeFraction >= milestone) {
                drawMotifDiamond(center, diamondCell, reachedColor)
            } else {
                drawMotifDiamond(center, diamondCell, aheadColor, strokeWidth = 1.5.dp.toPx())
            }
        }
    }
}

// -------------------------------------------------------------- running heads

/**
 * A section running head: a tracked-out label, a hairline rule to the margin, and
 * whatever control the section owns parked at the end.
 */
@Composable
fun SectionHeading(
    text: String,
    modifier: Modifier = Modifier,
    trailing: @Composable (() -> Unit)? = null,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm),
    ) {
        Text(
            text = text.uppercase(),
            style = MorphoSectionLabel,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        HorizontalDivider(
            modifier = Modifier.weight(1f),
            thickness = 1.dp,
            color = MorphoTheme.accents.rule,
        )
        if (trailing != null) trailing()
    }
}

/** A centred ornament: hairline, copper diamond, hairline. Ends a run of sections. */
@Composable
fun MotifOrnament(modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clearAndSetSemantics { },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm),
    ) {
        HorizontalDivider(
            modifier = Modifier.weight(1f),
            thickness = 1.dp,
            color = MorphoTheme.accents.rule,
        )
        MorphoMark(
            kind = MarkKind.DIAMOND,
            color = MorphoTheme.accents.motifActive,
            size = 8.dp,
        )
        HorizontalDivider(
            modifier = Modifier.weight(1f),
            thickness = 1.dp,
            color = MorphoTheme.accents.rule,
        )
    }
}

/** The icon's four marks at rest — a static wordmark companion. */
@Composable
fun MotifSignature(modifier: Modifier = Modifier, size: Dp = 8.dp) {
    val accents = MorphoTheme.accents
    Row(
        modifier = modifier.clearAndSetSemantics { },
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        MorphoMark(MarkKind.SQUARE, accents.motifBase, size = size)
        MorphoMark(MarkKind.DIAMOND, accents.motifActive, size = size)
        MorphoMark(MarkKind.DIAMOND, accents.motifMastered, size = size)
        MorphoMark(MarkKind.DIAMOND, accents.motifMastered, size = size)
    }
}

@ThemePreviews
@Composable
private fun MotifPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.lg)) {
            SectionHeading("Today")
            MotifMilestoneRail(fraction = 0.49f)
            MotifOrnament()
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                MotifSignature()
            }
        }
    }
}

package dev.morpho.ui.today

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import dev.morpho.domain.model.HeatmapCell
import dev.morpho.domain.model.HeatmapData
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme
import java.time.LocalDate

private val CELL_SIZE = 10.dp
private val CELL_GAP = 2.dp
private const val ROWS = 7
private val LABEL_WIDTH = 16.dp

@Composable
fun ActivityHeatmap(
    data: HeatmapData,
    modifier: Modifier = Modifier,
) {
    // The grid warms from a resting mist cell to the icon's copper as a day fills up.
    val ink = MorphoTheme.accents.motifBase
    val copper = MorphoTheme.accents.motifActive
    val empty = MorphoTheme.accents.ringTrack
    val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
    val labelStyle = MaterialTheme.typography.labelSmall
    val dayLabels = mapOf(0 to "M", 2 to "W", 4 to "F")

    val cellStep = CELL_SIZE + CELL_GAP
    val columns = data.weeks
    val gridWidth = LABEL_WIDTH + (cellStep * columns)
    val gridHeight = cellStep * ROWS

    Row(
        modifier = modifier.horizontalScroll(rememberScrollState()),
    ) {
        Column(modifier = Modifier.padding(end = MorphoTheme.spacing.xxs)) {
            for (row in 0 until ROWS) {
                val label = dayLabels[row]
                if (label != null) {
                    Text(
                        text = label,
                        style = labelStyle,
                        color = labelColor,
                        modifier = Modifier
                            .height(cellStep)
                            .padding(end = MorphoTheme.spacing.xxs),
                    )
                } else {
                    androidx.compose.foundation.layout.Spacer(
                        modifier = Modifier.height(cellStep),
                    )
                }
            }
        }

        Canvas(
            modifier = Modifier
                .width(cellStep * columns)
                .height(gridHeight),
        ) {
            val cellPx = CELL_SIZE.toPx()
            val stepPx = cellStep.toPx()
            val corner = CornerRadius(2.dp.toPx())

            data.cells.forEachIndexed { index, cell ->
                val col = index / ROWS
                val row = index % ROWS
                val color = when (cell.intensity) {
                    0 -> empty
                    1 -> ink.copy(alpha = 0.45f)
                    2 -> ink
                    3 -> copper.copy(alpha = 0.7f)
                    else -> copper
                }
                drawRoundRect(
                    color = color,
                    topLeft = Offset(col * stepPx, row * stepPx),
                    size = Size(cellPx, cellPx),
                    cornerRadius = corner,
                )
            }
        }
    }
}

@ThemePreviews
@Composable
private fun ActivityHeatmapPreview() {
    val today = LocalDate.now()
    val weeks = 16
    val cells = (0 until weeks * 7).map { i ->
        HeatmapCell(
            date = today.minusDays((weeks * 7 - 1 - i).toLong()),
            intensity = listOf(0, 1, 2, 3, 4, 0, 1, 2, 3, 0)[i % 10],
        )
    }
    PreviewBox {
        ActivityHeatmap(
            data = HeatmapData(cells = cells, weeks = weeks, maxActivity = 50),
        )
    }
}

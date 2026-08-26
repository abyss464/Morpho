package dev.morpho.ui.home

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.morpho.domain.model.DailyActivity
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme
import java.time.LocalDate

@Composable
fun WeeklyBarChart(
    activity: List<DailyActivity>,
    modifier: Modifier = Modifier,
) {
    val primary = MaterialTheme.colorScheme.primary
    val dimmed = primary.copy(alpha = 0.35f)
    val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
    val labelStyle = MaterialTheme.typography.labelSmall
    val today = LocalDate.now()
    val maxWords = activity.maxOfOrNull { it.wordsStudied } ?: 0
    val barHeight = 56.dp
    val dayLabels = listOf("M", "T", "W", "T", "F", "S", "S")

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
    ) {
        Canvas(
            modifier = Modifier
                .fillMaxWidth()
                .height(barHeight),
        ) {
            val barCount = activity.size.coerceAtMost(7)
            val gap = 8.dp.toPx()
            val totalGap = gap * (barCount - 1)
            val barWidth = (size.width - totalGap) / barCount
            val minBarHeight = 4.dp.toPx()
            val cornerRadius = CornerRadius(4.dp.toPx())

            activity.take(7).forEachIndexed { index, day ->
                val isToday = day.date == today
                val proportion = if (maxWords > 0) {
                    day.wordsStudied.toFloat() / maxWords
                } else {
                    0f
                }
                val h = if (day.wordsStudied == 0) {
                    minBarHeight
                } else {
                    (proportion * (size.height - minBarHeight) + minBarHeight)
                }
                val x = index * (barWidth + gap)
                val y = size.height - h
                drawRoundRect(
                    color = if (isToday) primary else dimmed,
                    topLeft = Offset(x, y),
                    size = Size(barWidth, h),
                    cornerRadius = cornerRadius,
                )
            }
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceAround,
        ) {
            activity.take(7).forEachIndexed { index, day ->
                val isToday = day.date == today
                val dayIndex = (day.date.dayOfWeek.value - 1).coerceIn(0, 6)
                Text(
                    text = dayLabels[dayIndex],
                    style = labelStyle,
                    fontWeight = if (isToday) FontWeight.Bold else FontWeight.Normal,
                    color = if (isToday) primary else labelColor,
                    modifier = Modifier.align(Alignment.CenterVertically),
                )
            }
        }
    }
}

@ThemePreviews
@Composable
private fun WeeklyBarChartPreview() {
    val today = LocalDate.now()
    PreviewBox {
        WeeklyBarChart(
            activity = (6 downTo 0).map { daysAgo ->
                DailyActivity(
                    date = today.minusDays(daysAgo.toLong()),
                    wordsStudied = listOf(12, 45, 30, 0, 55, 20, 38)[6 - daysAgo],
                    newLearned = listOf(8, 20, 15, 0, 25, 10, 18)[6 - daysAgo],
                    reviewed = listOf(4, 25, 15, 0, 30, 10, 20)[6 - daysAgo],
                )
            },
        )
    }
}

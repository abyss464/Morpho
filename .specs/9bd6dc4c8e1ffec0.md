---
file: app/app/src/main/kotlin/dev/morpho/ui/home/HomeViewModel.kt
---

# Home ViewModel

## Exposed status (HomeUiState)

- **loading** — whether currently loading
- **overall** — overall progress: total word count, learned word count, mastered word count
- **today** — today's progress: new words today, daily goal, reviews today, pending review count
- **streakDays** — consecutive learning days
- **contentVersion** — current content version number
- **greetingPeriod** — greeting period (morning/afternoon/evening)
- **weeklyActivity** — recent daily activity data, used for activity charts
- **heatmapData** — heatmap data
- **estimatedDaysRemaining** — estimated remaining days based on recent pace, empty when data is insufficient
- **activityChartStyle** — activity chart style (bar chart/heatmap)
- **hasContent** — whether there is content to learn (total word count greater than zero)

## Accepted operations

### refresh()
Pull all data from the repository database and refresh the entire home page status.

### setActivityChartStyle(style)
Switch the activity chart style and persist it to settings.

package dev.morpho.ui.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.MenuBook
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

sealed interface HomeFeature {
    val icon: ImageVector
    val labelRes: Int

    data object Learning : HomeFeature {
        override val icon = Icons.AutoMirrored.Rounded.MenuBook
        override val labelRes = R.string.home_start_learning
    }

    data object Review : HomeFeature {
        override val icon = Icons.Rounded.Refresh
        override val labelRes = R.string.home_start_review
    }

    // data object Reading : HomeFeature { ... }
    // data object Vocabulary : HomeFeature { ... }
}

private val activeFeatures = listOf(HomeFeature.Learning, HomeFeature.Review)

@Composable
fun QuickAccessRow(
    onFeatureClick: (HomeFeature) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm),
    ) {
        activeFeatures.forEach { feature ->
            OutlinedCard(
                modifier = Modifier
                    .weight(1f)
                    .clickable { onFeatureClick(feature) },
                shape = MorphoTheme.radii.shapeMd,
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(MorphoTheme.spacing.md),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
                ) {
                    Icon(
                        imageVector = feature.icon,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(24.dp),
                    )
                    Text(
                        text = stringResource(feature.labelRes),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
        }
    }
}

@ThemePreviews
@Composable
private fun QuickAccessRowPreview() {
    PreviewBox {
        QuickAccessRow(onFeatureClick = {})
    }
}

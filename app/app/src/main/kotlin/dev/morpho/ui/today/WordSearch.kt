package dev.morpho.ui.today

import androidx.compose.foundation.background
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.data.repository.IndexedWord
import dev.morpho.data.stream.StreamSnapshot
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** A look-up result: the word and the unit it sits in. */
data class SearchHit(val wordId: Long, val word: String, val unit: Int)

/**
 * Words matching [query], as the web client's look-up ranks them: an exact match first,
 * then words starting with the query, then words containing it; shorter words first within
 * each, then learning order. At most [limit].
 */
fun searchWords(index: List<IndexedWord>, query: String, limit: Int = HITS): List<SearchHit> {
    val t = query.trim().lowercase()
    if (t.isEmpty()) return emptyList()
    return index.withIndex()
        .mapNotNull { (i, w) ->
            val lower = w.word.lowercase()
            val score = when {
                lower == t -> 0
                lower.startsWith(t) -> 1
                lower.contains(t) -> 2
                else -> return@mapNotNull null
            }
            Triple(score, lower.length, i)
        }
        .sortedWith(compareBy({ it.first }, { it.second }, { it.third }))
        .take(limit)
        .map { (_, _, i) ->
            SearchHit(index[i].wordId, index[i].word, i / StreamSnapshot.UNIT_SIZE + 1)
        }
}

private const val HITS = 8

/**
 * "Look up a word": a search field and, while there is a query, its matches under it. The
 * keyboard's search key opens the first match; picking one clears the field.
 */
@Composable
fun WordSearch(index: List<IndexedWord>, onPick: (Long) -> Unit, modifier: Modifier = Modifier) {
    val spacing = MorphoTheme.spacing
    val colors = MaterialTheme.colorScheme
    val focus = LocalFocusManager.current
    var query by rememberSaveable { mutableStateOf("") }
    val hits = remember(query, index) { searchWords(index, query) }
    // Keep the field and its matches in view above the keyboard while typing.
    val bring = remember { BringIntoViewRequester() }
    LaunchedEffect(hits) { if (query.isNotBlank()) bring.bringIntoView() }
    val shape = MorphoTheme.radii.shapeSm
    val label = stringResource(R.string.today_lookup)
    val pick = { hit: SearchHit? ->
        if (hit != null) {
            query = ""
            focus.clearFocus()
            onPick(hit.wordId)
        }
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .bringIntoViewRequester(bring),
        verticalArrangement = Arrangement.spacedBy(spacing.xxs),
    ) {
        BasicTextField(
            value = query,
            onValueChange = { query = it },
            singleLine = true,
            textStyle = MaterialTheme.typography.bodyLarge.copy(color = colors.onSurface),
            cursorBrush = SolidColor(colors.primary),
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                imeAction = ImeAction.Search,
            ),
            keyboardActions = KeyboardActions(onSearch = { pick(hits.firstOrNull()) }),
            modifier = Modifier
                .fillMaxWidth()
                .semantics { contentDescription = label },
            decorationBox = { field ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget)
                        .clip(shape)
                        .background(colors.surfaceContainerLow)
                        .border(1.dp, MorphoTheme.accents.ringTrack, shape)
                        .padding(horizontal = spacing.sm),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(spacing.xs),
                ) {
                    Icon(
                        Icons.Rounded.Search,
                        contentDescription = null,
                        tint = colors.onSurfaceVariant,
                        modifier = Modifier.size(20.dp),
                    )
                    Box(Modifier.weight(1f)) {
                        if (query.isEmpty()) {
                            Text(label, style = MaterialTheme.typography.bodyLarge, color = colors.onSurfaceVariant)
                        }
                        field()
                    }
                }
            },
        )
        if (query.isNotBlank()) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(shape)
                    .background(colors.surfaceContainerLow)
                    .border(1.dp, MorphoTheme.accents.ringTrack, shape),
            ) {
                if (hits.isEmpty()) {
                    Text(
                        text = stringResource(R.string.today_lookup_none, query.trim()),
                        style = MaterialTheme.typography.bodyMedium,
                        color = colors.onSurfaceVariant,
                        modifier = Modifier.padding(spacing.sm),
                    )
                }
                hits.forEachIndexed { i, hit ->
                    if (i > 0) HorizontalDivider(color = MorphoTheme.accents.ringTrack)
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = spacing.minTouchTarget)
                            .pressMotion()
                            .clickable { pick(hit) }
                            .padding(horizontal = spacing.sm),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(text = hit.word, style = MorphoTheme.reading.wordOption, color = colors.onSurface)
                        Text(
                            text = stringResource(R.string.today_unit, hit.unit),
                            style = MaterialTheme.typography.labelMedium,
                            color = colors.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}

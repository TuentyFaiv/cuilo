package com.example.app.ui.molecules

import androidx.compose.material3.Text
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyChild
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onChildren
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onParent
import androidx.compose.ui.test.v2.runComposeUiTest
import kotlin.test.Test

@OptIn(ExperimentalTestApi::class)
class ExampleMoleculeTest {
    @Test
    fun groupsItsLabelDescriptionAndContentForAssistiveTechnology() = runComposeUiTest {
        setContent {
            ExampleMolecule(label = "Notifications", description = "Choose how we reach you.") { Text("Email") }
        }

        onNodeWithText("Notifications").onParent()
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.IsTraversalGroup, true))
            .assert(hasAnyChild(hasText("Choose how we reach you.")))
        onNodeWithText("Email").assertIsDisplayed()
    }

    @Test
    fun leavesOutTheDescriptionWhenThereIsNone() = runComposeUiTest {
        setContent { ExampleMolecule(label = "Notifications") { Text("Email") } }

        onNodeWithText("Notifications").onParent().onChildren().assertCountEquals(2)
    }
}

package com.example.app.ui.molecules

import androidx.compose.material3.Text
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyChild
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onChildren
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onParent
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class ExampleMoleculeTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun groupsItsLabelDescriptionAndContentForAssistiveTechnology() {
        compose.setContent {
            ExampleMolecule(label = "Notifications", description = "Choose how we reach you.") { Text("Email") }
        }

        compose.onNodeWithText("Notifications").onParent()
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.IsTraversalGroup, true))
            .assert(hasAnyChild(hasText("Choose how we reach you.")))
        compose.onNodeWithText("Email").assertIsDisplayed()
    }

    @Test
    fun leavesOutTheDescriptionWhenThereIsNone() {
        compose.setContent { ExampleMolecule(label = "Notifications") { Text("Email") } }

        compose.onNodeWithText("Notifications").onParent().onChildren().assertCountEquals(2)
    }
}

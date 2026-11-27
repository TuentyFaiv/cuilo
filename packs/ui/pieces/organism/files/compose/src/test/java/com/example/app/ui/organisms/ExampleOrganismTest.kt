package com.example.app.ui.organisms

import androidx.compose.material3.Text
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class ExampleOrganismTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun marksItsHeadingAndShowsItsActionsAndContent() {
        compose.setContent {
            ExampleOrganism(heading = "Team", actions = { Text("Invite") }) { Text("Three members") }
        }

        compose.onNodeWithText("Team").assert(isHeading())
        compose.onNodeWithText("Invite").assertIsDisplayed()
        compose.onNodeWithText("Three members").assertIsDisplayed()
    }

    @Test
    fun showsItsContentWithoutActions() {
        compose.setContent { ExampleOrganism(heading = "Team") { Text("Three members") } }

        compose.onNodeWithText("Team").assert(isHeading())
        compose.onNodeWithText("Three members").assertIsDisplayed()
    }
}

package com.example.app.ui.organisms

import androidx.compose.material3.Text
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.v2.runComposeUiTest
import kotlin.test.Test

@OptIn(ExperimentalTestApi::class)
class ExampleOrganismTest {
    @Test
    fun marksItsHeadingAndShowsItsActionsAndContent() = runComposeUiTest {
        setContent {
            ExampleOrganism(heading = "Team", actions = { Text("Invite") }) { Text("Three members") }
        }

        onNodeWithText("Team").assert(isHeading())
        onNodeWithText("Invite").assertIsDisplayed()
        onNodeWithText("Three members").assertIsDisplayed()
    }

    @Test
    fun showsItsContentWithoutActions() = runComposeUiTest {
        setContent { ExampleOrganism(heading = "Team") { Text("Three members") } }

        onNodeWithText("Team").assert(isHeading())
        onNodeWithText("Three members").assertIsDisplayed()
    }
}

package com.example.app.ui.atoms

import androidx.compose.material3.Text
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.v2.runComposeUiTest
import kotlin.test.Test

@OptIn(ExperimentalTestApi::class)
class ExampleAtomTest {
    @Test
    fun showsItsContentInTheNeutralTone() = runComposeUiTest {
        setContent { ExampleAtom { Text("Draft") } }

        onNodeWithText("Draft").assertIsDisplayed()
    }

    @Test
    fun showsItsContentInTheAccentTone() = runComposeUiTest {
        setContent { ExampleAtom(tone = ExampleAtomTone.Accent) { Text("New") } }

        onNodeWithText("New").assertIsDisplayed()
    }
}

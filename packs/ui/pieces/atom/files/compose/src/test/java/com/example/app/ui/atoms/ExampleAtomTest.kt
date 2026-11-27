package com.example.app.ui.atoms

import androidx.compose.material3.Text
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class ExampleAtomTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun showsItsContentInTheNeutralTone() {
        compose.setContent { ExampleAtom { Text("Draft") } }

        compose.onNodeWithText("Draft").assertIsDisplayed()
    }

    @Test
    fun showsItsContentInTheAccentTone() {
        compose.setContent { ExampleAtom(tone = ExampleAtomTone.Accent) { Text("New") } }

        compose.onNodeWithText("New").assertIsDisplayed()
    }
}

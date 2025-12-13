package moe.fuqiuluo.mamu

import android.content.Context
import android.os.Bundle
import androidx.activity.ComponentActivity      
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.*
import moe.fuqiuluo.mamu.ui.screen.HomeScreen
import moe.fuqiuluo.mamu.ui.theme.MXTheme
import moe.fuqiuluo.mamu.ui.tutorial.screen.TutorialPracticeScreen


class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MXTheme {
                var showPracticeScreen by remember { mutableStateOf(false) }

                if (showPracticeScreen) {
                    TutorialPracticeScreen(
                        onBack = { showPracticeScreen = false }
                    )
                } else {
                    HomeScreen(
                        onStartPractice = { showPracticeScreen = true }
                    )
                }
            }
        }
    }
}
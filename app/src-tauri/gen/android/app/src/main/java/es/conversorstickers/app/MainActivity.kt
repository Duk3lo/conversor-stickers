package es.conversorstickers.app

import android.graphics.Color
import android.os.Bundle
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // The interface is always dark: transparent bars with light icons.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)

    val controller = WindowCompat.getInsetsController(window, window.decorView)
    controller.isAppearanceLightStatusBars = false
    controller.isAppearanceLightNavigationBars = false

    // The WebView does not see the insets (env(safe-area-inset-*) depends on its version): the
    // root view leaves room for the bars, the screen cutout and the keyboard.
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView.findViewById(android.R.id.content)) { view, insets ->
      val gaps = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or
          WindowInsetsCompat.Type.displayCutout() or
          WindowInsetsCompat.Type.ime()
      )
      view.setPadding(gaps.left, gaps.top, gaps.right, gaps.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }
}

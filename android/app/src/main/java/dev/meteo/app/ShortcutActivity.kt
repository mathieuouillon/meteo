package dev.meteo.app

import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import dev.meteo.app.widget.RustBridge

/**
 * Opened by a launcher shortcut: selects that city, then starts the app
 * fresh so it shows it. Draws nothing.
 */
class ShortcutActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent.getStringExtra(Shortcuts.EXTRA_CITY)?.let { RustBridge.selectCity(filesDir.absolutePath, it) }
        startActivity(
            Intent()
                .setComponent(ComponentName(packageName, "android.app.NativeActivity"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK),
        )
        finish()
    }
}

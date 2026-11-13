package dev.meteo.app.wear

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Bundle
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import dev.meteo.app.R
import dev.meteo.app.widget.RustBridge
import dev.meteo.app.widget.parseWidgetState

/**
 * The watch app's one screen: the weather now, a button to use the watch's
 * location (the tile can't ask for permission itself), and how to add the tile.
 */
class WearActivity : Activity() {
    private lateinit var summary: TextView
    private lateinit var locate: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pad = (resources.displayMetrics.widthPixels * 0.12).toInt()
        summary = TextView(this).apply { gravity = Gravity.CENTER; textSize = 16f }
        locate = Button(this).apply {
            setText(R.string.use_location)
            setOnClickListener { requestPermissions(arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION), 1) }
        }
        val hint = TextView(this).apply { gravity = Gravity.CENTER; textSize = 12f; setText(R.string.add_tile) }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER
                setPadding(pad, pad, pad, pad)
                addView(summary)
                addView(locate)
                addView(hint)
            },
        )
    }

    override fun onResume() {
        super.onResume()
        show()
        locate.visibility = if (hasLocation()) android.view.View.GONE else android.view.View.VISIBLE
        WatchRefresh.start(this) { runOnUiThread { show() } }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (hasLocation()) {
            locate.visibility = android.view.View.GONE
            WatchRefresh.start(this) { runOnUiThread { show() } }
        }
    }

    private fun hasLocation() =
        checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED

    private fun show() {
        val language = resources.configuration.locales[0].language
        val view = parseWidgetState(RustBridge.state(filesDir.absolutePath, language)).view
        summary.text = view?.let { "${it.city}\n${it.temperature} · ${it.condition}\n${it.highLow}" }
            ?: if (language == "fr") "Chargement…" else "Loading…"
    }
}

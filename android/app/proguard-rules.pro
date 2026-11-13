# R8 rules for the release build.

# Called from Rust through JNI by name: keep the classes and members.
-keep class dev.meteo.app.MeteoApp { *; }
-keep class dev.meteo.app.widget.RustBridge { *; }
-keepclasseswithmembernames class * { native <methods>; }

# (Slint's own Java helper lives inside libmeteo.so, not in the app's dex,
# so R8 never sees it.)

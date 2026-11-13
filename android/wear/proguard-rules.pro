# Called from Rust through JNI by name.
-keep class dev.meteo.app.widget.RustBridge { *; }
-keepclasseswithmembernames class * { native <methods>; }

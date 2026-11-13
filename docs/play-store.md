# Publishing Météo on Google Play

Everything to paste into the Play Console, and the steps in order. Things
only you can do (account, key, payment, contact email) are marked **you**.

## 1. Before the first upload

1. **You:** create the upload key (once, and back it up with its password):
   ```bash
   mkdir -p ~/keys && keytool -genkeypair -v -keystore ~/keys/meteo-upload.jks -alias upload -keyalg RSA -keysize 2048 -validity 10000
   ```
   Then `android/keystore.properties` (ignored by git):
   ```
   storeFile=/Users/<you>/keys/meteo-upload.jks
   storePassword=…
   keyAlias=upload
   keyPassword=…
   ```
2. **You:** create a Google Play developer account (personal, US$25 once) and
   verify your identity.
3. **You:** publish `docs/privacy-policy.md` at a public URL (for example
   GitHub Pages, or a public Gist), after replacing `CONTACT_EMAIL` with the
   address you want to use for the app.
4. **Package name:** `dev.meteo.app` is permanent once uploaded. If you own a
   domain, consider changing it now to match (`applicationId` in
   `android/app/build.gradle.kts`).
5. Raise `versionCode` (and `versionName`) in `android/app/build.gradle.kts`
   for every upload.

## 2. Build the bundle

```bash
cd android && ./gradlew bundleRelease
```

Upload `android/app/build/outputs/bundle/release/app-release.aab`. Accept
**Play App Signing** when asked: Google keeps the final signing key, your
upload key only proves uploads are yours.

## 3. Store listing

**App name:** Météo

**Short description** (80 characters max)

- EN: `Weather in the colours of the sky: 7 days, rain in the next hour, widgets.`
- FR: `La météo aux couleurs du ciel : 7 jours, pluie dans l'heure, widgets.`

**Full description (EN)**

```
Météo shows the weather in the colours of the sky: warm amber on a clear afternoon, cool blue in the rain, deep blue at night, with a quiet landscape and rain, snow and clouds that move with the real forecast.

• Now and the next 2 hours: rain in 15-minute steps, "Rain in 30 min".
• 7 days, hour by hour: temperature curve, rain amounts and chances.
• Do the models agree? Five weather models (ECMWF, ICON, GFS, Météo-France, UK Met Office) are compared each day, for temperature and for rain, with their range shown on the chart.
• Compared with normal: "2° colder than usual".
• Details: wind with a compass that points north, feels like, humidity, UV, pressure, air quality, pollen, sunrise and sunset.
• Home-screen widgets: a large one, next-hours rain, and a round one.
• Optional notifications: a morning summary and a rain alert.
• Your cities, or your approximate location.
• English and French, metric or imperial, weather colours or your wallpaper's (Material You).

No account, no ads, no tracking. Forecasts from Open-Meteo.com (CC BY 4.0).
```

**Full description (FR)**

```
Météo affiche le temps aux couleurs du ciel : ambre chaud par un après-midi dégagé, bleu frais sous la pluie, bleu profond la nuit, avec un paysage calme et une pluie, une neige et des nuages qui suivent les vraies prévisions.

• Maintenant et les 2 prochaines heures : la pluie par quarts d'heure, « Pluie dans 30 min ».
• 7 jours, heure par heure : courbe des températures, quantités et risques de pluie.
• Les modèles sont-ils d'accord ? Cinq modèles (ECMWF, ICON, GFS, Météo-France, UK Met Office) comparés chaque jour, pour la température et la pluie, avec leur écart sur le graphique.
• Par rapport à la normale : « 2° de moins que la normale ».
• Détails : vent avec une boussole qui indique le nord, ressenti, humidité, UV, pression, qualité de l'air, pollens, lever et coucher du soleil.
• Widgets : un grand, la pluie des prochaines heures, et un rond.
• Notifications au choix : résumé du matin et alerte pluie.
• Vos villes, ou votre position approximative.
• Français et anglais, unités métriques ou impériales, couleurs de la météo ou de votre fond d'écran (Material You).

Sans compte, sans publicité, sans pistage. Prévisions d'Open-Meteo.com (CC BY 4.0).
```

**Category:** Weather. **Tags:** weather, forecast, rain.

**Graphics** (all in English and French if you can):

- App icon 512 × 512 PNG: `docs/play-icon-512.png`.
- Feature graphic 1024 × 500.
- At least 2 phone screenshots (`docs/look-*.png`, `docs/android-*.png` are a
  start; take fresh ones from a phone for the final listing).

## 4. App content (Policy section)

- **Privacy policy:** the URL from step 1.3.
- **Ads:** No.
- **App access:** All functionality is available without special access.
- **Content rating:** fill the questionnaire; category "Utility/Productivity",
  no violence, no user-generated content, no sharing of location with other
  users → rated Everyone / PEGI 3.
- **Target audience:** 13 and over (not designed for children).
- **News app:** No. **Government app:** No. **Financial features:** none.
- **Health apps:** No.

**Data safety form**

| Question | Answer |
| --- | --- |
| Does the app collect or share user data? | Yes (location is sent to a third party to get forecasts) |
| Is all data encrypted in transit? | Yes (HTTPS) |
| Can users request deletion? | Data isn't kept by the developer; uninstalling deletes local data |
| Location → Approximate location | Collected, **not** shared for other purposes; used for **App functionality**; processed ephemerally; optional (users can search a city instead) |
| App activity, identifiers, personal info, etc. | Not collected |

("Shared" in Play's sense excludes service providers acting on your behalf,
which is how Open-Meteo is used here.)

**Permissions declarations:** none needed (no background location, no exact
alarms, no SMS/call logs).

## 5. Testing, then production

New personal accounts must run a **closed test with at least 12 testers for
14 days in a row** before applying for production access.

1. Testing → Closed testing → create a track, upload the `.aab`.
2. Add testers by email list or a Google Group; share the opt-in link.
3. After 14 days with 12+ opted-in testers, apply for production in the
   Dashboard, answering the questions about your testing.
4. Production → create a release with the same (or a newer) bundle, choose
   countries, and roll out.

Internal testing (up to 100 testers, available within minutes) is useful for
your own phones meanwhile, but doesn't count towards the 14 days.

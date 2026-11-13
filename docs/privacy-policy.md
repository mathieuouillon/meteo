# Météo — Privacy policy

_Last updated: 13 November 2026_

Météo is a weather app. It has no account, no advertising, no analytics and no
server of its own. This page explains the little data it handles.

## What the app sends, and to whom

To show the weather, Météo asks [Open-Meteo](https://open-meteo.com) for
forecasts. Each request contains:

- **The coordinates of the place** you're looking at (a saved city, or your
  approximate position if you tap "Use my location"), rounded to two decimals
  (about 1 km).
- **The name you type** when you search for a city.

These go to Open-Meteo's forecast, air quality, climate archive and
geocoding services over an encrypted connection (HTTPS). To name the place
you're in, the app also uses Android's built-in geocoder with the same
rounded coordinates; on most phones that service is provided by Google. Like any web
server, Open-Meteo sees your IP address. Météo sends nothing else: no device
identifier, no name, no email. See
[Open-Meteo's terms and privacy policy](https://open-meteo.com/en/terms).

## Location

Météo asks for **approximate location** only, and only after you tap "Use my
location". It then reads your position once each time you open the app with
the "📍" place selected, to keep it up to date, and stops. It never reads your location in
the background (not for widgets or notifications either). You can refuse,
or remove the "📍" place, and use the city search instead.

## What stays on your phone

Your saved cities, your settings, the last forecasts (so the app works
offline) and the times of the last notifications are stored in the app's
private storage on your phone. They're never uploaded. Uninstalling the app,
or clearing its storage, deletes them.

## Notifications

If you turn on the morning summary or rain alerts, the app checks the
forecast in the background about every 15 minutes, with the same requests as
above. Notifications are created on your phone.

## Children

Météo isn't aimed at children and collects no personal information from
anyone.

## Changes and contact

If this policy changes, the new version will be posted here with a new date.
Questions: CONTACT_EMAIL.

---

# Météo — Politique de confidentialité

_Dernière mise à jour : 13 novembre 2026_

Météo est une application météo. Elle n'a ni compte, ni publicité, ni mesure
d'audience, ni serveur à elle. Voici les quelques données qu'elle utilise.

## Ce que l'application envoie, et à qui

Pour afficher la météo, Météo interroge [Open-Meteo](https://open-meteo.com).
Chaque requête contient :

- **Les coordonnées du lieu** consulté (une ville enregistrée, ou votre
  position approximative si vous touchez « Utiliser ma position »), arrondies
  à deux décimales (environ 1 km).
- **Le nom que vous tapez** quand vous cherchez une ville.

Elles sont envoyées aux services de prévisions, de qualité de l'air,
d'archives climatiques et de géocodage d'Open-Meteo, par une connexion
chiffrée (HTTPS). Pour nommer le lieu où vous êtes, l'application utilise
aussi le géocodeur intégré d'Android avec les mêmes coordonnées arrondies ;
sur la plupart des téléphones, ce service est fourni par Google. Comme tout serveur web, Open-Meteo voit votre adresse IP.
Météo n'envoie rien d'autre : aucun identifiant d'appareil, aucun nom, aucun
e-mail. Voir les
[conditions et la politique de confidentialité d'Open-Meteo](https://open-meteo.com/en/terms).

## Position

Météo demande uniquement la **position approximative**, et seulement après
que vous avez touché « Utiliser ma position ». Elle lit ensuite votre position
une fois à chaque ouverture de l'application quand le lieu « 📍 » est
choisi, pour le tenir à jour, puis s'arrête. Elle ne lit jamais votre position en arrière-plan (ni
pour les widgets, ni pour les notifications). Vous pouvez refuser, ou
supprimer le lieu « 📍 », et chercher une ville à la place.

## Ce qui reste sur votre téléphone

Vos villes, vos réglages, les dernières prévisions (pour fonctionner hors
ligne) et l'heure des dernières notifications sont enregistrés dans le
stockage privé de l'application. Ils ne sont jamais envoyés. Désinstaller
l'application, ou effacer ses données, les supprime.

## Notifications

Si vous activez le résumé du matin ou les alertes pluie, l'application
vérifie les prévisions en arrière-plan environ toutes les 15 minutes, avec
les mêmes requêtes que ci-dessus. Les notifications sont créées sur votre
téléphone.

## Enfants

Météo ne s'adresse pas aux enfants et ne collecte d'informations
personnelles sur personne.

## Modifications et contact

Si cette politique change, la nouvelle version sera publiée ici avec une
nouvelle date. Questions : CONTACT_EMAIL.

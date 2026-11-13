//! English and French. Every text the app shows comes from here, so a
//! language is one `match` per string and nothing is translated in the UI.

use serde::{Deserialize, Serialize};

use crate::condition::Condition;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// From a locale like "fr", "fr-FR", "fr_CA.UTF-8"; English otherwise.
    pub fn from_code(code: &str) -> Self {
        if code.to_ascii_lowercase().starts_with("fr") { Lang::Fr } else { Lang::En }
    }

    /// ISO 639-1 code, e.g. for the geocoding API.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    fn pick(self, en: &'static str, fr: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Fr => fr,
        }
    }

    // ── Forecast labels ────────────────────────────────────────────────

    pub fn today(self) -> &'static str {
        self.pick("Today", "Auj.")
    }

    pub fn now(self) -> &'static str {
        self.pick("Now", "Maint.")
    }

    /// Short weekday, 0 = Sunday.
    pub fn weekday(self, index: usize) -> &'static str {
        const EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const FR: [&str; 7] = ["Dim.", "Lun.", "Mar.", "Mer.", "Jeu.", "Ven.", "Sam."];
        match self {
            Lang::En => EN[index % 7],
            Lang::Fr => FR[index % 7],
        }
    }

    pub fn condition(self, c: Condition) -> &'static str {
        use Condition::*;
        match c {
            Clear => self.pick("Clear", "Dégagé"),
            PartlyCloudy => self.pick("Partly cloudy", "Éclaircies"),
            Cloudy => self.pick("Cloudy", "Nuageux"),
            Fog => self.pick("Fog", "Brouillard"),
            Drizzle => self.pick("Drizzle", "Bruine"),
            Rain => self.pick("Rain", "Pluie"),
            Snow => self.pick("Snow", "Neige"),
            Thunderstorm => self.pick("Thunderstorm", "Orage"),
        }
    }

    /// "Day 15° · Night 10°".
    pub fn day_night(self, max: &str, min: &str) -> String {
        match self {
            Lang::En => format!("Day {max} · Night {min}"),
            Lang::Fr => format!("Jour {max} · Nuit {min}"),
        }
    }

    /// "Updated 2 h ago", for a forecast downloaded `age_secs` seconds ago.
    pub fn updated_ago(self, age_secs: i64) -> String {
        let age = age_secs.max(0);
        let (m, h, d) = (age / 60, age / 3600, age / 86_400);
        match (self, age) {
            (Lang::En, 0..60) => "Updated just now".into(),
            (Lang::En, 60..3600) => format!("Updated {m} min ago"),
            (Lang::En, 3600..86_400) => format!("Updated {h} h ago"),
            (Lang::En, 86_400..172_800) => "Updated yesterday".into(),
            (Lang::En, _) => format!("Updated {d} days ago"),
            (Lang::Fr, 0..60) => "Mis à jour à l’instant".into(),
            (Lang::Fr, 60..3600) => format!("Mis à jour il y a {m} min"),
            (Lang::Fr, 3600..86_400) => format!("Mis à jour il y a {h} h"),
            (Lang::Fr, 86_400..172_800) => "Mis à jour hier".into(),
            (Lang::Fr, _) => format!("Mis à jour il y a {d} jours"),
        }
    }

    /// "Offline · updated 2 h ago".
    pub fn offline(self, age_secs: i64) -> String {
        let updated = self.updated_ago(age_secs);
        let mut chars = updated.chars();
        let lower = chars.next().map(|c| c.to_lowercase().collect::<String>() + chars.as_str()).unwrap_or_default();
        format!("{} · {lower}", self.pick("Offline", "Hors ligne"))
    }

    // ── Details ────────────────────────────────────────────────────────

    /// Why it feels warmer or colder than the thermometer says.
    pub fn feels_note(self, temperature: f64, apparent: f64) -> &'static str {
        if apparent < temperature - 1.5 {
            self.pick("Wind makes it feel colder", "Le vent rend la sensation plus froide")
        } else if apparent > temperature + 1.5 {
            self.pick("Humidity makes it feel warmer", "L’humidité rend la sensation plus chaude")
        } else {
            self.pick("Close to the actual temperature", "Proche de la température réelle")
        }
    }

    pub fn feels_like(self, value: &str) -> String {
        match self {
            Lang::En => format!("Feels like {value}"),
            Lang::Fr => format!("Ressenti {value}"),
        }
    }

    pub fn dew_point(self, value: &str) -> String {
        match self {
            Lang::En => format!("Dew point {value}"),
            Lang::Fr => format!("Point de rosée {value}"),
        }
    }

    pub fn gusts(self, value: &str) -> String {
        match self {
            Lang::En => format!("Gusts {value}"),
            Lang::Fr => format!("Rafales {value}"),
        }
    }

    /// Compass point the wind comes from, from degrees (0 = north).
    pub fn compass(self, degrees: f64) -> &'static str {
        const EN: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        const FR: [&str; 8] = ["N", "NE", "E", "SE", "S", "SO", "O", "NO"];
        let i = ((degrees.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8;
        match self {
            Lang::En => EN[i],
            Lang::Fr => FR[i],
        }
    }

    /// "From the SW" / "Vient du SO".
    pub fn wind_from(self, degrees: f64) -> String {
        match self {
            Lang::En => format!("From the {}", self.compass(degrees)),
            Lang::Fr => format!("Vient du {}", self.compass(degrees)),
        }
    }

    pub fn uv_level(self, uv: f64) -> &'static str {
        match uv {
            u if u < 3.0 => self.pick("Low", "Faible"),
            u if u < 6.0 => self.pick("Moderate", "Modéré"),
            u if u < 8.0 => self.pick("High", "Élevé"),
            u if u < 11.0 => self.pick("Very high", "Très élevé"),
            _ => self.pick("Extreme", "Extrême"),
        }
    }

    /// For a detail the chosen source doesn't provide.
    pub fn not_available(self) -> &'static str {
        self.pick("Not available with this source", "Non disponible avec cette source")
    }

    pub fn pressure_level(self, hpa: f64) -> &'static str {
        match hpa {
            p if p < 1005.0 => self.pick("Low · unsettled", "Basse · instable"),
            p if p > 1022.0 => self.pick("High · settled", "Haute · stable"),
            _ => self.pick("Normal", "Normale"),
        }
    }

    /// "9 h 28 min of daylight".
    pub fn daylight(self, minutes: i64) -> String {
        let (h, m) = (minutes / 60, minutes % 60);
        match self {
            Lang::En => format!("{h} h {m:02} min of daylight"),
            Lang::Fr => format!("{h} h {m:02} min de jour"),
        }
    }

    // ── Next 2 hours ───────────────────────────────────────────────────

    /// "Light rain" / "Rain" / "Heavy rain" for a rate in mm/h.
    pub fn rain_word(self, mm_per_h: f64) -> &'static str {
        match mm_per_h {
            r if r < 1.0 => self.pick("Light rain", "Pluie faible"),
            r if r < 4.0 => self.pick("Rain", "Pluie modérée"),
            _ => self.pick("Heavy rain", "Forte pluie"),
        }
    }
    pub fn no_rain_soon(self) -> &'static str {
        self.pick("No rain expected in the next 2 hours", "Pas de pluie prévue dans les 2 prochaines heures")
    }
    pub fn rain_starting(self, word: &str, minutes: u32) -> String {
        match self {
            Lang::En => format!("{word} starting in about {minutes} min"),
            Lang::Fr => format!("{word} dans environ {minutes} min"),
        }
    }
    pub fn rain_stopping(self, word: &str, clock: &str) -> String {
        match self {
            Lang::En => format!("{word}, stopping around {clock}"),
            Lang::Fr => format!("{word}, jusqu’à {clock} environ"),
        }
    }
    pub fn rain_lasting(self, word: &str) -> String {
        match self {
            Lang::En => format!("{word} for at least 2 hours"),
            Lang::Fr => format!("{word} pendant au moins 2 heures"),
        }
    }
    /// Label under the 15-minute bars: "+30 min", "+1 h", "+1 h 30".
    pub fn in_minutes(self, minutes: u32) -> String {
        match (minutes / 60, minutes % 60) {
            (0, 0) => self.now().into(),
            (0, m) => format!("+{m} min"),
            (h, 0) => format!("+{h} h"),
            (h, m) => format!("+{h} h {m}"),
        }
    }

    // ── Models, air, normals ───────────────────────────────────────────

    pub fn models_agree(self, percent: u8, models: usize) -> String {
        match self {
            Lang::En => format!("Models agree {percent}% · {models} models"),
            Lang::Fr => format!("Accord des modèles {percent} % · {models} modèles"),
        }
    }

    /// European AQI levels.
    pub fn aqi_level(self, aqi: f64) -> &'static str {
        match aqi {
            a if a < 20.0 => self.pick("Good", "Bon"),
            a if a < 40.0 => self.pick("Fair", "Correct"),
            a if a < 60.0 => self.pick("Moderate", "Moyen"),
            a if a < 80.0 => self.pick("Poor", "Mauvais"),
            a if a < 100.0 => self.pick("Very poor", "Très mauvais"),
            _ => self.pick("Extremely poor", "Extrêmement mauvais"),
        }
    }
    pub fn pollen_name(self, kind: &str) -> &'static str {
        match kind {
            "alder" => self.pick("Alder", "Aulne"),
            "birch" => self.pick("Birch", "Bouleau"),
            "grass" => self.pick("Grass", "Graminées"),
            "mugwort" => self.pick("Mugwort", "Armoise"),
            "olive" => self.pick("Olive", "Olivier"),
            _ => self.pick("Ragweed", "Ambroisie"),
        }
    }
    /// 0 low, 1 moderate, 2 high, 3 very high.
    pub fn pollen_level(self, level: usize) -> &'static str {
        match level {
            0 => self.pick("Low", "Faible"),
            1 => self.pick("Moderate", "Modéré"),
            2 => self.pick("High", "Élevé"),
            _ => self.pick("Very high", "Très élevé"),
        }
    }
    pub fn no_pollen(self) -> &'static str {
        self.pick("No pollen right now", "Pas de pollen en ce moment")
    }
    pub fn not_here(self) -> &'static str {
        self.pick("Not available here", "Non disponible ici")
    }
    /// "3° warmer than usual for the date", from a signed difference.
    pub fn vs_normal(self, diff_c: f64, delta: &str) -> String {
        let d = delta.trim_start_matches(['+', '-']);
        match (self, diff_c) {
            (_, x) if x.abs() < 1.0 => self.pick("Normal for the date", "Normal pour la saison").into(),
            (Lang::En, x) if x > 0.0 => format!("{d} warmer than usual"),
            (Lang::En, _) => format!("{d} colder than usual"),
            (Lang::Fr, x) if x > 0.0 => format!("{d} de plus que la normale"),
            (Lang::Fr, _) => format!("{d} de moins que la normale"),
        }
    }

    // ── App messages ───────────────────────────────────────────────────

    pub fn loading(self) -> &'static str {
        self.pick("Loading…", "Chargement…")
    }
    pub fn load_failed(self, error: &str) -> String {
        match self {
            Lang::En => format!("Couldn’t load the forecast: {error}"),
            Lang::Fr => format!("Impossible de charger la météo : {error}"),
        }
    }
    pub fn searching(self) -> &'static str {
        self.pick("Searching…", "Recherche…")
    }
    pub fn no_city(self) -> &'static str {
        self.pick("No matching city", "Aucune ville trouvée")
    }
    pub fn search_failed(self, error: &str) -> String {
        match self {
            Lang::En => format!("Search failed: {error}"),
            Lang::Fr => format!("La recherche a échoué : {error}"),
        }
    }
    pub fn finding_location(self) -> &'static str {
        self.pick("Finding your location…", "Recherche de votre position…")
    }
    pub fn location_unavailable(self) -> &'static str {
        self.pick("Location isn’t available", "La localisation n’est pas disponible")
    }
    pub fn permission_denied(self) -> &'static str {
        self.pick("Location permission wasn’t granted", "L’accès à la position n’a pas été autorisé")
    }
    pub fn no_fix(self) -> &'static str {
        self.pick("Couldn’t find your location. Is location turned on?", "Position introuvable. La localisation est-elle activée ?")
    }
    pub fn my_location(self) -> &'static str {
        self.pick("My location", "Ma position")
    }

    /// Fixed texts of the UI.
    pub fn labels(self) -> Labels {
        Labels {
            forecast: self.pick("7-day forecast", "Prévisions sur 7 jours"),
            details: self.pick("Details", "Détails"),
            feels_like: self.pick("Feels like", "Ressenti"),
            humidity: self.pick("Humidity", "Humidité"),
            wind: self.pick("Wind", "Vent"),
            sun: self.pick("Sun", "Soleil"),
            sunrise: self.pick("Sunrise", "Lever"),
            sunset: self.pick("Sunset", "Coucher"),
            uv_index: self.pick("UV index", "Indice UV"),
            pressure: self.pick("Pressure", "Pression"),
            refresh: self.pick("Refresh", "Actualiser"),
            data_source: self.pick("Data source", "Source des données"),
            data_source_help: self.pick(
                "Forecasts come from Open-Meteo, which runs the models of national weather services. Detailed regional models are best near home; some cover fewer days.",
                "Les prévisions viennent d’Open-Meteo, qui fait tourner les modèles des services météo nationaux. Les modèles régionaux détaillés sont les meilleurs près de chez vous ; certains couvrent moins de jours.",
            ),
            search: self.pick("Search for a city", "Rechercher une ville"),
            done: self.pick("Done", "OK"),
            use_location: self.pick("Use my location", "Utiliser ma position"),
            saved_cities: self.pick("Saved cities", "Villes enregistrées"),
            next_hours: self.pick("Next 2 hours", "2 prochaines heures"),
            air_quality: self.pick("Air quality", "Qualité de l’air"),
            pollen: self.pick("Pollen", "Pollens"),
            settings: self.pick("Settings", "Réglages"),
            units: self.pick("Units", "Unités"),
            metric: self.pick("Metric (°C, km/h)", "Métrique (°C, km/h)"),
            imperial: self.pick("Imperial (°F, mph)", "Impériales (°F, mph)"),
            colors: self.pick("Colours", "Couleurs"),
            colors_weather: self.pick("From the weather", "Selon la météo"),
            colors_wallpaper: self.pick("From the wallpaper", "Selon le fond d’écran"),
            language: self.pick("Language", "Langue"),
            language_system: self.pick("Phone’s language", "Langue du téléphone"),
            notifications: self.pick("Notifications", "Notifications"),
            morning_summary: self.pick("Morning summary", "Résumé du matin"),
            morning_summary_help: self.pick("The day’s weather around 7:30", "La météo du jour vers 7 h 30"),
            rain_alerts: self.pick("Rain alerts", "Alertes pluie"),
            rain_alerts_help: self.pick("When rain is about to start", "Quand la pluie va commencer"),
        }
    }
}

/// Fixed texts of the UI, set once at startup.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Labels {
    pub forecast: &'static str,
    pub details: &'static str,
    pub feels_like: &'static str,
    pub humidity: &'static str,
    pub wind: &'static str,
    pub sun: &'static str,
    pub sunrise: &'static str,
    pub sunset: &'static str,
    pub uv_index: &'static str,
    pub pressure: &'static str,
    pub refresh: &'static str,
    pub data_source: &'static str,
    pub data_source_help: &'static str,
    pub search: &'static str,
    pub done: &'static str,
    pub use_location: &'static str,
    pub saved_cities: &'static str,
    pub next_hours: &'static str,
    pub air_quality: &'static str,
    pub pollen: &'static str,
    pub settings: &'static str,
    pub units: &'static str,
    pub metric: &'static str,
    pub imperial: &'static str,
    pub colors: &'static str,
    pub colors_weather: &'static str,
    pub colors_wallpaper: &'static str,
    pub language: &'static str,
    pub language_system: &'static str,
    pub notifications: &'static str,
    pub morning_summary: &'static str,
    pub morning_summary_help: &'static str,
    pub rain_alerts: &'static str,
    pub rain_alerts_help: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_language_from_locale() {
        assert_eq!(Lang::from_code("fr-FR"), Lang::Fr);
        assert_eq!(Lang::from_code("fr_CA.UTF-8"), Lang::Fr);
        assert_eq!(Lang::from_code("en_US"), Lang::En);
        assert_eq!(Lang::from_code("de"), Lang::En);
    }

    #[test]
    fn french_texts() {
        let fr = Lang::Fr;
        assert_eq!(fr.weekday(5), "Ven.");
        assert_eq!(fr.condition(Condition::Rain), "Pluie");
        assert_eq!(fr.updated_ago(150), "Mis à jour il y a 2 min");
        assert_eq!(fr.offline(7200), "Hors ligne · mis à jour il y a 2 h");
        assert_eq!(Lang::En.offline(7200), "Offline · updated 2 h ago");
        assert_eq!(fr.compass(225.0), "SO");
        assert_eq!(Lang::En.compass(350.0), "N");
        assert_eq!(fr.uv_level(6.5), "Élevé");
        assert_eq!(fr.daylight(568), "9 h 28 min de jour");
    }
}

//! The user's saved cities and which one is shown, persisted as JSON.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Location;

const FILE_NAME: &str = "places.json";

/// Saved cities. Never empty: the last city can't be removed, and a missing
/// or unreadable file gives Paris.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Places {
    cities: Vec<Location>,
    selected: usize,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            cities: vec![Location::paris()],
            selected: 0,
        }
    }
}

impl Places {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE_NAME))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .filter(|p| !p.cities.is_empty())
            .map(|mut p| {
                p.selected = p.selected.min(p.cities.len() - 1);
                p
            })
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        crate::cache::write_atomic(dir, FILE_NAME, &serde_json::to_vec_pretty(self)?)
    }

    pub fn cities(&self) -> &[Location] {
        &self.cities
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> &Location {
        &self.cities[self.selected]
    }

    pub fn select(&mut self, index: usize) {
        if index < self.cities.len() {
            self.selected = index;
        }
    }

    /// Adds `city` (or finds it if already saved) and selects it.
    pub fn add(&mut self, city: Location) {
        match self.cities.iter().position(|c| c.key() == city.key()) {
            Some(i) => self.selected = i,
            None => {
                self.cities.push(city);
                self.selected = self.cities.len() - 1;
            }
        }
    }

    /// Sets the device's position: replaces the previous one (so there is
    /// only ever one "📍" entry), or adds it at the top, and selects it.
    pub fn set_device(&mut self, mut here: Location) {
        here.is_device = true;
        match self.cities.iter().position(|c| c.is_device) {
            Some(i) => {
                self.cities[i] = here;
                self.selected = i;
            }
            None => {
                self.cities.insert(0, here);
                self.selected = 0;
            }
        }
    }

    /// Removes a city, keeping at least one. The selection follows the city
    /// that was selected, or moves to its neighbour if that one was removed.
    pub fn remove(&mut self, index: usize) {
        if self.cities.len() <= 1 || index >= self.cities.len() {
            return;
        }
        self.cities.remove(index);
        if index < self.selected || self.selected == self.cities.len() {
            self.selected -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn city(name: &str, lat: f64) -> Location {
        Location::new(name, "", lat, 0.0)
    }

    #[test]
    fn add_select_remove() {
        let mut p = Places::default();
        p.add(city("Lyon", 45.75));
        p.add(city("Nice", 43.7));
        assert_eq!(p.selected().name, "Nice");

        // Adding a saved city again just selects it.
        p.add(city("Lyon again", 45.7501));
        assert_eq!((p.cities().len(), p.selected().name.as_str()), (3, "Lyon"));

        // Removing a city before the selection keeps the same city selected.
        p.remove(0);
        assert_eq!(p.selected().name, "Lyon");
        // Removing the selected last city selects its neighbour.
        p.select(1);
        p.remove(1);
        assert_eq!(p.selected().name, "Lyon");
        // The last city can't be removed.
        p.remove(0);
        assert_eq!(p.cities().len(), 1);
    }

    #[test]
    fn device_position_is_a_single_entry_at_the_top() {
        let mut p = Places::default();
        p.set_device(city("Here", 10.0));
        assert_eq!((p.selected_index(), p.cities().len()), (0, 2));
        assert!(p.selected().is_device);

        // Moving replaces the entry instead of adding another one.
        p.select(1);
        p.set_device(city("Elsewhere", 20.0));
        assert_eq!(p.cities().len(), 2);
        assert_eq!(p.selected().name, "Elsewhere");
        assert_eq!(p.cities()[1].name, "Paris");
    }

    #[test]
    fn persists_and_falls_back_to_paris() {
        let dir = std::env::temp_dir().join(format!("meteo-places-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Places::load(&dir), Places::default());

        let mut p = Places::default();
        p.add(city("Lyon", 45.75));
        p.save(&dir).unwrap();
        assert_eq!(Places::load(&dir), p);

        std::fs::write(dir.join(FILE_NAME), b"[]").unwrap();
        assert_eq!(Places::load(&dir), Places::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

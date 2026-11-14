//! Rain radar: the last two hours of radar images from RainViewer, over an
//! OpenStreetMap base map, as a 3 × 3 block of map tiles around the place.

use serde_json::Value;

use crate::api::{Error, client};

/// About 220 km per tile in mid-latitudes; RainViewer's free tiles go up to 7.
pub const ZOOM: u32 = 7;
/// Tiles per side of the block.
pub const GRID: u32 = 3;
pub const TILE: u32 = 256;

pub const CREDIT: &str = "Radar: RainViewer · Map: © OpenStreetMap contributors";

/// One radar image.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Unix time.
    pub time: i64,
    /// The image's address, without the tile part.
    pub url: String,
}

/// The tiles around a place, row by row, and where the place falls in the
/// block, in pixels from its top left corner.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub tiles: Vec<(u32, u32)>,
    pub place: (f64, f64),
}

/// Web Mercator, as every web map: the place in pixels at `ZOOM`.
fn world_pixel(latitude: f64, longitude: f64) -> (f64, f64) {
    let size = f64::from(TILE) * f64::from(1u32 << ZOOM);
    let lat = latitude.clamp(-85.0, 85.0).to_radians();
    let x = (longitude + 180.0) / 360.0 * size;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * size;
    (x, y)
}

pub fn block(latitude: f64, longitude: f64) -> Block {
    let (x, y) = world_pixel(latitude, longitude);
    let n = 1i64 << ZOOM;
    let (cx, cy) = ((x / f64::from(TILE)) as i64, (y / f64::from(TILE)) as i64);
    let half = i64::from(GRID / 2);
    let (left, top) = (cx - half, (cy - half).clamp(0, n - i64::from(GRID)));
    let mut tiles = Vec::new();
    for row in 0..i64::from(GRID) {
        for col in 0..i64::from(GRID) {
            // Wrapping around the date line.
            tiles.push(((left + col).rem_euclid(n) as u32, (top + row) as u32));
        }
    }
    let place = (x - (left * i64::from(TILE)) as f64, y - (top * i64::from(TILE)) as f64);
    Block { tiles, place }
}

/// OpenStreetMap's own tiles (the app greys them to suit the radar).
pub fn map_url((x, y): (u32, u32)) -> String {
    format!("https://tile.openstreetmap.org/{ZOOM}/{x}/{y}.png")
}

/// Colour scheme 2 ("universal blue"), smoothed, snow shown apart.
pub fn radar_url(frame: &Frame, (x, y): (u32, u32)) -> String {
    format!("{}/{TILE}/{ZOOM}/{x}/{y}/2/1_1.png", frame.url)
}

pub async fn frames() -> Result<Vec<Frame>, Error> {
    let body = client()?
        .get("https://api.rainviewer.com/public/weather-maps.json")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    parse_frames(&body)
}

/// The past frames, oldest first.
pub fn parse_frames(json: &[u8]) -> Result<Vec<Frame>, Error> {
    let v: Value = serde_json::from_slice(json)?;
    let host = v["host"].as_str().ok_or(Error::MissingData)?;
    let frames: Vec<Frame> = v["radar"]["past"]
        .as_array()
        .ok_or(Error::MissingData)?
        .iter()
        .filter_map(|f| Some(Frame { time: f["time"].as_i64()?, url: format!("{host}{}", f["path"].as_str()?) }))
        .collect();
    if frames.is_empty() { Err(Error::MissingData) } else { Ok(frames) }
}

/// One tile's PNG, or `None` if it couldn't be had.
pub async fn tile(url: String) -> Option<Vec<u8>> {
    let response = client().ok()?.get(url).send().await.ok()?.error_for_status().ok()?;
    Some(response.bytes().await.ok()?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_around_grenoble() {
        let b = block(45.19, 5.72);
        // Tile 66, 45 at zoom 7 holds Grenoble: the block is 65–67 × 44–46.
        assert_eq!(b.tiles.len(), 9);
        assert_eq!(b.tiles[0], (65, 44));
        assert_eq!(b.tiles[4], (66, 45));
        assert_eq!(b.tiles[8], (67, 46));
        // In the middle tile.
        assert!((256.0..512.0).contains(&b.place.0) && (256.0..512.0).contains(&b.place.1), "{:?}", b.place);
    }

    #[test]
    fn block_across_the_date_line() {
        let b = block(-41.0, 179.9); // near Wellington's longitude, east edge
        assert_eq!(b.tiles[2].0, 0, "wraps to the first column");
        assert_eq!(b.tiles[0].0, 126);
    }

    #[test]
    fn frames_from_rainviewer() {
        let json = br#"{"host":"https://tilecache.rainviewer.com","radar":{"past":[
            {"time":1794640800,"path":"/v2/radar/c0b6dac30f2e"},{"time":1794641400,"path":"/v2/radar/3b8f824b148b"}],"nowcast":[]}}"#;
        let f = parse_frames(json).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(
            radar_url(&f[1], (66, 45)),
            "https://tilecache.rainviewer.com/v2/radar/3b8f824b148b/256/7/66/45/2/1_1.png"
        );
        assert_eq!(map_url((66, 45)), "https://tile.openstreetmap.org/7/66/45.png");
        assert!(parse_frames(br#"{"host":"h","radar":{"past":[]}}"#).is_err());
    }
}

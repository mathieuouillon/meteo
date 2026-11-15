//! Rain radar: the last two hours of radar images from RainViewer, over an
//! OpenStreetMap base map, as a 3 × 3 block of map tiles around the place;
//! then the next three hours, forecast at a grid of points over the same map.

use serde_json::Value;

use crate::api::{Error, client};

/// About 220 km per tile in mid-latitudes; RainViewer's free tiles go up to 7.
pub const ZOOM: u32 = 7;
/// Twice as wide (a whole country like France).
pub const WIDE_ZOOM: u32 = 6;
/// Forecast points per side of the picture.
pub const RAIN_GRID: usize = 15;
/// 15-minute forecast steps after the radar: three hours.
pub const FORECAST_STEPS: usize = 12;
/// Tiles per side of the block.
pub const GRID: u32 = 3;
pub const TILE: u32 = 256;

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
    pub zoom: u32,
    pub tiles: Vec<(u32, u32)>,
    pub place: (f64, f64),
    /// The block's top left corner, in world pixels.
    pub corner: (f64, f64),
}

impl Block {
    /// The latitude and longitude of a point of the block, in pixels from
    /// its top left corner.
    pub fn lat_lon(&self, x: f64, y: f64) -> (f64, f64) {
        let size = f64::from(TILE) * f64::from(1u32 << self.zoom);
        let (wx, wy) = (self.corner.0 + x, self.corner.1 + y);
        let lon = (wx / size * 360.0 - 180.0 + 540.0).rem_euclid(360.0) - 180.0;
        let n = std::f64::consts::PI * (1.0 - 2.0 * wy / size);
        (n.sinh().atan().to_degrees(), lon)
    }
}

/// Web Mercator, as every web map: the place in pixels at `zoom`.
fn world_pixel(latitude: f64, longitude: f64, zoom: u32) -> (f64, f64) {
    let size = f64::from(TILE) * f64::from(1u32 << zoom);
    let lat = latitude.clamp(-85.0, 85.0).to_radians();
    let x = (longitude + 180.0) / 360.0 * size;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * size;
    (x, y)
}

pub fn block(latitude: f64, longitude: f64, zoom: u32) -> Block {
    let (x, y) = world_pixel(latitude, longitude, zoom);
    let n = 1i64 << zoom;
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
    let corner = ((left * i64::from(TILE)) as f64, (top * i64::from(TILE)) as f64);
    Block { zoom, tiles, place: (x - corner.0, y - corner.1), corner }
}

/// OpenStreetMap's own tiles (the app greys them to suit the radar).
pub fn map_url(zoom: u32, (x, y): (u32, u32)) -> String {
    format!("https://tile.openstreetmap.org/{zoom}/{x}/{y}.png")
}

/// Colour scheme 2 ("universal blue"), smoothed, snow shown apart.
pub fn radar_url(frame: &Frame, zoom: u32, (x, y): (u32, u32)) -> String {
    format!("{}/{TILE}/{zoom}/{x}/{y}/2/1_1.png", frame.url)
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

/// The next three hours of rain at a grid of points.
#[derive(Debug, Clone, PartialEq)]
pub struct RainForecast {
    /// Unix time at the end of each 15 minutes.
    pub times: Vec<i64>,
    /// Per step, the rain rate (mm/h) at each point, row by row.
    pub steps: Vec<Vec<f64>>,
}

/// One request for all the points (latitude, longitude), row by row.
pub async fn rain_forecast(points: &[(f64, f64)]) -> Result<RainForecast, Error> {
    let join = |f: fn(&(f64, f64)) -> f64| points.iter().map(|p| format!("{:.3}", f(p))).collect::<Vec<_>>().join(",");
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &minutely_15=precipitation&forecast_minutely_15={FORECAST_STEPS}&timeformat=unixtime&timezone=GMT",
        join(|p| p.0),
        join(|p| p.1)
    );
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_rain_forecast(&body, points.len())
}

/// Open-Meteo answers a list (one entry per point) for several points.
pub fn parse_rain_forecast(json: &[u8], points: usize) -> Result<RainForecast, Error> {
    let v: Value = serde_json::from_slice(json)?;
    let places = v.as_array().ok_or(Error::MissingData)?;
    if places.len() != points {
        return Err(Error::MissingData);
    }
    let times: Vec<i64> = places[0]["minutely_15"]["time"]
        .as_array()
        .ok_or(Error::MissingData)?
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    let steps = (0..times.len())
        .map(|i| {
            places
                .iter()
                // mm in 15 minutes, as mm/h.
                .map(|p| p["minutely_15"]["precipitation"].get(i).and_then(Value::as_f64).unwrap_or(0.0) * 4.0)
                .collect()
        })
        .collect();
    Ok(RainForecast { times, steps })
}

/// Rain rates (mm/h) and their colours, like RainViewer's "universal blue":
/// blues for rain, then yellow, orange and red for downpours.
pub const RAIN_COLOURS: [(f64, [u8; 3]); 6] = [
    (0.2, [166, 225, 255]),
    (1.0, [72, 168, 240]),
    (3.0, [35, 83, 200]),
    (8.0, [247, 210, 62]),
    (16.0, [240, 138, 36]),
    (40.0, [215, 38, 61]),
];

/// Below this, nothing is drawn; up to the scale's first step, rain fades in
/// (soft edges instead of a hard outline).
const RAIN_FADE_FROM: f64 = 0.1;

/// The colour for a rain rate, blended between the scale's steps; `None`
/// below a trace. The alpha grows with light rain.
pub fn rain_colour(mm_per_hour: f64) -> Option<[u8; 4]> {
    let (first, _) = RAIN_COLOURS[0];
    if mm_per_hour < RAIN_FADE_FROM {
        return None;
    }
    let i = RAIN_COLOURS.iter().rposition(|(at, _)| mm_per_hour >= *at).unwrap_or(0);
    let (at, c) = RAIN_COLOURS[i];
    let rgb = match RAIN_COLOURS.get(i + 1) {
        Some(&(next_at, n)) => {
            let k = ((mm_per_hour - at) / (next_at - at)).clamp(0.0, 1.0);
            [0, 1, 2].map(|j| (f64::from(c[j]) + (f64::from(n[j]) - f64::from(c[j])) * k).round() as u8)
        }
        None => c,
    };
    let alpha = if mm_per_hour < first {
        150.0 * (mm_per_hour - RAIN_FADE_FROM) / (first - RAIN_FADE_FROM)
    } else {
        150.0 + 105.0 * ((mm_per_hour - first) / 2.0).clamp(0.0, 1.0)
    };
    let alpha = alpha.round() as u8;
    Some([rgb[0], rgb[1], rgb[2], alpha])
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
        let b = block(45.19, 5.72, ZOOM);
        // Tile 66, 45 at zoom 7 holds Grenoble: the block is 65–67 × 44–46.
        assert_eq!(b.tiles.len(), 9);
        assert_eq!(b.tiles[0], (65, 44));
        assert_eq!(b.tiles[4], (66, 45));
        assert_eq!(b.tiles[8], (67, 46));
        // In the middle tile.
        assert!((256.0..512.0).contains(&b.place.0) && (256.0..512.0).contains(&b.place.1), "{:?}", b.place);
    }

    #[test]
    fn points_of_the_block_back_to_places() {
        for zoom in [ZOOM, WIDE_ZOOM] {
            let b = block(45.19, 5.72, zoom);
            let (lat, lon) = b.lat_lon(b.place.0, b.place.1);
            assert!((lat - 45.19).abs() < 1e-9 && (lon - 5.72).abs() < 1e-9, "{lat} {lon}");
        }
        // The wide block spans about twice the distance.
        let (near, wide) = (block(45.19, 5.72, ZOOM), block(45.19, 5.72, WIDE_ZOOM));
        let span = |b: &Block| b.lat_lon(768.0, 0.0).1 - b.lat_lon(0.0, 0.0).1;
        assert!((span(&wide) / span(&near) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rain_forecast_at_points() {
        let json = br#"[{"minutely_15":{"time":[100,1000],"precipitation":[0.0,0.5]}},
                        {"minutely_15":{"time":[100,1000],"precipitation":[1.0,null]}}]"#;
        let f = parse_rain_forecast(json, 2).unwrap();
        assert_eq!(f.times, [100, 1000]);
        assert_eq!(f.steps, [vec![0.0, 4.0], vec![2.0, 0.0]], "mm/h, missing as dry");
        assert!(parse_rain_forecast(json, 3).is_err());
    }

    #[test]
    fn rain_colours() {
        assert_eq!(rain_colour(0.05), None, "below a trace");
        assert!(rain_colour(0.15).unwrap()[3] < rain_colour(0.2).unwrap()[3], "fading in");
        assert_eq!(rain_colour(1.0).map(|c| [c[0], c[1], c[2]]), Some([72, 168, 240]));
        assert_eq!(rain_colour(2.0).map(|c| c[2]), Some(220), "between 1 and 3 mm/h");
        assert_eq!(rain_colour(100.0).map(|c| [c[0], c[3]]), Some([215, 255]));
        assert!(rain_colour(0.2).unwrap()[3] < rain_colour(3.0).unwrap()[3], "light rain fainter");
    }

    #[test]
    fn block_across_the_date_line() {
        let b = block(-41.0, 179.9, ZOOM); // near Wellington's longitude, east edge
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
            radar_url(&f[1], ZOOM, (66, 45)),
            "https://tilecache.rainviewer.com/v2/radar/3b8f824b148b/256/7/66/45/2/1_1.png"
        );
        assert_eq!(map_url(ZOOM, (66, 45)), "https://tile.openstreetmap.org/7/66/45.png");
        assert!(parse_frames(br#"{"host":"h","radar":{"past":[]}}"#).is_err());
    }
}

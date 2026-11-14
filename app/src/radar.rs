//! The rain radar screen: downloads the map and the radar frames around the
//! place, puts each together into one picture, and plays them in a loop.

use std::cell::RefCell;
use std::time::Duration;

use meteo_core::radar::{self, Frame, GRID, TILE};
use meteo_core::{Lang, Location};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, Weak};

use crate::{MainWindow, block_on};

/// The pictures' side, in pixels, centred on the place.
const SIDE: u32 = 512;
const STEP: Duration = Duration::from_millis(500);
/// The newest frame stays this many steps before the loop starts again.
const HOLD: u32 = 4;

type Buffer = SharedPixelBuffer<Rgba8Pixel>;

#[derive(Default)]
struct Player {
    /// Time label and picture of each frame, oldest first.
    frames: Vec<(SharedString, Image)>,
    at: usize,
    hold: u32,
    playing: bool,
    timer: Timer,
    /// Bumped on each opening, so a download for an earlier one is dropped.
    generation: u64,
}

thread_local! {
    static PLAYER: RefCell<Player> = RefCell::default();
}

/// Opens the radar for `place`: the newest frame first, then the others.
pub fn open(ui: &MainWindow, place: &Location, utc_offset: i64, lang: Lang) {
    let generation = PLAYER.with(|p| {
        let mut p = p.borrow_mut();
        p.frames.clear();
        p.timer.stop();
        p.generation += 1;
        p.generation
    });
    ui.set_radar_map(Image::default());
    ui.set_radar_rain(Image::default());
    ui.set_radar_time(SharedString::new());
    ui.set_radar_status(lang.labels().radar_loading.into());
    let surface = ui.get_theme().surface;
    let dark = u32::from(surface.red()) + u32::from(surface.green()) + u32::from(surface.blue()) < 3 * 128;
    let block = radar::block(place.latitude, place.longitude);
    let (origin, marker) = crop(block.place);
    ui.set_radar_marker_x((marker.0 / f64::from(SIDE)) as f32);
    ui.set_radar_marker_y((marker.1 / f64::from(SIDE)) as f32);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        block_on(async {
            let Ok(frames) = radar::frames().await else {
                return show_failure(&weak, generation, lang);
            };
            let last = frames.len() - 1;
            let map_urls = block.tiles.iter().map(|&t| radar::map_url(t)).collect();
            let rain_urls = |f: &Frame| block.tiles.iter().map(|&t| radar::radar_url(f, t)).collect::<Vec<_>>();
            let (map, newest) = tokio::join!(fetch_all(map_urls), fetch_all(rain_urls(&frames[last])));
            if map.iter().all(Option::is_none) {
                return show_failure(&weak, generation, lang);
            }
            let mut map = compose(&map, origin);
            grey(&mut map, dark);
            let newest = compose(&newest, origin);
            let time = clock(frames[last].time, utc_offset);
            let w = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = w.upgrade() else { return };
                if !current(generation) {
                    return;
                }
                ui.set_radar_map(Image::from_rgba8(map));
                ui.set_radar_status(lang.labels().radar_hint.into());
                PLAYER.with(|p| p.borrow_mut().frames.push((time.into(), Image::from_rgba8(newest))));
                show_frame(&ui, 0);
            });
            // The older frames, all at once, then the loop starts.
            let older: Vec<(i64, Buffer)> = {
                let mut out = Vec::new();
                for f in &frames[..last] {
                    out.push((f.time, compose(&fetch_all(rain_urls(f)).await, origin)));
                }
                out
            };
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                if !current(generation) {
                    return;
                }
                PLAYER.with(|p| {
                    let mut p = p.borrow_mut();
                    let newest = p.frames.pop();
                    p.frames =
                        older.into_iter().map(|(t, b)| (clock(t, utc_offset).into(), Image::from_rgba8(b))).collect();
                    p.frames.extend(newest);
                    p.at = p.frames.len() - 1;
                });
                play(&ui, true);
            });
        });
    });
}

/// Closing: stops the loop and lets the pictures go.
pub fn close() {
    PLAYER.with(|p| {
        let mut p = p.borrow_mut();
        p.timer.stop();
        p.frames.clear();
        p.generation += 1;
    });
}

/// Tap on the map: pause or play.
pub fn toggle(ui: &MainWindow) {
    let playing = PLAYER.with(|p| p.borrow().playing);
    play(ui, !playing);
}

fn play(ui: &MainWindow, on: bool) {
    let weak = ui.as_weak();
    PLAYER.with(|p| {
        let mut p = p.borrow_mut();
        p.playing = on && p.frames.len() > 1;
        if !p.playing {
            p.timer.stop();
            return;
        }
        p.timer.start(TimerMode::Repeated, STEP, move || {
            let Some(ui) = weak.upgrade() else { return };
            let next = PLAYER.with(|p| {
                let mut p = p.borrow_mut();
                let last = p.frames.len().saturating_sub(1);
                if p.at == last && p.hold < HOLD {
                    p.hold += 1;
                    return None;
                }
                p.hold = 0;
                p.at = if p.at >= last { 0 } else { p.at + 1 };
                Some(p.at)
            });
            if let Some(i) = next {
                show_frame(&ui, i);
            }
        });
    });
    ui.set_radar_playing(on);
}

fn show_frame(ui: &MainWindow, i: usize) {
    let frame = PLAYER.with(|p| p.borrow().frames.get(i).cloned());
    if let Some((time, image)) = frame {
        ui.set_radar_time(time);
        ui.set_radar_rain(image);
    }
}

fn current(generation: u64) -> bool {
    PLAYER.with(|p| p.borrow().generation == generation)
}

fn show_failure(weak: &Weak<MainWindow>, generation: u64, lang: Lang) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade()
            && current(generation)
        {
            ui.set_radar_status(lang.labels().radar_failed.into());
        }
    });
}

/// "10:20", local time at the place.
fn clock(unix: i64, utc_offset: i64) -> String {
    let t = meteo_core::date::local_iso_minute(unix, utc_offset);
    t.get(11..16).unwrap_or_default().to_owned()
}

/// Every URL at once; `None` where one failed.
async fn fetch_all(urls: Vec<String>) -> Vec<Option<Vec<u8>>> {
    let mut set = tokio::task::JoinSet::new();
    for (i, url) in urls.into_iter().enumerate() {
        set.spawn(async move { (i, radar::tile(url).await) });
    }
    let mut out = vec![None; set.len()];
    while let Some(Ok((i, bytes))) = set.join_next().await {
        out[i] = bytes;
    }
    out
}

/// Where the picture starts in the block, and where the place is in it.
fn crop(place: (f64, f64)) -> ((u32, u32), (f64, f64)) {
    let max = f64::from(GRID * TILE - SIDE);
    let half = f64::from(SIDE / 2);
    let x = (place.0 - half).round().clamp(0.0, max);
    let y = (place.1 - half).round().clamp(0.0, max);
    ((x as u32, y as u32), (place.0 - x, place.1 - y))
}

/// The block's tiles as one picture, cut to `SIDE` from `origin`; a missing
/// tile stays transparent.
fn compose(tiles: &[Option<Vec<u8>>], origin: (u32, u32)) -> Buffer {
    let mut out = Buffer::new(SIDE, SIDE);
    let pixels = out.make_mut_slice();
    for (i, tile) in tiles.iter().enumerate() {
        let Some((w, h, rgba)) = tile.as_deref().and_then(decode) else { continue };
        let (left, top) = ((i as u32 % GRID) * TILE, (i as u32 / GRID) * TILE);
        for y in 0..h.min(TILE) {
            let Some(row) = (top + y).checked_sub(origin.1).filter(|r| *r < SIDE) else { continue };
            for x in 0..w.min(TILE) {
                let Some(col) = (left + x).checked_sub(origin.0).filter(|c| *c < SIDE) else { continue };
                let s = ((y * w + x) * 4) as usize;
                pixels[(row * SIDE + col) as usize] = Rgba8Pixel::new(rgba[s], rgba[s + 1], rgba[s + 2], rgba[s + 3]);
            }
        }
    }
    out
}

/// The map in soft greys, so the radar's colours stand out: light, or
/// inverted for a dark theme.
fn grey(map: &mut Buffer, dark: bool) {
    for p in map.make_mut_slice() {
        let g = (0.3 * f32::from(p.r) + 0.59 * f32::from(p.g) + 0.11 * f32::from(p.b)) / 255.0;
        let v = if dark { 0.1 + (1.0 - g) * 0.35 } else { 1.0 - (1.0 - g) * 0.55 };
        let v = (v * 255.0).round() as u8;
        (p.r, p.g, p.b) = (v, v, v);
    }
}

/// A PNG as RGBA bytes.
fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None, // expanded by the transformations
    };
    Some((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_keeps_the_place_in_the_middle() {
        let ((x, y), marker) = crop((400.0, 300.0));
        assert_eq!((x, y), (144, 44));
        assert_eq!(marker, (256.0, 256.0));
        // Near the block's edge, the picture stops at the edge.
        let ((x, _), marker) = crop((700.0, 384.0));
        assert_eq!(x, 256);
        assert_eq!(marker.0, 444.0);
    }

    #[test]
    fn tiles_put_together() {
        // A 1 × 1 red PNG stands in for a tile: only its corner pixel is set.
        let mut png_bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png_bytes, 1, 1);
            e.set_color(png::ColorType::Rgb);
            e.write_header().unwrap().write_image_data(&[255, 0, 0]).unwrap();
        }
        let mut tiles = vec![None; 9];
        tiles[4] = Some(png_bytes); // the middle tile: its corner is at 256, 256
        let out = compose(&tiles, (128, 128));
        let p = out.as_slice()[(128 * SIDE + 128) as usize];
        assert_eq!((p.r, p.g, p.b, p.a), (255, 0, 0, 255));
        assert_eq!(out.as_slice()[0].a, 0, "missing tiles stay transparent");
    }
}

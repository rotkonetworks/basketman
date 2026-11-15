//! Sponsor boards: what is printed on the twenty slots of an arena.
//!
//! The renderer asks for one thing, a board texture per slot. Where it comes
//! from is decided here, in this order:
//!
//! 1. A real advertiser: an entry of the sponsor manifest
//!    (`assets/site/sponsors.json`) whose `slots` (and optional `clubs`)
//!    match. Its picture is a PNG the game's owner ships next to the
//!    manifest.
//! 2. The in-game sponsor who holds the slot (a fictional company, drawn
//!    here from its name on its brand colours with a bitmap font).
//! 3. The league's own board, for slots nobody bought.
//!
//! Everything lives in one texture atlas, so a whole frame is one mesh. The
//! atlas has a wide cell (256 x 64) and a square cell (128 x 128) for every
//! board, and a white pixel for the flat-coloured triangles of the scene.
//!
//! The manifest is plain JSON:
//!
//! ```json
//! { "version": 1,
//!   "boards": [
//!     { "name": "Acme Hoops",              // shown in tooltips and used as the alt text
//!       "image": "sponsors/acme.png",      // PNG, relative to the manifest
//!       "slots": ["courtside", "centre"],  // see `slot_matches`; omitted or empty = nothing
//!       "clubs": ["Helsinki Kings"],       // optional: team or arena names; omitted = every club
//!       "bg": [255, 255, 255] }            // optional: what to fill around the picture with
//!   ] }
//! ```
//!
//! The first entry that matches a club and a slot wins.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, ColorImage, Context, Pos2, TextureHandle, TextureOptions, pos2};
use serde::Deserialize;

use basketman_core::sponsor::{SlotKind, slot_kind};

use super::font::{CHAR_H, CHAR_W, GLYPHS};

/// The wide board cell, in pixels.
pub const WIDE: (usize, usize) = (512, 128);
/// The square board cell.
pub const SQUARE: (usize, usize) = (256, 256);
/// One entry of the atlas: a wide and a square cell side by side.
const BLOCK: (usize, usize) = (WIDE.0 + SQUARE.0, SQUARE.1);
const COLUMNS: usize = 4;

/// The league's own board.
const LEAGUE_NAME: &str = "BASKETMAN LEAGUE";
const LEAGUE_BG: [u8; 3] = [24, 48, 104];
const LEAGUE_FG: [u8; 3] = [240, 240, 236];

// ----------------------------------------------------------- the manifest

/// One real advertiser's boards.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ManifestBoard {
    pub name: String,
    /// PNG path relative to the manifest.
    #[serde(default)]
    pub image: Option<String>,
    /// Which slots: `all`, `courtside`, `table_side`, `far_side`,
    /// `baseline`, `baseline_west`, `baseline_east`, `centre`, `padding`,
    /// `table`, or a slot number as text.
    #[serde(default)]
    pub slots: Vec<String>,
    /// Club or arena names this applies to; empty means every club.
    #[serde(default)]
    pub clubs: Vec<String>,
    /// A second picture for the square boards (the basket padding), when
    /// the main one is drawn for the long boards. PNG path relative to the
    /// manifest.
    #[serde(default)]
    pub image_square: Option<String>,
    /// Fill around the picture, if it does not have the board's shape.
    #[serde(default)]
    pub bg: Option<[u8; 3]>,
    /// The board as geometry: a JSON file of triangles made by
    /// `tools/board_mesh.py`, relative to the manifest. A picture goes soft
    /// when it is seen small and at an angle; triangles do not. When this
    /// loads it is drawn instead of `image`, in `ink` on `bg`.
    #[serde(default)]
    pub vector: Option<String>,
    /// The same for the square boards.
    #[serde(default)]
    pub vector_square: Option<String>,
    /// Colour of the vector artwork.
    #[serde(default)]
    pub ink: Option<[u8; 3]>,
}

/// A board's artwork as triangles: `w` wide and 1 high, origin bottom left.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VectorArt {
    pub w: f32,
    /// Six numbers a triangle: x0, y0, x1, y1, x2, y2.
    pub tris: Vec<f32>,
}

impl VectorArt {
    pub fn parse(bytes: &[u8]) -> Option<VectorArt> {
        let art: VectorArt = serde_json::from_slice(bytes).ok()?;
        (art.w > 0.0 && art.tris.len() >= 6 && art.tris.len().is_multiple_of(6)).then_some(art)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Manifest {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub boards: Vec<ManifestBoard>,
}

impl Manifest {
    /// Read a manifest. Unknown keys (such as `_comment`) are ignored.
    pub fn parse(bytes: &[u8]) -> Result<Manifest, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("sponsors.json: {e}"))
    }

    /// The entry that owns `slot` of the arena of `club` (a team or arena
    /// name), if any.
    pub fn resolve(&self, club: &str, arena: &str, slot: usize) -> Option<usize> {
        self.boards.iter().position(|b| {
            let club_ok = b.clubs.is_empty()
                || b.clubs
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(club) || c.eq_ignore_ascii_case(arena));
            club_ok && b.slots.iter().any(|s| slot_matches(s, slot))
        })
    }
}

/// Does the selector `sel` name slot `slot`?
pub fn slot_matches(sel: &str, slot: usize) -> bool {
    let kind = slot_kind(slot);
    match sel.trim().to_ascii_lowercase().as_str() {
        "all" => true,
        "courtside" => matches!(kind, SlotKind::TableSide(_) | SlotKind::FarSide(_)),
        "table_side" => matches!(kind, SlotKind::TableSide(_)),
        "far_side" => matches!(kind, SlotKind::FarSide(_)),
        "baseline" => matches!(kind, SlotKind::BaselineA(_) | SlotKind::BaselineB(_)),
        "baseline_west" => matches!(kind, SlotKind::BaselineA(_)),
        "baseline_east" => matches!(kind, SlotKind::BaselineB(_)),
        "centre" | "center" => kind == SlotKind::Centre,
        "padding" => matches!(kind, SlotKind::Padding(_)),
        "table" => kind == SlotKind::Table,
        n => n.parse::<usize>().is_ok_and(|n| n == slot),
    }
}

// ---------------------------------------------------------- the pictures

/// An RGBA picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Pic {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Pic {
    fn filled(w: usize, h: usize, c: [u8; 3]) -> Pic {
        let mut rgba = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        Pic { w, h, rgba }
    }

    fn put(&mut self, x: usize, y: usize, c: [u8; 3]) {
        if x < self.w && y < self.h {
            let i = (y * self.w + x) * 4;
            self.rgba[i..i + 3].copy_from_slice(&c);
        }
    }

    #[cfg(test)]
    pub fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}

/// Width of `text` in glyph cells (the last glyph needs no gap).
fn text_cells(text: &str) -> usize {
    (text.chars().count() * CHAR_W).saturating_sub(2)
}

fn glyph(c: char) -> &'static [u8; CHAR_H] {
    let c = c.to_ascii_uppercase();
    let i = if (' '..='~').contains(&c) {
        c as usize - 32
    } else {
        '?' as usize - 32
    };
    &GLYPHS[i]
}

/// Draw `text` with its top left at (`x`, `y`), each font pixel `s` pixels.
fn draw_text(pic: &mut Pic, text: &str, x: usize, y: usize, s: usize, c: [u8; 3]) {
    for (n, ch) in text.chars().enumerate() {
        let g = glyph(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits >> col & 1 == 1 {
                    for dy in 0..s {
                        for dx in 0..s {
                            pic.put(x + (n * CHAR_W + col) * s + dx, y + row * s + dy, c);
                        }
                    }
                }
            }
        }
    }
}

/// Lay `name` out in up to `max_lines` lines inside `w` x `h` pixels, at the
/// largest scale that fits. Returns the lines and the scale.
fn layout(name: &str, w: usize, h: usize, max_lines: usize) -> (Vec<String>, usize) {
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut best: (Vec<String>, usize) = (vec![name.to_string()], 1);
    for lines in 1..=max_lines.min(words.len().max(1)) {
        // Split the words into `lines` runs, as even as the text allows.
        let total: usize = words.iter().map(|w| w.len() + 1).sum();
        let mut out: Vec<String> = vec![String::new(); lines];
        let mut k = 0;
        let mut used = 0;
        for (i, word) in words.iter().enumerate() {
            let left = words.len() - i;
            if k + 1 < lines
                && !out[k].is_empty()
                && (used * lines >= total * (k + 1) || left < lines - k)
            {
                k += 1;
            }
            if !out[k].is_empty() {
                out[k].push(' ');
            }
            out[k].push_str(word);
            used += word.len() + 1;
        }
        out.retain(|l| !l.is_empty());
        let widest = out.iter().map(|l| text_cells(l)).max().unwrap_or(1);
        let tall = out.len() * CHAR_H - (out.len() - 1) * 3;
        let s = ((w * 88 / 100) / widest).min((h * 80 / 100) / tall).max(1);
        if s > best.1 || best.1 == 1 && lines == 1 {
            best = (out, s);
        }
    }
    best
}

/// A board with `name` on brand colours, `w` x `h` pixels.
pub fn render_text_board(
    name: &str,
    bg: [u8; 3],
    fg: [u8; 3],
    w: usize,
    h: usize,
    max_lines: usize,
) -> Pic {
    let mut pic = Pic::filled(w, h, bg);
    // A rule along the top and bottom in the lettering colour.
    for x in 0..w {
        for t in 0..(h / 24).max(1) {
            pic.put(x, t, fg);
            pic.put(x, h - 1 - t, fg);
        }
    }
    let (lines, s) = layout(&name.to_uppercase(), w, h, max_lines);
    let line_h = (CHAR_H - 3) * s;
    let total = line_h * (lines.len() - 1) + CHAR_H * s;
    let top = h.saturating_sub(total) / 2;
    for (i, l) in lines.iter().enumerate() {
        let lw = text_cells(l) * s;
        let x = w.saturating_sub(lw) / 2;
        // The glyphs sit low in their cell (three blank rows above).
        draw_text(
            &mut pic,
            l,
            x,
            (top + i * line_h).saturating_sub(3 * s / 2),
            s,
            fg,
        );
    }
    pic
}

/// Fit a PNG into a `w` x `h` cell, keeping its shape, on `bg`.
pub fn render_png(bytes: &[u8], bg: [u8; 3], w: usize, h: usize) -> Result<Pic, String> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| format!("png: {e}"))?
        .to_rgba8();
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    if iw < 1.0 || ih < 1.0 {
        return Err("png: empty".into());
    }
    let k = (w as f32 / iw).min(h as f32 / ih);
    let (nw, nh) = (((iw * k) as u32).max(1), ((ih * k) as u32).max(1));
    let small = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    let mut pic = Pic::filled(w, h, bg);
    let (ox, oy) = ((w - nw as usize) / 2, (h - nh as usize) / 2);
    for (x, y, p) in small.enumerate_pixels() {
        let a = p[3] as u32;
        let i = ((oy + y as usize) * w + ox + x as usize) * 4;
        for c in 0..3 {
            pic.rgba[i + c] = ((p[c] as u32 * a + pic.rgba[i + c] as u32 * (255 - a)) / 255) as u8;
        }
    }
    Ok(pic)
}

// ------------------------------------------------------------ the atlas

/// Where a board is drawn from.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BoardKey {
    League,
    /// A fictional sponsor, by index into the world's pool.
    Sponsor(usize),
    /// A real advertiser, by index into the manifest.
    Real(usize),
}

/// Texture rectangle of a cell: u0, v0, u1, v1.
pub type UvRect = [f32; 4];

#[derive(Copy, Clone, Debug)]
pub struct Cells {
    pub wide: UvRect,
    pub square: UvRect,
}

/// What the atlas is built from: the in-game pool and the manifest.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    /// Name, background, lettering of each fictional sponsor.
    pub sponsors: Vec<(String, [u8; 3], [u8; 3])>,
    pub manifest: Manifest,
    /// PNG bytes of each manifest entry, when they loaded.
    pub images: Vec<Option<Vec<u8>>>,
    /// The square pictures, for the entries that have one.
    pub squares: Vec<Option<Vec<u8>>>,
}

pub struct Atlas {
    pub texture: TextureHandle,
    /// Where a flat-coloured triangle samples.
    pub white: Pos2,
    cells: HashMap<BoardKey, Cells>,
    /// Identifies what it was built from.
    pub stamp: u64,
}

impl Atlas {
    pub fn cells(&self, key: BoardKey) -> Cells {
        self.cells
            .get(&key)
            .or_else(|| self.cells.get(&BoardKey::League))
            .copied()
            .expect("the league board always exists")
    }
}

/// The pictures of every board in `src`, and the cell layout. Pure; the
/// atlas texture is made from it.
pub fn compose(src: &Source) -> (ColorImage, Pos2, HashMap<BoardKey, Cells>) {
    let mut keys = vec![BoardKey::League];
    keys.extend((0..src.sponsors.len()).map(BoardKey::Sponsor));
    keys.extend((0..src.manifest.boards.len()).map(BoardKey::Real));
    let rows = keys.len().div_ceil(COLUMNS) + 1;
    let (aw, ah) = (COLUMNS * BLOCK.0, rows * BLOCK.1);
    let mut rgba = vec![0u8; aw * ah * 4];
    let mut cells = HashMap::new();
    let mut blit = |pic: &Pic, ox: usize, oy: usize| {
        for y in 0..pic.h {
            let (s, d) = (y * pic.w * 4, ((oy + y) * aw + ox) * 4);
            rgba[d..d + pic.w * 4].copy_from_slice(&pic.rgba[s..s + pic.w * 4]);
        }
    };
    for (n, key) in keys.iter().enumerate() {
        let (ox, oy) = ((n % COLUMNS) * BLOCK.0, (n / COLUMNS) * BLOCK.1);
        let (wide, square) = match key {
            BoardKey::League => (
                render_text_board(LEAGUE_NAME, LEAGUE_BG, LEAGUE_FG, WIDE.0, WIDE.1, 1),
                render_text_board(LEAGUE_NAME, LEAGUE_BG, LEAGUE_FG, SQUARE.0, SQUARE.1, 3),
            ),
            BoardKey::Sponsor(i) => {
                let (name, bg, fg) = &src.sponsors[*i];
                (
                    render_text_board(name, *bg, *fg, WIDE.0, WIDE.1, 2),
                    render_text_board(name, *bg, *fg, SQUARE.0, SQUARE.1, 3),
                )
            }
            BoardKey::Real(i) => {
                let b = &src.manifest.boards[*i];
                let bg = b.bg.unwrap_or([255, 255, 255]);
                let png = src.images.get(*i).and_then(|x| x.as_deref());
                let own = src.squares.get(*i).and_then(|x| x.as_deref());
                let pics = png.and_then(|bytes| {
                    Some((
                        render_png(bytes, bg, WIDE.0, WIDE.1).ok()?,
                        render_png(own.unwrap_or(bytes), bg, SQUARE.0, SQUARE.1).ok()?,
                    ))
                });
                // A picture that is missing or broken shows the name.
                pics.unwrap_or_else(|| {
                    (
                        render_text_board(&b.name, bg, [30, 30, 30], WIDE.0, WIDE.1, 2),
                        render_text_board(&b.name, bg, [30, 30, 30], SQUARE.0, SQUARE.1, 3),
                    )
                })
            }
        };
        blit(&wide, ox, oy);
        blit(&square, ox + WIDE.0, oy);
        let rect = |x: usize, y: usize, w: usize, h: usize| -> UvRect {
            // Half a pixel in, so linear filtering does not pull in a
            // neighbour.
            [
                (x as f32 + 0.5) / aw as f32,
                (y as f32 + 0.5) / ah as f32,
                ((x + w) as f32 - 0.5) / aw as f32,
                ((y + h) as f32 - 0.5) / ah as f32,
            ]
        };
        cells.insert(
            *key,
            Cells {
                wide: rect(ox, oy, WIDE.0, WIDE.1),
                square: rect(ox + WIDE.0, oy, SQUARE.0, SQUARE.1),
            },
        );
    }
    // The white pixel block in the last row.
    let (wx, wy) = (aw - 8, ah - 8);
    for y in wy..wy + 8 {
        for x in wx..wx + 8 {
            let i = (y * aw + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    let white = pos2((wx as f32 + 4.0) / aw as f32, (wy as f32 + 4.0) / ah as f32);
    let image = ColorImage::from_rgba_unmultiplied([aw, ah], &rgba);
    (image, white, cells)
}

/// Identifies what an atlas is built from: the sponsor pool and the manifest
/// with its pictures.
pub fn stamp(sponsors: &[(String, [u8; 3], [u8; 3])], l: &Loaded) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    for (n, bg, fg) in sponsors {
        eat(n.as_bytes());
        eat(bg);
        eat(fg);
    }
    for (i, b) in l.manifest.boards.iter().enumerate() {
        eat(b.name.as_bytes());
        for set in [&l.images, &l.squares] {
            match set.get(i) {
                Some(Some(bytes)) => eat(&(bytes.len() as u64 + 1).to_le_bytes()),
                _ => eat(&[0]),
            }
        }
    }
    h
}

/// The atlas for the sources `make` returns, made once and kept in the egui
/// context until `stamp` changes.
pub fn atlas(ctx: &Context, stamp: u64, make: impl FnOnce() -> Source) -> Arc<Atlas> {
    let id = egui::Id::NULL.with("basketman-board-atlas");
    if let Some(a) = ctx.data(|d| d.get_temp::<Arc<Atlas>>(id))
        && a.stamp == stamp
    {
        return a;
    }
    let (image, white, cells) = compose(&make());
    // Boards are mostly seen small and at an angle: without smaller copies
    // of the picture to sample from, lettering shimmers as the camera moves.
    let options = TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..TextureOptions::LINEAR
    };
    let texture = ctx.load_texture("boards", image, options);
    let a = Arc::new(Atlas {
        texture,
        white,
        cells,
        stamp,
    });
    ctx.data_mut(|d| d.insert_temp(id, a.clone()));
    a
}

// ------------------------------------------------------- loading the manifest

/// The manifest as loaded, shared through the egui context.
#[derive(Clone, Debug, Default)]
pub struct Loaded {
    pub manifest: Manifest,
    pub images: Vec<Option<Vec<u8>>>,
    pub squares: Vec<Option<Vec<u8>>>,
    /// Vector artwork of each entry, for the long and the square boards.
    pub vectors: Vec<Option<Arc<VectorArt>>>,
    pub vector_squares: Vec<Option<Arc<VectorArt>>>,
}

fn loaded_id() -> egui::Id {
    egui::Id::NULL.with("basketman-sponsor-manifest")
}

/// What has been loaded so far (nothing at first).
pub fn loaded(ctx: &Context) -> Arc<Loaded> {
    ctx.data(|d| d.get_temp::<Arc<Loaded>>(loaded_id()))
        .unwrap_or_default()
}

/// Hand over manifest bytes and the PNGs they name, which `fetch` returns by
/// relative path. Bad JSON leaves the game on its own sponsors.
pub fn install(ctx: &Context, manifest: &[u8], fetch: impl Fn(&str) -> Option<Vec<u8>>) {
    let Ok(manifest) = Manifest::parse(manifest) else {
        return;
    };
    let images = manifest
        .boards
        .iter()
        .map(|b| b.image.as_deref().and_then(&fetch))
        .collect();
    let squares = manifest
        .boards
        .iter()
        .map(|b| b.image_square.as_deref().and_then(&fetch))
        .collect();
    let art = |path: &Option<String>| {
        path.as_deref()
            .and_then(&fetch)
            .and_then(|bytes| VectorArt::parse(&bytes))
            .map(Arc::new)
    };
    let vectors = manifest.boards.iter().map(|b| art(&b.vector)).collect();
    let vector_squares = manifest
        .boards
        .iter()
        .map(|b| art(&b.vector_square))
        .collect();
    let loaded = Loaded {
        manifest,
        images,
        squares,
        vectors,
        vector_squares,
    };
    ctx.data_mut(|d| d.insert_temp(loaded_id(), Arc::new(loaded)));
    ctx.request_repaint();
}

/// Start loading the manifest, once per context.
///
/// Natively it is read from `assets/site/sponsors.json` in the crate. In the
/// browser it is fetched as `site/sponsors.json` relative to the page.
pub fn start_loading(ctx: &Context) {
    let flag = egui::Id::NULL.with("basketman-sponsor-manifest-started");
    if ctx.data(|d| d.get_temp::<bool>(flag)).is_some() {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(flag, true));
    #[cfg(not(target_arch = "wasm32"))]
    {
        // `BASKETMAN_SPONSORS_DIR` points at another folder to read from.
        let dirs = [
            std::env::var("BASKETMAN_SPONSORS_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_default(),
            std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/site")),
            std::path::PathBuf::from("assets/site"),
        ];
        for dir in dirs {
            if let Ok(bytes) = std::fs::read(dir.join("sponsors.json")) {
                install(ctx, &bytes, |p| std::fs::read(dir.join(p)).ok());
                break;
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    fetch_manifest(ctx.clone());
}

/// Browser: `site/sponsors.json` relative to the page, then every picture it
/// names (`site/<image>`), then `install`. A missing manifest or picture
/// leaves the game on its own sponsors.
#[cfg(target_arch = "wasm32")]
fn fetch_manifest(ctx: Context) {
    ehttp::fetch(ehttp::Request::get("site/sponsors.json"), move |res| {
        let Some(bytes) = res.ok().filter(|r| r.ok).map(|r| r.bytes) else {
            return;
        };
        let Ok(manifest) = Manifest::parse(&bytes) else {
            return;
        };
        let wanted: Vec<String> = manifest
            .boards
            .iter()
            .flat_map(|b| {
                [
                    b.image.clone(),
                    b.image_square.clone(),
                    b.vector.clone(),
                    b.vector_square.clone(),
                ]
            })
            .flatten()
            .collect();
        if wanted.is_empty() {
            install(&ctx, &bytes, |_| None);
            return;
        }
        let got = Arc::new(std::sync::Mutex::new(HashMap::<String, Vec<u8>>::new()));
        let left = Arc::new(std::sync::atomic::AtomicUsize::new(wanted.len()));
        for path in wanted {
            let (ctx, got, left, bytes) = (ctx.clone(), got.clone(), left.clone(), bytes.clone());
            let key = path.clone();
            ehttp::fetch(ehttp::Request::get(format!("site/{path}")), move |res| {
                if let Ok(r) = res
                    && r.ok
                    && let Ok(mut g) = got.lock()
                {
                    g.insert(key, r.bytes);
                }
                if left.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1
                    && let Ok(g) = got.lock()
                {
                    install(&ctx, &bytes, |p| g.get(p).cloned());
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use basketman_core::sponsor::SLOTS;
    use eframe::egui::Color32;

    const SAMPLE: &str = r#"{
        "version": 1, "_comment": "ignored",
        "boards": [
          { "name": "Acme", "image": "a.png", "slots": ["centre", "baseline_west"], "clubs": ["Helsinki Kings"] },
          { "name": "Everywhere", "slots": ["courtside"] },
          { "name": "ByNumber", "slots": ["19"], "clubs": ["Helsinki Arena"] }
        ] }"#;

    #[test]
    fn manifest_parses_and_ignores_comments() {
        let m = Manifest::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(m.boards.len(), 3);
        assert_eq!(m.boards[0].clubs, vec!["Helsinki Kings"]);
        assert!(Manifest::parse(b"{ nonsense").is_err());
        assert_eq!(Manifest::parse(b"{}").unwrap().boards.len(), 0);
    }

    #[test]
    fn slots_resolve_by_selector_and_club() {
        let m = Manifest::parse(SAMPLE.as_bytes()).unwrap();
        // Centre is slot 16, the west baseline boards are 12 and 13.
        assert_eq!(m.resolve("Helsinki Kings", "Helsinki Arena", 16), Some(0));
        assert_eq!(m.resolve("Tampere Foxes", "Tampere Hall", 16), None);
        assert_eq!(m.resolve("Helsinki Kings", "x", 12), Some(0));
        assert_eq!(m.resolve("Helsinki Kings", "x", 14), None);
        // Courtside is open to every club.
        assert_eq!(m.resolve("Tampere Foxes", "Tampere Hall", 3), Some(1));
        assert_eq!(m.resolve("Tampere Foxes", "Tampere Hall", 9), Some(1));
        // An arena name and a slot number work too.
        assert_eq!(m.resolve("Other", "helsinki arena", 19), Some(2));
        assert_eq!(m.resolve("Other", "Elsewhere", 19), None);
    }

    #[test]
    fn every_slot_has_a_group() {
        for s in 0..SLOTS {
            assert!(slot_matches("all", s));
        }
        let n = |sel: &str| (0..SLOTS).filter(|&s| slot_matches(sel, s)).count();
        assert_eq!(n("courtside"), 12);
        assert_eq!(n("baseline"), 4);
        assert_eq!(n("padding"), 2);
        assert_eq!(n("centre"), 1);
        assert_eq!(n("table"), 1);
    }

    #[test]
    fn text_boards_show_their_lettering() {
        let pic = render_text_board(
            "Norvik Insurance",
            [20, 40, 90],
            [250, 250, 250],
            256,
            64,
            2,
        );
        assert_eq!((pic.w, pic.h), (256, 64));
        let lit = (0..64)
            .flat_map(|y| (0..256).map(move |x| (x, y)))
            .filter(|&(x, y)| pic.at(x, y) == [250, 250, 250])
            .count();
        // Lettering and the two rules, but mostly background.
        assert!(lit > 300 && lit < 256 * 64 / 2, "{lit}");
        assert_eq!(pic.at(3, 32), [20, 40, 90]);
        // Different names give different pictures.
        let other = render_text_board("Valko Dairy", [20, 40, 90], [250, 250, 250], 256, 64, 2);
        assert_ne!(pic, other);
        // Lettering is big enough to read: a whole letter is 12 rows tall at
        // scale 1; the layout picks more than that for a short name.
        let (_, scale) = layout("VALKO DAIRY", 256, 64, 2);
        assert!(scale >= 3, "{scale}");
    }

    #[test]
    fn png_fits_the_cell() {
        let mut img = image::RgbaImage::new(40, 20);
        for p in img.pixels_mut() {
            *p = image::Rgba([200, 0, 0, 255]);
        }
        let mut bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        let pic = render_png(&bytes, [255, 255, 255], 256, 64).unwrap();
        assert_eq!(pic.at(128, 32), [200, 0, 0]);
        assert_eq!(pic.at(2, 2), [255, 255, 255]);
        let sq = render_png(&bytes, [255, 255, 255], 128, 128).unwrap();
        assert_eq!(sq.at(64, 64), [200, 0, 0]);
        assert_eq!(sq.at(64, 5), [255, 255, 255]);
        assert!(render_png(b"not a png", [0; 3], 8, 8).is_err());
    }

    #[test]
    fn the_atlas_holds_every_board_and_a_white_pixel() {
        let src = Source {
            sponsors: vec![
                ("Valko Dairy".into(), [200, 20, 20], [255, 255, 255]),
                ("Norvik Freight".into(), [20, 20, 200], [255, 255, 0]),
            ],
            manifest: Manifest::parse(SAMPLE.as_bytes()).unwrap(),
            images: vec![],
            squares: vec![],
        };
        let (img, white, cells) = compose(&src);
        assert_eq!(cells.len(), 1 + 2 + 3);
        let at = |u: f32, v: f32| {
            img[(
                (u * img.width() as f32) as usize,
                (v * img.height() as f32) as usize,
            )]
        };
        assert_eq!(at(white.x, white.y), Color32::WHITE);
        let c = cells[&BoardKey::Sponsor(0)].wide;
        // The corner of the sponsor's cell is its background.
        let px = at(c[0] + 0.002, (c[1] + c[3]) / 2.0);
        assert_eq!((px.r(), px.g(), px.b()), (200, 20, 20));
        // Cells do not overlap.
        let a = cells[&BoardKey::Sponsor(0)].wide;
        let b = cells[&BoardKey::Sponsor(1)].wide;
        assert!(a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1]);
        // A manifest entry without a picture still gets a board.
        assert!(cells.contains_key(&BoardKey::Real(2)));
    }
}

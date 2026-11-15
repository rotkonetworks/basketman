//! Look and feel: warm paper, ink text, one violet accent, mono eyebrows,
//! and frosted-glass cards floating over a faded photograph.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Rect,
    RichText, Shape, Stroke, StrokeKind, TextStyle, TextureHandle, TextureOptions, Ui, pos2,
    vec2,
};
use eframe::epaint::{Mesh, RectShape, Shadow};

pub const BG: Color32 = Color32::from_rgb(0xFA, 0xF8, 0xF3);
pub const TEXT: Color32 = Color32::from_rgb(0x1B, 0x18, 0x22);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x4C, 0x48, 0x56);
pub const TEXT_MUTE: Color32 = Color32::from_rgb(0x6A, 0x66, 0x76);
pub const LINE: Color32 = Color32::from_rgb(0xE6, 0xE1, 0xD5);
pub const LINE_2: Color32 = Color32::from_rgb(0xD8, 0xD2, 0xC3);
pub const ACCENT: Color32 = Color32::from_rgb(0x62, 0x43, 0xD9);
pub const POS: Color32 = Color32::from_rgb(0x1C, 0x91, 0x52);
pub const WARN: Color32 = Color32::from_rgb(0xA8, 0x64, 0x0D);
const SPECTRUM: [Color32; 6] = [
    Color32::from_rgb(0x8B, 0x5C, 0xF6),
    Color32::from_rgb(0x4F, 0x86, 0xF7),
    Color32::from_rgb(0x29, 0xC7, 0xD6),
    Color32::from_rgb(0x34, 0xC7, 0x59),
    Color32::from_rgb(0xFB, 0xB0, 0x24),
    Color32::from_rgb(0xFB, 0x5E, 0x6D),
];

/// How strongly the photograph shows through, 0..=255.
const PHOTO_OPACITY: u8 = 58;
/// White frost laid over the blurred photo inside a card, 0..=255.
const FROST: u8 = 118;
pub const CARD_RADIUS: u8 = 12;

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn mono_semibold() -> FontFamily {
    FontFamily::Name("mono-semibold".into())
}

pub(crate) fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

pub(crate) fn ink(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(TEXT.r(), TEXT.g(), TEXT.b(), alpha)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("inter", &include_bytes!("../assets/Inter-Regular.ttf")[..]),
        (
            "inter-semibold",
            &include_bytes!("../assets/Inter-SemiBold.ttf")[..],
        ),
        (
            "jbmono",
            &include_bytes!("../assets/JetBrainsMono-Regular.ttf")[..],
        ),
        (
            "jbmono-semibold",
            &include_bytes!("../assets/JetBrainsMono-SemiBold.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // Ours first, egui's bundled fonts stay behind as glyph fallback.
    let with_fallback = |fonts: &FontDefinitions, first: &str, base: FontFamily| {
        let mut list = vec![first.to_owned()];
        list.extend(fonts.families.get(&base).cloned().unwrap_or_default());
        list
    };
    let semi = with_fallback(&fonts, "inter-semibold", FontFamily::Proportional);
    let mono_semi = with_fallback(&fonts, "jbmono-semibold", FontFamily::Monospace);
    fonts.families.insert(semibold(), semi);
    fonts.families.insert(mono_semibold(), mono_semi);
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jbmono".to_owned());
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Light);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, semibold())),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.5, FontFamily::Proportional),
            ),
            (TextStyle::Small, FontId::new(11.0, FontFamily::Monospace)),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = vec2(10.0, 7.0);
        style.spacing.button_padding = vec2(12.0, 6.0);
        style.spacing.interact_size.y = 28.0;

        let v = &mut style.visuals;
        let radius = CornerRadius::same(6);
        v.panel_fill = BG;
        v.window_fill = white(248);
        v.window_stroke = Stroke::new(1.0, LINE_2);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(8);
        v.popup_shadow = Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: ink(36),
        };
        v.extreme_bg_color = white(200);
        v.faint_bg_color = ink(9);
        v.hyperlink_color = ACCENT;
        v.slider_trailing_fill = true;
        v.selection.bg_fill =
            Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), 44);
        v.selection.stroke = Stroke::new(1.0, ACCENT);

        let w = &mut v.widgets;
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.corner_radius = radius;

        w.inactive.weak_bg_fill = white(170);
        w.inactive.bg_fill = ink(22);
        w.inactive.bg_stroke = Stroke::new(1.0, LINE_2);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.corner_radius = radius;

        w.hovered.weak_bg_fill = white(255);
        w.hovered.bg_fill = ink(34);
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;

        w.active.weak_bg_fill = TEXT;
        w.active.bg_fill = TEXT;
        w.active.bg_stroke = Stroke::new(1.0, TEXT);
        w.active.fg_stroke = Stroke::new(1.0, BG);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;

        w.open = w.hovered;
    });
}

/// The faded photograph behind everything, plus a pre-blurred copy that the
/// glass cards sample so they look frosted. Cheap to clone.
#[derive(Clone)]
pub struct Backdrop {
    sharp: TextureHandle,
    blur: TextureHandle,
    aspect: f32,
    /// Where the photo sits on screen this frame.
    rect: Rect,
    /// Inside the shell's own glass panels: cards are quiet inner boxes
    /// instead of frosted glass of their own.
    pub inset: bool,
}

impl Backdrop {
    pub fn load(ctx: &egui::Context) -> Self {
        // Both PNGs are single-channel ink masks: white studio background is
        // zero, the figure is opaque. Tinting them with ink keeps the page
        // colour untouched wherever the photo is empty.
        let load = |name: &str, bytes: &[u8]| {
            let mask = image::load_from_memory(bytes)
                .expect("bundled backdrop is a valid PNG")
                .to_luma8();
            let size = [mask.width() as usize, mask.height() as usize];
            let pixels = mask.pixels().map(|p| ink(p.0[0])).collect();
            (
                ctx.load_texture(
                    name,
                    egui::ColorImage::new(size, pixels),
                    TextureOptions::LINEAR,
                ),
                size,
            )
        };
        let (sharp, size) = load("backdrop", include_bytes!("../assets/wilt.png"));
        let (blur, _) = load("backdrop-blur", include_bytes!("../assets/wilt_blur.png"));
        Backdrop {
            sharp,
            blur,
            aspect: size[0] as f32 / size[1] as f32,
            rect: Rect::NOTHING,
            inset: false,
        }
    }

    /// Fill the page and draw the photo, full height, hugging the right edge.
    pub fn paint(&mut self, ui: &Ui) {
        let screen = ui.ctx().content_rect();
        let height = screen.height().max(520.0);
        let size = vec2(height * self.aspect, height);
        let right = screen.right() - (screen.width() * 0.03).min(48.0);
        self.rect = Rect::from_min_size(pos2(right - size.x, screen.top()), size);

        let painter = ui.painter();
        painter.rect_filled(screen, 0.0, BG);
        painter.image(
            self.sharp.id(),
            self.rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::from_white_alpha(PHOTO_OPACITY),
        );
    }

    /// Shapes for a frosted panel at `rect`: page colour, the blurred photo
    /// where the sharp one would be, white frost, and a light rim.
    fn glass(&self, rect: Rect, radius: u8, shadow: bool) -> Vec<Shape> {
        let img = self.rect;
        let uv = |p: egui::Pos2| {
            pos2(
                (p.x - img.min.x) / img.width(),
                (p.y - img.min.y) / img.height(),
            )
        };
        let mut shapes = Vec::new();
        if shadow {
            shapes.push(
                Shadow {
                    offset: [0, 10],
                    blur: 32,
                    spread: 0,
                    color: ink(20),
                }
                .as_shape(rect, radius)
                .into(),
            );
        }
        shapes.push(RectShape::filled(rect, radius, BG).into());
        if img.is_positive() {
            // UVs run past 0..1 outside the photo; the texture's transparent
            // border clamps to nothing there.
            shapes.push(
                RectShape::filled(rect, radius, Color32::from_white_alpha(PHOTO_OPACITY))
                    .with_texture(
                        self.blur.id(),
                        Rect::from_min_max(uv(rect.min), uv(rect.max)),
                    )
                    .into(),
            );
        }
        shapes.push(RectShape::filled(rect, radius, white(FROST)).into());
        shapes.push(
            RectShape::stroke(rect, radius, Stroke::new(1.0, LINE_2), StrokeKind::Outside).into(),
        );
        shapes.push(
            RectShape::stroke(
                rect,
                radius,
                Stroke::new(1.0, white(235)),
                StrokeKind::Inside,
            )
            .into(),
        );
        shapes
    }

    /// A frosted card sized to its contents.
    pub fn card<R>(&self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        if self.inset {
            // A box inside a panel: a lighter sheet and a hairline.
            let slot = ui.painter().add(Shape::Noop);
            let out = egui::Frame::NONE
                .inner_margin(Margin::same(13))
                .show(ui, add);
            let rect = out.response.rect;
            ui.painter().set(
                slot,
                Shape::Vec(vec![
                    RectShape::filled(rect, 10, white(150)).into(),
                    RectShape::stroke(rect, 10, Stroke::new(1.0, LINE), StrokeKind::Inside).into(),
                ]),
            );
            return out.inner;
        }
        let slot = ui.painter().add(Shape::Noop);
        let out = egui::Frame::NONE
            .inner_margin(Margin::same(18))
            .show(ui, add);
        ui.painter().set(
            slot,
            Shape::Vec(self.glass(out.response.rect, CARD_RADIUS, true)),
        );
        out.inner
    }
}


pub(crate) fn spectrum(rect: Rect) -> Mesh {
    let mut mesh = Mesh::default();
    let step = rect.width() / (SPECTRUM.len() - 1) as f32;
    for (i, color) in SPECTRUM.into_iter().enumerate() {
        let x = rect.left() + step * i as f32;
        mesh.colored_vertex(pos2(x, rect.top()), color);
        mesh.colored_vertex(pos2(x, rect.bottom()), color);
        if i > 0 {
            let n = (i * 2) as u32;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    mesh
}

/// Small uppercase mono label, used for section names and table headers.
pub fn eyebrow_text(text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .font(FontId::new(10.5, FontFamily::Monospace))
        .extra_letter_spacing(1.1)
        .color(TEXT_MUTE)
}

pub fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(eyebrow_text(text));
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(34.0, semibold()))
            .color(TEXT),
    );
}

pub fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text).font(FontId::new(13.0, FontFamily::Monospace))
}

/// Solid ink button for the one main action on screen.
pub fn primary_button(ui: &mut Ui, enabled: bool, text: &str) -> bool {
    let label = RichText::new(text).family(semibold()).color(BG);
    ui.add_enabled(
        enabled,
        egui::Button::new(label).fill(TEXT).stroke(Stroke::NONE),
    )
    .clicked()
}

/// A glass tile with a label, one big figure and a line of context.
pub fn stat(ui: &mut Ui, backdrop: &Backdrop, label: &str, value: &str, note: &str) {
    backdrop.card(ui, |ui| {
        ui.set_min_width(150.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            eyebrow(ui, label);
            ui.label(RichText::new(value).font(FontId::new(26.0, semibold())));
            ui.label(RichText::new(note).size(12.0).color(TEXT_DIM));
        });
    });
}

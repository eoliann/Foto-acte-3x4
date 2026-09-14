#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Color32, ColorImage, Context, RichText, Slider, Stroke, TextureHandle, TextureOptions,
    Vec2,
};
use image::{
    DynamicImage, GenericImageView, ImageDecoder, ImageReader, RgbImage, imageops::FilterType,
};
use jpeg_encoder::{ColorType, Density, Encoder};

const DPI: u16 = 300;
const PHOTO_WIDTH: u32 = 354;
const PHOTO_HEIGHT: u32 = 472;
const SHEET_WIDTH: u32 = 1772;
const SHEET_HEIGHT: u32 = 1181;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([820.0, 600.0])
            .with_title("Foto acte 3x4 by eoliann"),
        ..Default::default()
    };

    eframe::run_native(
        "Foto acte 3x4 by eoliann",
        options,
        Box::new(|cc| Ok(Box::new(PhotoApp::new(cc)))),
    )
}

struct PhotoApp {
    source: Option<DynamicImage>,
    source_path: Option<PathBuf>,
    sheet_texture: Option<TextureHandle>,
    zoom: f32,
    offset_x: f32,
    offset_y: f32,
    status: String,
}

impl PhotoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        Self {
            source: None,
            source_path: None,
            sheet_texture: None,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            status: "Încarcă o fotografie pentru a începe.".to_owned(),
        }
    }

    fn open_dialog(&mut self, ctx: &Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Fotografii", &["jpg", "jpeg", "png", "webp", "bmp"])
            .pick_file()
        {
            self.load_image(ctx, path);
        }
    }

    fn load_image(&mut self, ctx: &Context, path: PathBuf) {
        match open_oriented(&path) {
            Ok(image) => {
                self.source = Some(image);
                self.source_path = Some(path);
                self.zoom = 1.0;
                self.offset_x = 0.0;
                self.offset_y = 0.0;
                self.refresh_sheet_preview(ctx);
                self.status =
                    "Reglează încadrarea, apoi salvează imaginea pentru print.".to_owned();
            }
            Err(error) => {
                self.status = format!("Fotografia nu a putut fi deschisă: {error}");
            }
        }
    }

    fn refresh_sheet_preview(&mut self, ctx: &Context) {
        let Some(source) = &self.source else {
            return;
        };
        let portrait = crop_portrait(source, self.zoom, self.offset_x, self.offset_y, 180, 240);
        let sheet = compose_sheet(&portrait, 900, 600);
        self.sheet_texture = Some(ctx.load_texture(
            "previzualizare-coala",
            color_image(&DynamicImage::ImageRgb8(sheet)),
            TextureOptions::LINEAR,
        ));
    }

    fn save_dialog(&mut self) {
        let Some(source) = &self.source else {
            return;
        };
        let default_name = self
            .source_path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .map(|name| format!("{name}_6_poze_3x4.jpg"))
            .unwrap_or_else(|| "6_poze_3x4.jpg".to_owned());

        let Some(path) = rfd::FileDialog::new()
            .add_filter("Imagine JPEG", &["jpg", "jpeg"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };

        let portrait = crop_portrait(
            source,
            self.zoom,
            self.offset_x,
            self.offset_y,
            PHOTO_WIDTH,
            PHOTO_HEIGHT,
        );
        let sheet = compose_sheet(&portrait, SHEET_WIDTH, SHEET_HEIGHT);

        match save_jpeg_300_dpi(&sheet, &path) {
            Ok(()) => self.status = format!("Imagine salvată: {}", path.display()),
            Err(error) => self.status = format!("Imaginea nu a putut fi salvată: {error}"),
        }
    }
}

fn open_oriented(path: &Path) -> image::ImageResult<DynamicImage> {
    let mut decoder = ImageReader::open(path)?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}

impl eframe::App for PhotoApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if let Some(path) = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .find_map(|file| file.path.clone())
        }) {
            self.load_image(ctx, path);
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Foto acte 3x4");
                ui.separator();
                ui.label("6 fotografii pe coală 15x10 cm • 300 DPI");
            });
            ui.add_space(8.0);
        });

        egui::SidePanel::left("controls")
            .resizable(false)
            .exact_width(290.0)
            .show(ctx, |ui| {
                ui.add_space(14.0);
                if ui
                    .add_sized(
                        [ui.available_width(), 42.0],
                        egui::Button::new("Încarcă fotografia"),
                    )
                    .clicked()
                {
                    self.open_dialog(ctx);
                }
                ui.label(
                    RichText::new("Poți și să tragi fotografia peste fereastră.")
                        .small()
                        .weak(),
                );

                ui.add_space(22.0);
                ui.heading("Încadrare");
                ui.label("Păstrează capul și bustul în interiorul fotografiei.");
                ui.add_enabled_ui(self.source.is_some(), |ui| {
                    let mut changed = false;
                    changed |= ui
                        .add(Slider::new(&mut self.zoom, 1.0..=3.0).text("Zoom"))
                        .changed();
                    changed |= ui
                        .add(Slider::new(&mut self.offset_x, -1.0..=1.0).text("Stânga / dreapta"))
                        .changed();
                    changed |= ui
                        .add(Slider::new(&mut self.offset_y, -1.0..=1.0).text("Sus / jos"))
                        .changed();
                    if ui.button("Resetează încadrarea").clicked() {
                        self.zoom = 1.0;
                        self.offset_x = 0.0;
                        self.offset_y = 0.0;
                        changed = true;
                    }
                    if changed {
                        self.refresh_sheet_preview(ctx);
                    }
                });

                ui.add_space(22.0);
                let save = egui::Button::new(RichText::new("Salvează pentru print").strong())
                    .fill(Color32::from_rgb(32, 103, 178));
                if ui
                    .add_enabled_ui(self.source.is_some(), |ui| {
                        ui.add_sized([ui.available_width(), 44.0], save).clicked()
                    })
                    .inner
                {
                    self.save_dialog();
                }

                ui.add_space(16.0);
                ui.separator();
                ui.label(RichText::new(&self.status).small());
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new("La imprimare: mărime reală / 100%, fără «Fit to page».")
                            .small()
                            .strong(),
                    );
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(14.0);
            ui.heading("Previzualizare coală");
            ui.label("Fiecare fotografie are 3x4 cm; spațiile albe permit tăierea.");
            ui.add_space(10.0);

            if let Some(texture) = &self.sheet_texture {
                let available = ui.available_size() - Vec2::new(8.0, 8.0);
                let ratio = texture.size_vec2().x / texture.size_vec2().y;
                let mut size = Vec2::new(available.x, available.x / ratio);
                if size.y > available.y {
                    size = Vec2::new(available.y * ratio, available.y);
                }
                let response = ui.add(
                    egui::Image::new(texture)
                        .fit_to_exact_size(size)
                        .corner_radius(2.0),
                );
                ui.painter().rect_stroke(
                    response.rect,
                    2.0,
                    Stroke::new(1.0_f32, Color32::from_gray(155)),
                    egui::StrokeKind::Outside,
                );
            } else {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(
                        ui.available_width().min(720.0),
                        ui.available_width().min(720.0) * 2.0 / 3.0,
                    ),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 3.0, Color32::from_gray(235));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Previzualizarea va apărea aici",
                    egui::FontId::proportional(18.0),
                    Color32::from_gray(100),
                );
            }
        });
    }
}

fn crop_portrait(
    source: &DynamicImage,
    zoom: f32,
    offset_x: f32,
    offset_y: f32,
    target_width: u32,
    target_height: u32,
) -> RgbImage {
    let (width, height) = source.dimensions();
    let target_ratio = target_width as f64 / target_height as f64;
    let source_ratio = width as f64 / height as f64;

    let (base_width, base_height) = if source_ratio > target_ratio {
        ((height as f64 * target_ratio).round() as u32, height)
    } else {
        (width, (width as f64 / target_ratio).round() as u32)
    };

    let crop_width = ((base_width as f32 / zoom).round() as u32).clamp(1, width);
    let crop_height = ((base_height as f32 / zoom).round() as u32).clamp(1, height);
    let max_x = width - crop_width;
    let max_y = height - crop_height;
    let x = (((offset_x + 1.0) * 0.5) * max_x as f32).round() as u32;
    let y = (((offset_y + 1.0) * 0.5) * max_y as f32).round() as u32;

    source
        .crop_imm(x.min(max_x), y.min(max_y), crop_width, crop_height)
        .resize_exact(target_width, target_height, FilterType::Lanczos3)
        .to_rgb8()
}

fn compose_sheet(portrait: &RgbImage, sheet_width: u32, sheet_height: u32) -> RgbImage {
    let mut sheet = RgbImage::from_pixel(sheet_width, sheet_height, image::Rgb([255, 255, 255]));
    let photo_width = portrait.width();
    let photo_height = portrait.height();
    let horizontal_gap = (sheet_width - 3 * photo_width) / 4;
    let vertical_gap = (sheet_height - 2 * photo_height) / 3;

    for row in 0..2 {
        for column in 0..3 {
            let x = horizontal_gap + column * (photo_width + horizontal_gap);
            let y = vertical_gap + row * (photo_height + vertical_gap);
            image::imageops::replace(&mut sheet, portrait, x as i64, y as i64);
        }
    }
    sheet
}

fn save_jpeg_300_dpi(image: &RgbImage, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let mut encoder = Encoder::new(file, 95);
    encoder.set_density(Density::Inch { x: DPI, y: DPI });
    encoder
        .encode(
            image.as_raw(),
            image.width() as u16,
            image.height() as u16,
            ColorType::Rgb,
        )
        .map_err(|error| error.to_string())
}

fn color_image(image: &DynamicImage) -> ColorImage {
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
}

fn configure_style(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(9.0, 9.0);
    style.visuals.panel_fill = Color32::from_rgb(248, 248, 246);
    style.visuals.window_fill = Color32::from_rgb(248, 248, 246);
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_has_exact_print_dimensions() {
        let source = DynamicImage::ImageRgb8(RgbImage::new(1200, 800));
        let portrait = crop_portrait(&source, 1.0, 0.0, 0.0, PHOTO_WIDTH, PHOTO_HEIGHT);

        assert_eq!(portrait.dimensions(), (PHOTO_WIDTH, PHOTO_HEIGHT));
    }

    #[test]
    fn sheet_has_exact_dimensions_and_six_copies() {
        let red = image::Rgb([200, 10, 10]);
        let portrait = RgbImage::from_pixel(PHOTO_WIDTH, PHOTO_HEIGHT, red);
        let sheet = compose_sheet(&portrait, SHEET_WIDTH, SHEET_HEIGHT);
        let horizontal_gap = (SHEET_WIDTH - 3 * PHOTO_WIDTH) / 4;
        let vertical_gap = (SHEET_HEIGHT - 2 * PHOTO_HEIGHT) / 3;

        assert_eq!(sheet.dimensions(), (SHEET_WIDTH, SHEET_HEIGHT));
        for row in 0..2 {
            for column in 0..3 {
                let x = horizontal_gap + column * (PHOTO_WIDTH + horizontal_gap);
                let y = vertical_gap + row * (PHOTO_HEIGHT + vertical_gap);
                assert_eq!(*sheet.get_pixel(x, y), red);
                assert_eq!(
                    *sheet.get_pixel(x + PHOTO_WIDTH - 1, y + PHOTO_HEIGHT - 1),
                    red
                );
            }
        }
        assert_eq!(*sheet.get_pixel(0, 0), image::Rgb([255, 255, 255]));
    }
}

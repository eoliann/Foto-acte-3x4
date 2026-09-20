#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};

use eframe::egui::{
    self, Color32, ColorImage, Context, FontId, RichText, Slider, Stroke, TextStyle, TextureHandle,
    TextureOptions, Vec2,
};
use image::{
    DynamicImage, GenericImageView, GrayImage, ImageDecoder, ImageReader, Luma, RgbImage,
    imageops::FilterType,
};
use jpeg_encoder::{ColorType, Density, Encoder};
use rten::{Model, ValueView};

const DPI: u16 = 300;
const PHOTO_WIDTH: u32 = 354;
const PHOTO_HEIGHT: u32 = 472;
const SHEET_WIDTH: u32 = 1772;
const SHEET_HEIGHT: u32 = 1181;
const MODNET_MODEL: &[u8] = include_bytes!("../assets/modnet.onnx");

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 860.0])
            .with_min_inner_size([860.0, 700.0])
            .with_title("Foto acte 3x4 by eoliann on GitHub "),
        ..Default::default()
    };

    eframe::run_native(
        "Foto acte 3x4 by eoliann on GitHub",
        options,
        Box::new(|cc| Ok(Box::new(PhotoApp::new(cc)))),
    )
}

struct PhotoApp {
    source: Option<Arc<DynamicImage>>,
    source_path: Option<PathBuf>,
    sheet_texture: Option<TextureHandle>,
    zoom: f32,
    offset_x: f32,
    offset_y: f32,
    brightness: i32,
    contrast: f32,
    background: Background,
    custom_background: [u8; 3],
    mask: Option<GrayImage>,
    source_generation: u64,
    ai_state: AiState,
    inference_tx: mpsc::Sender<InferenceJob>,
    inference_rx: mpsc::Receiver<InferenceResult>,
    printing: bool,
    print_tx: mpsc::Sender<String>,
    print_rx: mpsc::Receiver<String>,
    status: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Background {
    Original,
    White,
    LightGray,
    LightBlue,
    Custom,
}

impl Background {
    fn color(self, custom: [u8; 3]) -> Option<[u8; 3]> {
        match self {
            Self::Original => None,
            Self::White => Some([255, 255, 255]),
            Self::LightGray => Some([232, 234, 237]),
            Self::LightBlue => Some([200, 225, 245]),
            Self::Custom => Some(custom),
        }
    }
}

enum AiState {
    Idle,
    Running,
    Ready,
    Error(String),
}

struct InferenceJob {
    generation: u64,
    source: Arc<DynamicImage>,
}

struct InferenceResult {
    generation: u64,
    mask: Result<GrayImage, String>,
}

#[derive(Clone, Copy)]
struct RenderSettings {
    zoom: f32,
    offset_x: f32,
    offset_y: f32,
    brightness: i32,
    contrast: f32,
    background: Option<[u8; 3]>,
}

impl PhotoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        let (inference_tx, job_rx) = mpsc::channel();
        let (result_tx, inference_rx) = mpsc::channel();
        start_inference_worker(job_rx, result_tx, cc.egui_ctx.clone());
        let (print_tx, print_rx) = mpsc::channel();
        Self {
            source: None,
            source_path: None,
            sheet_texture: None,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            brightness: 0,
            contrast: 0.0,
            background: Background::Original,
            custom_background: [255, 255, 255],
            mask: None,
            source_generation: 0,
            ai_state: AiState::Idle,
            inference_tx,
            inference_rx,
            printing: false,
            print_tx,
            print_rx,
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
                self.source_generation = self.source_generation.wrapping_add(1);
                self.source = Some(Arc::new(image));
                self.source_path = Some(path);
                self.zoom = 1.0;
                self.offset_x = 0.0;
                self.offset_y = 0.0;
                self.brightness = 0;
                self.contrast = 0.0;
                self.mask = None;
                self.ai_state = AiState::Idle;
                if self.background != Background::Original {
                    self.request_background_removal();
                }
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
        let portrait = render_portrait(source, self.mask.as_ref(), self.settings(), 180, 240);
        let sheet = compose_sheet(&portrait, 900, 600);
        let preview = color_image(&DynamicImage::ImageRgb8(sheet));
        if let Some(texture) = &mut self.sheet_texture {
            texture.set(preview, TextureOptions::LINEAR);
        } else {
            self.sheet_texture =
                Some(ctx.load_texture("previzualizare-coala", preview, TextureOptions::LINEAR));
        }
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

        if self.background != Background::Original && self.mask.is_none() {
            self.status =
                "Așteaptă finalizarea eliminării fundalului înainte de export.".to_owned();
            return;
        }

        let portrait = render_portrait(
            source,
            self.mask.as_ref(),
            self.settings(),
            PHOTO_WIDTH,
            PHOTO_HEIGHT,
        );
        let sheet = compose_sheet(&portrait, SHEET_WIDTH, SHEET_HEIGHT);

        match save_jpeg_300_dpi(&sheet, &path) {
            Ok(()) => self.status = format!("Imagine salvată: {}", path.display()),
            Err(error) => self.status = format!("Imaginea nu a putut fi salvată: {error}"),
        }
    }

    fn receive_print_results(&mut self) {
        while let Ok(message) = self.print_rx.try_recv() {
            self.printing = false;
            self.status = message;
        }
    }

    fn print_direct(&mut self, ctx: &Context) {
        if self.printing {
            return;
        }
        let Some(source) = &self.source else {
            return;
        };

        if self.background != Background::Original && self.mask.is_none() {
            self.status =
                "Așteaptă finalizarea eliminării fundalului înainte de printare.".to_owned();
            return;
        }

        let portrait = render_portrait(
            source,
            self.mask.as_ref(),
            self.settings(),
            PHOTO_WIDTH,
            PHOTO_HEIGHT,
        );
        let sheet = compose_sheet(&portrait, SHEET_WIDTH, SHEET_HEIGHT);

        let mut temp_dir = std::env::temp_dir();
        temp_dir.push("Foto-acte-3x4");
        if let Err(error) = std::fs::create_dir_all(&temp_dir) {
            self.status = format!("Nu s-a putut pregăti fișierul pentru print: {error}");
            return;
        }

        let stem = self
            .source_path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .unwrap_or("coala");
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let file_name = format!("{stem}_6_poze_3x4_print_{timestamp}.jpg");
        let path = temp_dir.join(file_name);

        if let Err(error) = save_jpeg_300_dpi(&sheet, &path) {
            self.status = format!("Fișierul pentru print nu a putut fi creat: {error}");
            return;
        }

        // Dialogul de printare este afișat de un script PowerShell care folosește
        // .NET (System.Windows.Forms.PrintDialog + System.Drawing.Printing).
        // Metoda clasică `Start-Process -Verb Print` nu funcționează când
        // aplicația implicită pentru JPG este una UWP (Photos), care nu
        // înregistrează verbul „print”, deci dialogul nu apărea deloc.
        let script_path = temp_dir.join("print_coala.ps1");
        if let Err(error) = std::fs::write(&script_path, PRINT_SCRIPT) {
            self.status = format!("Scriptul de printare nu a putut fi creat: {error}");
            return;
        }

        self.printing = true;
        self.status = "Se deschide dialogul de printare…".to_owned();
        let result_tx = self.print_tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let message = run_print_dialog(&script_path, &path);
            let _ = result_tx.send(message);
            repaint.request_repaint();
        });
    }

    fn settings(&self) -> RenderSettings {
        RenderSettings {
            zoom: self.zoom,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            brightness: self.brightness,
            contrast: self.contrast,
            background: self.background.color(self.custom_background),
        }
    }

    fn request_background_removal(&mut self) {
        let Some(source) = self.source.clone() else {
            return;
        };
        self.mask = None;
        self.ai_state = AiState::Running;
        if self
            .inference_tx
            .send(InferenceJob {
                generation: self.source_generation,
                source,
            })
            .is_err()
        {
            self.ai_state = AiState::Error("Motorul AI nu a putut fi pornit.".to_owned());
        }
    }

    fn receive_inference_results(&mut self, ctx: &Context) {
        let mut preview_changed = false;
        while let Ok(result) = self.inference_rx.try_recv() {
            if result.generation != self.source_generation {
                continue;
            }
            match result.mask {
                Ok(mask) => {
                    self.mask = Some(mask);
                    self.ai_state = AiState::Ready;
                    self.status =
                        "Fundal eliminat. Poți alege culoarea și salva fotografia.".to_owned();
                    preview_changed = true;
                }
                Err(error) => {
                    self.mask = None;
                    self.ai_state = AiState::Error(error);
                }
            }
        }
        if preview_changed {
            self.refresh_sheet_preview(ctx);
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
        self.receive_inference_results(ctx);
        self.receive_print_results();

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
                ui.label(
                    RichText::new("Foto acte 3x4")
                        .heading()
                        .color(Color32::BLACK),
                );
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(Color32::BLACK),
                );
                ui.separator();
                ui.label(
                    RichText::new("6 fotografii pe coală 15x10 cm • 300 DPI").color(Color32::BLACK),
                );
            });
            ui.add_space(8.0);
        });

        egui::SidePanel::left("controls")
            .resizable(false)
            .exact_width(310.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                            .color(Color32::BLACK),
                    );

                    ui.add_space(22.0);
                    ui.label(RichText::new("Încadrare").heading().color(Color32::BLACK));
                    ui.label(
                        RichText::new("Păstrează capul și bustul în interiorul fotografiei.")
                            .color(Color32::BLACK),
                    );
                    ui.add_enabled_ui(self.source.is_some(), |ui| {
                        let mut changed = false;
                        changed |= ui
                            .add(
                                Slider::new(&mut self.zoom, 1.0..=3.0)
                                    .text("Zoom")
                                    .text_color(Color32::BLACK),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                Slider::new(&mut self.offset_x, -1.0..=1.0)
                                    .text("Stânga / dreapta")
                                    .text_color(Color32::BLACK),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                Slider::new(&mut self.offset_y, -1.0..=1.0)
                                    .text("Sus / jos")
                                    .text_color(Color32::BLACK),
                            )
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

                    ui.add_space(18.0);
                    ui.label(RichText::new("Imagine").heading().color(Color32::BLACK));
                    ui.add_enabled_ui(self.source.is_some(), |ui| {
                        let mut changed = false;
                        changed |= ui
                            .add(
                                Slider::new(&mut self.brightness, -100..=100)
                                    .text("Luminozitate")
                                    .text_color(Color32::BLACK),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                Slider::new(&mut self.contrast, -100.0..=100.0)
                                    .text("Contrast")
                                    .text_color(Color32::BLACK),
                            )
                            .changed();
                        if ui.button("Resetează luminozitatea").clicked() {
                            self.brightness = 0;
                            self.contrast = 0.0;
                            changed = true;
                        }
                        if changed {
                            self.refresh_sheet_preview(ctx);
                        }
                    });

                    ui.add_space(18.0);
                    ui.label(RichText::new("Fundal").heading().color(Color32::BLACK));
                    ui.label(
                        RichText::new(
                            "Înlocuirea folosește AI local; fotografia nu părăsește calculatorul.",
                        )
                        .color(Color32::BLACK),
                    );
                    ui.add_enabled_ui(self.source.is_some(), |ui| {
                        let previous = self.background;
                        ui.radio_value(
                            &mut self.background,
                            Background::Original,
                            RichText::new("Original").color(Color32::BLACK),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(
                                &mut self.background,
                                Background::White,
                                RichText::new("Alb").color(Color32::BLACK),
                            );
                            ui.radio_value(
                                &mut self.background,
                                Background::LightGray,
                                RichText::new("Gri deschis").color(Color32::BLACK),
                            );
                            ui.radio_value(
                                &mut self.background,
                                Background::LightBlue,
                                RichText::new("Albastru deschis").color(Color32::BLACK),
                            );
                            ui.radio_value(
                                &mut self.background,
                                Background::Custom,
                                RichText::new("Personalizat").color(Color32::BLACK),
                            );
                        });

                        let mut color_changed = false;
                        if self.background == Background::Custom {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Culoare:").color(Color32::BLACK));
                                color_changed = ui
                                    .color_edit_button_srgb(&mut self.custom_background)
                                    .changed();
                            });
                        }

                        if self.background != previous {
                            if self.background != Background::Original && self.mask.is_none() {
                                self.request_background_removal();
                            }
                            self.refresh_sheet_preview(ctx);
                        } else if color_changed {
                            self.refresh_sheet_preview(ctx);
                        }
                    });

                    match &self.ai_state {
                        AiState::Running => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new("AI procesează fotografia...")
                                        .color(Color32::BLACK),
                                );
                            });
                        }
                        AiState::Ready => {
                            ui.label(
                                RichText::new("Fundal eliminat cu succes.")
                                    .color(Color32::DARK_GREEN),
                            );
                        }
                        AiState::Error(error) => {
                            ui.label(
                                RichText::new(format!("Eroare AI: {error}"))
                                    .color(Color32::DARK_RED),
                            );
                            if ui.button("Încearcă din nou").clicked() {
                                self.request_background_removal();
                            }
                        }
                        AiState::Idle => {}
                    }

                    ui.add_space(22.0);
                    let save = egui::Button::new(RichText::new("Salvează pentru print").strong())
                        .fill(Color32::from_rgb(32, 103, 178));
                    let can_save = self.source.is_some()
                        && (self.background == Background::Original || self.mask.is_some());
                    if ui
                        .add_enabled_ui(can_save, |ui| {
                            ui.add_sized([ui.available_width(), 44.0], save).clicked()
                        })
                        .inner
                    {
                        self.save_dialog();
                    }

                    ui.add_space(8.0);
                    let print_label = if self.printing {
                        "Se deschide dialogul…"
                    } else {
                        "Printează direct"
                    };
                    let print = egui::Button::new(RichText::new(print_label).strong())
                        .fill(Color32::from_rgb(46, 125, 50));
                    let can_print = can_save && !self.printing;
                    if ui
                        .add_enabled_ui(can_print, |ui| {
                            ui.add_sized([ui.available_width(), 44.0], print).clicked()
                        })
                        .inner
                    {
                        self.print_direct(ctx);
                    }

                    ui.add_space(16.0);
                    ui.separator();
                    ui.label(RichText::new(&self.status).small().color(Color32::BLACK));
                    ui.label(
                        RichText::new("La imprimare: mărime reală / 100%, fără «Fit to page».")
                            .small()
                            .strong()
                            .color(Color32::BLACK),
                    );
                    ui.add_space(10.0);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(14.0);
            ui.label(
                RichText::new("Previzualizare coală")
                    .heading()
                    .color(Color32::BLACK),
            );
            ui.label(
                RichText::new("Fiecare fotografie are 3x4 cm; spațiile albe permit tăierea.")
                    .color(Color32::BLACK),
            );
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
                    Color32::BLACK,
                );
            }
        });
    }
}

fn render_portrait(
    source: &DynamicImage,
    mask: Option<&GrayImage>,
    settings: RenderSettings,
    target_width: u32,
    target_height: u32,
) -> RgbImage {
    let (width, height) = source.dimensions();
    let (x, y, crop_width, crop_height) = crop_rectangle(
        width,
        height,
        settings.zoom,
        settings.offset_x,
        settings.offset_y,
        target_width,
        target_height,
    );

    let mut portrait = source
        .crop_imm(x, y, crop_width, crop_height)
        .resize_exact(target_width, target_height, FilterType::Lanczos3)
        .to_rgb8();
    apply_adjustments(&mut portrait, settings.brightness, settings.contrast);

    if let (Some(background), Some(mask)) = (settings.background, mask) {
        let cropped_mask =
            image::imageops::crop_imm(mask, x, y, crop_width, crop_height).to_image();
        let resized_mask = image::imageops::resize(
            &cropped_mask,
            target_width,
            target_height,
            FilterType::Triangle,
        );
        for (pixel, alpha) in portrait.pixels_mut().zip(resized_mask.pixels()) {
            let foreground = alpha[0] as u16;
            let backdrop = 255 - foreground;
            for channel in 0..3 {
                pixel[channel] = ((pixel[channel] as u16 * foreground
                    + background[channel] as u16 * backdrop
                    + 127)
                    / 255) as u8;
            }
        }
    }

    portrait
}

fn crop_rectangle(
    width: u32,
    height: u32,
    zoom: f32,
    offset_x: f32,
    offset_y: f32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32, u32, u32) {
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

    (x.min(max_x), y.min(max_y), crop_width, crop_height)
}

fn apply_adjustments(image: &mut RgbImage, brightness: i32, contrast: f32) {
    let contrast = contrast.clamp(-100.0, 100.0) * 2.0;
    let factor = (259.0 * (contrast + 255.0)) / (255.0 * (259.0 - contrast));
    for pixel in image.pixels_mut() {
        for channel in &mut pixel.0 {
            let adjusted = factor * (*channel as f32 - 128.0) + 128.0 + brightness as f32;
            *channel = adjusted.round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn start_inference_worker(
    job_rx: mpsc::Receiver<InferenceJob>,
    result_tx: mpsc::Sender<InferenceResult>,
    ctx: Context,
) {
    std::thread::spawn(move || {
        let model = Model::load_static_slice(MODNET_MODEL)
            .map_err(|error| format!("Modelul MODNet nu poate fi încărcat: {error}"));
        while let Ok(job) = job_rx.recv() {
            let mask = match &model {
                Ok(model) => infer_foreground_mask(model, &job.source),
                Err(error) => Err(error.clone()),
            };
            if result_tx
                .send(InferenceResult {
                    generation: job.generation,
                    mask,
                })
                .is_err()
            {
                break;
            }
            ctx.request_repaint();
        }
    });
}

fn infer_foreground_mask(model: &Model, source: &DynamicImage) -> Result<GrayImage, String> {
    let (source_width, source_height) = source.dimensions();
    let short_edge = source_width.min(source_height) as f32;
    let long_edge = source_width.max(source_height) as f32;
    let scale = (512.0 / short_edge).min(1024.0 / long_edge);
    let input_width = round_to_multiple_of_32(source_width as f32 * scale);
    let input_height = round_to_multiple_of_32(source_height as f32 * scale);
    let resized = source
        .resize_exact(input_width, input_height, FilterType::Triangle)
        .to_rgb8();

    let area = (input_width * input_height) as usize;
    let mut input_data = vec![0.0_f32; area * 3];
    for (index, pixel) in resized.pixels().enumerate() {
        input_data[index] = pixel[0] as f32 / 127.5 - 1.0;
        input_data[area + index] = pixel[1] as f32 / 127.5 - 1.0;
        input_data[2 * area + index] = pixel[2] as f32 / 127.5 - 1.0;
    }

    let input = ValueView::from_shape(
        [1, 3, input_height as usize, input_width as usize],
        &input_data,
    )
    .map_err(|error| format!("Intrare AI invalidă: {error}"))?;
    let output = model
        .run_one(input.into(), None)
        .map_err(|error| format!("Procesarea AI a eșuat: {error}"))?;
    let ([_, _, mask_height, mask_width], values) = output
        .into_shape_vec::<f32, 4>()
        .map_err(|error| format!("Rezultat AI invalid: {error}"))?;

    let mask = GrayImage::from_fn(mask_width as u32, mask_height as u32, |x, y| {
        let value = values[y as usize * mask_width + x as usize].clamp(0.0, 1.0);
        Luma([(value * 255.0).round() as u8])
    });
    Ok(image::imageops::resize(
        &mask,
        source_width,
        source_height,
        FilterType::Triangle,
    ))
}

fn round_to_multiple_of_32(value: f32) -> u32 {
    ((value / 32.0).round().max(1.0) as u32) * 32
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

/// Script PowerShell care afișează dialogul nativ de printare și tipărește
/// coala 10x15 la scară reală (hârtie 4x6 inch, landscape, margini zero).
/// Verbul clasic `Start-Process -Verb Print` nu poate fi folosit, deoarece pe
/// Windows 10/11 aplicația implicită pentru JPG este una UWP (Photos), care nu
/// înregistrează verbul „print”, iar dialogul nu apărea deloc.
const PRINT_SCRIPT: &str = r#"param([Parameter(Mandatory = $true)][string]$ImagePath)
$ErrorActionPreference = 'Stop'
try {
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName System.Windows.Forms
    $script:printImage = [System.Drawing.Image]::FromFile($ImagePath)
    $doc = New-Object System.Drawing.Printing.PrintDocument
    $doc.DocumentName = 'Foto acte 3x4 - coala 10x15'
    $paper = New-Object System.Drawing.Printing.PaperSize('Foto 10x15', 400, 600)
    try { $doc.DefaultPageSettings.PaperSize = $paper } catch { }
    $doc.DefaultPageSettings.Landscape = $true
    $doc.DefaultPageSettings.Margins = New-Object System.Drawing.Printing.Margins(0, 0, 0, 0)
    try { $doc.DefaultPageSettings.Color = $true } catch { }
    $handler = {
        param($sender, $e)
        $e.Graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $e.Graphics.DrawImage($script:printImage, $e.PageBounds)
        $e.HasMorePages = $false
    }
    $doc.add_PrintPage($handler)
    $dlg = New-Object System.Windows.Forms.PrintDialog
    $dlg.UseEXDialog = $true
    $dlg.Document = $doc
    $result = $dlg.ShowDialog()
    if ($result -eq [System.Windows.Forms.DialogResult]::OK) {
        $doc.Print()
        Write-Output 'PRINT_OK'
    } else {
        Write-Output 'PRINT_CANCELLED'
    }
} catch {
    Write-Output ('PRINT_ERROR: ' + $_.Exception.Message)
} finally {
    if ($script:printImage) { $script:printImage.Dispose() }
    if ($doc) { $doc.Dispose() }
    if ($dlg) { $dlg.Dispose() }
}
"#;

/// Ascunde fereastra de consolă a unui proces copil (Windows).
/// Fără `CREATE_NO_WINDOW`, `powershell.exe` — aplicație de consolă —
/// deschide o fereastră neagră care rămâne deasupra dialogului de printare.
/// Ferestrele GUI create de copil (dialogul WinForms) apar normal.
fn hide_console_window(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn run_print_dialog(script_path: &Path, image_path: &Path) -> String {
    let mut command = std::process::Command::new("powershell");
    command.args([
        "-NoProfile",
        "-STA",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &script_path.to_string_lossy(),
        "-ImagePath",
        &image_path.to_string_lossy(),
    ]);
    hide_console_window(&mut command);
    let output = command.output();

    match output {
        Err(error) => open_with_default_viewer(
            image_path,
            &format!("dialogul de printare nu a putut porni ({error})"),
        ),
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.lines().any(|line| line.trim() == "PRINT_OK") {
                format!(
                    "Coala a fost trimisă la imprimantă. Fișierul rămâne salvat la: {}",
                    image_path.display()
                )
            } else if stdout.lines().any(|line| line.trim() == "PRINT_CANCELLED") {
                "Printare anulată. Poți redeschide dialogul oricând.".to_owned()
            } else if let Some(error) = stdout
                .lines()
                .find_map(|line| line.strip_prefix("PRINT_ERROR:"))
            {
                open_with_default_viewer(
                    image_path,
                    &format!("dialogul a eșuat ({})", error.trim()),
                )
            } else if output.status.success() {
                "Dialogul de printare a fost închis.".to_owned()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                open_with_default_viewer(
                    image_path,
                    &format!(
                        "dialogul a eșuat (cod {:?}): {}",
                        output.status.code(),
                        stderr.trim()
                    ),
                )
            }
        }
    }
}

/// Soluție de rezervă: deschide imaginea în aplicația implicită din Windows
/// (verb „open”, funcționează și cu aplicații UWP precum Photos), de unde
/// utilizatorul poate printa manual (Print / Ctrl+P, hârtie 10x15, 100%).
fn open_with_default_viewer(image_path: &Path, reason: &str) -> String {
    match std::process::Command::new("explorer")
        .arg(image_path)
        .spawn()
    {
        Ok(_) => format!(
            "Nu am putut deschide direct dialogul de printare, {reason}. Am deschis imaginea \
             în aplicația implicită: {}. De acolo folosește Print (Ctrl+P), hârtie 10x15 cm, \
             mărime reală / 100%.",
            image_path.display()
        ),
        Err(error) => format!(
            "Printarea directă nu a funcționat ({reason}; nici imaginea nu s-a putut deschide: \
             {error}). Fișierul este salvat la: {}. Deschide-l manual și printează-l la 100%.",
            image_path.display()
        ),
    }
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
    // Textele șterse („weak") sunt negre, nu gri. Restul textelor primesc
    // negru explicit acolo unde fundalul este deschis; butoanele și
    // câmpurile de valori își păstrează textul deschis pe fundal închis.
    style.visuals.weak_text_color = Some(Color32::BLACK);
    // Fonturi mai mari, lizibile fără efort.
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(16.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(13.0));
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_has_exact_print_dimensions() {
        let source = DynamicImage::ImageRgb8(RgbImage::new(1200, 800));
        let portrait = render_portrait(
            &source,
            None,
            RenderSettings {
                zoom: 1.0,
                offset_x: 0.0,
                offset_y: 0.0,
                brightness: 0,
                contrast: 0.0,
                background: None,
            },
            PHOTO_WIDTH,
            PHOTO_HEIGHT,
        );

        assert_eq!(portrait.dimensions(), (PHOTO_WIDTH, PHOTO_HEIGHT));
    }

    #[test]
    fn mask_replaces_only_the_background() {
        let source = DynamicImage::ImageRgb8(RgbImage::from_pixel(3, 4, image::Rgb([200, 20, 10])));
        let mut mask = GrayImage::from_pixel(3, 4, Luma([0]));
        mask.put_pixel(1, 1, Luma([255]));
        let portrait = render_portrait(
            &source,
            Some(&mask),
            RenderSettings {
                zoom: 1.0,
                offset_x: 0.0,
                offset_y: 0.0,
                brightness: 0,
                contrast: 0.0,
                background: Some([255, 255, 255]),
            },
            3,
            4,
        );

        assert_eq!(*portrait.get_pixel(0, 0), image::Rgb([255, 255, 255]));
        assert_eq!(*portrait.get_pixel(1, 1), image::Rgb([200, 20, 10]));
    }

    #[test]
    fn brightness_is_applied_before_compositing() {
        let source =
            DynamicImage::ImageRgb8(RgbImage::from_pixel(3, 4, image::Rgb([100, 100, 100])));
        let mask = GrayImage::from_pixel(3, 4, Luma([0]));
        let portrait = render_portrait(
            &source,
            Some(&mask),
            RenderSettings {
                zoom: 1.0,
                offset_x: 0.0,
                offset_y: 0.0,
                brightness: 50,
                contrast: 0.0,
                background: Some([10, 20, 30]),
            },
            3,
            4,
        );

        assert_eq!(*portrait.get_pixel(1, 1), image::Rgb([10, 20, 30]));
    }

    #[test]
    fn embedded_ai_model_produces_a_source_sized_mask() {
        let model =
            Model::load_static_slice(MODNET_MODEL).expect("embedded MODNet model should load");
        let source =
            DynamicImage::ImageRgb8(RgbImage::from_pixel(96, 128, image::Rgb([180, 150, 120])));
        let mask = infer_foreground_mask(&model, &source).expect("MODNet inference should succeed");

        assert_eq!(mask.dimensions(), source.dimensions());
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

    #[cfg(windows)]
    #[test]
    fn hidden_powershell_still_captures_output() {
        let mut command = std::process::Command::new("powershell");
        command.args(["-NoProfile", "-Command", "Write-Output PRINT_OK"]);
        super::hide_console_window(&mut command);
        let output = command.output().expect("powershell should run");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.lines().any(|line| line.trim() == "PRINT_OK"));
    }
}

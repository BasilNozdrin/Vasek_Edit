//! GUI application — state, rendering, and input for vasek-edit.

use std::path::Path;

use editor_core::{Document, LineEnding};
use eframe::egui::{self, Color32, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2};

// ── constants ─────────────────────────────────────────────────────────────────

const FONT_SIZE_DEFAULT: f32 = 14.0;
const FONT_SIZE_MIN: f32 = 8.0;
const FONT_SIZE_MAX: f32 = 36.0;
const MINIMAP_PANEL_W: f32 = 90.0;
const GUTTER_PAD: f32 = 6.0;

// ── colours ───────────────────────────────────────────────────────────────────

const BG: Color32 = Color32::from_rgb(30, 30, 35);
const LINE_HL: Color32 = Color32::from_rgb(42, 46, 56);
const GUTTER_FG: Color32 = Color32::from_rgb(95, 99, 115);
const GUTTER_LINE: Color32 = Color32::from_rgb(55, 58, 68);
const TEXT_FG: Color32 = Color32::from_rgb(220, 220, 230);
const CURSOR_CLR: Color32 = Color32::WHITE;
const STATUS_BG: Color32 = Color32::from_rgb(25, 25, 70);
const STATUS_FG: Color32 = Color32::WHITE;
const MINIMAP_BG: Color32 = Color32::from_rgb(22, 22, 28);
const MINIMAP_CONTENT: Color32 = Color32::from_rgb(80, 82, 95);
const MINIMAP_VIEWPORT: Color32 = Color32::from_rgba_premultiplied(80, 120, 220, 70);

// ── GuiApp ────────────────────────────────────────────────────────────────────

/// Top-level GUI application state.
pub struct GuiApp {
    pub doc: Option<Document>,
    show_line_numbers: bool,
    show_minimap: bool,
    font_size: f32,
    message: String,
    /// Vertical scroll position in pixels (tracked from ScrollArea output).
    scroll_y: f32,
    /// Forced scroll target set by minimap clicks; consumed once.
    target_scroll: Option<f32>,
    /// True when the cursor moved this frame (triggers auto-scroll).
    cursor_moved: bool,
}

impl GuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<&Path>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let doc = path.and_then(|p| Document::open(p).ok());
        Self {
            doc,
            show_line_numbers: true,
            show_minimap: true,
            font_size: FONT_SIZE_DEFAULT,
            message: String::new(),
            scroll_y: 0.0,
            target_scroll: None,
            cursor_moved: false,
        }
    }

    // ── file operations ───────────────────────────────────────────────────────

    fn open_path(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(doc) => {
                self.doc = Some(doc);
                self.message = String::new();
                self.scroll_y = 0.0;
                self.target_scroll = None;
            }
            Err(e) => self.message = format!("Error opening file: {e}"),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.open_path(&path);
        }
    }

    fn new_doc(&mut self) {
        let tmp = std::env::temp_dir().join("vasek_new.txt");
        let _ = std::fs::write(&tmp, "");
        self.open_path(&tmp);
    }

    fn save(&mut self) {
        match self.doc.as_mut().map(|d| d.save()) {
            Some(Ok(())) => {
                let name = self
                    .doc
                    .as_ref()
                    .and_then(|d| d.path().file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                self.message = format!("Saved {name}");
            }
            Some(Err(e)) => self.message = format!("Save failed: {e}"),
            None => {}
        }
    }

    fn save_as(&mut self) {
        let Some(path) = rfd::FileDialog::new().save_file() else {
            return;
        };
        let Some(content) = self.doc.as_ref().map(DocToString::to_string) else {
            return;
        };
        match std::fs::write(&path, content) {
            Ok(()) => {
                self.message = format!("Saved to {}", path.display());
                self.open_path(&path);
            }
            Err(e) => self.message = format!("Save failed: {e}"),
        }
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn font_id(&self) -> FontId {
        FontId::monospace(self.font_size)
    }

    fn row_height(&self, ctx: &egui::Context) -> f32 {
        let fid = self.font_id();
        ctx.fonts_mut(|f| f.row_height(&fid))
    }

    fn window_title(&self) -> String {
        match &self.doc {
            Some(doc) => {
                let dirty = if doc.is_dirty() { "• " } else { "" };
                let name = doc
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                format!("{dirty}{name} — vasek-edit")
            }
            None => "vasek-edit".into(),
        }
    }

    // ── input ─────────────────────────────────────────────────────────────────

    fn handle_input(&mut self, ctx: &egui::Context) {
        let mut moved = false;
        ctx.input(|i| {
            for event in &i.events {
                let Some(doc) = self.doc.as_mut() else {
                    continue;
                };
                match event {
                    // Text insertion — skip ctrl/alt combos and tabs
                    egui::Event::Text(text)
                        if !i.modifiers.ctrl
                            && !i.modifiers.alt
                            && !i.modifiers.mac_cmd
                            && !text.contains('\t') =>
                    {
                        doc.insert_at_cursor(text);
                        moved = true;
                    }
                    egui::Event::Paste(text) => {
                        doc.insert_at_cursor(text);
                        moved = true;
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => match key {
                        Key::Enter => {
                            doc.insert_at_cursor("\n");
                            moved = true;
                        }
                        Key::Tab => {
                            doc.insert_at_cursor("    ");
                            moved = true;
                        }
                        Key::Backspace => {
                            doc.backspace();
                            moved = true;
                        }
                        Key::Delete => {
                            doc.delete_forward();
                            moved = true;
                        }
                        Key::ArrowLeft if modifiers.ctrl => {
                            doc.word_left();
                            moved = true;
                        }
                        Key::ArrowRight if modifiers.ctrl => {
                            doc.word_right();
                            moved = true;
                        }
                        Key::ArrowLeft => {
                            doc.move_left();
                            moved = true;
                        }
                        Key::ArrowRight => {
                            doc.move_right();
                            moved = true;
                        }
                        Key::ArrowUp => {
                            doc.move_up();
                            moved = true;
                        }
                        Key::ArrowDown => {
                            doc.move_down();
                            moved = true;
                        }
                        Key::Home => {
                            doc.move_home();
                            moved = true;
                        }
                        Key::End => {
                            doc.move_end();
                            moved = true;
                        }
                        Key::PageUp => {
                            doc.page_up(20);
                            moved = true;
                        }
                        Key::PageDown => {
                            doc.page_down(20);
                            moved = true;
                        }
                        Key::Z if modifiers.ctrl && !modifiers.shift => {
                            doc.undo();
                            moved = true;
                        }
                        Key::Y if modifiers.ctrl => {
                            doc.redo();
                            moved = true;
                        }
                        Key::Z if modifiers.ctrl && modifiers.shift => {
                            doc.redo();
                            moved = true;
                        }
                        Key::S if modifiers.ctrl => self.save(),
                        Key::O if modifiers.ctrl => self.open_dialog(),
                        Key::N if modifiers.ctrl => self.new_doc(),
                        Key::F1 => self.show_line_numbers = !self.show_line_numbers,
                        Key::F2 => self.show_minimap = !self.show_minimap,
                        Key::Plus | Key::Equals if modifiers.ctrl => {
                            self.font_size = (self.font_size + 1.0).min(FONT_SIZE_MAX);
                        }
                        Key::Minus if modifiers.ctrl => {
                            self.font_size = (self.font_size - 1.0).max(FONT_SIZE_MIN);
                        }
                        Key::Num0 if modifiers.ctrl => {
                            self.font_size = FONT_SIZE_DEFAULT;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        });
        self.cursor_moved = moved;
    }

    // ── rendering ─────────────────────────────────────────────────────────────

    fn render_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New           Ctrl+N").clicked() {
                    self.new_doc();
                    ui.close();
                }
                if ui.button("Open…         Ctrl+O").clicked() {
                    self.open_dialog();
                    ui.close();
                }
                ui.separator();
                if ui.button("Save          Ctrl+S").clicked() {
                    self.save();
                    ui.close();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as();
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("Edit", |ui| {
                let can_undo = self.doc.is_some();
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo   Ctrl+Z"))
                    .clicked()
                {
                    if let Some(doc) = &mut self.doc {
                        doc.undo();
                        self.cursor_moved = true;
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(can_undo, egui::Button::new("Redo   Ctrl+Y"))
                    .clicked()
                {
                    if let Some(doc) = &mut self.doc {
                        doc.redo();
                        self.cursor_moved = true;
                    }
                    ui.close();
                }
            });

            ui.menu_button("View", |ui| {
                if ui
                    .checkbox(&mut self.show_line_numbers, "Line numbers   F1")
                    .clicked()
                {
                    ui.close();
                }
                if ui
                    .checkbox(&mut self.show_minimap, "Minimap        F2")
                    .clicked()
                {
                    ui.close();
                }
                ui.separator();
                ui.label("Font size");
                ui.horizontal(|ui| {
                    if ui.button("−").clicked() {
                        self.font_size = (self.font_size - 1.0).max(FONT_SIZE_MIN);
                    }
                    ui.label(format!("{:.0}", self.font_size));
                    if ui.button("+").clicked() {
                        self.font_size = (self.font_size + 1.0).min(FONT_SIZE_MAX);
                    }
                    if ui.button("Reset").clicked() {
                        self.font_size = FONT_SIZE_DEFAULT;
                    }
                });
            });
        });
    }

    fn render_status(&self, ui: &mut egui::Ui) {
        let Some(doc) = &self.doc else {
            ui.label("No file open — use File → Open or Ctrl+O");
            return;
        };
        let dirty = if doc.is_dirty() { " [+]" } else { "" };
        let name = doc
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        let enc = match doc.line_ending() {
            LineEnding::Lf => "UTF-8 LF",
            LineEnding::CrLf => "UTF-8 CRLF",
        };
        let pos = format!("{}:{}", doc.cursor.line + 1, doc.cursor.col + 1);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("  {name}{dirty}  "))
                    .color(STATUS_FG)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("  {enc}  {pos}  ")).color(STATUS_FG));
                if !self.message.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("  {}  ", &self.message))
                            .color(Color32::YELLOW),
                    );
                }
            });
        });
    }

    fn render_editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let font_id = self.font_id();
        let row_h = {
            let fid = font_id.clone();
            ctx.fonts_mut(|f| f.row_height(&fid))
        };
        let char_w = {
            let fid = font_id.clone();
            ctx.fonts_mut(|f| f.glyph_width(&fid, ' '))
        };

        // Snapshot read-only doc state before any closures
        let (total_lines, gutter_w, cursor_line, cursor_col) = {
            let doc = self.doc.as_ref().unwrap();
            let tl = doc.line_count().max(1);
            let digits = format!("{tl}").len();
            let gw = if self.show_line_numbers {
                digits as f32 * char_w + GUTTER_PAD * 2.5
            } else {
                0.0
            };
            (tl, gw, doc.cursor.line, doc.cursor.col)
        };

        let total_h = total_lines as f32 * row_h;
        let avail_w = ui.available_width();
        let cursor_moved = self.cursor_moved;

        let mut scroll_area = egui::ScrollArea::both().auto_shrink([false, false]);
        if let Some(y) = self.target_scroll.take() {
            scroll_area = scroll_area.vertical_scroll_offset(y);
        }

        let mut click_info: Option<(Pos2, Pos2)> = None;

        let out = scroll_area.show(ui, |ui| {
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(avail_w.max(400.0), total_h), Sense::click());

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, BG);

            // Visible line band
            let clip = ui.clip_rect();
            let first = ((clip.min.y - rect.min.y) / row_h).floor().max(0.0) as usize;
            let last = (((clip.max.y - rect.min.y) / row_h).ceil() as usize).min(total_lines);

            // Borrow doc immutably just for reading lines
            let doc = self.doc.as_ref().unwrap();

            for i in first..last {
                let y = rect.min.y + i as f32 * row_h;

                // Current-line highlight
                if i == cursor_line {
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.min.x, y),
                            Vec2::new(rect.width(), row_h),
                        ),
                        0.0,
                        LINE_HL,
                    );
                }

                // Gutter
                if self.show_line_numbers {
                    painter.vline(
                        rect.min.x + gutter_w,
                        y..=y + row_h,
                        Stroke::new(1.0, GUTTER_LINE),
                    );
                    painter.text(
                        Pos2::new(rect.min.x + gutter_w - GUTTER_PAD, y),
                        egui::Align2::RIGHT_TOP,
                        format!("{}", i + 1),
                        font_id.clone(),
                        GUTTER_FG,
                    );
                }

                // Line text
                let line = doc.line_at(i).map(|c| c.into_owned()).unwrap_or_default();
                painter.text(
                    Pos2::new(rect.min.x + gutter_w + GUTTER_PAD, y),
                    egui::Align2::LEFT_TOP,
                    &line,
                    font_id.clone(),
                    TEXT_FG,
                );

                // Cursor (bar style)
                if i == cursor_line {
                    let col_ch = line[..cursor_col.min(line.len())].chars().count();
                    let cx = rect.min.x + gutter_w + GUTTER_PAD + col_ch as f32 * char_w;
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(cx, y), Vec2::new(2.0, row_h)),
                        0.0,
                        CURSOR_CLR,
                    );
                }
            }

            // Click-to-place-cursor
            if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    click_info = Some((pos, rect.min));
                }
            }

            // Auto-scroll cursor into view on movement
            if cursor_moved {
                let cy = rect.min.y + cursor_line as f32 * row_h;
                ui.scroll_to_rect(
                    Rect::from_min_size(Pos2::new(rect.min.x, cy), Vec2::new(1.0, row_h)),
                    Some(egui::Align::Center),
                );
            }
        });

        self.scroll_y = out.state.offset.y;

        // Apply click — doc borrow released above
        if let Some((click, rect_min)) = click_info {
            let raw_line = ((click.y - rect_min.y) / row_h).floor() as usize;
            let raw_chars = ((click.x - rect_min.x - gutter_w - GUTTER_PAD) / char_w)
                .floor()
                .max(0.0) as usize;
            let doc = self.doc.as_mut().unwrap();
            let line = raw_line.min(doc.line_count().saturating_sub(1));
            let line_str = doc
                .line_at(line)
                .map(|c| c.into_owned())
                .unwrap_or_default();
            let col = char_to_byte_col(&line_str, raw_chars);
            doc.cursor.line = line;
            doc.cursor.col = col;
            doc.flush_history();
            self.cursor_moved = true;
        }
    }

    fn render_welcome(ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("vasek-edit\n\nCtrl+O to open a file\nCtrl+N for a new file")
                    .size(18.0)
                    .color(Color32::from_gray(120)),
            );
        });
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 1. Keyboard input
        if self.doc.is_some() {
            self.handle_input(&ctx);
        }

        // 2. Window title
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // 3. Menu bar
        egui::Panel::top("menu").show_inside(ui, |ui| {
            self.render_menu(ui, &ctx);
        });

        // 4. Status bar
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(STATUS_BG)
                    .inner_margin(egui::Margin::symmetric(4, 3)),
            )
            .show_inside(ui, |ui| {
                self.render_status(ui);
            });

        // 5. Minimap side panel
        if self.show_minimap {
            // Extract doc state so the immutable borrow ends before any mutation below.
            let minimap_state = self
                .doc
                .as_ref()
                .map(|d| (d.line_count().max(1), d.cursor.line));

            if let Some((total_lines, cursor_line)) = minimap_state {
                let scroll_y = self.scroll_y;
                let row_h = self.row_height(&ctx);
                let mut minimap_jump: Option<usize> = None;

                egui::Panel::right("minimap")
                    .exact_size(MINIMAP_PANEL_W)
                    .resizable(false)
                    .frame(egui::Frame::new().fill(MINIMAP_BG))
                    .show_inside(ui, |ui| {
                        minimap_jump =
                            render_minimap(ui, total_lines, cursor_line, scroll_y, row_h);
                    });

                if let (Some(line), Some(doc)) = (minimap_jump, self.doc.as_mut()) {
                    let clamped = line.min(doc.line_count().saturating_sub(1));
                    doc.cursor.line = clamped;
                    doc.flush_history();
                    self.target_scroll = Some(clamped as f32 * row_h);
                    self.cursor_moved = true;
                }
            }
        }

        // 6. Central editor
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show_inside(ui, |ui| {
                if self.doc.is_some() {
                    self.render_editor(ui, &ctx);
                } else {
                    Self::render_welcome(ui);
                }
            });

        // 7. Reset per-frame cursor flag
        self.cursor_moved = false;
    }
}

// ── minimap ───────────────────────────────────────────────────────────────────

/// Render the minimap content and return the target line if the user clicks.
fn render_minimap(
    ui: &mut egui::Ui,
    total_lines: usize,
    cursor_line: usize,
    scroll_y: f32,
    row_h: f32,
) -> Option<usize> {
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, MINIMAP_BG);

    let h = rect.height();
    if h < 1.0 || total_lines == 0 {
        return None;
    }

    // Density bars — one pixel row per minimap pixel
    let lines_per_px = total_lines as f32 / h;
    for py in 0..h as usize {
        let src = ((py as f32 * lines_per_px) as usize).min(total_lines - 1);
        if src % 2 == 0 || src == cursor_line {
            painter.hline(
                rect.min.x + 4.0..=rect.max.x - 4.0,
                rect.min.y + py as f32,
                Stroke::new(1.0, MINIMAP_CONTENT),
            );
        }
    }

    // Viewport band
    let vp_top = scroll_y / row_h;
    let vp_h_lines = h / row_h;
    let band_top = rect.min.y + (vp_top / total_lines as f32) * h;
    let band_h = (vp_h_lines / total_lines as f32 * h).max(4.0);
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.min.x, band_top),
            Vec2::new(rect.width(), band_h),
        ),
        0.0,
        MINIMAP_VIEWPORT,
    );

    // Mouse interaction
    let resp = ui.interact(rect, ui.id(), Sense::click_and_drag());
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.clicked() || resp.dragged() {
            let frac = ((pos.y - rect.min.y) / h).clamp(0.0, 1.0);
            return Some((frac * total_lines as f32) as usize);
        }
    }
    None
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Convert a character-column index to a byte offset within `line`.
fn char_to_byte_col(line: &str, char_col: usize) -> usize {
    line.char_indices()
        .nth(char_col)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// `Document::to_string` bridge (Document doesn't implement `Display` yet).
trait DocToString {
    fn to_string(&self) -> String;
}

impl DocToString for Document {
    fn to_string(&self) -> String {
        (0..self.line_count())
            .filter_map(|i| self.line_at(i).map(|c| c.into_owned()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

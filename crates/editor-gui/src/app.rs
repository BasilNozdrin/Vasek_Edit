//! GUI application — state, rendering, and input for vasek-edit.

use std::path::Path;

use editor_core::{Document, LineEnding};
use eframe::egui::{self, Color32, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2};

// ── constants ─────────────────────────────────────────────────────────────────

const FONT_SIZE_DEFAULT: f32 = 14.0;
const FONT_SIZE_MIN: f32 = 8.0;
const FONT_SIZE_MAX: f32 = 36.0;
const GUTTER_PAD: f32 = 6.0;

// Minimap overlay geometry
const MINIMAP_W: f32 = 110.0;
const MINIMAP_MAX_H: f32 = 230.0;
const MINIMAP_LINE_H: f32 = 2.0;
const MINIMAP_CHAR_W: f32 = 1.0;
const MINIMAP_MAX_CHARS: usize = 90;
const MINIMAP_PADDING: f32 = 6.0;
const MINIMAP_CORNER_R: f32 = 4.0;

// ── colours ───────────────────────────────────────────────────────────────────

const BG: Color32 = Color32::from_rgb(30, 30, 35);
const LINE_HL: Color32 = Color32::from_rgb(42, 46, 56);
const SEL_HL: Color32 = Color32::from_rgba_premultiplied(55, 100, 180, 90);
const GUTTER_FG: Color32 = Color32::from_rgb(95, 99, 115);
const GUTTER_LINE: Color32 = Color32::from_rgb(55, 58, 68);
const TEXT_FG: Color32 = Color32::from_rgb(220, 220, 230);
const CURSOR_CLR: Color32 = Color32::WHITE;
const STATUS_BG: Color32 = Color32::from_rgb(25, 25, 70);
const STATUS_FG: Color32 = Color32::WHITE;
// Minimap: semi-transparent so the editor shows through
const MINIMAP_BG: Color32 = Color32::from_rgba_premultiplied(16, 16, 20, 210);
const MINIMAP_CONTENT: Color32 = Color32::from_rgb(90, 95, 115);
const MINIMAP_VIEWPORT: Color32 = Color32::from_rgba_premultiplied(50, 90, 200, 55);
const MINIMAP_CURSOR_ROW: Color32 = Color32::from_rgba_premultiplied(200, 160, 40, 60);

// ── GuiApp ────────────────────────────────────────────────────────────────────

/// Top-level GUI application state.
pub struct GuiApp {
    pub doc: Option<Document>,
    show_line_numbers: bool,
    show_minimap: bool,
    font_size: f32,
    message: String,
    scroll_y: f32,
    target_scroll: Option<f32>,
    cursor_moved: bool,
    /// Selection anchor (line, byte-col). None = no selection.
    sel_anchor: Option<(usize, usize)>,
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
            sel_anchor: None,
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
                self.sel_anchor = None;
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

    /// Extract the currently selected text, if any.
    fn selected_text(&self) -> Option<String> {
        let anchor = self.sel_anchor?;
        let doc = self.doc.as_ref()?;
        let cursor = (doc.cursor.line, doc.cursor.col);
        if anchor == cursor {
            return None;
        }
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        let (sl, sc) = start;
        let (el, ec) = end;

        if sl == el {
            let line = doc.line_at(sl)?.into_owned();
            let sc = sc.min(line.len());
            let ec = ec.min(line.len());
            Some(line[sc..ec].to_string())
        } else {
            let mut out = String::new();
            for i in sl..=el {
                let line = doc.line_at(i).map(|c| c.into_owned()).unwrap_or_default();
                if i == sl {
                    out.push_str(&line[sc.min(line.len())..]);
                } else if i == el {
                    out.push_str(&line[..ec.min(line.len())]);
                } else {
                    out.push_str(&line);
                }
                if i < el {
                    out.push('\n');
                }
            }
            Some(out)
        }
    }

    // ── input ─────────────────────────────────────────────────────────────────

    fn handle_input(&mut self, ctx: &egui::Context) {
        let mut moved = false;
        let mut text_to_copy: Option<String> = None;

        ctx.input(|i| {
            for event in &i.events {
                match event {
                    // ── text insertion ────────────────────────────────────────
                    egui::Event::Text(text)
                        if !i.modifiers.ctrl
                            && !i.modifiers.alt
                            && !i.modifiers.mac_cmd
                            && !text.contains('\t') =>
                    {
                        if let Some(doc) = self.doc.as_mut() {
                            doc.insert_at_cursor(text);
                            self.sel_anchor = None;
                            moved = true;
                        }
                    }
                    egui::Event::Paste(text) => {
                        if let Some(doc) = self.doc.as_mut() {
                            doc.insert_at_cursor(text);
                            self.sel_anchor = None;
                            moved = true;
                        }
                    }

                    // ── key events ────────────────────────────────────────────
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        // Snapshot cursor position before any movement (for anchor).
                        let cur = self.doc.as_ref().map(|d| (d.cursor.line, d.cursor.col));

                        match key {
                            // ── editing ───────────────────────────────────────
                            Key::Enter => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.insert_at_cursor("\n");
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }
                            Key::Tab => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.insert_at_cursor("    ");
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }
                            Key::Backspace => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.backspace();
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }
                            Key::Delete => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.delete_forward();
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }

                            // ── word navigation ───────────────────────────────
                            Key::ArrowLeft if modifiers.ctrl && modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.word_left();
                                    moved = true;
                                }
                            }
                            Key::ArrowRight if modifiers.ctrl && modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.word_right();
                                    moved = true;
                                }
                            }
                            Key::ArrowLeft if modifiers.ctrl => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.word_left();
                                    moved = true;
                                }
                            }
                            Key::ArrowRight if modifiers.ctrl => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.word_right();
                                    moved = true;
                                }
                            }

                            // ── arrow navigation with shift (selection) ───────
                            Key::ArrowLeft if modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_left();
                                    moved = true;
                                }
                            }
                            Key::ArrowRight if modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_right();
                                    moved = true;
                                }
                            }
                            Key::ArrowUp if modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_up();
                                    moved = true;
                                }
                            }
                            Key::ArrowDown if modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_down();
                                    moved = true;
                                }
                            }

                            // ── arrow navigation (plain) ──────────────────────
                            Key::ArrowLeft => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_left();
                                    moved = true;
                                }
                            }
                            Key::ArrowRight => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_right();
                                    moved = true;
                                }
                            }
                            Key::ArrowUp => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_up();
                                    moved = true;
                                }
                            }
                            Key::ArrowDown => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_down();
                                    moved = true;
                                }
                            }

                            // ── Home / End ────────────────────────────────────
                            Key::Home if modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_home();
                                    moved = true;
                                }
                            }
                            Key::End if modifiers.shift => {
                                if self.sel_anchor.is_none() {
                                    self.sel_anchor = cur;
                                }
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_end();
                                    moved = true;
                                }
                            }
                            Key::Home => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_home();
                                    moved = true;
                                }
                            }
                            Key::End => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.move_end();
                                    moved = true;
                                }
                            }

                            // ── Page Up / Down ────────────────────────────────
                            Key::PageUp => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.page_up(20);
                                    moved = true;
                                }
                            }
                            Key::PageDown => {
                                self.sel_anchor = None;
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.page_down(20);
                                    moved = true;
                                }
                            }

                            // ── undo / redo ───────────────────────────────────
                            Key::Z if modifiers.ctrl && !modifiers.shift => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.undo();
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }
                            Key::Y if modifiers.ctrl => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.redo();
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }
                            Key::Z if modifiers.ctrl && modifiers.shift => {
                                if let Some(doc) = self.doc.as_mut() {
                                    doc.redo();
                                    self.sel_anchor = None;
                                    moved = true;
                                }
                            }

                            // ── select all ────────────────────────────────────
                            Key::A if modifiers.ctrl => {
                                if let Some(doc) = self.doc.as_mut() {
                                    let total = doc.line_count();
                                    if total > 0 {
                                        self.sel_anchor = Some((0, 0));
                                        let last = total - 1;
                                        let last_len = doc
                                            .line_at(last)
                                            .map(|c| c.into_owned())
                                            .map_or(0, |s| s.len());
                                        doc.cursor.line = last;
                                        doc.cursor.col = last_len;
                                        moved = true;
                                    }
                                }
                            }

                            // ── copy ──────────────────────────────────────────
                            Key::C if modifiers.ctrl => {
                                text_to_copy = self.selected_text();
                            }

                            // ── file / app ────────────────────────────────────
                            Key::S if modifiers.ctrl => self.save(),
                            Key::O if modifiers.ctrl => self.open_dialog(),
                            Key::N if modifiers.ctrl => self.new_doc(),

                            // ── view toggles ──────────────────────────────────
                            Key::F1 => self.show_line_numbers = !self.show_line_numbers,
                            Key::F2 => self.show_minimap = !self.show_minimap,

                            // ── font size: F9 smaller, F10 larger ─────────────
                            Key::F9 => {
                                self.font_size = (self.font_size - 1.0).max(FONT_SIZE_MIN);
                            }
                            Key::F10 => {
                                self.font_size = (self.font_size + 1.0).min(FONT_SIZE_MAX);
                            }
                            // Ctrl+- / Ctrl+= also work
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
                        }
                    }
                    _ => {}
                }
            }
        });

        if let Some(text) = text_to_copy {
            ctx.copy_text(text);
        }

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
                let has_doc = self.doc.is_some();
                if ui
                    .add_enabled(has_doc, egui::Button::new("Undo   Ctrl+Z"))
                    .clicked()
                {
                    if let Some(doc) = &mut self.doc {
                        doc.undo();
                        self.sel_anchor = None;
                        self.cursor_moved = true;
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(has_doc, egui::Button::new("Redo   Ctrl+Y"))
                    .clicked()
                {
                    if let Some(doc) = &mut self.doc {
                        doc.redo();
                        self.sel_anchor = None;
                        self.cursor_moved = true;
                    }
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(has_doc, egui::Button::new("Select All  Ctrl+A"))
                    .clicked()
                {
                    if let Some(doc) = &mut self.doc {
                        let total = doc.line_count();
                        if total > 0 {
                            self.sel_anchor = Some((0, 0));
                            let last = total - 1;
                            let last_len = doc
                                .line_at(last)
                                .map(|c| c.into_owned())
                                .map_or(0, |s| s.len());
                            doc.cursor.line = last;
                            doc.cursor.col = last_len;
                            self.cursor_moved = true;
                        }
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
                ui.label("Font size  (F9 / F10)");
                ui.horizontal(|ui| {
                    if ui.button("−  F9").clicked() {
                        self.font_size = (self.font_size - 1.0).max(FONT_SIZE_MIN);
                    }
                    ui.label(format!("{:.0}px", self.font_size));
                    if ui.button("+  F10").clicked() {
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
            ui.label("No file open — Ctrl+O to open, Ctrl+N for new");
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
        let sel_info = self
            .sel_anchor
            .map(|a| {
                let c = (doc.cursor.line, doc.cursor.col);
                let (s, e) = if a <= c { (a, c) } else { (c, a) };
                let lines = e.0 - s.0;
                if lines == 0 {
                    format!("  [{} sel]", e.1.saturating_sub(s.1))
                } else {
                    format!("  [{} lines sel]", lines + 1)
                }
            })
            .unwrap_or_default();

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("  {name}{dirty}  "))
                    .color(STATUS_FG)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("  {enc}  {pos}{sel_info}  ")).color(STATUS_FG),
                );
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

        // Snapshot read-only doc state before closures
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
        let sel_anchor = self.sel_anchor;

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

            let clip = ui.clip_rect();
            let first = ((clip.min.y - rect.min.y) / row_h).floor().max(0.0) as usize;
            let last = (((clip.max.y - rect.min.y) / row_h).ceil() as usize).min(total_lines);

            let doc = self.doc.as_ref().unwrap();

            // Pre-compute normalised selection bounds (start <= end)
            let sel_bounds = sel_anchor.map(|anchor| {
                let cursor = (cursor_line, cursor_col);
                if anchor <= cursor {
                    (anchor, cursor)
                } else {
                    (cursor, anchor)
                }
            });

            for i in first..last {
                let y = rect.min.y + i as f32 * row_h;
                let line = doc.line_at(i).map(|c| c.into_owned()).unwrap_or_default();

                // Current-line background
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

                // Selection highlight
                if let Some(((sl, sc), (el, ec))) = sel_bounds {
                    if i >= sl && i <= el {
                        let text_x0 = rect.min.x + gutter_w + GUTTER_PAD;

                        let hx0 = if i == sl {
                            let ch = char_count_prefix(&line, sc);
                            text_x0 + ch as f32 * char_w
                        } else {
                            text_x0
                        };

                        let hx1 = if i == el {
                            let ch = char_count_prefix(&line, ec);
                            text_x0 + ch as f32 * char_w
                        } else {
                            // extend to right edge (covers trailing newline visually)
                            rect.max.x
                        };

                        if hx1 > hx0 {
                            painter.rect_filled(
                                Rect::from_min_max(Pos2::new(hx0, y), Pos2::new(hx1, y + row_h)),
                                0.0,
                                SEL_HL,
                            );
                        }
                    }
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
                painter.text(
                    Pos2::new(rect.min.x + gutter_w + GUTTER_PAD, y),
                    egui::Align2::LEFT_TOP,
                    &line,
                    font_id.clone(),
                    TEXT_FG,
                );

                // Cursor bar
                if i == cursor_line {
                    let col_ch = char_count_prefix(&line, cursor_col);
                    let cx = rect.min.x + gutter_w + GUTTER_PAD + col_ch as f32 * char_w;
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(cx, y), Vec2::new(2.0, row_h)),
                        0.0,
                        CURSOR_CLR,
                    );
                }
            }

            if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    click_info = Some((pos, rect.min));
                }
            }

            if cursor_moved {
                let cy = rect.min.y + cursor_line as f32 * row_h;
                ui.scroll_to_rect(
                    Rect::from_min_size(Pos2::new(rect.min.x, cy), Vec2::new(1.0, row_h)),
                    Some(egui::Align::Center),
                );
            }
        });

        self.scroll_y = out.state.offset.y;

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
            self.sel_anchor = None;
            self.cursor_moved = true;
        }
    }

    fn render_welcome(ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(
                    "vasek-edit\n\nCtrl+O  open file\nCtrl+N  new file\n\nF9 / F10  font size",
                )
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

        if self.doc.is_some() {
            self.handle_input(&ctx);
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        egui::Panel::top("menu").show_inside(ui, |ui| {
            self.render_menu(ui, &ctx);
        });

        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(STATUS_BG)
                    .inner_margin(egui::Margin::symmetric(4, 3)),
            )
            .show_inside(ui, |ui| {
                self.render_status(ui);
            });

        // Central editor panel
        let central_resp = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show_inside(ui, |ui| {
                if self.doc.is_some() {
                    self.render_editor(ui, &ctx);
                } else {
                    Self::render_welcome(ui);
                }
            });

        // Minimap overlay — rendered on top of the central panel
        if self.show_minimap {
            let panel_rect = central_resp.response.rect;
            let minimap_data = self.doc.as_ref().map(|doc| {
                let total = doc.line_count().max(1);
                let n_rows = total.min(minimap_max_rows());
                let cursor_line = doc.cursor.line;
                // Map minimap rows → file lines, sampling evenly
                let lines: Vec<String> = (0..n_rows)
                    .map(|row| {
                        let idx = if n_rows >= total {
                            row
                        } else {
                            row * total / n_rows
                        };
                        doc.line_at(idx).map(|c| c.into_owned()).unwrap_or_default()
                    })
                    .collect();
                (lines, total, cursor_line)
            });

            if let Some((lines, total_lines, cursor_line)) = minimap_data {
                let scroll_y = self.scroll_y;
                let row_h = self.row_height(&ctx);

                if let Some(target) = render_minimap_overlay(
                    &ctx,
                    &lines,
                    total_lines,
                    cursor_line,
                    scroll_y,
                    row_h,
                    panel_rect,
                ) {
                    if let Some(doc) = self.doc.as_mut() {
                        let clamped = target.min(doc.line_count().saturating_sub(1));
                        doc.cursor.line = clamped;
                        doc.flush_history();
                        self.target_scroll = Some(clamped as f32 * row_h);
                        self.cursor_moved = true;
                    }
                }
            }
        }

        self.cursor_moved = false;
    }
}

// ── minimap overlay ───────────────────────────────────────────────────────────

fn minimap_max_rows() -> usize {
    (MINIMAP_MAX_H / MINIMAP_LINE_H) as usize
}

/// Render the minimap as a floating overlay in the top-right corner of `panel_rect`.
/// Returns the target file line if the user clicks.
fn render_minimap_overlay(
    ctx: &egui::Context,
    lines: &[String],
    total_lines: usize,
    cursor_line: usize,
    scroll_y: f32,
    row_h: f32,
    panel_rect: Rect,
) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    let n_rows = lines.len();
    let actual_h = n_rows as f32 * MINIMAP_LINE_H;
    let minimap_rect = Rect::from_min_size(
        Pos2::new(
            panel_rect.max.x - MINIMAP_W - MINIMAP_PADDING,
            panel_rect.min.y + MINIMAP_PADDING,
        ),
        Vec2::new(MINIMAP_W, actual_h),
    );

    egui::Area::new(egui::Id::new("minimap_overlay"))
        .fixed_pos(minimap_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let painter = ui.painter();

            // Background
            painter.rect_filled(minimap_rect, MINIMAP_CORNER_R, MINIMAP_BG);

            // Viewport band
            let frac_top = (scroll_y / row_h) / total_lines as f32;
            // Estimate visible lines from panel height; fall back to a rough fraction
            let vp_frac = (row_h * 30.0 / (total_lines as f32 * row_h)).clamp(0.0, 1.0);
            let band_top = minimap_rect.min.y + frac_top * actual_h;
            let band_h = (vp_frac * actual_h).max(4.0).min(actual_h);
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(
                        minimap_rect.min.x,
                        band_top.min(minimap_rect.max.y - band_h),
                    ),
                    Vec2::new(MINIMAP_W, band_h),
                ),
                MINIMAP_CORNER_R,
                MINIMAP_VIEWPORT,
            );

            // Line content — one row per sampled file line
            for (row, line) in lines.iter().enumerate() {
                let y = minimap_rect.min.y + row as f32 * MINIMAP_LINE_H;

                // Highlight cursor's row
                let file_line = if n_rows >= total_lines {
                    row
                } else {
                    row * total_lines / n_rows
                };
                if file_line == cursor_line {
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(minimap_rect.min.x, y),
                            Vec2::new(MINIMAP_W, MINIMAP_LINE_H),
                        ),
                        0.0,
                        MINIMAP_CURSOR_ROW,
                    );
                }

                // Draw non-whitespace characters as 1 × 1.5 px dots
                let mut x = minimap_rect.min.x + 2.0;
                for ch in line.chars().take(MINIMAP_MAX_CHARS) {
                    if !ch.is_whitespace() {
                        painter.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(x, y),
                                Vec2::new(MINIMAP_CHAR_W, MINIMAP_LINE_H - 0.5),
                            ),
                            0.0,
                            MINIMAP_CONTENT,
                        );
                    }
                    x += MINIMAP_CHAR_W;
                    if x >= minimap_rect.max.x - 2.0 {
                        break;
                    }
                }
            }

            // Interaction — click or drag to jump
            let resp = ui.interact(minimap_rect, ui.id(), Sense::click_and_drag());
            if let Some(pos) = resp.interact_pointer_pos() {
                if resp.clicked() || resp.dragged() {
                    let frac = ((pos.y - minimap_rect.min.y) / actual_h).clamp(0.0, 1.0);
                    return Some((frac * total_lines as f32) as usize);
                }
            }
            None
        })
        .inner
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Number of chars in `s` up to byte offset `byte_col` (clamped to `s.len()`).
fn char_count_prefix(s: &str, byte_col: usize) -> usize {
    s[..byte_col.min(s.len())].chars().count()
}

/// Convert a char-column index to a byte offset within `line`.
fn char_to_byte_col(line: &str, char_col: usize) -> usize {
    line.char_indices()
        .nth(char_col)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

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

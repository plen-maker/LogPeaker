//! Files page: breadcrumb path, search, list/grid switch, sortable table.

use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::demo::{self, Node};
use super::theme::*;
use super::widgets::*;
use super::Workspace;
use crate::icons as ic;

struct Entry {
    name: String,
    dir: bool,
    size: u64,
    age: u64,
}

impl Workspace {
    pub(super) fn cwd_node(&self) -> &Node {
        let mut n = &self.fs;
        for name in &self.cwd {
            if let Some(c) = n.children.iter().find(|c| &c.name == name) {
                n = c;
            }
        }
        n
    }

    fn cwd_node_mut(&mut self) -> &mut Node {
        let mut n = &mut self.fs;
        for name in self.cwd.clone() {
            match n.children.iter().position(|c| c.name == name) {
                Some(i) => n = &mut n.children[i],
                None => break,
            }
        }
        n
    }

    pub(super) fn path_string(&self, name: &str) -> String {
        let mut s = String::new();
        for c in &self.cwd {
            s.push('/');
            s.push_str(c);
        }
        s.push('/');
        s.push_str(name);
        s
    }

    fn entries(&self) -> Vec<Entry> {
        let q = self.search.to_lowercase();
        let mut v: Vec<Entry> = self
            .cwd_node()
            .children
            .iter()
            .filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q))
            .map(|c| Entry { name: c.name.clone(), dir: c.dir, size: c.size, age: c.age_s })
            .collect();
        let key = self.sort_key;
        v.sort_by(|a, b| {
            let o = b.dir.cmp(&a.dir).then_with(|| match key {
                1 => a.size.cmp(&b.size),
                2 => a.age.cmp(&b.age),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
            if self.sort_desc { o.reverse() } else { o }
        });
        v
    }

    pub(super) fn open_entry(&mut self, name: &str, now: f64) {
        let Some(node) = self.cwd_node().children.iter().find(|c| c.name == name) else { return };
        if node.dir {
            self.cwd.push(name.to_string());
            self.sel_file = None;
            self.search.clear();
        } else {
            self.open_quick_look(name, now);
        }
    }

    pub(super) fn open_quick_look(&mut self, name: &str, now: f64) {
        self.quick_look = Some(name.to_string());
        self.sel_file = Some(name.to_string());
        self.ql_t.snap(0.0);
        self.ql_t.set(1.0, now, 210.0);
        self.touched.remove("ql_focused");
    }

    pub(super) fn files_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        let entries = self.entries();
        let overlay_open = self.quick_look.is_some() || self.confirm_delete.is_some() || self.ctx_menu.is_some() || self.cc_open;

        // ---- path bar
        let mut x = r.left();
        let cy = r.top() + 22.0;
        let mut crumbs: Vec<String> = vec!["Device".into()];
        crumbs.extend(self.cwd.iter().cloned());
        let mut goto: Option<usize> = None;
        for (i, c) in crumbs.iter().enumerate() {
            let last = i + 1 == crumbs.len();
            let g = p.layout_no_wrap(c.clone(), font(16.0, if last { "inter_medium" } else { "inter" }), TEXT);
            let rect = Rect::from_min_size(Pos2::new(x - 6.0, cy - 22.0), Vec2::new(g.size().x + 12.0, 44.0));
            let resp = ui.interact(rect, Id::new(("crumb", i)), Sense::click());
            let h = hv(ui, Id::new(("crumb_h", i)), resp.hovered(), 120.0);
            text(&p, Pos2::new(x, cy), Align2::LEFT_CENTER, c, font(16.0, if last { "inter_medium" } else { "inter" }), if last { TEXT } else { mix(TEXT2, TEXT, h) });
            if resp.has_focus() {
                focus_ring(&p, rect.shrink2(Vec2::new(0.0, 6.0)), 8.0);
            }
            if resp.clicked() && !last {
                goto = Some(i);
            }
            x += g.size().x + 12.0;
            if !last {
                text(&p, Pos2::new(x - 2.0, cy), Align2::LEFT_CENTER, "›", font(16.0, "inter"), TEXT3);
                x += 18.0;
            }
        }
        if let Some(i) = goto {
            self.cwd.truncate(i);
            self.sel_file = None;
        }

        // ---- search + view switch
        let vs = Rect::from_min_size(Pos2::new(r.right() - 96.0, r.top()), Vec2::new(96.0, 44.0));
        p.rect_filled(vs, cr(12.0), Color32::from_white_alpha(10));
        p.rect_stroke(vs, cr(12.0), Stroke::new(1.0, border()), StrokeKind::Inside);
        for (k, glyph, grid) in [(0, ic::LIST, false), (1, ic::LAYOUT_GRID, true)] {
            let seg = Rect::from_min_size(vs.min + Vec2::new(k as f32 * 48.0, 0.0), Vec2::new(48.0, 44.0));
            let resp = ui.interact(seg, Id::new(("viewsw", k)), Sense::click());
            let on = self.grid_view == grid;
            let t = hv(ui, Id::new(("viewsw_t", k)), on, 160.0);
            p.rect_filled(seg.shrink(4.0), cr(9.0), Color32::from_white_alpha((t * 44.0) as u8));
            icon(&p, glyph, seg.center(), 18.0, mix(TEXT3, TEXT, t.max(if resp.hovered() { 0.6 } else { 0.0 })));
            if resp.has_focus() {
                focus_ring(&p, seg.shrink(4.0), 9.0);
            }
            if resp.clicked() {
                self.grid_view = grid;
            }
        }
        let sb = Rect::from_min_size(Pos2::new(vs.left() - 14.0 - 260.0, r.top()), Vec2::new(260.0, 44.0));
        p.rect_filled(sb, cr(12.0), Color32::from_white_alpha(10));
        p.rect_stroke(sb, cr(12.0), Stroke::new(1.0, border()), StrokeKind::Inside);
        icon(&p, ic::SEARCH, Pos2::new(sb.left() + 24.0, sb.center().y), 17.0, TEXT3);
        let mut sui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    Pos2::new(sb.left() + 44.0, sb.top() + 4.0),
                    Pos2::new(sb.right() - 36.0, sb.bottom() - 4.0),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        sui.visuals_mut().override_text_color = Some(TEXT);
        let te = egui::TextEdit::singleline(&mut self.search)
            .id(Id::new("files_search"))
            .hint_text("Search")
            .frame(egui::Frame::NONE)
            .vertical_align(egui::Align::Center)
            .font(font(14.0, "inter"))
            .desired_width(180.0);
        sui.add(te);
        if !self.search.is_empty()
            && icon_button(ui, Id::new("search_clear"), Pos2::new(sb.right() - 22.0, sb.center().y), ic::X, 15.0, TEXT3).clicked()
        {
            self.search.clear();
        }

        // ---- table
        let table = Rect::from_min_max(Pos2::new(r.left(), r.top() + 60.0), Pos2::new(r.right(), r.bottom() - 34.0));
        panel(&p, table, 14.0, PANEL);
        let hdr = Rect::from_min_size(table.min, Vec2::new(table.width(), 42.0));
        let cols = [table.left() + 58.0, table.left() + table.width() * 0.58, table.left() + table.width() * 0.76];
        for (k, label) in ["Name", "Size", "Modified"].iter().enumerate() {
            let hit = Rect::from_min_size(Pos2::new(cols[k] - 10.0, hdr.top()), Vec2::new(if k == 0 { 200.0 } else { 130.0 }, 42.0));
            let resp = ui.interact(hit, Id::new(("sorthdr", k)), Sense::click());
            let h = hv(ui, Id::new(("sorthdr_h", k)), resp.hovered(), 120.0);
            let arrow = if self.sort_key == k { if self.sort_desc { "  ↓" } else { "  ↑" } } else { "" };
            text(&p, Pos2::new(cols[k], hdr.center().y), Align2::LEFT_CENTER, format!("{label}{arrow}"), font(12.5, "inter_medium"), mix(TEXT3, TEXT, h.max(if self.sort_key == k { 0.7 } else { 0.0 })));
            if resp.has_focus() {
                focus_ring(&p, hit.shrink2(Vec2::new(0.0, 8.0)), 8.0);
            }
            if resp.clicked() {
                if self.sort_key == k {
                    self.sort_desc = !self.sort_desc;
                } else {
                    self.sort_key = k;
                    self.sort_desc = false;
                }
            }
        }
        p.line_segment([Pos2::new(table.left() + 12.0, hdr.bottom()), Pos2::new(table.right() - 12.0, hdr.bottom())], Stroke::new(1.0, hairline()));

        let body = Rect::from_min_max(Pos2::new(table.left() + 6.0, hdr.bottom() + 4.0), Pos2::new(table.right() - 6.0, table.bottom() - 6.0));
        let mut open: Option<String> = None;
        let mut select: Option<String> = None;
        let mut menu: Option<(Pos2, String)> = None;
        let refocus = std::mem::take(&mut self.refocus_files);
        let sel_now = self.sel_file.clone();

        if entries.is_empty() {
            text(&p, body.center(), Align2::CENTER_CENTER, if self.search.is_empty() { "This folder is empty" } else { "No matching items" }, font(14.0, "inter"), TEXT3);
        } else if !self.grid_view {
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            bui.set_clip_rect(body);
            bui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(&mut bui, 48.0, entries.len(), |ui, range| {
                for i in range {
                    let e = &entries[i];
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::hover());
                    let id = Id::new(("frow", e.name.as_str()));
                    let resp = ui.interact(rect, id, Sense::click());
                    let sel = sel_now.as_deref() == Some(e.name.as_str());
                    if refocus && sel {
                        ui.memory_mut(|m| m.request_focus(id));
                    }
                    let pt = ui.painter();
                    if sel {
                        pt.rect_filled(rect, cr(10.0), SELECTED);
                    } else if resp.hovered() {
                        pt.rect_filled(rect, cr(10.0), Color32::from_white_alpha(9));
                    }
                    let cy = rect.center().y;
                    icon(pt, if e.dir { ic::FOLDER } else { ic::FILE_TEXT }, Pos2::new(rect.left() + 28.0, cy), 18.0, if sel { TEXT } else { TEXT2 });
                    text(pt, Pos2::new(rect.left() + 52.0, cy), Align2::LEFT_CENTER, &e.name, font(14.0, if sel { "inter_medium" } else { "inter" }), TEXT);
                    text(pt, Pos2::new(cols[1], cy), Align2::LEFT_CENTER, demo::fmt_size(e.size), font(13.0, "inter"), TEXT2);
                    text(pt, Pos2::new(cols[2], cy), Align2::LEFT_CENTER, demo::fmt_age(e.age), font(13.0, "inter"), TEXT2);
                    if resp.has_focus() {
                        focus_ring(pt, rect.shrink(2.0), 10.0);
                    }
                    if resp.clicked() {
                        select = Some(e.name.clone());
                    }
                    if resp.double_clicked() {
                        open = Some(e.name.clone());
                    }
                    if resp.secondary_clicked() || resp.long_touched() {
                        select = Some(e.name.clone());
                        menu = Some((resp.interact_pointer_pos().unwrap_or(rect.center()), e.name.clone()));
                    }
                }
            });
        } else {
            let (tw, th, g) = (170.0, 132.0, 14.0);
            let per = (((body.width() - 12.0) + g) / (tw + g)).floor().max(1.0) as usize;
            for (i, e) in entries.iter().enumerate() {
                let (cx, cyi) = (i % per, i / per);
                let rect = Rect::from_min_size(body.min + Vec2::new(6.0 + cx as f32 * (tw + g), 8.0 + cyi as f32 * (th + g)), Vec2::new(tw, th));
                if rect.bottom() > body.bottom() {
                    break;
                }
                let id = Id::new(("frow", e.name.as_str()));
                let resp = ui.interact(rect, id, Sense::click());
                let sel = sel_now.as_deref() == Some(e.name.as_str());
                if refocus && sel {
                    ui.memory_mut(|m| m.request_focus(id));
                }
                let pt = ui.painter();
                pt.rect_filled(rect, cr(12.0), if sel { SELECTED } else if resp.hovered() { Color32::from_white_alpha(12) } else { Color32::from_white_alpha(6) });
                icon(pt, if e.dir { ic::FOLDER } else { ic::FILE_TEXT }, Pos2::new(rect.center().x, rect.top() + 50.0), 34.0, if sel { TEXT } else { TEXT2 });
                text(pt, Pos2::new(rect.center().x, rect.top() + 96.0), Align2::CENTER_CENTER, &e.name, font(13.5, "inter_medium"), TEXT);
                text(pt, Pos2::new(rect.center().x, rect.top() + 116.0), Align2::CENTER_CENTER, if e.dir { "Folder".to_string() } else { demo::fmt_size(e.size) }, font(12.0, "inter"), TEXT3);
                if resp.has_focus() {
                    focus_ring(pt, rect, 12.0);
                }
                if resp.clicked() {
                    select = Some(e.name.clone());
                }
                if resp.double_clicked() {
                    open = Some(e.name.clone());
                }
                if resp.secondary_clicked() || resp.long_touched() {
                    select = Some(e.name.clone());
                    menu = Some((resp.interact_pointer_pos().unwrap_or(rect.center()), e.name.clone()));
                }
            }
        }

        // ---- status line
        let sel_count = self.sel_file.as_ref().map_or(0, |s| entries.iter().filter(|e| &e.name == s).count());
        text(&p, Pos2::new(r.left() + 4.0, r.bottom() - 14.0), Align2::LEFT_CENTER, format!("{} items  ·  {} selected", entries.len(), sel_count), font(12.5, "inter"), TEXT3);

        // ---- keyboard (only while nothing modal is open and no text field has focus)
        if !overlay_open && !ui.ctx().egui_wants_keyboard_input() && !entries.is_empty() {
            let (up, down, enter, space, back) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Space),
                    i.key_pressed(egui::Key::Backspace),
                )
            });
            let cur = sel_now.as_ref().and_then(|s| entries.iter().position(|e| &e.name == s));
            if down {
                select = Some(entries[cur.map_or(0, |c| (c + 1).min(entries.len() - 1))].name.clone());
            } else if up {
                select = Some(entries[cur.map_or(0, |c| c.saturating_sub(1))].name.clone());
            }
            if let Some(c) = cur {
                if enter {
                    open = Some(entries[c].name.clone());
                }
                if space && !entries[c].dir {
                    let n = entries[c].name.clone();
                    self.open_quick_look(&n, now);
                }
            }
            if back && !self.cwd.is_empty() {
                self.cwd.pop();
                self.sel_file = None;
            }
        }

        if let Some(s) = select {
            self.sel_file = Some(s);
        }
        if let Some(m) = menu {
            self.ctx_menu = Some(m);
            self.ctx_t.snap(0.0);
            self.ctx_t.set(1.0, now, 140.0);
        }
        if let Some(o) = open {
            self.open_entry(&o, now);
        }
    }

    pub(super) fn delete_file(&mut self, name: &str) {
        self.cwd_node_mut().children.retain(|c| c.name != name);
        if self.sel_file.as_deref() == Some(name) {
            self.sel_file = None;
        }
    }

    pub(super) fn file_content(&self, name: &str) -> String {
        self.cwd_node()
            .children
            .iter()
            .find(|c| c.name == name)
            .map(|c| if c.dir { format!("{name} is a folder.\n") } else { c.content.clone() })
            .unwrap_or_default()
    }
}

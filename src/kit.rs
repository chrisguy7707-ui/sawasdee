//! Colours, fonts and hand-drawn widgets shared by every screen.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, epaint::PathShape, vec2, Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Painter, Pos2, Rect,
    Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

use crate::lesson::Card;

pub const WHITE: Color32 = Color32::WHITE;

// ---------------------------------------------------------------- light / dark theme

static DARK: AtomicBool = AtomicBool::new(false);

pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

fn pick(light: (u8, u8, u8), dark: (u8, u8, u8)) -> Color32 {
    let (r, g, b) = if is_dark() { dark } else { light };
    Color32::from_rgb(r, g, b)
}

/// Window background.
pub fn bg() -> Color32 { pick((250, 249, 245), (22, 25, 31)) }
/// Cards, panels and lesson pages.
pub fn surface() -> Color32 { pick((255, 255, 255), (32, 36, 44)) }
/// Main text.
pub fn fg() -> Color32 { pick((55, 60, 70), (230, 233, 238)) }
/// Secondary text.
pub fn muted() -> Color32 { pick((135, 140, 150), (150, 158, 172)) }
pub fn border() -> Color32 { pick((226, 226, 222), (56, 62, 74)) }
pub fn primary_soft() -> Color32 { pick((214, 244, 238), (20, 66, 61)) }
pub fn green_soft() -> Color32 { pick((220, 246, 208), (30, 62, 34)) }
pub fn red_soft() -> Color32 { pick((255, 226, 226), (80, 34, 38)) }
/// Locked nodes, empty buttons.
pub fn locked() -> Color32 { pick((222, 222, 218), (58, 64, 75)) }
/// Hover background.
pub fn hover() -> Color32 { pick((244, 244, 240), (42, 47, 57)) }
/// Empty part of progress bars.
pub fn track() -> Color32 { pick((229, 229, 225), (50, 56, 67)) }
/// Space left by a used word tile.
pub fn ghost() -> Color32 { pick((236, 236, 232), (42, 47, 57)) }
/// Disabled text and icons.
pub fn faint() -> Color32 { pick((190, 190, 185), (92, 99, 112)) }

/// A readable text colour for an accent colour on its soft background.
pub fn ink(c: Color32, f: f32) -> Color32 {
    if is_dark() {
        let l = |v: u8| (v as f32 + (255.0 - v as f32) * 0.3) as u8;
        Color32::from_rgb(l(c.r()), l(c.g()), l(c.b()))
    } else {
        darken(c, f)
    }
}
pub const PRIMARY: Color32 = Color32::from_rgb(20, 170, 150);
pub const ORANGE: Color32 = Color32::from_rgb(255, 150, 40);
pub const RED: Color32 = Color32::from_rgb(235, 80, 80);
pub const GREEN: Color32 = Color32::from_rgb(70, 180, 60);
pub const BLUE: Color32 = Color32::from_rgb(30, 150, 240);
pub const GOLD: Color32 = Color32::from_rgb(255, 195, 0);
pub const TONE: Color32 = Color32::from_rgb(235, 120, 30);

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn darken(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8)
}

pub fn lighten(c: Color32, f: f32) -> Color32 {
    if is_dark() && f >= 0.3 {
        // Strong tints become dark tints in dark mode.
        let s = surface();
        let m = |v: u8, t: u8| (v as f32 + (t as f32 - v as f32) * f) as u8;
        return Color32::from_rgb(m(c.r(), s.r()), m(c.g(), s.g()), m(c.b(), s.b()));
    }
    let l = |v: u8| (v as f32 + (255.0 - v as f32) * f) as u8;
    Color32::from_rgb(l(c.r()), l(c.g()), l(c.b()))
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn text_size(ui: &Ui, text: &str, f: FontId) -> Vec2 {
    ui.painter().layout_no_wrap(text.to_string(), f, fg()).size()
}

/// A chunky button with a darker bottom edge.
pub fn button3d(ui: &mut Ui, text: &str, size: Vec2, fill: Color32, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let depth = 3.0;
    let pressed = enabled && resp.is_pointer_button_down_on();
    let (face_c, edge_c, text_c) = if enabled {
        let f = if resp.hovered() { lighten(fill, 0.08) } else { fill };
        (f, darken(fill, 0.78), WHITE)
    } else {
        (locked(), darken(locked(), 0.85), faint())
    };
    let p = ui.painter();
    let r = CornerRadius::same(((size.y - depth) / 2.0).clamp(12.0, 30.0) as u8);
    p.rect_filled(Rect::from_min_max(rect.min + vec2(0.0, depth), rect.max), r, edge_c);
    let face = if pressed {
        Rect::from_min_max(rect.min + vec2(0.0, depth), rect.max)
    } else {
        Rect::from_min_max(rect.min, rect.max - vec2(0.0, depth))
    };
    p.rect_filled(face, r, face_c);
    p.text(face.center(), Align2::CENTER_CENTER, text, bold(16.0), text_c);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// White button with grey outline.
pub fn outline_button(ui: &mut Ui, text: &str, size: Vec2, text_color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let depth = 3.0;
    let pressed = resp.is_pointer_button_down_on();
    let p = ui.painter();
    let r = CornerRadius::same(((size.y - depth) / 2.0).clamp(12.0, 30.0) as u8);
    p.rect_filled(Rect::from_min_max(rect.min + vec2(0.0, depth), rect.max), r, border());
    let face = if pressed {
        Rect::from_min_max(rect.min + vec2(0.0, depth), rect.max)
    } else {
        Rect::from_min_max(rect.min, rect.max - vec2(0.0, depth))
    };
    p.rect_filled(face, r, if resp.hovered() { hover() } else { surface() });
    p.rect_stroke(face, r, Stroke::new(2.0, border()), StrokeKind::Inside);
    p.text(face.center(), Align2::CENTER_CENTER, text, bold(15.0), text_color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Normal,
    Selected,
    Right,
    Wrong,
    Faded,
    Ghost,
}

fn look_colors(look: Look) -> (Color32, Color32, Color32) {
    // (fill, border, text)
    match look {
        Look::Normal => (surface(), border(), fg()),
        Look::Selected => (primary_soft(), PRIMARY, ink(PRIMARY, 0.8)),
        Look::Right => (green_soft(), GREEN, ink(GREEN, 0.75)),
        Look::Wrong => (red_soft(), RED, ink(RED, 0.8)),
        Look::Faded => (hover(), ghost(), faint()),
        Look::Ghost => (ghost(), ghost(), Color32::TRANSPARENT),
    }
}

fn paint_card_frame(p: &Painter, rect: Rect, look: Look, hovered: bool, radius: u8) {
    let (fill, border, _) = look_colors(look);
    let r = CornerRadius::same(radius);
    if look == Look::Ghost {
        p.rect_filled(rect, r, fill);
        return;
    }
    let edge = Rect::from_min_max(rect.min + vec2(0.0, 3.0), rect.max);
    p.rect_filled(edge, r, border);
    let face = Rect::from_min_max(rect.min, rect.max - vec2(0.0, 3.0));
    let fill = if hovered && look == Look::Normal { hover() } else { fill };
    p.rect_filled(face, r, fill);
    p.rect_stroke(face, r, Stroke::new(2.0, border), StrokeKind::Inside);
}

/// A big answer card used by choice exercises.
pub fn choice_card(ui: &mut Ui, number: usize, card: &Card, size: Vec2, look: Look, interactive: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if interactive { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    paint_card_frame(p, rect, look, resp.hovered() && interactive, 20);
    let (_, border, text_c) = look_colors(look);
    let face = Rect::from_min_max(rect.min, rect.max - vec2(0.0, 3.0));
    // number badge
    let badge = Rect::from_center_size(Pos2::new(face.left() + 26.0, face.center().y), vec2(26.0, 26.0));
    let mut outline = diamond_points(badge.center(), 15.0, 1.4);
    outline.push(outline[0]);
    p.add(egui::Shape::line(outline, Stroke::new(2.0, if look == Look::Normal { self::border() } else { border })));
    p.text(badge.center(), Align2::CENTER_CENTER, number.to_string(), bold(13.0), if look == Look::Normal { muted() } else { text_c });
    let compact = size.x < 220.0;
    let x = face.left() + if compact { 46.0 } else { 52.0 };
    if card.thai {
        if card.sub.is_empty() {
            p.text(Pos2::new(x, face.center().y), Align2::LEFT_CENTER, &card.main, bold(26.0), text_c);
        } else {
            p.text(Pos2::new(x, face.center().y - 9.0), Align2::LEFT_CENTER, &card.main, bold(24.0), text_c);
            paint_pron(p, Pos2::new(x, face.center().y + 17.0), false, &card.sub, &card.tones, 14.0);
        }
    } else if card.sub.is_empty() {
        p.text(Pos2::new(x, face.center().y), Align2::LEFT_CENTER, &card.main, bold(if compact { 15.0 } else { 18.0 }), text_c);
    } else {
        p.text(Pos2::new(x, face.center().y - 10.0), Align2::LEFT_CENTER, &card.main, bold(18.0), text_c);
        p.text(Pos2::new(x, face.center().y + 14.0), Align2::LEFT_CENTER, &card.sub, font(13.0), muted());
    }
    if interactive {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn tile_size(ui: &Ui, card: &Card) -> Vec2 {
    let main = text_size(ui, &card.main, if card.thai { bold(22.0) } else { bold(17.0) });
    let sub = if card.sub.is_empty() { Vec2::ZERO } else { text_size(ui, &pron_line(&card.sub, &card.tones), font(12.0)) };
    let w = main.x.max(sub.x) + 30.0;
    let h = if card.thai { 66.0 } else { 50.0 };
    vec2(w.max(48.0), h)
}

fn pron_line(pron: &str, tones: &str) -> String {
    if tones.is_empty() {
        pron.to_string()
    } else {
        format!("{pron} {tones}")
    }
}

/// Hangul pronunciation in grey followed by the tone arrows in orange.
/// `centered` centres the whole line on `pos`, otherwise it starts at `pos` (vertically centred).
pub fn paint_pron(p: &Painter, pos: Pos2, centered: bool, pron: &str, tones: &str, size: f32) {
    let g1 = p.layout_no_wrap(pron.to_string(), font(size), muted());
    let g2 = p.layout_no_wrap(tones.to_string(), bold(size), TONE);
    let gap = if tones.is_empty() { 0.0 } else { size * 0.35 };
    let total = g1.size().x + gap + g2.size().x;
    let x0 = if centered { pos.x - total / 2.0 } else { pos.x };
    let h = g1.size().y.max(g2.size().y);
    let y0 = pos.y - h / 2.0;
    let w1 = g1.size().x;
    p.galley(Pos2::new(x0, y0 + (h - g1.size().y) / 2.0), g1, muted());
    p.galley(Pos2::new(x0 + w1 + gap, y0 + (h - g2.size().y) / 2.0), g2, TONE);
}

/// A word tile for sentence building.
pub fn tile(ui: &mut Ui, card: &Card, look: Look) -> Response {
    let size = tile_size(ui, card);
    let interactive = matches!(look, Look::Normal | Look::Selected);
    let (rect, resp) = ui.allocate_exact_size(size, if interactive { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    paint_card_frame(p, rect, look, resp.hovered() && interactive, 16);
    if look != Look::Ghost {
        let (_, _, text_c) = look_colors(look);
        let face = Rect::from_min_max(rect.min, rect.max - vec2(0.0, 3.0));
        if card.thai && !card.sub.is_empty() {
            p.text(Pos2::new(face.center().x, face.center().y + 4.0), Align2::CENTER_BOTTOM, &card.main, bold(22.0), text_c);
            paint_pron(p, Pos2::new(face.center().x, face.center().y + 14.0), true, &card.sub, &card.tones, 12.0);
        } else {
            p.text(face.center(), Align2::CENTER_CENTER, &card.main, if card.thai { bold(22.0) } else { bold(17.0) }, text_c);
        }
    }
    if interactive {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn progress_bar(p: &Painter, rect: Rect, frac: f32, color: Color32) {
    let r = CornerRadius::same((rect.height() / 2.0) as u8);
    p.rect_filled(rect, r, track());
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.0 {
        let w = (rect.width() * frac).max(rect.height());
        let fill = Rect::from_min_size(rect.min, vec2(w, rect.height()));
        p.rect_filled(fill, r, color);
        // a small diamond at the end of the bar instead of a shine strip
        if rect.height() >= 10.0 {
            paint_diamond(p, Pos2::new(fill.right() - rect.height() * 0.5, rect.center().y), rect.height() * 0.3, Color32::from_white_alpha(170));
        }
    }
}

pub fn paint_flame(p: &Painter, c: Pos2, s: f32, color: Color32) {
    p.circle_filled(c + vec2(0.0, s * 0.15), s * 0.32, color);
    p.add(Shape::convex_polygon(
        vec![c + vec2(-s * 0.31, s * 0.08), c + vec2(0.04 * s, -s * 0.5), c + vec2(s * 0.31, s * 0.08)],
        color,
        Stroke::NONE,
    ));
    let inner = if color == locked() { darken(locked(), 0.9) } else { GOLD };
    p.circle_filled(c + vec2(0.0, s * 0.24), s * 0.16, inner);
    p.add(Shape::convex_polygon(
        vec![c + vec2(-s * 0.15, s * 0.2), c + vec2(0.0, -s * 0.08), c + vec2(s * 0.15, s * 0.2)],
        inner,
        Stroke::NONE,
    ));
}

pub fn paint_bolt(p: &Painter, c: Pos2, s: f32, color: Color32) {
    p.add(Shape::convex_polygon(
        vec![c + vec2(0.1 * s, -0.5 * s), c + vec2(-0.3 * s, 0.08 * s), c + vec2(0.02 * s, 0.08 * s)],
        color,
        Stroke::NONE,
    ));
    p.add(Shape::convex_polygon(
        vec![c + vec2(-0.02 * s, -0.08 * s), c + vec2(0.3 * s, -0.08 * s), c + vec2(-0.1 * s, 0.5 * s)],
        color,
        Stroke::NONE,
    ));
}

pub fn paint_star(p: &Painter, c: Pos2, r: f32, color: Color32) {
    let inner = r * 0.45;
    let pt = |i: usize, rad: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        c + vec2(a.cos() * rad, a.sin() * rad)
    };
    let pent: Vec<Pos2> = (0..5).map(|k| pt(2 * k + 1, inner)).collect();
    p.add(Shape::convex_polygon(pent, color, Stroke::NONE));
    for k in 0..5 {
        let tip = pt(2 * k, r);
        let a = pt((2 * k + 9) % 10, inner);
        let b = pt(2 * k + 1, inner);
        p.add(Shape::convex_polygon(vec![a, tip, b], color, Stroke::NONE));
    }
}

pub fn paint_check(p: &Painter, c: Pos2, s: f32, color: Color32, width: f32) {
    let pts = vec![c + vec2(-0.42 * s, 0.0), c + vec2(-0.12 * s, 0.3 * s), c + vec2(0.45 * s, -0.32 * s)];
    p.add(Shape::line(pts.clone(), Stroke::new(width, color)));
    for q in pts {
        p.circle_filled(q, width / 2.0, color);
    }
}

pub fn paint_cross(p: &Painter, c: Pos2, s: f32, color: Color32, width: f32) {
    let a = s * 0.35;
    p.line_segment([c + vec2(-a, -a), c + vec2(a, a)], Stroke::new(width, color));
    p.line_segment([c + vec2(a, -a), c + vec2(-a, a)], Stroke::new(width, color));
    for q in [c + vec2(-a, -a), c + vec2(a, a), c + vec2(a, -a), c + vec2(-a, a)] {
        p.circle_filled(q, width / 2.0, color);
    }
}

pub fn paint_lock(p: &Painter, c: Pos2, s: f32, color: Color32) {
    p.circle_stroke(c + vec2(0.0, -s * 0.12), s * 0.22, Stroke::new(s * 0.1, color));
    let body = Rect::from_center_size(c + vec2(0.0, s * 0.16), vec2(s * 0.62, s * 0.46));
    p.rect_filled(body, CornerRadius::same((s * 0.08) as u8), color);
}

pub fn paint_trophy(p: &Painter, c: Pos2, s: f32, color: Color32) {
    // cup
    p.circle_filled(c + vec2(0.0, -s * 0.08), s * 0.3, color);
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, -s * 0.28), vec2(s * 0.6, s * 0.3)), CornerRadius::ZERO, color);
    // handles
    p.circle_stroke(c + vec2(-s * 0.32, -s * 0.2), s * 0.12, Stroke::new(s * 0.07, color));
    p.circle_stroke(c + vec2(s * 0.32, -s * 0.2), s * 0.12, Stroke::new(s * 0.07, color));
    // stem and base
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, s * 0.27), vec2(s * 0.12, s * 0.2)), CornerRadius::ZERO, color);
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, s * 0.4), vec2(s * 0.42, s * 0.1)), CornerRadius::same(2), color);
}

pub fn paint_speaker(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let body = Rect::from_center_size(c + vec2(-s * 0.28, 0.0), vec2(s * 0.2, s * 0.3));
    p.rect_filled(body, CornerRadius::same(2), color);
    p.add(Shape::convex_polygon(
        vec![c + vec2(-s * 0.2, -s * 0.15), c + vec2(s * 0.05, -s * 0.38), c + vec2(s * 0.05, s * 0.38), c + vec2(-s * 0.2, s * 0.15)],
        color,
        Stroke::NONE,
    ));
    for (i, rad) in [s * 0.22, s * 0.4].iter().enumerate() {
        let pts: Vec<Pos2> = (0..=10)
            .map(|k| {
                let a = -0.8 + 1.6 * k as f32 / 10.0;
                c + vec2(s * 0.08 + a.cos() * rad, a.sin() * rad)
            })
            .collect();
        p.add(Shape::Path(PathShape::line(pts, Stroke::new(s * (0.08 - i as f32 * 0.01), color))));
    }
}

/// Square speaker button. `big` makes the large blue version.
pub fn speaker_button(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size + 4.0), Sense::click());
    let p = ui.painter();
    let r = CornerRadius::same((size * 0.22) as u8);
    p.rect_filled(Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max), r, darken(color, 0.78));
    let pressed = resp.is_pointer_button_down_on();
    let face = if pressed {
        Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max)
    } else {
        Rect::from_min_max(rect.min, rect.max - vec2(0.0, 4.0))
    };
    p.rect_filled(face, r, if resp.hovered() { lighten(color, 0.1) } else { color });
    paint_speaker(p, face.center(), size * 0.55, WHITE);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Small flat speaker icon button (for lists).
pub fn small_speaker(ui: &mut Ui, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(10), lighten(color, 0.85));
    }
    paint_speaker(p, rect.center(), 22.0, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// The app's mascot: a little round elephant.
pub fn paint_mascot(p: &Painter, c: Pos2, s: f32, happy: bool) {
    let skin = Color32::from_rgb(160, 182, 214);
    let skin_dark = Color32::from_rgb(132, 154, 190);
    let ear_in = Color32::from_rgb(244, 180, 196);
    // shadow
    p.add(Shape::ellipse_filled(c + vec2(0.0, s * 1.08), vec2(s * 0.9, s * 0.14), Color32::from_black_alpha(22)));
    // ears
    for dir in [-1.0f32, 1.0] {
        p.circle_filled(c + vec2(dir * s * 0.88, -s * 0.1), s * 0.55, skin_dark);
        p.circle_filled(c + vec2(dir * s * 0.9, -s * 0.08), s * 0.36, ear_in);
    }
    // head
    p.circle_filled(c, s * 0.82, skin);
    // trunk
    let pts: Vec<Pos2> = (0..=14)
        .map(|k| {
            let t = k as f32 / 14.0;
            let x = (t * 3.4).sin() * s * 0.16 + t * t * s * 0.25;
            c + vec2(x, s * 0.12 + t * s * 0.78)
        })
        .collect();
    for (i, q) in pts.iter().enumerate() {
        let w = s * (0.25 - 0.1 * i as f32 / 14.0);
        p.circle_filled(*q, w, skin);
    }
    // eyes
    for dir in [-1.0f32, 1.0] {
        let e = c + vec2(dir * s * 0.32, -s * 0.12);
        if happy {
            let pts: Vec<Pos2> = (0..=8)
                .map(|k| {
                    let a = std::f32::consts::PI + std::f32::consts::PI * k as f32 / 8.0;
                    e + vec2(a.cos() * s * 0.13, a.sin() * s * 0.13 + s * 0.05)
                })
                .collect();
            p.add(Shape::Path(PathShape::line(pts, Stroke::new(s * 0.07, Color32::from_rgb(40, 45, 60)))));
        } else {
            p.circle_filled(e, s * 0.15, WHITE);
            p.circle_filled(e + vec2(0.0, s * 0.02), s * 0.09, Color32::from_rgb(40, 45, 60));
            p.circle_filled(e + vec2(s * 0.03, -s * 0.02), s * 0.03, WHITE);
        }
        // cheeks
        p.circle_filled(c + vec2(dir * s * 0.5, s * 0.18), s * 0.1, Color32::from_rgba_unmultiplied(255, 140, 160, 110));
    }
    // little head tuft
    p.circle_filled(c + vec2(-s * 0.08, -s * 0.84), s * 0.08, skin_dark);
    p.circle_filled(c + vec2(s * 0.06, -s * 0.88), s * 0.07, skin_dark);
}

/// The Thai flag as a small rounded rectangle.
pub fn paint_flag(p: &Painter, rect: Rect) {
    let h = rect.height() / 6.0;
    let stripes = [
        (0.0, 1.0, Color32::from_rgb(237, 28, 36)),
        (1.0, 1.0, WHITE),
        (2.0, 2.0, Color32::from_rgb(36, 29, 79)),
        (4.0, 1.0, WHITE),
        (5.0, 1.0, Color32::from_rgb(237, 28, 36)),
    ];
    for (y, n, c) in stripes {
        p.rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, y * h), vec2(rect.width(), n * h)),
            CornerRadius::ZERO,
            c,
        );
    }
    p.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.5, border()), StrokeKind::Outside);
}

/// A white rounded card with a border; returns the inner response.
pub fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(surface())
        .stroke(Stroke::new(2.0, border()))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(egui::Margin::same(18))
}

/// A small notebook icon (the mistake notebook).
pub fn paint_note(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let page = Rect::from_center_size(c, vec2(s * 0.72, s * 0.88));
    p.rect_filled(page, CornerRadius::same((s * 0.12) as u8), color);
    for k in 0..3 {
        let y = page.top() + s * (0.26 + 0.18 * k as f32);
        p.line_segment([Pos2::new(page.left() + s * 0.17, y), Pos2::new(page.right() - s * 0.12, y)], Stroke::new((s * 0.07).max(1.5), WHITE));
    }
    p.line_segment([Pos2::new(page.left() + s * 0.08, page.top() + s * 0.1), Pos2::new(page.left() + s * 0.08, page.bottom() - s * 0.1)], Stroke::new((s * 0.06).max(1.2), WHITE));
}

/// Outline points of a soft diamond (a rounded rhombus), the shape of the lesson path nodes and
/// a nod to Thai ornament. `p` near 1 is a sharp diamond, 2 is a circle; 1.45 is in between.
pub fn diamond_points(c: Pos2, r: f32, p: f32) -> Vec<Pos2> {
    let n = 40;
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            let (s, co) = a.sin_cos();
            let x = co.signum() * co.abs().powf(2.0 / p);
            let y = s.signum() * s.abs().powf(2.0 / p);
            c + vec2(x, y) * r
        })
        .collect()
}

pub fn paint_diamond(pt: &Painter, c: Pos2, r: f32, fill: Color32) {
    pt.add(egui::Shape::convex_polygon(diamond_points(c, r, 1.4), fill, Stroke::NONE));
}

/// A row of small diamonds, used as a quiet Thai-style border on the unit banners.
pub fn paint_diamond_row(pt: &Painter, left_center: Pos2, count: usize, step: f32, r: f32, color: Color32) {
    for i in 0..count {
        paint_diamond(pt, left_center + vec2(step * i as f32, 0.0), r, color);
    }
}

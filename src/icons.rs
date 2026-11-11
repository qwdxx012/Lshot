use crate::gdiplus::Gfx;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Pen,
    Line,
    Arrow,
    Rectangle,
    Highlighter,
    Text,
    Undo,
    Copy,
    Save,
    Close,
}

pub const ICON_BASE: f32 = 14.0;

pub fn draw(g: &Gfx, icon: Icon, center_x: f32, center_y: f32, size: f32, color: u32) {
    let scale = size / ICON_BASE;
    let left = center_x - size / 2.0;
    let top = center_y - size / 2.0;
    let p = |x: f32, y: f32| (left + x * scale, top + y * scale);
    let w = |value: f32| (value * scale).max(0.6);

    match icon {
        Icon::Pen => {
            let outline = [
                p(2.8, 11.2),
                p(4.0, 8.4),
                p(10.0, 2.4),
                p(11.6, 4.0),
                p(5.6, 10.0),
            ];
            g.stroke_polygon(color, w(1.25), &outline);
            let tip = [p(2.8, 11.2), p(4.0, 8.4), p(5.6, 10.0)];
            g.fill_polygon(color, &tip);
            g.line(color, w(1.0), p(9.2, 3.2).0, p(9.2, 3.2).1, p(10.8, 4.8).0, p(10.8, 4.8).1);
        }
        Icon::Line => {
            let a = p(2.6, 11.4);
            let b = p(11.4, 2.6);
            g.line(color, w(1.7), a.0, a.1, b.0, b.1);
        }
        Icon::Arrow => {
            let a = p(2.6, 11.4);
            let b = p(10.4, 3.6);
            g.line(color, w(1.5), a.0, a.1, b.0, b.1);
            let head = [p(6.6, 3.2), p(11.0, 3.0), p(10.8, 7.4)];
            g.stroke_polyline(color, w(1.5), &head, false);
        }
        Icon::Rectangle => {
            let a = p(2.5, 3.3);
            let b = p(11.5, 10.7);
            g.stroke_round_rect(
                color,
                w(1.5),
                a.0.round() as i32,
                a.1.round() as i32,
                (b.0 - a.0).round() as i32,
                (b.1 - a.1).round() as i32,
                0,
            );
        }
        Icon::Highlighter => {
            let outline = [p(5.4, 6.9), p(9.4, 3.0), p(11.9, 5.5), p(8.2, 9.7)];
            g.stroke_polygon(color, w(1.2), &outline);
            let head = [p(9.4, 3.0), p(11.9, 5.5), p(10.9, 6.5), p(8.4, 4.0)];
            g.fill_polygon(color, &head);
            let body = [p(2.6, 11.4), p(5.4, 6.9), p(8.2, 9.7), p(4.9, 11.6)];
            g.fill_polygon(color, &body);
        }
        Icon::Text => {
            let a = p(2.8, 3.4);
            let b = p(11.2, 3.4);
            g.line(color, w(1.5), a.0, a.1, b.0, b.1);
            let c = p(2.8, 4.8);
            g.line(color, w(1.5), a.0, a.1, c.0, c.1);
            let d = p(11.2, 4.8);
            g.line(color, w(1.5), b.0, b.1, d.0, d.1);
            let e = p(7.0, 3.4);
            let f = p(7.0, 11.4);
            g.line(color, w(1.5), e.0, e.1, f.0, f.1);
        }
        Icon::Undo => {
            let center = p(7.0, 7.0);
            let radius = 4.2 * scale;
            g.arc(
                color,
                w(1.4),
                center.0 - radius,
                center.1 - radius,
                radius * 2.0,
                radius * 2.0,
                -25.0,
                285.0,
            );
            let tail = [p(3.2, 2.2), p(3.2, 5.6), p(6.6, 5.6)];
            g.stroke_polyline(color, w(1.4), &tail, false);
        }
        Icon::Copy => {
            let points = [p(2.8, 7.8), p(2.8, 2.8), p(7.8, 2.8), p(7.8, 4.6)];
            g.stroke_polyline(color, w(1.2), &points, false);
            let a = p(5.2, 5.2);
            let b = p(11.2, 11.2);
            g.stroke_round_rect(
                color,
                w(1.2),
                a.0.round() as i32,
                a.1.round() as i32,
                (b.0 - a.0).round() as i32,
                (b.1 - a.1).round() as i32,
                0,
            );
        }
        Icon::Save => {
            let body = [
                p(2.7, 2.7),
                p(9.7, 2.7),
                p(11.3, 4.3),
                p(11.3, 11.3),
                p(2.7, 11.3),
            ];
            g.stroke_polygon(color, w(1.2), &body);
            let flap = [p(4.7, 2.7), p(4.7, 5.9), p(8.7, 5.9), p(8.7, 2.7)];
            g.stroke_polyline(color, w(1.2), &flap, false);
            let a = p(4.7, 8.9);
            let b = p(9.3, 11.3);
            g.stroke_round_rect(
                color,
                w(1.2),
                a.0.round() as i32,
                a.1.round() as i32,
                (b.0 - a.0).round() as i32,
                (b.1 - a.1).round() as i32,
                0,
            );
        }
        Icon::Close => {
            let a = p(3.2, 3.2);
            let b = p(10.8, 10.8);
            g.line(color, w(1.6), a.0, a.1, b.0, b.1);
            let c = p(10.8, 3.2);
            let d = p(3.2, 10.8);
            g.line(color, w(1.6), c.0, c.1, d.0, d.1);
        }
    }
}

pub fn draw_in_box(g: &Gfx, icon: Icon, x: i32, y: i32, box_size: i32, color: u32) -> i32 {
    let icon_size = box_size as f32 * (14.0 / 24.0);
    draw(
        g,
        icon,
        x as f32 + box_size as f32 / 2.0,
        y as f32 + box_size as f32 / 2.0,
        icon_size,
        color,
    );
    icon_size.round() as i32
}

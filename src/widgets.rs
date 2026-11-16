use egui::{Color32, FontId, Response, Sense, Shape, Stroke, Ui, Vec2, Widget};
use std::ops::RangeInclusive;

pub struct Knob<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    reset: f32,
    label: &'a str,
    unit: &'a str,
    color: Color32,
    size: f32,
}
impl<'a> Knob<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        let reset = 0f32.clamp(*range.start(), *range.end());
        Self {
            value,
            range,
            reset,
            label: "",
            unit: "",
            color: Color32::from_rgb(53, 195, 255),
            size: 48.,
        }
    }
    pub fn default(mut self, value: f32) -> Self {
        self.reset = value;
        self
    }
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }
    pub fn unit(mut self, unit: &'a str) -> Self {
        self.unit = unit;
        self
    }
    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}
impl Widget for Knob<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (min, max) = (*self.range.start(), *self.range.end());
        let width = self.size + 12.;
        let height = self.size + 28.;
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
        let before = *self.value;
        if response.clicked() {
            response.request_focus();
        }
        let fine = ui.input(|i| if i.modifiers.shift { 0.1 } else { 1. });
        if response.dragged() {
            *self.value -= ui.input(|i| i.pointer.delta().y) * (max - min) / 150. * fine;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0. {
                *self.value += scroll.signum() * (max - min) * 0.015 * fine;
                ui.input_mut(|i| {
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
            }
        }
        if response.has_focus() {
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight) {
                    *self.value += (max - min) * 0.01 * fine;
                }
                if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft) {
                    *self.value -= (max - min) * 0.01 * fine;
                }
                if i.key_pressed(egui::Key::Home) {
                    *self.value = min;
                }
                if i.key_pressed(egui::Key::End) {
                    *self.value = max;
                }
            });
        }
        if response.double_clicked() {
            *self.value = self.reset;
        }
        *self.value = self.value.clamp(min, max);
        if before != *self.value {
            response.mark_changed();
        }
        let center = egui::pos2(rect.center().x, rect.top() + self.size * 0.5 + 5.);
        let radius = self.size * 0.39;
        let p = ui.painter();
        p.circle_filled(
            center,
            radius,
            if ui.visuals().dark_mode {
                Color32::from_gray(28)
            } else {
                Color32::from_gray(224)
            },
        );
        p.circle_stroke(center, radius, Stroke::new(1_f32, Color32::from_gray(86)));
        let start = std::f32::consts::PI * 0.75;
        let sweep = std::f32::consts::PI * 1.5;
        let t = (*self.value - min) / (max - min).max(0.0001);
        let arc = |end: f32| {
            (0..=48)
                .map(|i| center + Vec2::angled(start + end * i as f32 / 48.) * (radius + 4.))
                .collect::<Vec<_>>()
        };
        p.add(Shape::line(
            arc(sweep),
            Stroke::new(2_f32, Color32::from_gray(57)),
        ));
        if t > 0. {
            p.add(Shape::line(arc(sweep * t), Stroke::new(2_f32, self.color)));
        }
        let needle = Vec2::angled(start + sweep * t);
        p.line_segment(
            [
                center + needle * (radius * 0.45),
                center + needle * (radius * 0.86),
            ],
            Stroke::new(2_f32, ui.visuals().text_color()),
        );
        if response.hovered() || response.has_focus() {
            p.circle_stroke(
                center,
                radius + 1.,
                Stroke::new(1_f32, self.color.gamma_multiply(0.7)),
            );
        }
        let value = if self.unit == "%" {
            format!("{:.0}%", *self.value * 100.)
        } else if self.unit == "dB" {
            format!("{:+.0}", *self.value)
        } else if self.unit == "Hz" {
            format!("{:.2}", *self.value)
        } else {
            format!("{:.0}{}", *self.value, self.unit)
        };
        p.text(
            egui::pos2(center.x, rect.top() + self.size + 10.),
            egui::Align2::CENTER_CENTER,
            value,
            FontId::monospace(10.),
            self.color,
        );
        p.text(
            egui::pos2(center.x, rect.bottom() - 2.),
            egui::Align2::CENTER_BOTTOM,
            self.label,
            FontId::proportional(9.),
            ui.visuals().text_color(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::slider(ui.is_enabled(), *self.value as f64, self.label)
        });
        response.on_hover_text(
            "Przeciągnij góra/dół · kółko · strzałki · SHIFT: dokładnie · dwuklik: reset",
        )
    }
}

pub fn meter(ui: &mut Ui, level: f32, color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(11., height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 1., Color32::from_gray(8));
    for i in 0..18 {
        let on = level > i as f32 / 18.;
        let y = rect.bottom() - 3. - i as f32 * (height - 5.) / 18.;
        let c = if i > 15 {
            Color32::from_rgb(246, 72, 95)
        } else if i > 12 {
            Color32::from_rgb(242, 194, 66)
        } else {
            color
        };
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + 2., y - 2.),
                Vec2::new(7., ((height - 5.) / 18. - 1.).max(1.)),
            ),
            0.,
            if on { c } else { c.gamma_multiply(0.12) },
        );
    }
}

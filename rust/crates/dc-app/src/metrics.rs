//! Label text measured with the real font (`QFontMetrics(_label_font())`):
//! Segoe UI at the label's point size, DemiBold for the full label.

use dc_creature::LabelMetrics;
use dc_render::{Font, FontMetrics, Weight};

pub struct FontLabelMetrics;

fn metrics(point_size: f64, demibold: bool) -> FontMetrics {
    let weight = if demibold { Weight::DEMI_BOLD } else { Weight::NORMAL };
    FontMetrics::new(&Font::with_weight("Segoe UI", point_size, weight))
}

impl LabelMetrics for FontLabelMetrics {
    fn horizontal_advance(&self, text: &str, point_size: f64, demibold: bool) -> f64 {
        metrics(point_size, demibold).horizontal_advance(text) as f64
    }

    fn height(&self, point_size: f64, demibold: bool) -> f64 {
        metrics(point_size, demibold).height() as f64
    }
}

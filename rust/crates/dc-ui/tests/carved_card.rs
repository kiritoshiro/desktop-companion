//! A carved card is as wide as it is asked to be, whatever the item spacing
//! of the row it sits in. The mode menu's three cards overflowed its 960 px
//! window when the row's 18 px spacing was added on both sides of the padding
//! (seen live, 2026-10-21).

use egui::{Align, Context, Layout, RawInput, Rect, pos2, vec2};

#[test]
fn a_card_keeps_its_width_in_a_spaced_row() {
    let ctx = Context::default();
    let mut widths = Vec::new();
    let input =
        RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(960.0, 860.0))), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| {
        ui.allocate_ui_with_layout(vec2(888.0, 0.0), Layout::left_to_right(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            for _ in 0..3 {
                let r = dc_ui::widgets::carved_card(ui, 284.0, 30.0, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("a");
                        ui.label("b");
                    });
                });
                widths.push(r.response.rect.width());
            }
        });
    });
    output.textures_delta.clear();
    assert_eq!(widths.len(), 3);
    for w in widths {
        assert!((w - 284.0).abs() < 0.5, "card drawn {w} px wide, asked for 284");
    }
}

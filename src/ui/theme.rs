use std::sync::Arc;
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontTweak, Stroke, TextStyle,
    epaint::text::HintingTarget,
};

/// nvencFFX palette: the app's own colors from `nvencFFX.py` plus the CustomTkinter
/// dark-theme defaults the original inherits (labels, borders, scrollbar).
pub mod colors {
    use super::Color32;

    // Backgrounds (nvencFFX.py)
    pub const PRIMARY_BG: Color32 = Color32::from_rgb(0x0e, 0x11, 0x13);
    pub const SECONDARY_BG: Color32 = Color32::from_rgb(0x17, 0x1b, 0x1f);

    // Accents (nvencFFX.py)
    pub const ACCENT_GREEN: Color32 = Color32::from_rgb(0x4f, 0xb6, 0x2f);
    pub const HOVER_GREEN: Color32 = Color32::from_rgb(0x47, 0xa3, 0x2a);
    pub const ACCENT_GREY: Color32 = Color32::from_rgb(0xb2, 0xb2, 0xb2);
    pub const HOVER_GREY: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x8e);
    pub const ACCENT_RED: Color32 = Color32::from_rgb(0xff, 0x55, 0x55);
    pub const HOVER_RED: Color32 = Color32::from_rgb(0xd8, 0x36, 0x36);
    pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(0xf0, 0xa0, 0x20);
    pub const HOVER_ORANGE: Color32 = Color32::from_rgb(0xd3, 0x8c, 0x1a);

    // Text (nvencFFX.py)
    pub const TEXT_COLOR_W: Color32 = Color32::WHITE;
    pub const TEXT_COLOR_B: Color32 = Color32::BLACK;
    pub const PLACEHOLDER: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);

    // CustomTkinter dark-theme defaults
    pub const LABEL_TEXT: Color32 = Color32::from_rgb(0xdc, 0xe4, 0xee);
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x99, 0x99, 0x99); // gray60
    pub const ENTRY_BORDER: Color32 = Color32::from_rgb(0x56, 0x5b, 0x5e);
    pub const CONTROL_BORDER: Color32 = Color32::from_rgb(0x94, 0x9a, 0x9f); // checkbox / radio
    pub const CHECKMARK: Color32 = Color32::from_rgb(0xe5, 0xe5, 0xe5); // gray90
    pub const SCROLLBAR: Color32 = Color32::from_rgb(0x69, 0x69, 0x69); // gray41
    pub const SCROLLBAR_HOVER: Color32 = Color32::from_rgb(0x87, 0x87, 0x87); // gray53
    pub const TRIM_TRACK: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
    /// Outline of self-drawn windows, matching the 1px border Windows 10 gives the main window
    pub const WINDOW_BORDER: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
}

/// Sizes of the CustomTkinter widgets the original layout is built from.
pub mod dimensions {
    pub const CORNER_RADIUS: u8 = 6;
    pub const WIDGET_HEIGHT: f32 = 28.0; // CTkButton / CTkEntry / CTkOptionMenu
    pub const BUTTON_WIDTH: f32 = 140.0; // CTkButton / CTkOptionMenu default width
    pub const BIG_BUTTON_HEIGHT: f32 = 40.0; // bottom bar
    pub const ENTRY_BORDER: f32 = 2.0;
    pub const CHECKBOX_SIZE: f32 = 24.0;
    pub const CHECKBOX_BORDER: f32 = 3.0;
    pub const RADIO_SIZE: f32 = 22.0;
    pub const RADIO_BORDER_UNCHECKED: f32 = 3.0;
    pub const RADIO_BORDER_CHECKED: f32 = 6.0;
    pub const BADGE_SIZE: f32 = 24.0;
    pub const CONTROL_TEXT_GAP: f32 = 6.0; // box/circle to label
    pub const FONT_SIZE: f32 = 13.0; // CTkFont default on Windows
}

/// Installs Roboto (the font CustomTkinter ships and uses on Windows) as the
/// primary proportional font. egui's bundled fonts stay as glyph fallbacks.
fn install_fonts(ctx: &egui::Context) {
    // egui's default (smooth) hinting leaves stems soft at 13-14px, and its options change
    // nothing on Consolas, whose own instructions skip horizontal fitting in smooth mode.
    // Mono hinting snaps the stems to whole pixels, the closest to Tk's GDI text.
    let crisp = || FontTweak { hinting_target: HintingTarget::Mono, ..Default::default() };

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "roboto".to_owned(),
        Arc::new(
            FontData::from_static(include_bytes!("../../assets/fonts/Roboto-Regular.ttf"))
                .tweak(crisp()),
        ),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "roboto".to_owned());

    // `font=("Consolas", 14)` of the input tooltip, command preview and help windows.
    // Read from the system (it cannot be redistributed); egui's Hack stays the fallback.
    if let Some(consolas) = system_font("consola.ttf") {
        fonts.font_data.insert("consolas".to_owned(), Arc::new(FontData::from_owned(consolas).tweak(crisp())));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "consolas".to_owned());
    }

    ctx.set_fonts(fonts);
}

/// A font file from `%WINDIR%\Fonts`.
fn system_font(file: &str) -> Option<Vec<u8>> {
    let windir = std::env::var_os("WINDIR")?;
    std::fs::read(std::path::Path::new(&windir).join("Fonts").join(file)).ok()
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(colors::LABEL_TEXT);
    visuals.panel_fill = colors::PRIMARY_BG;
    visuals.window_fill = colors::SECONDARY_BG;
    visuals.window_stroke = Stroke::new(1.0_f32, colors::ENTRY_BORDER);
    visuals.window_corner_radius = CornerRadius::same(dimensions::CORNER_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(dimensions::CORNER_RADIUS);
    // Hinting snaps stems to the pixel grid; subpixel binning trades that crispness for
    // smoother kerning (egui documents it as "more blurry"), so keep it off for 13px UI text.
    visuals.text_options.font_hinting = true;
    visuals.text_options.subpixel_binning = false;
    // The dark-mode default (2c - c^2) thickens glyphs; Tk/GDI text is thinner, and the
    // black labels on the green/grey buttons turned bold with it. Use raw coverage.
    visuals.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Off;
    // Also the scrollbar track: keep it invisible like CTkScrollableFrame's
    visuals.extreme_bg_color = colors::PRIMARY_BG;
    visuals.faint_bg_color = colors::SECONDARY_BG;
    visuals.code_bg_color = colors::SECONDARY_BG;

    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0x4f, 0xb6, 0x2f, 110);
    visuals.selection.stroke = Stroke::new(1.0_f32, colors::TEXT_COLOR_W);
    visuals.text_cursor.stroke = Stroke::new(1.5_f32, colors::LABEL_TEXT);

    // egui's built-in widgets are only used for the scrollbar handle; the CTk look
    // of every other control is drawn by `widgets.rs`.
    let pill = CornerRadius::same(4);
    for (w, fill) in [
        (&mut visuals.widgets.inactive, colors::SCROLLBAR),
        (&mut visuals.widgets.hovered, colors::SCROLLBAR_HOVER),
        (&mut visuals.widgets.active, colors::SCROLLBAR_HOVER),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0_f32, colors::LABEL_TEXT);
        w.corner_radius = pill;
        w.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_fill = colors::SECONDARY_BG;
    visuals.widgets.noninteractive.weak_bg_fill = colors::SECONDARY_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors::ENTRY_BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, colors::LABEL_TEXT);

    visuals
}

/// Installs the nvencFFX look (fonts, colors, spacing) into the egui context.
pub fn apply_theme(ctx: &egui::Context) {
    install_fonts(ctx);

    let visuals = visuals();
    // Set both theme slots so the OS light/dark preference never swaps the look.
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals.clone();

            use egui::FontFamily::{Monospace, Proportional};
            use egui::FontId;
            style.text_styles = [
                (TextStyle::Heading, FontId::new(18.0, Proportional)),
                (TextStyle::Body, FontId::new(dimensions::FONT_SIZE, Proportional)),
                (TextStyle::Button, FontId::new(dimensions::FONT_SIZE, Proportional)),
                (TextStyle::Monospace, FontId::new(12.0, Monospace)),
                (TextStyle::Small, FontId::new(11.0, Proportional)),
            ]
            .into();

            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 4.0);
            style.spacing.menu_margin = egui::Margin::same(3);
            style.spacing.interact_size.y = dimensions::WIDGET_HEIGHT;
            // CTkToolTip(delay=0.3) of the buttons' right-click hints
            style.interaction.tooltip_delay = 0.3;

            // CTkScrollableFrame: 8px grey pill, 4px from the window edge
            let mut scroll = egui::style::ScrollStyle::solid();
            scroll.bar_width = 8.0;
            scroll.bar_inner_margin = 0.0;
            scroll.bar_outer_margin = 4.0;
            scroll.handle_min_length = 24.0;
            style.spacing.scroll = scroll;
        });
    }
    ctx.set_theme(egui::Theme::Dark);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_colors() {
        assert_eq!(colors::PRIMARY_BG, Color32::from_rgb(14, 17, 19));
        assert_eq!(colors::SECONDARY_BG, Color32::from_rgb(23, 27, 31));
        assert_eq!(colors::ACCENT_GREEN, Color32::from_rgb(79, 182, 47));
        assert_eq!(colors::ACCENT_GREY, Color32::from_rgb(178, 178, 178));
        assert_eq!(colors::ACCENT_RED, Color32::from_rgb(255, 85, 85));
    }

    #[test]
    fn test_apply_theme() {
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            ctx.style_mut_of(theme, |style| {
                assert_eq!(style.visuals.panel_fill, colors::PRIMARY_BG);
                assert_eq!(style.visuals.override_text_color, Some(colors::LABEL_TEXT));
            });
        }
    }

    #[cfg(windows)]
    #[test]
    fn consolas_comes_from_the_system() {
        assert!(system_font("consola.ttf").is_some(), "every Windows has Consolas");
        assert!(system_font("nvencffx-none.ttf").is_none());
        // egui parses it on the first frame
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        let mut out = ctx.run_ui(Default::default(), |ui| {
            ui.label(egui::RichText::new("Stream #0:0 — тест").font(egui::FontId::monospace(14.0)));
        });
        out.textures_delta.clear();
        let families = ctx.fonts(|f| f.definitions().families[&FontFamily::Monospace].clone());
        assert_eq!(families[0], "consolas");
    }

    #[test]
    fn our_fonts_use_mono_hinting() {
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        let mut out = ctx.run_ui(Default::default(), |_| {});
        out.textures_delta.clear();
        ctx.fonts(|f| {
            let data = &f.definitions().font_data;
            for name in ["roboto", "consolas"] {
                let Some(font) = data.get(name) else { continue }; // Consolas is optional
                assert_eq!(font.tweak.hinting_target, HintingTarget::Mono, "{name}");
            }
            assert!(data.contains_key("roboto"));
        });
    }
}

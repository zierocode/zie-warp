//! This module defines CellGlyphCache, a struct which manages the caching of glyph values for cells
//! when rendering Grids within Warp.
use warpui::elements::DEFAULT_LINE_HEIGHT_RATIO;

use warpui::fonts::{Cache as FontCache, FamilyId, FontId, GlyphId, Properties};
use warpui::geometry::vector::{vec2f, Vector2F};
use warpui::platform::LineStyle;
use warpui::text_layout::{StyleAndFont, DEFAULT_TOP_BOTTOM_RATIO};
use warpui::PaintContext;

use std::collections::HashMap;

/// A cluster of glyphs from a single cell — one base glyph plus zero or more
/// combining-mark glyphs, each with a shaping-derived position offset.
pub type GlyphCluster = (FontId, Vec<(GlyphId, Vector2F)>);

/// Stores cached glyph values for characters/strings. Note that we normally only need to look up
/// characters - we only look up strings in the case of zerowidth characters (which act as modifiers
/// to the first character e.g. emoji variant selectors). We have 2 separate caches internally for
/// performance reasons (avoid allocating strings when we don't need to!).
#[derive(Default)]
pub struct CellGlyphCache {
    glyph_cache: HashMap<(char, FontId), Option<GlyphCluster>>,
    string_cache: HashMap<(String, FontId), Option<GlyphCluster>>,
}

impl CellGlyphCache {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn glyph_for_char(
        &mut self,
        char: char,
        font_id: FontId,
        font_cache: &FontCache,
        font_family: FamilyId,
        font_size: f32,
        properties: Properties,
        ctx: &mut PaintContext,
    ) -> Option<GlyphCluster> {
        self.glyph_cache
            .entry((char, font_id))
            .or_insert_with(|| {
                if char == '\u{0E33}' {
                    return shape_thai_sara_am(
                        font_id, font_cache, font_family, font_size, properties, ctx,
                    );
                }

                let mut string = String::new();
                string.push(char);
                let line = ctx.text_layout_cache.layout_line(
                    &string,
                    LineStyle {
                        font_size,
                        line_height_ratio: DEFAULT_LINE_HEIGHT_RATIO,
                        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                        fixed_width_tab_size: None,
                    },
                    &[(
                        (0..1),
                        StyleAndFont {
                            font_family,
                            properties,
                            style: Default::default(),
                        },
                    )],
                    f32::MAX,
                    Default::default(),
                    &font_cache.text_layout_system(),
                );
                let run = line.runs.first()?;
                let font_id = run.font_id;
                let glyphs: Vec<(GlyphId, Vector2F)> = run
                    .glyphs
                    .iter()
                    .map(|g| (g.id, g.position_along_baseline))
                    .collect();
                Some((font_id, glyphs))
            })
            .clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn glyph_for_string(
        &mut self,
        string: &str,
        font_id: FontId,
        font_cache: &FontCache,
        font_family: FamilyId,
        font_size: f32,
        properties: Properties,
        ctx: &mut PaintContext,
    ) -> Option<GlyphCluster> {
        let glyph = self
            .string_cache
            .entry((string.to_owned(), font_id))
            .or_insert_with(|| {
                // Calculate the length of total characters in the string.
                let run_length_chars = string.chars().count();
                let line = ctx.text_layout_cache.layout_line(
                    string,
                    LineStyle {
                        font_size,
                        // Note that we DO NOT paint the `Line` in this particular instance. As such,
                        // the line height ratio and baseline ratio are both NOT used. Hence, we arbitrarily
                        // set them to the default values.
                        line_height_ratio: DEFAULT_LINE_HEIGHT_RATIO,
                        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                        fixed_width_tab_size: None,
                    },
                    &[(
                        (0..run_length_chars),
                        StyleAndFont {
                            font_family,
                            properties,
                            style: Default::default(),
                        },
                    )],
                    f32::MAX,
                    Default::default(),
                    &font_cache.text_layout_system(),
                );
                let run = line.runs.first()?;
                let font_id = run.font_id;
                let glyphs: Vec<(GlyphId, Vector2F)> = run
                    .glyphs
                    .iter()
                    .map(|g| (g.id, g.position_along_baseline))
                    .collect();
                Some((font_id, glyphs))
            })
            .clone();

        glyph.or_else(|| {
            #[cfg(debug_assertions)]
            log::warn!("Falling back to glyph for first character of string, could not get glyph for entire string: {string:?}");
            let first_char = string.chars().next()?;
            let fallback = self.glyph_for_char(
                first_char,
                font_id,
                font_cache,
                font_family,
                font_size,
                properties,
                ctx,
            )?;
            self.string_cache
                .insert((string.to_owned(), font_id), Some(fallback.clone()));
            Some(fallback)
        })
    }
}

/// Shape Thai sara am (U+0E33) with a dummy base consonant (ก) so CoreText has
/// enough context to place the nikkhahit diacritic at the correct y position.
/// Returns only the sara am glyphs, with x positions adjusted to be relative
/// to the sara am cell's own origin (i.e., base advance subtracted out).
fn shape_thai_sara_am(
    _font_id: FontId,
    font_cache: &FontCache,
    font_family: FamilyId,
    font_size: f32,
    properties: Properties,
    ctx: &mut PaintContext,
) -> Option<GlyphCluster> {
    const BASE: char = '\u{0E01}'; // ก — dummy Thai consonant
    const SARA_AM: char = '\u{0E33}'; // ำ
    let string = [BASE, SARA_AM].iter().collect::<String>();

    let line = ctx.text_layout_cache.layout_line(
        &string,
        LineStyle {
            font_size,
            line_height_ratio: DEFAULT_LINE_HEIGHT_RATIO,
            baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
            fixed_width_tab_size: None,
        },
        &[(
            (0..2),
            StyleAndFont {
                font_family,
                properties,
                style: Default::default(),
            },
        )],
        f32::MAX,
        Default::default(),
        &font_cache.text_layout_system(),
    );

    let run = line.runs.first()?;
    let result_font_id = run.font_id;

    // Split glyphs by char index: index 0 = ก (base), index 1 = ำ (sara am).
    // Compute the advance of the base consonant as the minimum x position of
    // any sara am glyph (CoreText places sara am glyphs after the base advance).
    let sara_glyphs: Vec<_> = run.glyphs.iter().filter(|g| g.index >= 1).collect();
    let base_advance = run
        .glyphs
        .iter()
        .filter(|g| g.index == 0)
        .map(|g| g.position_along_baseline.x() + g.width)
        .fold(0f32, f32::max);

    if sara_glyphs.is_empty() {
        return None;
    }

    // Subtract base_advance from x so positions are relative to the sara am cell.
    let glyphs: Vec<(GlyphId, Vector2F)> = sara_glyphs
        .iter()
        .map(|g| {
            let pos = vec2f(
                g.position_along_baseline.x() - base_advance,
                g.position_along_baseline.y(),
            );
            (g.id, pos)
        })
        .collect();

    Some((result_font_id, glyphs))
}

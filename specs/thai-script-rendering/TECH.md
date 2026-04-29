# TECH.md — Thai Script Rendering

## Context

Warp's terminal grid renders one glyph per cell. When a cell contains a base character with zero-width combining marks (e.g. Thai `ที่` = `ท` + `-ี` + `-่`), the text shaping pipeline correctly produces multiple glyphs — one for the base, plus positioned glyphs for each mark. But `cell_glyph_cache.rs` discards all but the first glyph, so marks disappear.

Current flow:

1. `app/src/terminal/grid_renderer.rs:render_cell_glyph` (line 1665) extracts cell content as `CharOrStr::Char` or `CharOrStr::Str`.
2. For `Str` (zero-width chars present), it calls `glyphs.glyph_for_string()`.
3. `cell_glyph_cache.rs:glyph_for_string` (line 36) runs the string through `ctx.text_layout_cache.layout_line()` — which uses `cosmic_text` + `rustybuzz` (HarfBuzz-compatible) for OpenType shaping.
4. The shaper returns `LayoutLine` → `Run` → `Vec<LayoutGlyph>`, where each `LayoutGlyph` carries `(glyph_id, x, y, w, font_id)`.
5. `RunBuilder.push_glyph` (text_layout.rs:60) converts each `LayoutGlyph` to a `Glyph { id, position_along_baseline: vec2f(x, y), index, width }`.
6. **Bug:** line 76–78 of `cell_glyph_cache.rs` checks `if run.glyphs.len() > 1 { return None; }` — rejects multi-glyph clusters.
7. Fallback at line 83 grabs just `first_char`, losing all combining marks.

### Key files

| File | Role |
|------|------|
| `app/src/terminal/grid_renderer/cell_glyph_cache.rs` | Caches glyph lookups; contains the bug |
| `app/src/terminal/grid_renderer.rs:1665-1762` | `render_cell_glyph` — draws glyphs to scene |
| `crates/warpui_core/src/text_layout.rs:670-686` | `Run`, `Glyph` type definitions |
| `crates/warpui/src/windowing/winit/fonts/text_layout.rs:60-92` | `RunBuilder::push_glyph` — populates `Glyph.position_along_baseline` from `LayoutGlyph.(x,y)` |
| `crates/warpui_core/src/scene.rs` | `Scene::draw_glyph` — the low-level draw call |

### Types involved

```rust
// text_layout.rs — already exists
pub struct Glyph {
    pub id: GlyphId,
    pub position_along_baseline: Vector2F,  // offset from baseline origin
    pub index: usize,
    pub width: f32,
}

pub struct Run {
    pub font_id: FontId,
    pub glyphs: Vec<Glyph>,
    pub styles: TextStyle,
    pub width: f32,
}
```

`position_along_baseline` is the critical field. For combining marks, the shaper sets `y` to a negative value (above baseline) and `x` close to 0 (same horizontal position as the base). Warp currently ignores this per-glyph offset.

## Proposed changes

### 1. `cell_glyph_cache.rs` — return all glyphs in a cluster

Change `glyph_for_string` return type and logic:

```rust
// Before:
fn glyph_for_string(…) -> Option<(GlyphId, FontId)>

// After:
fn glyph_for_string(…) -> Option<(FontId, Vec<(GlyphId, Vector2F)>)>
```

Inside `glyph_for_string`:
- Remove the `if run.glyphs.len() > 1 { return None; }` guard.
- Collect all glyphs from `run.glyphs`: `(glyph.id, glyph.position_along_baseline)`.
- Return the shared `font_id` (from `run.font_id`) plus the glyph list.
- Update both `string_cache` and `glyph_cache` types to match.

Cache type change:
```rust
// Before:
string_cache: HashMap<(String, FontId), Option<(GlyphId, FontId)>>,

// After:
string_cache: HashMap<(String, FontId), Option<(FontId, Vec<(GlyphId, Vector2F)>)>>,
```

### 2. `grid_renderer.rs` — iterate glyphs in `render_cell_glyph`

In `render_cell_glyph` (line 1719–1761), change the glyph consumption:

```rust
// Before (line 1755–1759):
if let Some((glyph_id, font_id)) = glyph_and_font {
    ctx.scene.draw_glyph(origin, glyph_id, font_id, font_size, foreground_color);
}

// After:
if let Some((font_id, cluster_glyphs)) = glyph_and_font {
    for (glyph_id, glyph_offset) in cluster_glyphs {
        let glyph_origin = origin + glyph_offset;
        ctx.scene.draw_glyph(glyph_origin, glyph_id, font_id, font_size, foreground_color);
    }
}
```

`glyph_offset` is `position_along_baseline` from the shaper — it positions each mark relative to the cluster's baseline origin. For the base glyph, this is approximately `(0, 0)`. For an upper mark, this might be `(0, -12)` (above baseline).

### 3. Single-char path stays unchanged

`glyph_for_char` and the `CharOrStr::Char` branch in `render_cell_glyph` are unchanged. ASCII, CJK, box-drawing, Powerline — none of these produce multi-glyph clusters, so they flow through the existing single-glyph path.

### 4. Cursor rendering

No changes needed. Cursor is drawn at `grid_origin + glyph_offset` covering `cell_size` — this is drawn before glyphs (the scene draws background before foreground). Combining marks render on top of the cursor background, satisfying PRODUCT.md invariant 19.

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| **Clipping** — marks above base may extend past row height | Not a new problem; existing glyphs already overflow cells (e.g., Powerline glyphs). The terminal grid does not clip to cell bounds. Verify with existing overflow behavior. |
| **Cache memory** — `Vec` per cached entry instead of single tuple | Each cluster is at most a handful of glyphs (max 3–4 for Thai). The string cache is small (only cells with zero-width chars). Negligible. |
| **Font mismatch within cluster** — shaper could theoretically pick a different font for a mark | `RunBuilder` flushes the run on font_id change, so all glyphs in a single `Run` share the same `FontId`. Safe. |
| **Emoji variation selectors** — currently handled via the same code path | VS16 (`\u{FE0F}`) makes an emoji 2-wide; this is a pre-composed single glyph in practice. If any variation selector produces multi-glyph output, the fix handles it correctly by rendering all glyphs. |

## Testing and validation

### Unit tests

Add to `cell_glyph_cache.rs` or `grid_renderer_test.rs`:

- **Test:** `glyph_for_string("ที่")` returns 2+ glyphs (base `ท` + marks).
- **Test:** `glyph_for_string("สวัสดี")` returns correct glyph count for each cell when called cell-by-cell.
- **Test:** ASCII string `"abc"` through the string path still returns single glyph per cell.
- **Test:** emoji with ZWJ sequence returns at least 1 glyph (doesn't hit the old `len() > 1` failure).
- **Test:** empty string or whitespace-only string returns `None`.

### Reference tests

Warp has a reference test framework (`app/src/terminal/ref_tests/`):
- Add a reference test that writes Thai text to the grid and snapshots the rendered scene.
- Verify the snapshot shows combining marks visually distinct from base characters.
- Compare snapshot against a known-good baseline.

### Manual verification

For each PRODUCT.md invariant:
- Open Warp, run `echo "สวัสดี"` — verify all marks visible (invariant 1–3).
- `echo "ที่ นี่ นี้ ฟ้า"` — verify upper vowels + tone marks (invariants 4–7).
- `echo "พื้น"` — verify upper + lower vowels simultaneously (invariant 8).
- Select Thai text with mouse, copy, paste into TextEdit — verify characters intact (invariants 10–11).
- Double-click Thai word — verify reasonable selection boundaries (invariant 12).
- Search for `"สวัสดี"` in find-in-block — verify match (invariant 13).
- Run an existing ASCII-heavy command (`ls -la`, `htop`) — verify no regression (invariant 15).
- Test with ligatures enabled and disabled — no change (invariant 15).
- Test on macOS, Linux, Windows if available (invariant 20).

## End-to-end flow

```
User types: echo "ที่"
    │
    ▼
PTY/shell outputs UTF-8 bytes: [0xE0, 0xB8, 0x97, 0xE0, 0xB8, 0xB5, 0xE0, 0xB9, 0x88]
    │
    ▼
warp_terminal ANSI decoder → GridHandler stores in cell as CharOrStr::Str("ที่")
    │
    ▼
grid_renderer::render_cell → render_cell_glyph
    │
    ▼
cell_glyph_cache::glyph_for_string("ที่", font_id, …)
    │
    ▼
text_layout_cache.layout_line("ที่", …)
    │ → cosmic_text shapes with rustybuzz
    │ → returns LayoutLine with 1 Run containing 3 LayoutGlyphs:
    │   - LayoutGlyph { glyph_id: X, x: 0,  y: 0  }  // base consonant ท
    │   - LayoutGlyph { glyph_id: Y, x: 0,  y: -8 }  // upper vowel -ี
    │   - LayoutGlyph { glyph_id: Z, x: 0,  y: -16 } // tone mark -่
    │
    ▼
RunBuilder.push_glyph → Glyph { id, position_along_baseline: vec2f(x, y), … }
    │
    ▼
[FIXED] glyph_for_string returns Some((font_id, vec![
    (X, vec2f(0, 0)),
    (Y, vec2f(0, -8)),
    (Z, vec2f(0, -16)),
]))
    │
    ▼
render_cell_glyph iterates:
    scene.draw_glyph(origin + (0, 0),  X, font_id, …)   // ท at baseline
    scene.draw_glyph(origin + (0, -8), Y, font_id, …)   // -ี above
    scene.draw_glyph(origin + (0, -16), Z, font_id, …)  // -่ above -ี
```

## Follow-ups

- **Input line Thai rendering** — the block editor / input line may use a separate rendering path. If Thai appears broken there too, a separate fix is needed.
- **Font fallback** — if the selected terminal font lacks Thai glyphs, Warp may render tofu (□). Adding Thai to the fallback font chain is a separate feature.
- **Upstream contribution** — once validated, this fix is a candidate for upstreaming to `warpdotdev/warp`.

# Thai Script Rendering

## Summary

Thai script uses Unicode combining characters — upper vowels, lower vowels, and tone marks that render above or below the base consonant. Warp's terminal grid currently fails to render these combining marks; only the base consonant appears. This spec defines correct rendering of Thai (and by extension all combining-character scripts) in the terminal grid.

## Problem

Users typing or viewing Thai text in Warp see corrupted output. For example:

| Input | Rendered (broken) | Expected |
|-------|-------------------|----------|
| `สวัสดี` | `สวสด` | `สวัสดี` |
| `ก่อนที่จะรายงาน` | `กอนทจะรายงาน` | `ก่อนที่จะรายงาน` |

The rendering failure affects every terminal surface: local shell, SSH sessions, Docker containers, Vim, Claude Code, and any other TUI or CLI that outputs Thai text. It has been reported since August 2023 (GitHub issues [#3584](https://github.com/warpdotdev/Warp/issues/3584), [#8357](https://github.com/warpdotdev/warp/issues/8357)) and makes Warp unusable for Thai-speaking developers.

## Goals / Non-goals

**Goals:**
- All Thai combining marks render visually in terminal grid output, positioned correctly above or below their base character.
- Behavior extends to any script using Unicode combining characters (Vietnamese tone marks, Arabic diacritics, Hebrew niqqud, Devanagari matras, Latin combining diacritics).
- Works identically on macOS, Linux, and Windows.

**Non-goals:**
- Thai text in the Warp input editor (block list input line) — this uses a different rendering path and may be addressed separately.
- Thai font fallback or font selection — the fix uses the already-selected terminal font; if the font lacks Thai glyphs, fallback to a system font that has them is a separate concern.
- Complex text layout for scripts requiring reordering (Arabic, Indic) — this spec addresses combining-mark composition only.

## Behavior

### Core rendering

1. When a terminal grid cell contains a base character followed by one or more zero-width combining characters (Unicode general category `Mn`, `Mc`, or `Me`), the cell renders as a single visual cluster with all marks positioned correctly relative to the base glyph.

2. The visual cluster occupies the same grid cell as the base character. Combining marks that extend above or below the base glyph are not clipped at the cell boundary — they render in their full visual extent, overlapping adjacent rows if needed. This is consistent with how other terminals (iTerm, Terminal.app, Windows Terminal, Alacritty) handle combining marks.

3. Combined marks render at positions determined by font metrics and OpenType shaping — the exact pixel positions of marks above/below the base are the font's responsibility, not Warp's. Warp must pass the full grapheme cluster through the text shaping pipeline and render all resulting glyphs, not just the first one.

### Specific Thai cases

4. **Upper vowels** (`ิ` U+0E34, `ี` U+0E35, `ึ` U+0E36, `ื` U+0E37, `ั` U+0E31, `็` U+0E47) render above the base consonant.

5. **Lower vowel** (`ุ` U+0E38, `ู` U+0E39) render below the base consonant.

6. **Tone marks** (`่` U+0E48, `้` U+0E49, `๊` U+0E4A, `๋` U+0E4B) render above the base consonant (above upper vowels when both are present).

7. **Combined upper vowel + tone mark** — e.g., `ที่` (base `ท` + upper vowel `-ี` + tone mark `-่`) — renders with both marks stacked above the base in the correct vertical order.

8. **Combined upper + lower vowels** — e.g., `พื้น` (base `พ` + lower vowel `-ุ` + upper vowel `-ื` + final `น`) — renders with marks both above and below the base consonant simultaneously.

9. A single Thai word spanning multiple grid cells renders each cell's cluster independently. The visual result of adjacent clusters must appear as a coherent word — marks from one cell must not interfere with or clip marks from adjacent cells.

### Selection and copy

10. Selecting Thai text that spans cells with combining marks selects the full underlying text (including all combining characters), not just the base characters.

11. Copying selected Thai text preserves the original character sequence including all combining marks. Pasting into another application produces the correct Thai text.

12. Double-click word selection recognizes Thai word boundaries approximately (greedy match of Thai characters including combining marks). Exact linguistic word segmentation is not required — character-class-based boundaries are acceptable.

### Search

13. Find-in-block matches against the full cell content including combining marks. Searching for a complete Thai word (`สวัสดี`) finds matching text in the grid.

14. Searching for a decomposed substring (base consonant without its marks, e.g., `สวสด`) must match cells where the base characters appear regardless of combining marks. This preserves the existing substring-search behavior for scripts without combining marks.

### Non-regression

15. Existing rendering must not regress:
    - ASCII text renders identically to before.
    - Emoji (including ZWJ sequences and variation selectors) render as before.
    - CJK wide characters render as before.
    - Box-drawing characters and Powerline glyphs render as before.
    - Ligatures (when enabled) compose as before.
    - Text with only a single combining mark on a space or non-letter base renders correctly.

16. Single-glyph clusters (where the font has a pre-composed glyph for the base+mark combination) must render identically to before — the fix must not change behavior for already-working cases.

17. A cell containing only combining marks (no base character) renders the marks on the cell's default empty glyph, not as invisible. This matches behavior of standard terminals.

### Multi-glyph cluster rendering

18. When text shaping produces multiple glyphs for a single grapheme cluster (base + one or more combining marks), all glyphs in the cluster are rendered at the same grid cell position. The shaping engine positions each glyph relative to the cluster origin — Warp must respect those per-glyph offsets rather than forcing all glyphs to the cell's baseline origin.

19. Cursor rendering on a cell with combining marks draws the cursor at the full cell bounds, behind the glyphs. The cursor must not obscure the marks — glyphs render on top of the cursor background.

### Platform consistency

20. Thai text renders identically across macOS, Linux, and Windows given the same monospace font. Platform-specific font loading or rasterization differences must not affect combining-mark positioning.

21. On Windows, where the ConPTY host may re-encode Thai text, Warp's grid renderer must handle the text as received from the PTY without second-guessing the encoding. If the PTY delivers decomposed sequences (base + separate combining marks), Warp composes them into the same cell cluster.

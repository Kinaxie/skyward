//! The charging screen, shown as a cable goes in: the time large in the
//! middle of the black screen, the date above it and the battery under it,
//! as an always-on display shows them.

use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::font::Fonts;
use crate::theme::Metrics;
use skyward_info::Battery;

/// The clock's weight and size in dp, the date's and the battery's, and the
/// room between them.
const CLOCK_WEIGHT: f32 = 400.0;
const CLOCK_DP: f32 = 112.0;
const DATE_WEIGHT: f32 = 500.0;
const DATE_DP: f32 = 19.0;
const LEVEL_WEIGHT: f32 = 400.0;
const LEVEL_DP: f32 = 14.0;
const DATE_GAP_DP: f32 = 14.0;
const LEVEL_GAP_DP: f32 = 30.0;

/// The battery's outline: its width and height, what its fill keeps clear
/// of the outline, its nub, and the room before what it holds.
const PILL_W_DP: f32 = 26.0;
const PILL_H_DP: f32 = 13.0;
const PILL_INSET_DP: f32 = 2.0;
const NUB_W_DP: f32 = 2.0;
const PILL_GAP_DP: f32 = 6.0;

const BLACK: Rgba = Rgba::rgb(0, 0, 0);
/// The clock's ink, as gray as an always-on display keeps it; the date and
/// the battery a step dimmer.
const CLOCK_INK: Rgba = Rgba::rgb(206, 206, 206);
const INK: Rgba = Rgba::rgb(196, 196, 196);

/// What the screen shows, drawn once for as long as it holds.
pub struct Art {
    shows: (u8, String, String),
    block: Canvas,
    top: f32,
}

impl Art {
    /// The screen's middle, `clock` and `date` as the phone's own zone has
    /// them, the battery at `b`'s level.
    pub fn new(f: &Fonts, m: &Metrics, b: &Battery, clock: &str, date: &str) -> anyhow::Result<Art> {
        let (face, date_face, level_face) = (f.face(CLOCK_WEIGHT)?, f.face(DATE_WEIGHT)?, f.face(LEVEL_WEIGHT)?);
        let (clock_px, date_px, level_px) = (m.dp(CLOCK_DP), m.dp(DATE_DP), m.dp(LEVEL_DP));
        let probe = Canvas::new(1, 1);
        let clock_w = probe.text_width(&face, clock, clock_px);
        let date_w = probe.text_width(&date_face, date, date_px);
        let level = format!("{}%", b.level);
        let level_w = probe.text_width(&level_face, &level, level_px);
        let (pill_w, pill_h, pill_gap) = (m.dp(PILL_W_DP), m.dp(PILL_H_DP), m.dp(PILL_GAP_DP));
        let (date_h, clock_h, row_h) = (if date.is_empty() { 0.0 } else { date_face.cap(date_px) }, face.cap(clock_px), pill_h.max(level_px));
        let date_gap = if date.is_empty() { 0.0 } else { m.dp(DATE_GAP_DP) };
        let block_h = date_h + date_gap + clock_h + m.dp(LEVEL_GAP_DP) + row_h;
        let mut block = Canvas::new(m.w, block_h.ceil() as u32);
        let cx = m.w as f32 / 2.0;
        if !date.is_empty() {
            block.text_vc(&date_face, date, cx - date_w / 2.0, date_h / 2.0, date_px, INK, None);
        }
        let clock_top = date_h + date_gap;
        block.text(&face, clock, cx - clock_w / 2.0, clock_top + clock_h, clock_px, CLOCK_INK, None);
        // The battery under the clock: its outline, what it holds, its nub,
        // and the level beside it.
        let row_top = clock_top + clock_h + m.dp(LEVEL_GAP_DP);
        let row_w = pill_w + pill_gap + level_w;
        let (x, cy) = (cx - row_w / 2.0, row_top + row_h / 2.0);
        let (px, py) = (x, cy - pill_h / 2.0);
        block.stroke_round_rect(px, py, pill_w, pill_h, pill_h / 2.0, m.dp(1.4), INK);
        let inset = m.dp(PILL_INSET_DP);
        let fill_w = ((pill_w - inset * 2.0) * b.level.min(100) as f32 / 100.0).max(0.0);
        if fill_w > 0.0 {
            block.fill_round_rect(px + inset, py + inset, fill_w, pill_h - inset * 2.0, (pill_h - inset * 2.0) / 2.0, INK);
        }
        block.fill_round_rect(px + pill_w + m.dp(1.5), cy - pill_h * 0.22, m.dp(NUB_W_DP), pill_h * 0.44, m.dp(1.2), INK);
        block.text_vc(&level_face, &level, px + pill_w + pill_gap, cy, level_px, INK, None);
        let top = (m.h as f32 - block_h) / 2.0;
        Ok(Art { shows: (b.level, clock.to_string(), date.to_string()), block, top })
    }

    /// Whether the art shows `b` with the clock and date as they stand.
    pub fn shows(&self, b: &Battery, clock: &str, date: &str) -> bool {
        self.shows == (b.level, clock.to_string(), date.to_string())
    }
}

/// Draw the charging screen over the whole of `out`.
pub fn draw(out: &mut Canvas, art: &Art) {
    out.fill_rect(0.0, 0.0, out.w as f32, out.h as f32, BLACK);
    out.lay(&art.block, 0, art.top as i32, 1.0);
}

//! kitty's graphics protocol with Unicode placeholders: a picture is sent once, with a virtual
//! placement of its size in cells, and drawn as cells of U+10EEEE whose foreground colour is its ID
//! and whose combining marks say the row and column. They're text, so they move, scroll and clip
//! with the rest of the frame and leave nothing behind.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const PLACEHOLDER: char = '\u{10EEEE}';

/// The first of kitty's row and column marks, from its rowcolumn-diacritics table: row or column
/// `n` is the `n`th. Every cell says both, so each stands alone however little of a frame changes.
const MARKS: [char; 64] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}', '\u{0346}',
    '\u{034A}', '\u{034B}', '\u{034C}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}', '\u{035B}', '\u{0363}',
    '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}', '\u{036A}', '\u{036B}', '\u{036C}',
    '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}', '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}',
    '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}', '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}',
    '\u{059F}', '\u{05A0}', '\u{05A1}', '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}',
    '\u{0610}', '\u{0611}', '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}',
    '\u{0658}',
];

/// The most rows and columns a picture can have here.
pub const MOST: u16 = MARKS.len() as u16;

/// Sends a PNG under `id`, placed virtually over `cols` by `rows` cells, in chunks the terminal
/// takes, and quietly.
pub fn send(id: u32, png: &[u8], cols: u16, rows: u16) -> Vec<u8> {
    let data = crate::run::base64(png);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
    let mut out = Vec::with_capacity(data.len() + 64 * chunks.len());
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            out.extend(format!("\x1b_Ga=T,U=1,f=100,t=d,q=2,i={id},c={cols},r={rows},m={more};").as_bytes());
        } else {
            out.extend(format!("\x1b_Gm={more};").as_bytes());
        }
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
    out
}

/// Tells the terminal to forget picture `id`.
pub fn delete(id: u32) -> Vec<u8> {
    format!("\x1b_Ga=d,d=I,q=2,i={id}\x1b\\").into_bytes()
}

/// The picture's ID as a colour, which is how a placeholder says whose it is.
pub fn colour(id: u32) -> Color {
    Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8)
}

/// Placeholder cells for picture `id` over `area`, the rows `clip` shows.
pub fn placeholders(buf: &mut Buffer, area: super::Place, clip: Rect, id: u32) {
    let fg = colour(id);
    for (row, y) in area.rows_in(clip).filter(|(row, _)| *row < MOST) {
        for col in 0..area.cols.min(MOST) {
            let symbol: String = [PLACEHOLDER, MARKS[row as usize], MARKS[col as usize]].into_iter().collect();
            buf[(area.x + col, y)].set_symbol(&symbol).set_fg(fg);
        }
    }
}

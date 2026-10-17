// Typography and page layout.
pub const FONT_FAMILY: &str = "Libertinus Serif";
pub const MONO_FONT_FAMILY: &str = "DejaVu Sans Mono";
pub const FONT_SIZE: f32 = 19.0;
pub const LINE_HEIGHT: f32 = FONT_SIZE * 1.6;
// Size of a heading level's lines, as a multiple of FONT_SIZE (index level - 1).
pub const HEADING_SCALES: [f32; 6] = [1.6, 1.35, 1.15, 1.0, 1.0, 1.0];
pub const COLUMN_WIDTH: f32 = 680.0;
pub const TOP: f32 = 76.0;

// Colors.
pub const BACKGROUND: u32 = 0x151515;
pub const TEXT_COLOR: u32 = 0xC8C6C3;
pub const MARKER_COLOR: u32 = 0x73716F;
pub const QUOTE_COLOR: u32 = 0xA3A19E;
pub const CODE_COLOR: u32 = 0xB9C3C6;
pub const LINK_COLOR: u32 = 0x9FB6C9;
pub const CARET_COLOR: u32 = 0xFF2020;
pub const MATH_CARET_COLOR: u32 = 0x2F7BFF;
// Marks the hand-written caret inside a formula's LaTeX. Only live previews look for these pixels:
// they record the box and erase them. Other renders draw this colour like any other, so it must be
// one that no one types.
pub const CARET_SENTINEL: u32 = 0xFE02FD;
pub const AI_CARET_COLOR: u32 = 0x1FD65F;
pub const SELECTION_COLOR: u32 = 0x343434;
pub const PENDING_COLOR: u32 = 0x868583;
pub const FAILED_MARK_COLOR: u32 = 0x70484A;

// Math rendering cache entries kept per cache before it is cleared.
pub const MATH_CACHE_LIMIT: usize = 512;

// All generation requests, including launch warm-up, use this model.
// User-selected installed model; change this constant for another Ollama model.
pub const OLLAMA_MODEL: &str = "gemma4:e4b";
pub const OLLAMA_ADDRESS: &str = "127.0.0.1:11434";
pub const KEEP_ALIVE: &str = "30m";

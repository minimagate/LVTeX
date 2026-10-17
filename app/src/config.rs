// Typography and page layout.
pub const FONT_FAMILY: &str = "Libertinus Serif";
pub const FONT_SIZE: f32 = 19.0;
pub const LINE_HEIGHT: f32 = FONT_SIZE * 1.6;
pub const COLUMN_WIDTH: f32 = 680.0;
pub const TOP: f32 = 76.0;

// Colors.
pub const BACKGROUND: u32 = 0x151515;
pub const TEXT_COLOR: u32 = 0xC8C6C3;
pub const MARKER_COLOR: u32 = 0x73716F;
pub const CARET_COLOR: u32 = 0xFF0000;
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

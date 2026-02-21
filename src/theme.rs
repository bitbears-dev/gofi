/// Background color (R, G, B, A)
pub const BG_COLOR: (u8, u8, u8, u8) = (30, 30, 30, 200);

/// Selected item color (R, G, B, A)
pub const SELECTED_COLOR: (u8, u8, u8, u8) = (60, 100, 160, 255);

/// Unselected item color (R, G, B, A)
pub const ITEM_COLOR: (u8, u8, u8, u8) = (50, 50, 50, 255);

/// Highlight color for fuzzy-match characters (R, G, B)
pub const HIGHLIGHT_COLOR: (u8, u8, u8) = (0, 255, 255);

/// Font size in pixels
pub const FONT_SIZE: f32 = 18.0;

/// Height per list item (spacing)
pub const ITEM_HEIGHT: f32 = 28.0;

/// Height of the item background rectangle
pub const ITEM_RECT_HEIGHT: f32 = 24.0;

/// Y offset where the item list begins (below the search bar)
pub const SEARCH_BAR_Y: f32 = 50.0;

/// Horizontal padding from the window edge
pub const PADDING: f32 = 10.0;

/// X offset for item text (indented from PADDING)
pub const ITEM_TEXT_OFFSET_X: f32 = 20.0;

/// Y offset for item text within its rectangle (vertical centering tweak)
pub const ITEM_TEXT_OFFSET_Y: f32 = 2.0;

/// Bottom margin — stop drawing items before reaching this distance from the bottom
pub const BOTTOM_MARGIN: f32 = 30.0;

/// Combined top + bottom margin used to calculate the available scroll area
pub const TOP_BOTTOM_MARGIN: f32 = 80.0;

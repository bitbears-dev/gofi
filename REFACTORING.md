# Gofi リファクタリング提案

## 1. 🏗️ `main.rs` のファイル分割 (優先度: 高)

`main.rs` が 847行あり、複数の責務が混在している。以下のようにモジュール分割する。

| 分割先モジュール | 内容 |
|---|---|
| `app.rs` | `App` 構造体の定義 + `draw()`, `handle_key_action()` |
| `handlers.rs` | Wayland ハンドラ実装群 (`CompositorHandler`, `OutputHandler`, `SeatHandler`, `KeyboardHandler`, `PointerHandler`, `ShmHandler`, `WindowHandler`, `ProvidesRegistryState`) |
| `rendering.rs` | `draw_text_pixel()` 関数 |
| `fonts.rs` | `load_fonts()` 関数 |
| `key_repeat.rs` | `RepeatCommand`, `KeyRepeat` 構造体とキーリピート処理 |

`main.rs` には `main()` 関数と `mod` 宣言だけが残る形にする。

---

## 2. 🎨 描画ロジックの定数化・構造化 (優先度: 高)

`draw()` メソッドにマジックナンバーが多数ハードコードされている。

```rust
// 現状
paint.set_color_rgba8(30, 30, 30, 200);  // 背景色
tiny_skia::Color::from_rgba8(60, 100, 160, 255)  // 選択色
tiny_skia::Color::from_rgba8(50, 50, 50, 255)  // 非選択色
let item_height = 28.0;
let mut y = 50.0;
```

テーマ定数を構造体 or モジュールレベル定数にまとめる:

```rust
mod theme {
    pub const BG_COLOR: (u8, u8, u8, u8) = (30, 30, 30, 200);
    pub const SELECTED_COLOR: (u8, u8, u8, u8) = (60, 100, 160, 255);
    pub const ITEM_COLOR: (u8, u8, u8, u8) = (50, 50, 50, 255);
    pub const ITEM_HEIGHT: f32 = 28.0;
    pub const SEARCH_BAR_HEIGHT: f32 = 50.0;
    pub const FONT_SIZE: f32 = 18.0;
    pub const PADDING: f32 = 10.0;
}
```

---

## 3. ⌨️ `handle_key_action()` を `match` 式に変更 (優先度: 中)

`if-else if` チェーンを `match` に書き換えて可読性を向上させる。

```rust
match keysym {
    Keysym::Escape => self.exit = true,
    Keysym::Down => self.window_switcher_state.next(),
    Keysym::Up => self.window_switcher_state.prev(),
    Keysym::Return => {
        self.window_switcher_state.activate();
        self.exit = true;
    },
    Keysym::BackSpace => self.window_switcher_state.backspace(),
    _ if utf8.as_ref().is_some_and(|t| !t.chars().any(|c| c.is_control())) => {
        self.window_switcher_state.input_text(utf8.as_ref().unwrap());
    },
    _ => {},
}
```

---

## 4. ⚠️ エラーハンドリングの改善 (優先度: 中)

`main()` 内の `.unwrap()` を `anyhow` 導入で `main() -> Result<()>` にするか、少なくとも `.expect("meaningful message")` に統一して、失敗時のメッセージを改善する。

---

## 5. 📐 ウィンドウサイズ計算のロジック抽出 (優先度: 低)

`main()` 内 L96〜L119 のサイズ決定ロジックを関数化する:

```rust
fn calculate_window_size(output_state: &OutputState) -> (u32, u32) {
    // ...
}
```

---

## 6. ✅ ~~`test_switcher.rs` の改善~~ (優先度: 低) — 完了

~~テストが DBus 依存で実質機能していない。`window_switcher.rs` 内に既にまともなテストがあるため、`test_switcher.rs` は削除するか、モックを使ったテストに書き直す。~~

`window_switcher.rs` 内に十分なテスト (`test_filter_self_pid`, `test_ensure_visible`, `test_extract_json_from_gdbus`) が存在するため、`test_switcher.rs` を削除済み。

---

## 7. 🔧 `draw_text_pixel()` の引数の多さ (優先度: 低)

7個の引数を `TextDrawParams` のような構造体にまとめて、呼び出し側の可読性を改善する。

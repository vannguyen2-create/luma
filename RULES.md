# LUMA Rust — Coding Rules

Bất biến. Không ngoại lệ. Mỗi PR phải pass tất cả.

## I. Kiến trúc

1. **Owner mutates.** Ai own data, người đó `&mut`. Không bypass facade để sờ field trực tiếp.
2. **Giao tiếp:** `&T` (đọc), `Event` enum qua channel (async), return value (sync). Không `Arc<Mutex<>>`, không global state, không `static mut`.
3. **Dependency chảy xuống, không vòng.** `app → output → {store, scroll, viewport}`. `app → agent → provider`. `agent → tool`. Không ngược.

## II. Code

4. **Mọi `pub fn` có doc comment 1 dòng.** Private fn: chỉ comment khi logic không hiển nhiên. Không comment obvious.
5. **Error handling:** `Result<T, E>` cho mọi thứ có thể fail. Không `.unwrap()` ngoài test. `.expect("lý do cụ thể")` khi invariant chắc chắn. `thiserror` cho module error, `anyhow` cho app level.
6. **Naming:** module = `snake_case`. Struct/Enum = `PascalCase`. fn = `snake_case`, nói rõ hành động. Không viết tắt trừ convention: `tx/rx`, `buf`, `cfg`, `ctx`. Bool: `is_*`, `has_*`, `should_*`.
7. **Không magic number.** `const` với tên rõ nghĩa.
8. **Giới hạn:** Struct ≤ 7 fields. fn ≤ 40 lines. File ≤ 300 lines. Vượt → tách.

## III. Vận hành

9. **Test per module.** `#[cfg(test)] mod tests` trong mỗi file có logic. Viết code xong → viết test ngay. Minimum 1 test per `pub fn`.
10. **Không TODO.** Dùng `todo!("mô tả")` — compiler panic nếu hit, không quên.
11. **Không `unsafe`.** Toàn bộ project không cần unsafe.
12. **Dependencies:** approve list: `tokio`, `reqwest`, `serde`, `serde_json`, `smallvec`, `tokio-util`, `thiserror`, `anyhow`, `crossterm`. Thêm dep mới phải justify.
13. **Clippy clean.** `cargo clippy -- -D warnings`. Không `#[allow(clippy::...)]` trừ false positive có comment giải thích.
14. **Format:** `rustfmt` default. Không customize `rustfmt.toml`.

## IV. Process

15. **Implement theo thứ tự:** types → trait → simplest impl → test → next module. Không thiết kế trait 5 methods rồi implement 1.
16. **Generic khi ≥ 2 concrete types dùng.** Không generic "cho tương lai".
17. **Owned ở API boundary, borrowed ở hot path.** `String` cho public API, `&str` cho internal render.
18. **Đo trước khi optimize.** Có benchmark evidence mới dùng `SmallVec`, pre-alloc, etc.

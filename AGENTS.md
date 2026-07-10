# 工程規範（執行工程師必讀）

本專案為基於 ratatui + tokio 的 NetworkManager (nmcli) 非同步 TUI。程式碼可在任何
作業系統編譯與測試，但執行期僅支援 Linux。**開發環境是 Windows，不要嘗試實際執行
本程式或 nmcli。**

## 架構邊界（硬性規則，違反即退件）

| 模組 | 職責 | 禁止事項 |
| --- | --- | --- |
| `nm/` | nmcli 命令建構、執行、結束碼對應、監控 | 不得引用任何 TUI 型別 |
| `parse/` | 零拷貝解析 `nmcli -t` 輸出，回傳借用的 rows | 不得做任何 I/O、不得分配非必要記憶體 |
| `model/` | 擁有所有權的領域型別；唯一的借用→擁有轉換點 | — |
| `app.rs` | 純狀態機（`apply` / `on_msg` / `on_tick`），回傳 `Effect` | 不得做 I/O；所有邏輯必須可在任何 OS 上單元測試 |
| `effect.rs` | 唯一產生 nmcli 背景任務的地方 | — |
| `ui/` | 只負責繪製，讀取 `&App` | 絕不修改狀態、絕不做 I/O |

其他硬性規則：

- 生產程式碼禁用 `unwrap` / `expect`（`clippy::unwrap_used`）；測試碼不受限。
- 所有使用者可見字串一律**繁體中文**（zh-TW）；程式碼註解與識別字用英文。
- 新增或修改狀態機行為時，必須在 `tests/app_state.rs` 補上對應測試。
- 修改解析器時，必須在 `tests/fixtures/` 補上夾具並於 `tests/parser_fixtures.rs` 覆蓋。
- 快速鍵有異動時，同步更新 `src/ui/keys.rs` 的提示/說明與 `README.md` 的快速鍵表。

## 驗收關卡（交付前必須全數通過，並在回報中附上結果）

```sh
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

## 交付方式

- **不要自行 `git commit` / `git push`**，除非任務簡報明確要求。改動留在工作區，
  由技術主管審查驗收。
- 完成後回報：改了哪些檔案與原因、驗收關卡輸出摘要、已知取捨或未盡事項。
- 任務簡報未涵蓋的範圍不要順手擴張；發現簡報有誤或有更好做法，先在回報中說明。

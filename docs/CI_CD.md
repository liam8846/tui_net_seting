# CI/CD 流程

專案使用 GitHub Actions 執行持續整合與發行。工作流程定義於
`.github/workflows/ci-cd.yml`。

## 觸發條件

- 推送任何分支：執行品質檢查、測試及 Linux 全平台編譯。
- Pull Request：執行與分支推送相同的檢查。
- 手動執行：可從 GitHub Actions 頁面啟動完整流程。
- 推送 `v` 開頭的 Git 標籤（例如 `v0.1.0`）：所有檢查成功後，自動建立或更新
  GitHub Release，並附上各平台的二進位檔。

## 驗證項目

品質工作會依序執行：

```sh
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

交叉編譯矩陣涵蓋：

| CPU 架構 | glibc | musl |
| --- | --- | --- |
| AMD64 / x86-64 | 是 | 是 |
| x86 / i686 | 是 | — |
| ARM64 / AArch64 | 是 | 是 |
| ARMv7 hard-float | 是 | — |
| PowerPC64 little-endian | 是 | — |
| IBM s390x | 是 | — |
| RISC-V 64 | 是 | — |

發行檔名包含完整 Rust target triple，方便辨認 CPU 架構與 libc。`nmcli` 是執行期
相依套件，不會在 CI 內執行；實際使用時仍須在目標 Linux 系統安裝並啟用
NetworkManager。

## 發行版本

確認版本內容後建立並推送標籤：

```sh
git tag v0.1.0
git push origin v0.1.0
```

只有 `v*` 標籤會取得 Release 寫入權限。一般分支與 Pull Request 的工作只具有
儲存庫唯讀權限。

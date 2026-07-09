# tui_net_seting

An asynchronous terminal UI (TUI) for managing NetworkManager through `nmcli`,
built with [ratatui] and [tokio]. It lets you browse and change network
settings — devices, connection profiles, Wi-Fi, hotspot and radio switches —
without leaving the terminal, and never blocks the UI while `nmcli` runs.

> **Platform:** Linux with NetworkManager only. The code builds and its tests
> run on any OS (Windows/macOS included) via `cargo check` / `cargo test`, but
> the app itself must run on a machine where `nmcli` is available.

## Features

- **Devices** — list with type/state/active connection; connect / disconnect;
  a details pane (IP addresses, gateway, DNS, MAC, MTU, link speed).
- **Connections** — list profiles; activate / deactivate; add / edit / delete.
  The editor covers IPv4 & IPv6 method (auto / manual / disabled), static
  addresses, gateway, DNS servers, the autoconnect flag and the connection name.
- **Wi-Fi** — scan / rescan; access-point list with signal bars, security
  badges and the in-use marker; connect (prompting for a password on secured
  networks); disconnect; forget a saved network.
- **Hotspot** — create a Wi-Fi hotspot (interface / SSID / password) and tear
  it down.
- **Radio** — toggle global networking and the Wi-Fi radio.

## Running

```sh
cargo run --release
```

Run it as your normal desktop-session user: NetworkManager delegates
authorization to **polkit**, which by default grants an active local session
the right to control networking. Over plain SSH (no session) some operations
return "not authorized" — this surfaces as an error toast rather than a crash.

### Keys

| Key | Action |
| --- | --- |
| `1`–`5`, `Tab` / `Shift-Tab` | switch tabs |
| `j` / `k` / `↑` / `↓` | move selection |
| `r` | refresh (rescan on the Wi-Fi tab) |
| `?` | help overlay |
| `q` / `Ctrl-C` | quit |
| `Esc` | close a pane / cancel a modal |

Per-tab keys are shown in the status bar and the help overlay (`?`).

## Design

The module layout mirrors responsibility, not screens:

| Module | Responsibility |
| --- | --- |
| `nm/` | Everything that talks to `nmcli` (command building, execution, exit-code mapping, the change monitor). No TUI types. |
| `parse/` | Zero-copy parsing of `nmcli -t` terse output into borrowed rows. No I/O. |
| `model/` | Owned domain types + the single borrowed→owned boundary (`From<Row<'_>>`). |
| `event.rs` | The `Event` / `Action` / `Msg` enums and `AppError`. |
| `app.rs` | A pure state machine (`apply` / `on_msg` / `on_tick`) — no I/O, fully unit-testable. |
| `effect.rs` | The only place `nmcli` work is spawned; results flow back over an mpsc channel. |
| `ui/` | Rendering only — never mutates state, never does I/O. |
| `tui.rs` | Terminal lifecycle + a panic hook that restores the terminal first. |

The async event loop merges three sources with `tokio::select!`: crossterm's
`EventStream` (input), the worker-result channel, and a 250 ms tick (spinner /
toast expiry). Mutations are serialized — at most one is in flight at a time —
while reads and refreshes run concurrently. A long-lived `nmcli monitor` child
signals external changes; if it dies the app falls back to timer polling.

### Zero-copy parsing

`nmcli` terse output escapes `:` and `\` inside values. `parse/terse.rs` splits
lines on *unescaped* colons yielding borrowed `&str` slices, and unescaping
returns `Cow<'_, str>` that borrows unless an escape is actually present. Rows
borrow straight from the captured stdout buffer; the borrowed→owned conversion
happens exactly once, at the mpsc boundary, in the `model` layer. The device
details pane goes further and reparses its raw buffer at draw time, allocating
nothing.

## Security note

Connecting to a secured Wi-Fi network runs
`nmcli device wifi connect <ssid> password <secret>`. For the brief lifetime of
that process the password is visible in the process argument list
(`/proc/<pid>/cmdline`) to other users on the machine. This is an accepted
trade-off for v1 (nmcli's interactive `--ask` prompt is unusable behind a TUI).

## Development

```sh
cargo check --all-targets          # compiles on any OS
cargo clippy --all-targets -- -D warnings
cargo test                         # parser + state-machine tests, no nmcli needed
```

Production code paths deny `unwrap`/`expect` (`clippy::unwrap_used`); tests are
exempt. Parser tests use captured fixtures under `tests/fixtures/`, including
adversarial cases (escaped SSIDs, hidden networks, `--` placeholders).

[ratatui]: https://ratatui.rs
[tokio]: https://tokio.rs

# Fork notes — 닫기 버튼은 트레이로, 종료는 트레이 메뉴에서

Upstream: <https://github.com/hid276/rust-image-studio> (v1.2.7). This fork changes exactly one
behaviour on Windows.

## 바뀐 동작

| 동작 | upstream | 이 포크 |
| --- | --- | --- |
| 창의 **X** 버튼 | 프로세스 종료 | 창만 숨김 (트레이로) — 진행 중인 생성은 계속됨 |
| 트레이 아이콘 **왼쪽 클릭** | 없음 | 숨긴 창을 다시 표시하고 포커스 |
| 트레이 아이콘 **오른쪽 클릭 → 종료 (Quit)** | 없음 | **프로세스 완전 종료 (유일한 종료 경로)** |
| 작업 표시줄/Alt+Tab | 창 표시 | 창을 숨기면 같이 사라짐 |

Linux 빌드는 예전 그대로입니다(X = 종료). 트레이 백엔드가 GTK/libappindicator 시스템 라이브러리와
GTK 이벤트 루프를 요구해서, `tray-icon` 의존성은 `Cargo.toml` 에서 Windows 전용으로만 넣었습니다.

## 구현 지점

| 파일 | 내용 |
| --- | --- |
| `src/tray.rs` | 신규. `tray-icon` + `muda` 메뉴로 트레이 아이콘 생성, 클릭/메뉴 요청을 큐에 넣고 `request_repaint` 로 앱을 깨움 |
| `src/app.rs` | `logic()` 에서 트레이 요청 처리(창이 숨겨져 있어도 호출됨), `ui()` 서두에서 `close_requested` 를 `CancelClose` + `Visible(false)` 로 바꿔 숨김 처리, `quit()` 이 엔진을 내리고 `ViewportCommand::Close` 로 실제 종료 |
| `src/main.rs` | `mod tray;`, `window_icon()` 을 `pub` 으로 (트레이 아이콘이 같은 로고를 씀) |
| `Cargo.toml` | `[target.'cfg(windows)'.dependencies] tray-icon = "0.26"` |
| `.github/workflows/fork-release.yml` | 수동 실행 워크플로: Windows 릴리스 빌드 → zip 패키징 → GitHub Release 업로드 |

핵심은 eframe 0.37 의 `App::logic` 훅입니다. 창이 숨겨져 있으면 eframe 은 egui 패스를 돌리지 않고
`logic` 만 호출하므로, 트레이 클릭을 그 자리에서 받아 창을 다시 띄울 수 있습니다.

## 빌드/릴리스

로컬 빌드는 upstream 과 동일합니다.

```powershell
cargo build --release
# 결과: target/release/rust-image-studio.exe
```

릴리스는 GitHub Actions 에서 수동 실행합니다.

1. Actions → **fork-release** → *Run workflow*
2. `tag` 에 배포 태그(예: `v1.2.7-tray.1`), `prerelease` 는 그대로 두고 실행
3. 빌드가 끝나면 해당 태그의 Release 에 `RustImageStudio-windows-x64-tray.zip` 이 첨부됨

`Cargo.lock` 은 이 포크에서 커밋하지 않았습니다(트레이 의존성이 새로 추가됨). 워크플로가 빌드할 때
lock 을 새로 해석하므로 `--locked` 는 쓰지 않습니다.

## English summary

On Windows, closing the window with **X** now hides it to the notification area instead of exiting,
so a generation in progress survives. **Left click** the tray icon to get the window back; the
process ends only from **right click → 종료 (Quit)**. Linux builds keep the original behaviour
(close = quit) because the tray backend there needs GTK/AppIndicator. `fork-release.yml` builds
the Windows zip on demand and attaches it to a GitHub Release.

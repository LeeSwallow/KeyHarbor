# KeyHarbor 디자인 시스템

관리 앱(5단계, Tauri)과 웹 페이지(`site/`)가 같이 쓰는 토큰·컴포넌트·아이콘이다. 빌드 도구나 npm 없이 CSS 두 파일과 SVG만으로 쓴다.

| 파일 | 내용 |
|---|---|
| `tokens.css` | 글자, 간격(4px 격자), 반경, 선, 모션, 레이어, 밀도, 라이트·다크 색 |
| `components.css` | `kh-` 접두사 컴포넌트. `tokens.css`가 먼저 있어야 한다 |
| `icons/` | Lucide 0.460.0에서 고른 아이콘 41개(ISC, `icons/LICENSE`) |
| `index.html` | 쇼케이스: 조합 예시 → 아이콘 → 컴포넌트 → 토큰 |

## 보는 법

CSS mask는 `file://`에서 아이콘을 불러오지 못하므로 로컬 서버로 연다.

```sh
cd .   # 저장소 루트
python3 -m http.server 8765 --bind 127.0.0.1
open http://127.0.0.1:8765/design/index.html          # ?theme=dark&lang=en&density=compact
```

## 원칙

- **강조색은 하나**: 항구 청록(`--c-accent`). 버튼·포커스·선택에만 쓴다.
- **도메인 색은 종류, 상태 색은 상태**: 세션·자격 증명·개발자 비밀·결제·개인 정보는 `--c-dom-*`, 사용 중·곧 만료·만료·거부·변조는 `--c-ok/warn/danger/info`. 색만으로 뜻을 전하지 않고 아이콘이나 낱말을 같이 쓴다.
- **비밀 값은 봉인 표면에만**: 평문이 화면에 그려질 수 있는 곳은 `kh-secret`(`--c-sealed`) 하나다. 관리 앱에서만 보이고, 타이머가 끝나면 다시 가린다. CLI와 에이전트에는 이 컴포넌트에 해당하는 것이 없다(docs/design/03 §3.12).
- **에이전트가 보는 것은 placeholder**: `kh-ph`는 복사해도 안전한 값이라 점선 테두리로 봉인 값과 구분한다.
- **보안 입력 창은 겉모습이 다르다**: `kh-dialog.secure`는 청록 테두리와 바깥 고리를 두르고 누가 요청했는지 먼저 보여 준다. 다른 창이 흉내 내기 어렵게 하려는 표시이지 보안 경계는 아니다.
- **테마와 밀도는 속성으로**: `<html data-theme="light|dark" data-density="comfortable|compact">`. `data-theme`이 없으면 시스템 설정을 따른다.
- **컴포넌트가 아니라 토큰을 바꾼다**: 컴포넌트 CSS에 색·크기 리터럴을 새로 넣지 않는다.

## 아이콘

```html
<span class="kh-i" style="--i:url(icons/key-round.svg)"></span>      <!-- 16px -->
<span class="kh-i lg" style="--i:url(icons/lock.svg)"></span>        <!-- 20px, xl은 24px -->
```

`currentColor`로 칠해지므로 부모의 `color`를 따른다. 아이콘을 더할 때는 같은 Lucide 버전에서 가져와 `icons/`에 두고 `index.html`의 `ICONS` 목록에 이름을 넣는다.

## 컴포넌트

| 클래스 | 쓰임 |
|---|---|
| `kh-btn` + `primary·secondary·ghost·danger`, `sm·lg` | 버튼. 상태 `is-hover·is-focus·is-active·is-disabled·is-loading`은 쇼케이스용 강제 클래스 |
| `kh-ibtn` (`danger`, `is-pressed`) | 아이콘 버튼 |
| `kh-field`, `kh-label`, `kh-input`, `kh-textarea`, `kh-help`, `kh-inputwrap` | 입력 |
| `kh-secret` (`is-revealed`) | 봉인된 비밀 값 |
| `kh-ph` | `{{secret:item.field}}` placeholder |
| `kh-toggle`, `kh-seg`, `kh-tabs` | 켜고 끄기, 고르기 |
| `kh-chip` (`session·cred·dev·pay·personal·agent`), `kh-badge`, `kh-count`, `kh-kbd` | 표시 |
| `kh-origins` (`is-off`) | 값이 갈 수 있는 사이트 |
| `kh-toast`, `kh-menu` | 알림, 메뉴 |
| `kh-row`, `kh-audit` | 항목·세션 행, 사용 기록 행 |
| `kh-panel`, `kh-window`, `kh-titlebar`, `kh-dialog` (`secure`), `kh-empty`, `kh-itemhead` | 틀 |

## 구조의 출처

쇼케이스 배치(헤더의 테마·밀도·언어 전환 → 조합 예시 → 아이콘 → 컴포넌트 → 토큰)는 stickypane 디자인 시스템의 구성을 따랐다. 색, 모양, 컴포넌트 이름은 KeyHarbor가 따로 정했다.

# OpenKeylet 연동 조사

상태: 조사 및 안전한 수동 사용 경로 제안. 자동 연동 구현 아님. 대상은 OpenKeylet 커밋 [`66f3259`](https://github.com/omaekumiko2-create/OpenKeylet/tree/66f3259557477aa6cdc89c416c0664fd31202bc2)이며 이후 버전은 다시 확인해야 한다.

## 두 입력 경로의 차이

| 경로 | 목적지 선택 | KeyHarbor에서의 판단 |
|---|---|---|
| 사용자가 브라우저 확장 팝업에서 항목 선택 | 팝업은 현재 사이트와 금고의 URL 규칙을 비교한다. 대상 프레임도 다시 확인한다고 프로젝트가 설명한다 | 수동 로그인 후보. KeyHarbor와 실제 설치·페어링은 검증 필요 |
| OpenKeylet MCP `credential_fill` | 연결된 확장 중 활성 연결에 job을 전달하고, 확장은 현재 창의 활성 탭과 포커스된 컨트롤에 입력한다 | KeyHarbor 세션 ID와 특정 프로필/탭이 연결된다는 근거가 없다. 병렬 환경에서 자동 사용하지 않는다 |

근거: [확장 풀과 활성 연결](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/src-tauri/src/browser.rs), [확장의 활성 탭 입력](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/browser-extension/service-worker.js), [프로젝트 보안 경계](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/SECURITY.md). 확장이 focus/activation 이벤트를 보낼 때 활성 연결이 바뀔 수 있다. 아래 합성 시험에서도 두 프로필 사이에 활성 연결을 바꿔 입력 대상이 바뀌는 것을 확인했다.

OpenKeylet은 각 확장의 로컬 연결을 token으로 인증하고 페어링 시 extension origin을 확인한다. 이 인증은 확장과 브리지를 연결하는 것으로, KeyHarbor 세션·요청 origin·탭을 서로 묶는 권한 확인과는 다르다. 팝업 경로의 URL 검사도 에이전트 `credential_fill` 경로에 그대로 적용되지 않는다. OpenKeylet은 악의적인 에이전트, 같은 OS 사용자 프로세스와 잘못 포커스된 대상에 대한 방어를 보장하지 않는다고 설명한다.

## 수동 연동 후보

1. KeyHarbor 전용 프로필에서 사용자가 OpenKeylet 확장을 설치하고 프로젝트의 안내에 따라 직접 페어링한다. 프로필마다 설치와 페어링 상태가 어떻게 동작하는지 검증해야 한다.
2. 사용자가 올바른 창과 사이트를 확인하고 확장 팝업에서 계정을 선택해 입력한다. KeyHarbor는 비밀번호나 TOTP를 읽지 않는다.
3. 사용자가 인증을 완료한 뒤 KeyHarbor 세션을 재개한다. 재개 상태는 서버의 로그인 성공을 증명하지 않는다.

이 경로는 기존 제품을 조합하는 사용 절차이며, KeyHarbor가 설치를 자동화하거나 OpenKeylet과 직접 연결한다는 의미는 아니다. 실제 앱과 확장 버전, 설치 경로, 여러 프로필 연결, 2차 인증 및 사이트별 결과를 검증하기 전에는 지원 기능이라고 표현하지 않는다.

## 자동 연결에 필요한 조건

- 입력 요청이 `session_id`, 브라우저 프로필, 탭, 프레임, 정확한 origin, 금고 항목, 사용자 승인과 결합되고 실제 입력 직전 다시 검증돼야 한다.
- 잘못된 탭/포커스, 다른 프로필의 확장 활성화, 교차 출처 iframe, 리다이렉트, 만료·취소, 두 요청의 경합은 실패해야 한다.
- KeyHarbor가 비밀번호 평문·확장 페어링 토큰을 MCP 결과, 명령 인자, 환경 변수, SQLite, 로그에 넣지 않아야 한다. 단순 출력 마스킹은 완전한 방어가 아니다.
- 현 버전 OpenKeylet의 공개 동작만으로 이 조건을 충족한다고 가정하지 않는다. 금고 측의 대상 지정 및 재검증을 지원하는 인터페이스가 없으면 수동 경로를 유지하거나 협력 방식 자체를 다시 설계한다.

## 실험 계획

- 임시 금고, 합성 비밀번호와 localhost 테스트 사이트만 사용한다. 기존 사용자 금고, 프로필, OS 설정은 건드리지 않는다.
- 실제 확장 팝업의 origin 매칭과 입력을 확인하고 잘못된 origin/iframe이 차단되는지 확인한다.
- 두 독립 프로필의 확장이 동시에 연결된 상태에서 포커스를 바꿔 에이전트 입력 목적지가 달라지는지 살핀다. 결과와 실패 조건을 기록한다.
- MCP 트래픽, KeyHarbor DB와 로그에 합성 비밀번호가 없는지 확인하되, 이것만으로 시스템 전체 유출이 없다고 주장하지 않는다.

## 실행 결과 (2026-10-28)

위 커밋의 소스를 임시 디렉터리에 체크아웃하고 headless `browser_fixture`와 Playwright Chromium을 별도 임시 경로에 설치했다. OpenKeylet 자체 `scripts/test-browser.mjs`를 임시 금고와 합성 값으로 실행했다. 첫 시도는 요구되는 Chromium 빌드가 없어 시작 단계에서 실패했고, 설치 후 재시도는 통과했다. 출력에는 확장 페어링, 팝업 비밀번호 입력, origin 격리, 교차 출처 iframe 거부, passkey 검증 등이 포함됐다.

추가로 `tests/openkeylet-interop.mjs`를 실행해 **KeyHarbor가 띄운 전용 프로필**에서 사용자가 저장한 항목 이름 참조, 인증 대기, 확장 페어링, 테스트 사이트의 팝업 비밀번호 입력과 재개를 확인했다. 테스트가 이름과 일치하는 항목을 팝업에서 직접 선택했으며 KeyHarbor가 금고 항목을 조회하거나 검증한 것은 아니다. 테스트는 격리된 임시 금고와 Chrome for Testing, 합성 비밀번호를 사용한다. Chrome 확장을 시험하기 위해 테스트 전용 wrapper로 CDP를 켜고 Playwright로 팝업을 조작한다. KeyHarbor가 확장을 자동 설치하는 것은 아니다. 시험 중 KeyHarbor의 SQLite DB 및 존재하는 WAL/SHM 파일에서 합성 비밀번호 바이트열이 발견되지 않았다.

통합 시험의 첫 두 시도는 테스트 드라이버가 다른 내장 확장 worker를 선택해 실패했고, OpenKeylet 확장 ID를 지정한 뒤 통과했다.

추가로 `tests/openkeylet-parallel.mjs`에서 두 KeyHarbor 프로필을 하나의 합성 OpenKeylet 금고와 연결했다. 각 확장의 활성화를 명시적으로 전환하면서 OpenKeylet MCP `credential_fill`을 호출하자 입력 대상도 A에서 B로 바뀌었다. 양쪽 호출의 MCP 결과에는 합성 비밀번호가 없었다. 이 실험은 활성 확장 선택이 KeyHarbor 세션 ID에 묶여 있지 않음을 보여주는 재현 사례다. 실제 사용자 포커스 경합의 모든 타이밍이나 악의적인 에이전트에 대한 방어를 검증한 것은 아니다.

같은 자동 확장 로딩 시험을 설치된 일반 Google Chrome 156.0.8078.49에서 실행했을 때 확장 worker를 찾지 못해 실패했다. Chrome 공식 프로젝트의 [테스트 설명](https://github.com/GoogleChrome/chrome-launcher/blob/main/test/load-extension-test.ts)은 일반 Chrome 137부터 이 시험에서 사용하는 `--load-extension` 방식이 제거됐다고 설명한다. 이 변경은 관찰된 실패와 부합하지만 실패 원인을 독립적으로 확정한 것은 아니다. Chrome for Testing의 성공을 일반 Chrome의 확장 연동 성공으로 해석하지 않는다. 일반 Chrome에서 사용자가 직접 확장을 설치하고 페어링하는 흐름은 아직 검증하지 않았다.

이후 같은 Chrome에서 테스트 전용 wrapper가 `--remote-debugging-pipe`와 `--enable-unsafe-extension-debugging`을 켜고 `Extensions.loadUnpacked`로 합성 확장을 로드한 상태로 KeyHarbor가 감독하게 하자, 페어링·팝업 입력·인증 대기/재개 시험이 통과했다. 별도로 파이프 로드 후 Chrome을 종료하고 같은 프로필로 다시 열어 본 시도에서는 확장 worker가 나타나지 않았다. 이 결과는 해당 테스트 방식의 재시작 동작이며 사용자가 `chrome://extensions`에서 수동 설치한 확장의 영속성은 확인하지 않는다. 파이프 로딩은 테스트에서만 쓰며 KeyHarbor가 임의 확장을 설치하거나 안전하다고 보증하지 않는다.

이 시험들은 로그·프로세스 메모리의 비밀 비노출, 실제 사이트 로그인이나 실제 사용자의 확장 설치 과정을 검증하지 않는다. 자동 연동의 안전성을 증명하지 않는다.

### 통합 시험 재실행

테스트할 OpenKeylet 커밋을 별도 체크아웃하고 `npm ci`로 Playwright를 설치한 후, 해당 저장소에서 `cargo build --locked --manifest-path src-tauri/Cargo.toml --no-default-features --example browser_fixture`를 실행한다. Playwright 버전에 맞는 Chrome for Testing도 별도로 준비한다. KeyHarbor를 `cargo build --locked`로 빌드한 다음 아래 네 경로를 지정한다.

```sh
OPENKEYLET_REPO=/absolute/path/to/OpenKeylet \
OPENKEYLET_FIXTURE=/absolute/path/to/browser_fixture \
KEYHARBOR_TEST_CHROME='/absolute/path/to/Chrome for Testing' \
KEYHARBOR_BINARY=/absolute/path/to/keyharbor \
node tests/openkeylet-interop.mjs
```

병렬 시험에는 같은 환경 변수와 별도로 headless OpenKeylet 실행 파일을 `OPENKEYLET_BINARY`에 지정한 뒤 `node tests/openkeylet-parallel.mjs`를 실행한다. headless 실행 파일은 위 빌드 명령에서 `--example browser_fixture`를 빼고 별도로 빌드한다.

일반 Chrome에서 위 단일 프로필 시험을 디버깅 파이프 기반의 테스트 전용 확장 로딩으로 실행하려면 `OPENKEYLET_TEST_PIPE_INSTALL=1`을 추가한다. 이 옵션은 브라우저를 조작할 수 있는 디버깅 기능을 켜므로 임시 프로필과 합성 금고 외에는 사용하지 않는다.

테스트는 임시 디렉터리와 localhost 포트를 사용하며, 합성 금고와 프로필 등 진단 자료를 임시 디렉터리에 남긴다. 실제 사용자의 기존 금고나 프로필을 지정하지 않는다.

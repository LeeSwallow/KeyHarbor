# OpenBrowser와 OpenKeylet 핵심 구현 조사

조사일: 2026-10-28. 소스 코드와 프로젝트 문서를 읽은 결과이며, 직접 실행하거나 보안 감사를 수행하지 않았다.

## OpenBrowser: 브라우저와 세션

대상: [OpenBrowser](https://github.com/floomhq/openbrowser/tree/5680ac6fbe1334a20d87a02d1087c95fc3e20a9c), 커밋 `5680ac6fbe1334a20d87a02d1087c95fc3e20a9c`.

- [pool.py](https://github.com/floomhq/openbrowser/blob/5680ac6fbe1334a20d87a02d1087c95fc3e20a9c/ax_browser_broker/pool.py)는 브라우저 슬롯을 에이전트 소유자와 계정에 lease로 할당한다. 파일 잠금으로 할당 상태를 보호하고 heartbeat, 만료 정리, 반환을 지원한다. 동일 identity의 병렬 개수도 제한한다.
- 계정별 Chrome 프로필 디렉터리를 유지한다. 병렬 사용이 허용된 계정은 슬롯별 복제 프로필을 쓰며, 기본 프로필의 쿠키 DB 수정 시간이 더 최신이면 복제를 갱신한다. 이것은 쿠키 변경 시각을 이용한 휴리스틱이지 모든 사이트 저장소나 서버 세션의 일관성을 보장하지 않는다.
- [profiles.py](https://github.com/floomhq/openbrowser/blob/5680ac6fbe1334a20d87a02d1087c95fc3e20a9c/ax_browser_broker/profiles.py)는 일부 잠금·캐시 파일을 제외하고 rsync로 프로필을 복제한다. 목적지 삭제 옵션과 프로세스 종료를 사용하므로 그대로 적용하기보다 데이터 손실 및 실행 중 복제 문제를 따로 설계해야 한다.
- [auth.py](https://github.com/floomhq/openbrowser/blob/5680ac6fbe1334a20d87a02d1087c95fc3e20a9c/ax_browser_broker/auth.py)는 난수 토큰, 만료 시각, 대기·완료 상태를 가진 인증 요청을 만들고 브라우저 제어 인계 또는 VNC 경로를 제공한다. 요청의 완료 표시는 사이트의 로그인 성공을 증명하지 않는다.

## OpenKeylet: 인증 정보 사용

대상: [OpenKeylet](https://github.com/omaekumiko2-create/OpenKeylet/tree/66f3259557477aa6cdc89c416c0664fd31202bc2), 커밋 `66f3259557477aa6cdc89c416c0664fd31202bc2`.

- [mcp.rs](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/src-tauri/src/mcp.rs)는 항목 검색과 `credential_fill`, `http_send` 등 작업을 제공한다. 명시적으로 agent-visible로 설정한 값만 검색 결과에 포함하며, 숨겨진 값은 로컬에서 사용하도록 설계됐다.
- 브라우저 확장과 로컬 서비스는 [browser.rs](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/src-tauri/src/browser.rs)의 localhost 브리지, 페어링, 역할별 토큰으로 연결된다. [보안 문서](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/SECURITY.md)는 자동 완성 시 대상 origin과 프레임을 확인한다고 설명한다.
- [crypto.rs](https://github.com/omaekumiko2-create/OpenKeylet/blob/66f3259557477aa6cdc89c416c0664fd31202bc2/src-tauri/src/crypto.rs)는 XChaCha20-Poly1305를 사용한다. 현재 macOS는 권한을 제한한 로컬 마스터 키 파일을 사용하며, 다른 플랫폼에서는 시스템 keyring을 먼저 시도하고 파일로 대체할 수 있다. 소스의 Argon2id는 PIN 검증 등에 사용된다. PIN은 별도 암호화 키가 아니다.
- 보안 문서는 악의적인 에이전트나 같은 OS 사용자 프로세스를 방어 대상으로 삼지 않는다고 명시한다. 특히 HTTP 저장 URL은 허용 도메인 제한이 아니고 호출자가 다른 목적지를 지정할 수 있다. 일반 `credential_fill`은 현재 포커스된 대상을 신뢰할 수 있는지 증명하지 못한다. 이는 확장 팝업의 origin 검사와 구별해야 한다.

## KeyHarbor에 가져올 핵심

1. **계정과 세션 분리:** 계정은 기존 금고 항목을 참조하고, 작업별 세션은 소유자·계정·프로필·상태를 연결한다. 비밀번호는 세션 메타데이터에 넣지 않는다.
2. **독점 할당:** 같은 프로필의 동시 사용을 잠금으로 막고, 소유자·heartbeat·종료를 관리한다. 첫 버전은 같은 계정의 병렬 복제 대신 충돌을 알린다.
3. **인증 인계:** 에이전트가 인증 대기 상태를 표시하면 사용자가 같은 브라우저에서 로그인하고, 완료 후 작업을 재개한다. 재개 후 사이트별 성공 여부는 별도로 확인한다.
4. **금고 어댑터:** 우선 기존 금고와 연결하는 경계를 정한다. 에이전트에는 비밀번호 값을 반환하지 않고, 대상 사이트와 선택한 계정에 한정해 입력을 요청하는 흐름을 검증한다. OpenKeylet 연동 자체는 아직 검증하지 않았다.
5. **권한 경계:** 요청 계정·대상 origin·작업을 확인하고, 리다이렉트와 프레임 변경을 고려한다. 평문을 모델에 주지 않는 것만으로 악의적인 에이전트, 로컬 프로세스, 로그인된 계정의 오용이 차단되지는 않는다.

## 검증 순서

1. 로컬 브라우저 프로필을 생성하고 사용자 로그인 후 종료·재실행해 테스트 사이트의 상태가 유지되는지 확인한다.
2. 동일 프로필의 중복 할당을 거부하고 서로 다른 프로필이 격리되는지 확인한다.
3. 인증 대기와 같은 세션에서의 재개를 확인한다.
4. 테스트용 계정으로 OpenKeylet 연결을 실험한다. 브라우저 대상 확인, 값의 로그·모델 노출, 페어링과 권한 경계를 점검한다.
5. Codex와 Claude Code의 MCP 연결을 검증하고 실패·재인증 흐름을 기록한다.

세션 복원은 쿠키, 사이트 저장소, passkey, 기기 인증과 서버 만료에 영향을 받는다. 각 항목의 지원 범위는 실제 환경에서 확인해야 한다.

## 라이선스

두 저장소의 확인한 커밋은 MIT 라이선스다. 소스의 실질적인 부분을 복사하거나 배포할 때는 해당 저작권 및 라이선스 고지를 포함해야 한다. 이 문서는 아이디어와 구현 관찰을 정리하며 코드를 복사하지 않았다.

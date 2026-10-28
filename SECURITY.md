# Security

## KeyHarbor가 약속하는 것

- CLI는 비밀 값을 화면·로그·오류 메시지로 출력하지 않는다. 평문 보기는 이후 관리 앱에서만 한다.
- 디스크의 비밀 필드는 모두 암호문이다(XChaCha20-Poly1305). 금고 파일만 복사해 가서는 열 수 없다 — 루트 키는 macOS login keychain에 있다.
- 다른 항목·도메인·금고로 옮긴 암호문, 바뀐 항목 메타데이터(사이트 목록 등)는 사용 전에 거부된다.
- 키 교체가 중간에 끊겨도 데이터를 잃지 않는다.

이 중 하나를 우회하는 방법, 또는 에이전트가 사용자가 허락하지 않은 비밀을 얻는 방법은 보안 문제다.

## 약속하지 않는 것

같은 macOS 사용자 권한으로 실행되는 악성 프로세스, 잠금 해제된 Mac에 대한 물리적 접근, 기기·키 분실 후 복구, 브라우저 프로필 자체의 암호화. 근거와 전체 위협 모델은 [docs/design/03](docs/design/03-security-whitepaper.md)에 있다. 이 설계는 외부 감사를 받지 않았다.

## 신고

공개 이슈로 올리지 말고 비공개로 알려 주세요: GitHub **Security → Report a vulnerability** ([새 advisory](https://github.com/LeeSwallow/KeyHarbor/security/advisories/new)). 일주일 안에 답한다. 수정은 다음 릴리스에 들어가고, 원하지 않는 경우가 아니면 신고자를 advisory에 적는다. 포상금은 없다.

## 지원 버전

최신 릴리스만. 1.0 전이므로 이전 버전에 수정을 백포트하지 않는다.

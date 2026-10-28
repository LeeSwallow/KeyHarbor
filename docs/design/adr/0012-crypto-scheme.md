# ADR-0012. 암호화 방식은 선행 사례의 합의를 따르고, 표준 형식이 있으면 재사용한다

| 항목 | 값 |
|---|---|
| 상태 | Proposed |
| 날짜 | 2026-10-28 |
| 관련 요구 | R-16, R-26 |

## 맥락

사용자는 가능성을 넓히기보다 오픈소스와 best practice를 분석해 암호화를 설계하길 원한다(R-26). 1Password, Bitwarden, KeePass(KDBX 4), age, SOPS, Tink, OpenKeylet을 비교했다([07 §7.2](../07-cryptography-design.md#72-선행-사례-분석)).

## 결정

- AEAD는 XChaCha20-Poly1305(RustCrypto). 무작위 192-bit nonce로 다중 프로세스 동시 암호화를 단순하게 한다.
- 루트 키 RK는 Keychain에 두고, HKDF-SHA256으로 KEK(도메인 키 감싸기)와 MK(메타데이터 MAC)를 나눈다.
- 도메인 키 DK는 무작위로 만들어 KEK로 감싼다(envelope).
- 레코드에 version · suite · key_id를 붙이고, AD에 vault · domain · item · field · 헤더를 길이 접두사로 묶는다.
- 항목 메타데이터(origin 목록 등)에 HMAC-SHA256을 건다.
- 내보내기는 자체 형식 대신 **age 형식(scrypt passphrase)**을 쓴다.

## 결과

- 독자 형식은 필드·DK 레코드와 메타 MAC 입력뿐이며, 모두 07에 바이트 단위로 명세한다.
- KeyHarbor가 없어도 사용자가 `age -d`로 내보내기를 열 수 있다.
- ADR-0003의 키 계층 부분과 03 §3.6.3의 내보내기 형식을 대체한다.

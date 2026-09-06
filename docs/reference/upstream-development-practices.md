# QEMU·UTM 개발 절차 참고 기록

Document status: **Reference**

Last reviewed: 2026-09-06

## 조사 범위

사용자 요청에 따라 공식 테스트·기여 문서를 읽어 개발 작업 단위, 검증, 실험 자료
관리를 비교했다. 아래 “관찰”은 링크된 문서가 설명하는 범위이며 프로젝트의 모든
내부 관행을 조사했다는 뜻은 아니다. 구현 코드나 디바이스 동작을 개발 입력으로
사용하지 않았다. BridgeVM의 독립 구현·출처 정책 및 기존 라이선스는 그대로다.

기존 제품명 검사는 operator 기록 밖의 UTM 이름을 일괄 금지해 이번 요청의 절차
비교도 거부했다. 사용자 요청에 맞춰 이 문서의 정확한 경로만 예외로 추가했다.
runtime과 다른 문서의 제품명 검사는 유지하고, 코드가 다른 제품에서 유래했다는
근거 없는 주장을 탐지하는 별도 검사도 그대로 적용한다. 합성 정책 테스트 4개로
이 문서와 operator 기록만 허용되고 runtime·다른 문서·비슷한 파일명은 거부됨을 확인했다.

## 공식 문서에서 확인한 사항

| 주제 | 공식 문서의 설명 | BridgeVM에 적용할 판단 |
| --- | --- | --- |
| QEMU 테스트 계층 | unit, QTest, block I/O 등 서로 다른 검사를 구분한다 | 변경된 계층의 빠른 검사를 먼저 하고 guest 동작 주장은 별도 live gate로 확인 |
| QEMU 임시 자료 | functional test의 scratch는 기본 정리하며 환경 변수로 디버깅용 보존 가능 | 모든 출력 디스크를 영구 보존할 필요는 없으나 종료·증거 추출·의존성 검증을 선행 |
| QEMU 로그와 입력 | 실행 로그를 별도 저장하고 다운로드 입력을 URL·SHA-256 기반으로 캐시한다 | 재사용 입력, 실행 출력, 원본 증거의 수명과 위치를 분리 |
| QEMU 비용 제어 | quick/thorough 실행과 대용량·장시간 작업의 선택적 실행을 구분한다 | 일상 개발을 무거운 실험과 분리하되 release 표본과 기준은 그대로 유지 |
| QEMU CI | 주로 GitLab에서 검사하고 merge 전에 gate를 적용한다. custom runner 절차도 문서화한다 | 우리는 GitHub-hosted 결정적 검사와 별도 물리 Mac 큐의 기존 경계를 유지 |
| UTM 변경 단위 | 기여 문서는 큰 작업의 사전 논의, 작은 PR 범위, 변경 이유와 관련 이슈 기록을 요구한다 | 실험 가설·검증 결과·결론을 한 작업 단위로 연결 |
| UTM AI 기여 검증 | AI 작성 변경은 PR 전 사람이 실제 검증하고 기기·OS를 명시하도록 요구한다 | 검증 주체와 환경을 정확히 기록하고 자동 검사만으로 사용자 동작을 입증하지 않음 |
| UTM 임시 진단 | 관련 없는 수정과 로그를 피하고 bring-up 전용 진단은 commit 전에 제거하도록 명시한다 | 필요한 진단 결과를 증거로 남긴 뒤 임시 instrumentation 제거 |
| UTM 의존성 | upstream을 고려한 패치 관리와 출처가 명시된 의존성 구성을 설명한다 | 기존 버전·라이선스·패치 출처 기록을 유지하고 수정된 checkout을 캐시로 삭제하지 않음 |

QEMU 테스트 계층의 근거는 [Testing in QEMU](https://www.qemu.org/docs/master/devel/testing/main.html),
scratch·로그·자산 관리의 근거는 [Functional testing with Python](https://www.qemu.org/docs/master/devel/testing/functional.html),
CI 관련 근거는 [Continuous Integration](https://www.qemu.org/docs/master/devel/testing/ci.html)이다.
조회한 현행 QEMU 테스트 문서는 11.1.50 문서로 표시되었다. 오래된
devel/ci.html 페이지의 Avocado 명칭을 현행 체계로 인용하지 않았다.

UTM 관련 근거는 공식 저장소의
[CONTRIBUTING.md](https://github.com/utmapp/UTM/blob/main/CONTRIBUTING.md)다.
UTM의 모든 CI workflow, artifact 만료 기간, 내부 디스크 삭제 정책까지 확인한 것은
아니다. 특히 UTM도 실험 디스크를 특정 기간 뒤 지운다거나 문서만 남긴다고 주장할
근거는 이번 조사에서 확보하지 않았다.

## 그대로 채택하지 않는 부분

QEMU의 scratch 삭제 기본값은 BridgeVM의 canonical Windows 이미지나 사용자
VM에 적용할 근거가 아니다. 공개 다운로드 가능한 테스트 자산과 개인 Windows 미디어는
재배포·복구 조건이 다르다. QEMU 문서의 custom runner 방식을 가져와 공개 저장소에
개인 Mac runner를 붙이지 않는다. 기존 AGENTS의 금지 조항이 유지된다.

다른 프로젝트가 느리거나 불안정한 검사를 선택 실행하도록 제공하더라도, 우리
release gate의 필수 표본을 생략하거나 실패를 무시할 수는 없다. 최종 shipping
증거는 기존 release tier와 해당 SHA의 hosted CI를 따른다.

UTM의 AI 기여 조항은 UTM에 기여할 때의 규칙이다. 이 조사 자체가 BridgeVM에
새로운 사람 승인 gate나 attribution 형식을 자동 도입하지 않는다. BridgeVM에서
적용할 정책 변경은 별도의 명시적 변경으로 검토해야 한다.

## 이번에 적용한 것과 이후 제안

이번 작업에서는 입력과 원본 증거를 보존하고, 종료된 두 캠페인의 대표 사례를
남긴 뒤 출력 복제본을 정리했다. 해시 목록, 파일별 journal, 삭제 전후 검증,
공간 실측과 복구 한계를 본 저장소 문서에서 확인할 수 있게 했다. 추가 요청에 따라
검증한 두 하네스 형식을 대상으로 계획/실행을 분리한 정리 도구와 결정적 테스트도
구현했다. 기존 두 차례 삭제는 일회성 로컬 스크립트로 수행했고, 새 도구의 실물
확인은 삭제 없는 plan 실행으로 구분한다.

향후 개발 흐름으로는 가설/사전 기준 기록 → 관련 deterministic 검사 → 필요한
실제 guest gate → 전체 결과 판정 → 증거 추출 → 보존 검토의 순서를 권한다.
이는 위 관찰과 BridgeVM의 기존 규칙을 종합한 우리의 제안이며, QEMU/UTM이
동일한 순서나 동일한 보존 정책을 사용한다는 진술은 아니다.

실제 운영 절차는 [실험 보존 절차](experiment-retention.md), 이번 수행 결과는
[2026-09-06 기록](../history/windows-hvf/experiment-storage-cleanup-20260906.md)을 참조한다.

# 실험 디스크 정리와 증거 보존 — 2026-09-06

Document status: **Historical evidence**

## 배경과 사용자 요청

개발 폴더는 확인 당시 GitHub main과 같은 `2028ae0084f82df44764e6d7d974f03332691c17`이었고
미커밋 변경이 없었다. 일반 개발 검사도 권한을 허용한 재실행에서 PASS했다.
하지만 내부 가용 공간 약 278 GiB는 T16 v2 캠페인의 320 GiB 요구량보다 작았다.
일반 소스 개발 가능 여부와 대규모 live 실험의 공간 요구를 구분해야 한다.

사용자는 과거 실험을 문서화하고 불필요한 디스크를 정리하도록 요청했고,
이어서 상세 설명은 별도 자산 폴더가 아닌 본 개발 저장소에 두도록 요청했다.
본 문서는 그 정리의 감사 기록이다. 새로운 guest 실행이나 성능 증거가 아니다.

## 실제 변경 범위

| 캠페인 | 입력/목적 | 개별 실행 | 캠페인 결론 | 출력 디스크 정리 | 보존 파일 재검증 |
| --- | --- | --- | --- | --- | --- |
| T16 a40f06e9d0835f324f413371fed57d75 | NVMe warm sequential A/A | 20/20 valid | noise 25.526%, 기준 2.94% 초과, STOP | 19개 삭제, 순번 1 보존 | 1,184개 SHA-256 일치 |
| T15 b930c0b3d0c074d4347c4bcd718fb154 | desktop boot A/A | 20/20 valid | noise 16.260%, 기준 2.54% 초과, STOP | 19개 삭제, 순번 1 보존 | 980개 SHA-256 일치 |

합계 출력 디스크 38개를 삭제하고, 40개 실행의 디스크 외 원본 파일 2,164개를
각각 삭제 전후 해시 대조했다. 작은 파일이라는 이유로 생략하지 않았으며 vars와
봉인된 실행 바이너리도 보존 대상에 포함했다. 대표 디스크는 캠페인 순번 1로
선택했고 성능 측정값에 따라 좋은 결과를 골라 남기지 않았다.

전 실행의 job ID·순번·처리 여부·receipt 해시·입력 이미지 해시·디스크 크기·주요
측정값은 [실행별 공개 목록](experiment-storage-cleanup-20260906.tsv)에 있다.
TSV는 개인 절대 경로와 guest 상태를 포함하지 않는다. 주요 수치는 탐색용이며
전체 raw 측정 및 캠페인 판정은 원본 증거와 아래 기존 계획을 따른다.

## T16의 실험 설계와 보존한 결론

하네스 커밋은 `3ffb66b1608b1a69ad0d2dae579c9bb59f1aadba`다.
baseline/candidate는 같은 바이너리 바이트를 사용했다. workload는
windows-nvme-warm-seq-v1, 파일 512 MiB, 전송 128 KiB, read 5 passes,
measured write 2 passes였다. 10개의 counterbalanced A/A pair가 모두 valid였다.

read throughput 중앙값은 baseline 555.565, candidate 542.926 MiB/s였다.
paired directional delta는 +2.724%, bootstrap 95% 구간은 -25.526%부터
+7.796%였다. 따라서 noise bound 25.526%가 사전 ceiling 2.94%를 넘었으며
PRP-prefix 진단·구현·A/B로 진행할 수 없다는 STOP이 유지된다.
정리 때문에 v1 실패를 v2 성공이나 새 release 증거로 대체하지 않는다.

원래 두 선행 T16 캠페인은 cleanup 및 CRLF marker 문제로 무효였다는 기록도
보존했다. 이번에 이 실패 캠페인의 디스크는 삭제하지 않았다.

## T15의 실험 설계와 보존한 결론

하네스 커밋은 `799dcf345a05dbf8b272354f987fcec8f2b23bd0`이고
측정 바이너리의 소스 커밋은 `d5d5ed9ce7e621a0b603b1195e872d16ec1f5659`다.
20회가 같은 binary/image/vars/renderer/firmware·호스트·OS·AC 전원 조건으로
실행됐다. 두 label의 차이는 코드 차이가 아니므로 측정 변동으로 해석한다.

baseline p50/p95는 26.087/28.354초, candidate p50/p95는 23.346/27.074초였다.
paired median -3.427%, bootstrap 95% 구간 -16.260%부터 +6.119%, noise bound
16.260%로 2.54% 기준을 초과했다. mapped-copy boot A/B를 허용하는 증거가 아니다.

두 캠페인의 원래 결론은
[성능 최적화 계획](../../plans/hvf-perf-optimization-plan.md)에 이미 기록되어 있다.
본 문서는 그 결론을 유지하며 캠페인 통계를 새로 계산했다고 주장하지 않는다.

## 무엇을 근거로 삭제했나

1. queued/running 큐가 비어 있고 관련 VM/worker 프로세스가 없음을 확인했다.
2. 각 job의 완료 시각, result=pass, exit_code=0, receipt valid/pass,
   cleanup_status=0을 확인했다. 이것은 개별 실행 종료를 확인하는 조건이지
   캠페인의 통계적 성공 조건이 아니다.
3. 해당 커밋의 tier 스크립트를 확인했다. T16의 원래 run-hvf-nvme-performance-tier.sh
   102–103행, T15 run-hvf-boot-performance-tier.sh 79행이 입력에서 출력 disk/vars를
   cp -c로 생성한다. 현재 개편된 파일명을 과거 커밋에 소급하지 않았다.
4. source input의 존재와 출력과 다른 inode임을 확인했다. 현재 source 전체 해시는
   새로 계산하지 않았으므로 최신 입력 무결성이 재입증됐다고 표현하지 않는다.
5. T16은 input-manifest/result/raw/done/power-log의 해시를 기존 receipt와 대조했다.
   T15는 input-manifest 해시와 boot timer 원본 존재를 확인했다. 나머지 보존 파일은
   삭제 전 현재 SHA-256을 기록하고 사후 동일성을 검증했다.
6. lab 안의 8 MiB 이하 manifest TSV를 검색해 삭제 출력의 done/running 경로와
   호환 경로를 참조하는 입력이 없는지 확인했다. 다른 형식의 모든 숨은 의존성이나
   외부 스크립트까지 전수 입증한 검사는 아니다.
7. 중첩 Git 저장소를 검사했고 renderer·driver 쪽 미커밋 수정들을 발견했다.
   그 소스 디렉터리는 정리 대상에서 제외했다. job 내부에는 중첩 Git이 없었다.
8. 명시된 media/target.raw 파일만 삭제했다. 절대 경로가 실제 경로와 같은지,
   일반 파일인지, hard link count가 1인지, device/inode/size/mtime/blocks가
   inventory와 같은지 확인한 뒤 파일별 intent/deleted 기록을 디스크에 flush했다.
9. 삭제 후 모든 보존 파일 해시를 다시 확인했다. 일회성 실행 결과 JSON은
   사후 검증까지 끝난 뒤에만 생성했다.

## 공간 실측과 남은 제약

| 구간 | 측정한 가용 공간 증가 | GiB 환산 |
| --- | ---: | ---: |
| T16 출력 19개 정리 | 13,691,113,472 bytes | 약 12.75 |
| T15 출력 19개 정리 | 2,127,450,112 bytes | 약 1.98 |
| 두 구간 증가량 합계 | 15,818,563,584 bytes | 약 14.73 |

T15 사후 시점 가용 공간은 314,632,577,024 bytes, 약 293.02 GiB였다.
320 GiB까지 약 26.98 GiB가 추가로 필요하다. 따라서 공간 부족 문제가 완전히
해결됐다거나 T16 v2를 바로 제출할 수 있다고 주장하지 않는다.

38개 디스크의 명목 크기는 회수 공간이 아니다. APFS 공유 블록 때문에 수 TiB로
표시되는 폴더도 실제 SSD에서 그만큼 독점 점유하지 않는다. 두 측정 구간 사이와
그 이후의 다른 작업·APFS 회수에 따른 변동이 있으므로 최초/최종 df 차이가 위
두 구간 증가량의 합과 정확히 같을 필요도 없다.

별도 이전 단계에서 worker debug 빌드 캐시를 Cargo로 정리해 약 743 MiB가
확보된 기록이 있지만, 위 실험 디스크 38개와 14.73 GiB 합계에는 포함하지 않았다.

## 원본 위치와 감사 자료 식별

로컬 workspace의 docs 아래 cleanup-20260906-t15 및 cleanup-20260906-t16
디렉터리에 private inventory, deletions journal, result JSON이 있다.
원본 evidence는 lab/windows-gpu-and-vm-assets/live-queue/done 아래 해당 job
디렉터리에 그대로 있다. 이동/요약으로 대체하지 않았다.

| 로컬 감사 파일 | SHA-256 |
| --- | --- |
| T15 inventory.json | 682c3265ff1b4316e8bf154c80785025fe9e99758ee8e546c9815b09027be6ec |
| T15 deletions.jsonl | 13bab369c723fc64b43d4b98555e2f9b05fbb83981b27d22fcbd0062cbd27358 |
| T15 result.json | 94e4cf0d35dd19fbf2e2f3f0d1578969d57f8b7823994c519121d958ed92380c |
| T16 inventory.json | fe41e51400a221fca84cf0e9d882cf894f3f94467a09d47b91b9022b40598b18 |
| T16 deletions.jsonl | 7e3348f5e8b7cfe35bef3c741781be68604c2e254d4c62ad29bae7ba7a9d0684 |
| T16 result.json | 08cce923824111d0aec2afd9a0e8725e5892ec98b0faa4e779a41c69217f9103 |

감사 시에는 요약문만 신뢰하지 않고 위 파일의 해시와 inventory에 기록된 원본
해시를 대조한다. 공개 TSV의 receipt 해시도 같은 원본을 식별한다.
개인 경로를 포함한 감사 파일은 Git과 CI artifact 밖에 보존한다.

## 복구 한계와 재실행 순서

삭제된 최종 disk 바이트는 이 문서나 해시로 복원할 수 없다. 휴지통으로 이동한 것이
아니므로 일반 실행 취소도 불가능하다. 최종 전체 disk SHA-256은 이번에 계산하지
않았으며 inventory의 null은 이 사실을 뜻한다. receipt의 image_sha256는 입력
해시이고 T16 final_sha256는 workload 데이터 해시이므로 최종 disk 해시가 아니다.

재실행하려면 공개 TSV로 job을 찾고, 로컬 inventory/input-manifest로 입력을 찾는다.
모든 입력의 현재 해시와 출처를 재검증한 뒤 정확한 하네스/바이너리를 준비한다.
원래 campaign registry와 workload 설정을 따라 새 job을 구성하고 lane별 독립
APFS disk/vars clone을 만든다. 과거 done 경로에 덮어쓰지 않는다. hardware/OS/전원,
표본 수·순서·threshold가 달라지면 동일 실험이라고 부르지 않는다. 원본 상태나
시간적 조건이 재현되지 않으면 새로운 실험으로 기록한다.

## 보존했고 추가 판단이 필요한 자료

canonical image, prepared 및 campaign-inputs의 source disk/vars, 사용자 VM,
vTPM/키, 타사 콘텐츠, 미커밋 renderer/driver 소스, 실패/무효 캠페인과 work/pcdiag는
남겨두었다. 디렉터리 크기만으로 이 자료를 불필요하다고 판정하지 않았다.
추가 확보를 위해서는 새 대상의 input/output 관계와 미해결 오류 조사 필요성을
확인하는 별도 목록이 필요하다. 본 38개 allowlist를 다른 디스크로 확대하지 않는다.

운영 절차는 [실험 보존 절차](../../reference/experiment-retention.md),
다른 프로젝트의 공식 개발 문서에서 참고한 범위는
[개발 체계 비교](../../reference/upstream-development-practices.md)에 정리했다.

## 검증 범위

삭제 대상 38개 부재, 대표 2개 보존, 공개 목록의 40개 receipt 해시, private 원본
2,164개 해시를 확인한다. 문서 링크/분류 및 전체 scripts/check-project.sh 결과는
최종 작업 인수인계에서 보고한다. 추가 사용자 요청으로 만든 반복 사용 정리 도구는
[보존 절차의 도구 설명](../../reference/experiment-retention.md)에 있다. 위 38개
삭제는 새 도구 구현 이전의 일회성 스크립트 결과다. 새 도구 테스트와 실물 plan을
과거 삭제의 실행 도구나 새로운 live gate로 혼동하지 않는다.
runtime 수정, hosted CI 실행 또는 제품 상태 승격은 이번 범위에 포함하지 않는다.

정리 도구의 합성 테스트 13개, 문서 예외 정책 테스트 4개, 공개 목록 40개 receipt
해시·삭제 38개·대표 2개 존재 검사는 통과했다. 새 도구의 실제 T16 plan 실행은
삭제 후보 0개와 보존 파일 1,184개를 보고했다. 새 도구로 실제 디스크를 추가 삭제한
결과는 아니며, destructive apply는 합성 fixture에서 검증했다.

첫 전체 프로젝트 검사는 attribution honesty 단계 하나가 실패했다. 기존 제품명
금지가 요청된 개발 절차 비교의 인덱스 문구를 거부한 결과다. 인덱스는 일반적인
개발 절차 비교로 표기하고, 비교 문서 하나만 명시적 예외로 허용했다. 기존 실패를
성공으로 덮어쓰지 않으며 수정 후 전체 검사 결과를 별도로 기록한다.

수정 후 전체 `scripts/check-project.sh` 재실행은 2026-09-06에 exit 0,
`project check: PASS`로 완료됐다. 정리/정책 self-test가 Python gate에서 실행됐고
문서 분류·링크, structural budgets, attribution, Rust 및 Swift 검사도 통과했다.
shellcheck는 미설치로 생략됐고 shim suite에는 기존 skip 1개가 있다. 새 정리 도구의
실제 환경 검증은 plan까지만 수행했으며, 실제 apply에 대한 검증은 합성 테스트다.
이 결과는 로컬 결정적 검사 결과이고 해당 변경의 hosted CI 결과는 아직 없다.

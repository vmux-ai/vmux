common-open = 열기
common-close = 닫기
common-install = 설치
common-uninstall = 제거
common-update = 업데이트
common-retry = 재시도
common-refresh = 새로고침
common-remove = 제거
common-enable = 활성화
common-disable = 비활성화
common-new = 새로운
common-active = 활성
common-running = 달리기
common-done = 완료
common-failed = 실패
common-installed = 설치됨
common-items = { $count ->
    [one] { $count } 항목
   *[other] { $count } 항목
}
start-title = 시작
start-tagline = 프롬프트 하나. 무엇이든 완료되었습니다.

agents-title = 에이전트
agents-search = ACP 및 CLI 에이전트 검색…
agents-empty = 일치하는 상담원 없음
agents-empty-detail = 이름, 런타임 또는 ACP/CLI을 사용해 보세요.
agents-install-failed = 설치 실패
agents-updating = 업데이트 중…
agents-retrying = 재시도 중…
agents-preparing = 준비 중…

extensions-title = 확장
extensions-search = 검색이 설치되었거나 Chrome Web Store…
extensions-relaunch = 적용하려면 다시 시작하세요.
extensions-empty = 확장 프로그램이 설치되지 않았습니다.
extensions-no-match = 일치하는 확장자가 없습니다.
extensions-empty-detail = 위의 Chrome Web Store을 검색하고 Return을 누르세요.
extensions-no-match-detail = 다른 이름이나 확장 프로그램 ID를 사용해 보세요.
extensions-on = 켜짐
extensions-off = 끄기
extensions-enable-confirm = { $name }을(를) 활성화하시겠습니까?
extensions-enable-permissions = { $name }을 활성화하고 다음을 허용합니다.

lsp-title = 언어 서버
lsp-search = 언어 서버, 린터, 포맷터 검색…
lsp-loading = 카탈로그 로드 중…
lsp-empty = 일치하는 언어 서버가 없습니다.
lsp-empty-detail = 다른 언어, 린터 또는 포맷터를 사용해 보십시오.
lsp-needs = { $tool } 필요
lsp-status-available = 가능
lsp-status-on-path = PATH에서
lsp-status-installing = 설치 중…
lsp-status-installed = 설치됨
lsp-status-outdated = 업데이트 가능
lsp-status-running = 달리기
lsp-status-failed = 실패

spaces-title = 공백
spaces-new-placeholder = 새 스페이스 이름
spaces-empty = 공백 없음
spaces-default-name = 공간 { $number }
spaces-tabs = { $count ->
    [one] 탭 1개
   *[other] { $count } 탭
}
spaces-delete = 공간 삭제

team-title = 팀
team-just-you = 이 공간엔 너뿐이야
team-agents = { $count ->
    [one] 나와 상담사 1명
   *[other] 귀하와 { $count } 상담원
}
team-empty = 아직 여기에 아무도 없습니다
team-you = 당신
team-agent = 대리인

services-title = 백그라운드 서비스
services-processes = { $count ->
    [one] 프로세스 1개
   *[other] { $count } 프로세스
}
services-kill-all = 모두 죽이기
services-not-running = 서비스가 실행되고 있지 않습니다
services-start-with = 다음으로 시작하세요:
services-empty = 활성 프로세스 없음
services-filter = 프로세스 필터링…
services-no-match = 일치하는 프로세스가 없습니다.
services-connected = 연결됨
services-disconnected = 연결이 끊김
services-attached = 첨부
services-kill = 죽여라
services-memory = 메모리
services-size = 크기
services-shell = 쉘

error-title = 오류

history-search = 검색 기록
history-clear-all = 모두 지우기
history-clear-confirm = 모든 기록을 삭제하시겠습니까?
history-clear-warning = 이 작업은 취소할 수 없습니다.
history-cancel = 취소
history-today = 오늘
history-yesterday = 어제
history-days-ago = { $count }일 전
history-day-offset = 일 -{ $count }

settings-title = 설정
settings-loading = 설정 로드 중…
settings-stored = ~/.vmux/settings.ron에 저장됨
settings-other = 기타
settings-software-update = 소프트웨어 업데이트
settings-check-updates = 업데이트 확인
settings-check-updates-hint = 자동 업데이트가 활성화된 경우 실행 시와 매시간 자동으로 확인합니다.
settings-update-unavailable = 이용 불가
settings-update-unavailable-hint = 이 빌드에는 업데이터가 포함되어 있지 않습니다.
settings-update-checking = 확인 중…
settings-update-checking-hint = 업데이트 확인 중…
settings-update-check-again = 다시 확인하세요
settings-update-current = Vmux이(가) 최신 상태입니다.
settings-update-downloading = 다운로드 중…
settings-update-downloading-hint = Vmux { $version } 다운로드 중...
settings-update-installing = 설치 중…
settings-update-installing-hint = Vmux { $version } 설치 중…
settings-update-ready = 업데이트 준비
settings-update-ready-hint = Vmux { $version }이(가) 준비되었습니다. 적용하려면 다시 시작하세요.
settings-update-try-again = 다시 시도
settings-update-failed = 업데이트를 확인할 수 없습니다.
settings-item = 품목
settings-item-number = 항목 { $number }
settings-press-key = 키를 누르세요…
settings-saved = 저장됨
settings-record-key = 새 키 콤보를 기록하려면 클릭하세요.

tray-open-window = 창 열기
tray-close-window = 창 닫기
tray-pause-recording = 녹음 일시 정지
tray-resume-recording = 녹음 재개
tray-finish-recording = 녹음 종료
tray-quit = Vmux 종료

composer-attach-files = 파일 첨부(/upload)
composer-remove-attachment = 첨부파일 삭제

layout-back = 뒤로
layout-forward = 앞으로
layout-reload = 새로고침
layout-bookmark-page = 이 페이지를 북마크에 추가하세요
layout-remove-bookmark = 북마크 삭제
layout-pin-page = 이 페이지를 고정하세요
layout-unpin-page = 이 페이지 고정 해제
layout-manage-extensions = 확장 관리
layout-new-stack = 새로운 스택
layout-close-tab = 탭 닫기
layout-bookmark = 북마크
layout-pin = 핀
layout-new-tab = 새 탭
layout-team = 팀

command-switch-space = 공간 전환…
command-search-ask = 검색하거나 물어보세요…
command-new-tab-placeholder = URL을 검색하거나 입력하거나 터미널…을 선택하세요.
command-placeholder = URL을 입력하거나 탭을 검색하거나 > 명령을 입력하세요.
command-composer-placeholder = 명령의 경우 /를 입력하고 미디어의 경우 @를 입력합니다.
command-send = 보내기(Enter)
command-terminal = 터미널
command-open-terminal = 터미널에서 열기
command-stack = 스택
command-tabs = { $count ->
    [one] 탭 1개
   *[other] { $count } 탭
}
command-prompt = 프롬프트
command-new-tab = 새 탭
command-search = 검색
command-open-value = “{ $value }” 열기
command-search-value = “{ $value }” 검색

schema-appearance = 외관
schema-general = 일반
schema-layout = 레이아웃
schema-layout-detail = 창, 창, 사이드바 및 포커스 링.
schema-agent = 대리인
schema-agent-detail = 상담원 행동 및 도구 권한.
schema-shortcuts = 단축키
schema-shortcuts-detail = 읽기 전용 보기입니다. 바인딩을 변경하려면 settings.ron을 직접 편집하세요.
schema-terminal = 터미널
schema-browser = 브라우저
schema-mode = 모드
schema-mode-detail = 웹 페이지의 색 구성표. 장치는 시스템을 따릅니다.
schema-device = 장치
schema-light = 빛
schema-dark = 어둠
schema-language = 언어
schema-language-detail = ~/.vmux/locales/<tag>.ftl 카탈로그와 일치하는 시스템, en-US, ja 또는 BCP 47 태그를 사용하세요.
schema-auto-update = 자동 업데이트
schema-auto-update-detail = 시작 시 및 매 시간마다 업데이트를 확인하고 설치합니다.
schema-startup-url = 시작 URL
schema-startup-url-detail = 비어 있으면 명령 모음 프롬프트가 열립니다.
schema-search-engine = 검색 엔진
schema-search-engine-detail = 시작 및 명령 모음에서 웹 검색에 사용됩니다.
schema-window = 창
schema-pane = 창
schema-side-sheet = 사이드 시트
schema-focus-ring = 초점 링
schema-run-placement = 실행 배치 재정의 허용
schema-run-placement-detail = 에이전트가 실행 창 모드, 방향 및 앵커를 선택하도록 합니다.
schema-leader = 리더
schema-leader-detail = 코드 단축키에 대한 접두사 키입니다.
schema-chord-timeout = 코드 시간 초과
schema-chord-timeout-detail = 코드 접두어가 만료되기까지의 시간(밀리초)입니다.
schema-bindings = 바인딩
schema-confirm-close = 종료 확인
schema-confirm-close-detail = 실행 중인 프로세스가 있는 터미널을 닫기 전에 메시지를 표시합니다.
schema-default-theme = 기본 테마
schema-default-theme-detail = 테마 목록의 활성 테마 이름입니다.

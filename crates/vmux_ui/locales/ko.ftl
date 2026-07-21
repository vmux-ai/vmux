locale-name = 한국어
common-open = 열기
common-close = 닫기
common-install = 설치
common-uninstall = 제거
common-update = 업데이트
common-retry = 다시 시도
common-refresh = 새로고침
common-remove = 삭제
common-enable = 활성화
common-disable = 비활성화
common-new = 새로 만들기
common-active = 활성
common-running = 실행 중
common-done = 완료
common-failed = 실패
common-installed = 설치됨
common-items = { $count ->
    [one] 항목 { $count }개
   *[other] 항목 { $count }개
}
start-title = 시작
start-tagline = 프롬프트 하나로 무엇이든 완료하세요.

agents-title = 에이전트
agents-search = ACP 및 CLI 에이전트 검색…
agents-empty = 일치하는 에이전트 없음
agents-empty-detail = 이름, 런타임 또는 ACP/CLI로 검색해 보세요.
agents-install-failed = 설치 실패
agents-updating = 업데이트 중…
agents-retrying = 다시 시도 중…
agents-preparing = 준비 중…

extensions-title = 확장 프로그램
extensions-search = 설치된 확장 프로그램 또는 Chrome 웹 스토어 검색…
extensions-relaunch = 다시 실행하여 적용
extensions-empty = 설치된 확장 프로그램 없음
extensions-no-match = 일치하는 확장 프로그램 없음
extensions-empty-detail = 위에서 Chrome 웹 스토어를 검색한 뒤 Return 키를 누르세요.
extensions-no-match-detail = 다른 이름이나 확장 프로그램 ID로 검색해 보세요.
extensions-on = 켜짐
extensions-off = 꺼짐
extensions-enable-confirm = { $name }을(를) 활성화할까요?
extensions-enable-permissions = { $name }을(를) 활성화하고 다음 권한을 허용합니다:

lsp-title = 언어 서버
lsp-search = 언어 서버, 린터, 포매터 검색…
lsp-loading = 카탈로그 불러오는 중…
lsp-empty = 일치하는 언어 서버 없음
lsp-empty-detail = 다른 언어, 린터 또는 포매터로 검색해 보세요.
lsp-needs = { $tool } 필요
lsp-status-available = 사용 가능
lsp-status-on-path = PATH에 있음
lsp-status-installing = 설치 중…
lsp-status-installed = 설치됨
lsp-status-outdated = 업데이트 가능
lsp-status-running = 실행 중
lsp-status-failed = 실패

spaces-title = 스페이스
spaces-new-placeholder = 새 스페이스 이름
spaces-empty = 스페이스 없음
spaces-default-name = 스페이스 { $number }
spaces-tabs = { $count ->
    [one] 탭 1개
   *[other] 탭 { $count }개
}
spaces-delete = 스페이스 삭제

team-title = 팀
team-just-you = 이 스페이스에는 나만 있습니다
team-agents = { $count ->
    [one] 나와 에이전트 1개
   *[other] 나와 에이전트 { $count }개
}
team-empty = 아직 아무도 없습니다
team-you = 나
team-agent = 에이전트

services-title = 백그라운드 서비스
services-processes = { $count ->
    [one] 프로세스 1개
   *[other] 프로세스 { $count }개
}
services-kill-all = 모두 강제 종료
services-not-running = 서비스가 실행 중이 아닙니다
services-start-with = 다음으로 시작:
services-empty = 실행 중인 프로세스 없음
services-filter = 프로세스 필터링…
services-no-match = 일치하는 프로세스 없음
services-connected = 연결됨
services-disconnected = 연결 끊김
services-attached = 연결됨
services-kill = 강제 종료
services-memory = 메모리
services-size = 크기
services-shell = 셸

error-title = 오류

history-search = 기록 검색
history-clear-all = 모두 지우기
history-clear-confirm = 모든 기록을 지울까요?
history-clear-warning = 이 작업은 되돌릴 수 없습니다.
history-cancel = 취소
history-today = 오늘
history-yesterday = 어제
history-days-ago = { $count }일 전
history-day-offset = { $count }일 전

settings-title = 설정
settings-loading = 설정 불러오는 중…
settings-stored = ~/.vmux/settings.ron에 저장됨
settings-other = 기타
settings-software-update = 소프트웨어 업데이트
settings-check-updates = 업데이트 확인
settings-check-updates-hint = 자동 업데이트가 켜져 있으면 실행 시와 매시간 자동으로 확인합니다.
settings-update-unavailable = 사용할 수 없음
settings-update-unavailable-hint = 이 빌드에는 업데이터가 포함되어 있지 않습니다.
settings-update-checking = 확인 중…
settings-update-checking-hint = 업데이트 확인 중…
settings-update-check-again = 다시 확인
settings-update-current = Vmux가 최신 버전입니다.
settings-update-downloading = 다운로드 중…
settings-update-downloading-hint = Vmux { $version } 다운로드 중…
settings-update-installing = 설치 중…
settings-update-installing-hint = Vmux { $version } 설치 중…
settings-update-ready = 업데이트 준비됨
settings-update-ready-hint = Vmux { $version } 준비됨. 다시 시작하면 적용됩니다.
settings-update-try-again = 다시 시도
settings-update-failed = 업데이트를 확인할 수 없습니다.
settings-item = 항목
settings-item-number = 항목 { $number }
settings-press-key = 키를 누르세요…
settings-saved = 저장됨
settings-record-key = 클릭하여 새 키 조합 기록

tray-open-window = 창 열기
tray-close-window = 창 닫기
tray-pause-recording = 녹화 일시 정지
tray-resume-recording = 녹화 다시 시작
tray-finish-recording = 녹화 완료
tray-quit = Vmux 종료

composer-attach-files = 파일 첨부(/upload)
composer-remove-attachment = 첨부 파일 삭제

layout-back = 뒤로
layout-forward = 앞으로
layout-reload = 새로고침
layout-bookmark-page = 이 페이지 북마크
layout-remove-bookmark = 북마크 삭제
layout-pin-page = 이 페이지 고정
layout-unpin-page = 이 페이지 고정 해제
layout-manage-extensions = 확장 프로그램 관리
layout-new-stack = 새 스택
layout-close-tab = 탭 닫기
layout-bookmark = 북마크
layout-pin = 고정
layout-new-tab = 새 탭
layout-team = 팀

command-switch-space = 스페이스 전환…
command-search-ask = 검색 또는 질문…
command-new-tab-placeholder = 검색어나 URL을 입력하거나 터미널 선택…
command-placeholder = URL 입력, 탭 검색 또는 >로 명령 실행…
command-composer-placeholder = /로 명령 입력 또는 @로 미디어 첨부
command-send = 보내기(Enter)
command-terminal = 터미널
command-open-terminal = 터미널에서 열기
command-stack = 스택
command-tabs = { $count ->
    [one] 탭 1개
   *[other] 탭 { $count }개
}
command-prompt = 프롬프트
command-new-tab = 새 탭
command-search = 검색
command-open-value = “{ $value }” 열기
command-search-value = “{ $value }” 검색

schema-appearance = 모양
schema-general = 일반
schema-layout = 레이아웃
schema-layout-detail = 창, 패널, 사이드바, 포커스 링.
schema-agent = 에이전트
schema-agent-detail = 에이전트 동작 및 도구 권한.
schema-shortcuts = 단축키
schema-shortcuts-detail = 읽기 전용 보기입니다. 바인딩을 변경하려면 settings.ron을 직접 편집하세요.
schema-terminal = 터미널
schema-browser = 브라우저
schema-mode = 모드
schema-mode-detail = 웹 페이지 색상 모드입니다. 기기는 시스템 설정을 따릅니다.
schema-device = 기기
schema-light = 밝게
schema-dark = 어둡게
schema-language = 언어
schema-language-detail = 시스템 설정, en-US, ja 또는 일치하는 ~/.vmux/locales/<tag>.ftl 카탈로그가 있는 BCP 47 태그를 사용합니다.
schema-auto-update = 자동 업데이트
schema-auto-update-detail = 실행 시와 매시간 업데이트를 확인하고 설치합니다.
schema-startup-url = 시작 URL
schema-startup-url-detail = 비워 두면 명령 막대 프롬프트가 열립니다.
schema-search-engine = 검색 엔진
schema-search-engine-detail = 시작 화면과 명령 막대에서 웹 검색에 사용됩니다.
schema-window = 창
schema-pane = 패널
schema-side-sheet = 사이드 시트
schema-focus-ring = 포커스 링
schema-run-placement = 실행 위치 재정의 허용
schema-run-placement-detail = 에이전트가 실행 패널 모드, 방향, 기준 위치를 선택하도록 허용합니다.
schema-leader = 리더
schema-leader-detail = 코드 단축키의 접두 키입니다.
schema-chord-timeout = 코드 시간 제한
schema-chord-timeout-detail = 코드 접두가 만료되기 전까지의 시간(밀리초)입니다.
schema-bindings = 바인딩
schema-confirm-close = 닫기 확인
schema-confirm-close-detail = 실행 중인 프로세스가 있는 터미널을 닫기 전에 확인합니다.
schema-default-theme = 기본 테마
schema-default-theme-detail = 테마 목록에서 활성화할 테마 이름입니다.

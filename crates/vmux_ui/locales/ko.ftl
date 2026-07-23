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

tools-title = 도구
tools-search = 패키지, 에이전트, MCP, 언어 도구, 설정 파일 검색…
tools-open = 도구 열기
tools-fold = 도구 접기
tools-unfold = 도구 펼치기
tools-scanning = 로컬 도구 검색 중…
tools-no-installed = 설치된 도구 없음
tools-empty = 일치하는 도구 없음
tools-empty-detail = 패키지를 설치하거나 Stow 방식의 설정 파일 패키지를 추가하세요.
tools-apply = 적용
tools-homebrew = Homebrew
tools-homebrew-sync = 설치된 포뮬러와 애플리케이션은 자동으로 동기화됩니다.
tools-open-brewfile = Brewfile 열기
tools-managed = 관리됨
tools-provider-homebrew-formulae = Homebrew 포뮬러
tools-provider-homebrew-casks = Homebrew 애플리케이션
tools-provider-npm = npm 패키지
tools-provider-acp-agents = ACP 에이전트
tools-provider-language-tools = 언어 도구
tools-provider-mcp-servers = MCP 서버
tools-provider-dotfiles = 설정 파일
tools-status-available = 사용 가능
tools-status-missing = 누락
tools-status-conflict = 충돌
tools-forget = 잊기
tools-manage = 관리
tools-link = 연결
tools-unlink = 연결 해제
tools-import = 가져오기
tools-update-count = { $count ->
    [one] 업데이트 1개
   *[other] 업데이트 { $count }개
}
tools-conflict-count = { $count ->
    [one] 충돌 1개
   *[other] 충돌 { $count }개
}
tools-result-applied = 도구 적용됨
tools-result-imported = 도구 가져옴
tools-result-installed = { $name } 설치됨
tools-result-updated = { $name } 업데이트됨
tools-result-uninstalled = { $name } 제거됨
tools-result-forgotten = { $name } 잊음
tools-result-managed = { $name } 관리 시작됨
tools-result-linked = { $name } 연결됨
tools-result-unlinked = { $name } 연결 해제됨

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

settings-empty = (비어 있음)
settings-none = (없음)

schema-system = 시스템
schema-editor = 편집기
schema-recording = 녹화
schema-radius = 반경
schema-padding = 안쪽 여백
schema-gap = 간격
schema-width = 너비
schema-color = 색상
schema-red = 빨강
schema-green = 초록
schema-blue = 파랑
schema-follow-files = 파일 따라가기
schema-tidy-files = 파일 정리
schema-tidy-files-max = 파일 정리 임계값
schema-tidy-files-auto = 파일 자동 정리
schema-app-providers = 앱 제공자
schema-provider = 제공자
schema-kind = 종류
schema-models = 모델
schema-acp = ACP 에이전트
schema-id = ID
schema-name = 이름
schema-command = 명령
schema-arguments = 인수
schema-environment = 환경 변수
schema-working-directory = 작업 디렉터리
schema-shell = 셸
schema-font-family = 글꼴 모음
schema-startup-directory = 시작 디렉터리
schema-themes = 테마
schema-color-scheme = 색 구성표
schema-font-size = 글꼴 크기
schema-line-height = 줄 높이
schema-cursor-style = 커서 스타일
schema-cursor-blink = 커서 깜박임
schema-custom-themes = 사용자 지정 테마
schema-foreground = 전경
schema-background = 배경
schema-cursor = 커서
schema-ansi-colors = ANSI 색상
schema-keymap = 키맵
schema-explorer = 탐색기
schema-visible = 표시
schema-language-servers = 언어 서버
schema-servers = 서버
schema-language-id = 언어 ID
schema-root-markers = 루트 마커
schema-output-directory = 출력 디렉터리

menu-scene = 장면
menu-layout = 레이아웃
menu-terminal = 터미널
menu-browser = 브라우저
menu-service = 서비스
menu-bookmark = 북마크
menu-edit = 편집

layout-knowledge = 지식
layout-open-knowledge = 지식 열기
layout-open-welcome-knowledge = 지식 시작하기 열기
layout-open-path = { $path } 열기
layout-fold-knowledge = 지식 접기
layout-unfold-knowledge = 지식 펼치기
layout-bookmarks = 북마크
layout-new-folder = 새 폴더
layout-add-to-bookmarks = 북마크에 추가
layout-move-to-bookmarks = 북마크로 이동
layout-stack-number = 스택 { $number }
layout-fold-stack = 스택 접기
layout-unfold-stack = 스택 펼치기
layout-close-stack = 스택 닫기
layout-bookmark-in = { $folder }에 북마크

common-cancel = 취소
common-delete = 삭제
common-save = 저장
common-rename = 이름 변경
common-expand = 펼치기
common-collapse = 접기
common-loading = 로드 중…
common-error = 오류
common-output = 출력
common-pending = 대기 중
common-current = 현재
common-stop = 중지
services-command = Vmux 서비스
services-uptime-seconds = { $seconds }초
services-uptime-minutes = { $minutes }분 { $seconds }초
services-uptime-hours = { $hours }시간 { $minutes }분
services-uptime-days = { $days }일 { $hours }시간

error-page-failed-load = 페이지를 로드하지 못했습니다
error-page-not-found = 페이지를 찾을 수 없습니다
error-unknown-host = 알 수 없는 Vmux 앱 호스트: { $host }

history-title = 방문 기록

command-new-app-chat = 새 { $provider }/{ $model } 채팅(앱)
command-interactive-mode-user = Scene > 대화형 모드 > 사용자
command-interactive-mode-player = Scene > 대화형 모드 > 플레이어
command-minimize-window = Layout > 윈도우 > 최소화
command-toggle-layout = Layout > 레이아웃 > 레이아웃 전환
command-close-tab = Layout > 탭 > 탭 닫기
command-new-task = Layout > 탭 > 새 작업…
command-next-tab = Layout > 탭 > 다음 탭
command-prev-tab = Layout > 탭 > 이전 탭
command-rename-tab = Layout > 탭 > 탭 이름 변경
command-tab-select-1 = Layout > 탭 > 탭 1 선택
command-tab-select-2 = Layout > 탭 > 탭 2 선택
command-tab-select-3 = Layout > 탭 > 탭 3 선택
command-tab-select-4 = Layout > 탭 > 탭 4 선택
command-tab-select-5 = Layout > 탭 > 탭 5 선택
command-tab-select-6 = Layout > 탭 > 탭 6 선택
command-tab-select-7 = Layout > 탭 > 탭 7 선택
command-tab-select-8 = Layout > 탭 > 탭 8 선택
command-tab-select-last = Layout > 탭 > 마지막 탭 선택
command-close-pane = Layout > Pane > Pane 닫기
command-select-pane-left = Layout > Pane > 왼쪽 Pane 선택
command-select-pane-right = Layout > Pane > 오른쪽 Pane 선택
command-select-pane-up = Layout > Pane > 위쪽 Pane 선택
command-select-pane-down = Layout > Pane > 아래쪽 Pane 선택
command-swap-pane-prev = Layout > Pane > 이전 Pane과 바꾸기
command-swap-pane-next = Layout > Pane > 다음 Pane과 바꾸기
command-equalize-pane-size = Layout > Pane > Pane 크기 균등화
command-resize-pane-left = Layout > Pane > Pane 왼쪽으로 크기 조절
command-resize-pane-right = Layout > Pane > Pane 오른쪽으로 크기 조절
command-resize-pane-up = Layout > Pane > Pane 위로 크기 조절
command-resize-pane-down = Layout > Pane > Pane 아래로 크기 조절
command-stack-close = Layout > Stack > Stack 닫기
command-stack-next = Layout > Stack > 다음 Stack
command-stack-previous = Layout > Stack > 이전 Stack
command-stack-reopen = Layout > Stack > 닫은 페이지 다시 열기
command-stack-swap-prev = Layout > Stack > Stack을 왼쪽으로 이동
command-stack-swap-next = Layout > Stack > Stack을 오른쪽으로 이동
command-space-open = Layout > Space > Space
command-terminal-close = Terminal > 터미널 닫기
command-terminal-next = Terminal > 다음 터미널
command-terminal-prev = Terminal > 이전 터미널
command-terminal-clear = Terminal > 터미널 지우기
command-browser-prev-page = Browser > 탐색 > 뒤로
command-browser-next-page = Browser > 탐색 > 앞으로
command-browser-reload = Browser > 탐색 > 새로고침
command-browser-hard-reload = Browser > 탐색 > 강력 새로고침
command-open-in-place = Browser > 열기 > 여기서 열기
command-open-in-new-stack = Browser > 열기 > 새 Stack에서 열기
command-open-in-pane-top = Browser > 열기 > 위쪽 Pane에서 열기
command-open-in-pane-right = Browser > 열기 > 오른쪽 Pane에서 열기
command-open-in-pane-bottom = Browser > 열기 > 아래쪽 Pane에서 열기
command-open-in-pane-left = Browser > 열기 > 왼쪽 Pane에서 열기
command-open-in-new-tab = Browser > 열기 > 새 탭에서 열기
command-open-in-new-space = Browser > 열기 > 새 Space에서 열기
command-browser-zoom-in = Browser > 보기 > 확대
command-browser-zoom-out = Browser > 보기 > 축소
command-browser-zoom-reset = Browser > 보기 > 실제 크기
command-browser-dev-tools = Browser > 보기 > 개발자 도구
command-browser-open-command-bar = Browser > 막대 > 명령 막대
command-browser-open-page-in-command-bar = Browser > 막대 > 페이지 편집
command-browser-open-path-bar = Browser > 막대 > 경로 탐색기
command-browser-open-commands = Browser > 막대 > 명령
command-browser-open-history = Browser > 막대 > 방문 기록
command-service-open = Service > 서비스 모니터 열기
command-bookmark-toggle-active = Bookmark > 페이지 북마크
command-bookmark-pin-active = Bookmark > 페이지 고정

layout-tab = 탭
layout-no-stacks = Stack 없음
layout-loading = 로드 중…
layout-no-markdown-files = Markdown 파일 없음
layout-empty-folder = 빈 폴더
layout-worktree = worktree
layout-folder-name = 폴더 이름
layout-no-pins-bookmarks = 고정 또는 북마크 없음
layout-move-to = { $folder }(으)로 이동
layout-bookmark-current-page = 현재 페이지 북마크
layout-rename-folder = 폴더 이름 변경
layout-remove-folder = 폴더 제거
layout-update-downloading = 업데이트 다운로드 중
layout-update-installing = 업데이트 설치 중…
layout-update-ready = 새 버전 사용 가능
layout-restart-update = 재시작하여 업데이트

agent-preparing = 에이전트 준비 중…
agent-send-all-queued = 대기 중인 프롬프트 모두 지금 보내기(Esc)
agent-send = 보내기(Enter)
agent-ready = 준비되면 시작하세요.
agent-loading-older = 이전 메시지 로드 중…
agent-load-older = 이전 메시지 로드
agent-continued-from = { $source }에서 이어짐
agent-older-context-omitted = 이전 컨텍스트 생략됨
agent-interrupted = 중단됨
agent-allow-tool = { $tool }을(를) 허용할까요?
agent-deny = 거부
agent-allow-always = 항상 허용
agent-allow = 허용
agent-loading-sessions = 세션 로드 중…
agent-no-resumable-sessions = 재개할 수 있는 세션이 없습니다
agent-no-matching-sessions = 일치하는 세션 없음
agent-no-matching-models = 일치하는 모델 없음
agent-choice-help = ↑/↓ 또는 Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = 저장소 폴더 선택
agent-choose-repository-detail = 에이전트가 사용할 로컬 Git 저장소를 선택하세요.
agent-choosing = 선택 중…
agent-choose-folder = 폴더 선택
agent-queued = 대기 중
agent-attached = 첨부됨:
agent-cancel-queued = 대기 중인 프롬프트 취소
agent-resume-queued = 대기 중인 프롬프트 재개
agent-clear-queue = 대기열 지우기
agent-send-all-now = 모두 지금 보내기
agent-choose-option = 위에서 옵션을 선택하세요
agent-loading-media = 미디어 로드 중…
agent-no-matching-media = 일치하는 미디어 없음
agent-prompt-context = 프롬프트 컨텍스트
agent-details = 세부 정보
agent-path = 경로
agent-tool = 도구
agent-server = 서버
agent-bytes = { $count }바이트
agent-worked-for = { $duration } 동안 작업함
agent-worked-for-steps = { $count ->
    [one] { $duration } 동안 작업함 · 1단계
   *[other] { $duration } 동안 작업함 · { $count }단계
}
agent-tool-guardian-review = 가디언 검토
agent-tool-read-files = 파일 읽음
agent-tool-viewed-image = 이미지 봄
agent-tool-used-browser = 브라우저 사용함
agent-tool-searched-files = 파일 검색함
agent-tool-ran-commands = 명령 실행함
agent-thinking = 생각 중
agent-subagent = 하위 에이전트
agent-prompt = 프롬프트
agent-thread = 스레드
agent-parent = 상위
agent-children = 하위
agent-call = 호출
agent-raw-event = 원시 이벤트
agent-plan = 계획
agent-tasks = { $count ->
    [one] 작업 1개
   *[other] 작업 { $count }개
}
agent-edited = 편집됨
agent-reconnecting = 다시 연결 중 { $attempt }/{ $total }
agent-status-running = 실행 중
agent-status-done = 완료
agent-status-failed = 실패
agent-status-pending = 대기 중
agent-slash-attach-files = 파일 첨부
agent-slash-resume-session = 이전 세션 재개
agent-slash-select-model = 모델 선택
agent-slash-continue-cli = 이 세션을 CLI에서 계속하기
agent-session-just-now = 방금 전
agent-session-minutes-ago = { $count }분 전
agent-session-hours-ago = { $count }시간 전
agent-session-days-ago = { $count }일 전
agent-working-working = 작업 중
agent-working-thinking = 생각 중
agent-working-pondering = 숙고 중
agent-working-noodling = 궁리 중
agent-working-percolating = 정리 중
agent-working-conjuring = 구상 중
agent-working-cooking = 조리 중
agent-working-brewing = 빚는 중
agent-working-musing = 사색 중
agent-working-ruminating = 곱씹는 중
agent-working-scheming = 설계 중
agent-working-synthesizing = 종합 중
agent-working-tinkering = 손보는 중
agent-working-churning = 처리 중
agent-working-vibing = 흐름 타는 중
agent-working-simmering = 끓이는 중
agent-working-crafting = 다듬는 중
agent-working-divining = 파악 중
agent-working-mulling = 고민 중
agent-working-spelunking = 깊이 탐색 중

editor-toggle-explorer = 탐색기 토글(Cmd+B)
editor-unsaved = 저장되지 않음
editor-rendered-markdown = 실시간 편집이 가능한 렌더링된 Markdown
editor-note = 메모
editor-source-editor = 소스 편집기
editor-editor = 편집기
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = 정리
editor-always = 항상
editor-unchanged-previews = { $count ->
    [one] ✦ 변경 없는 미리보기 1개
   *[other] ✦ 변경 없는 미리보기 { $count }개
}
editor-open-externally = 외부 앱에서 열기
editor-changed-line = 변경된 줄
editor-go-to-definition = 정의로 이동
editor-find-references = 참조 찾기
editor-references = { $count ->
    [one] 참조 1개
   *[other] 참조 { $count }개
}
editor-lsp-starting = { $server } 시작 중…
editor-lsp-not-installed = { $server } — 설치되지 않음
editor-explorer = 탐색기
editor-open-editors = 열린 편집기
editor-outline = 개요
editor-new-file = 새 파일
editor-new-folder = 새 폴더
editor-delete-confirm = “{ $name }”을(를) 삭제할까요? 이 작업은 되돌릴 수 없습니다.
editor-created-folder = { $name } 폴더를 만들었습니다
editor-created-file = { $name } 파일을 만들었습니다
editor-renamed-to = 이름을 { $name }(으)로 변경했습니다
editor-deleted = { $name } 삭제됨
editor-failed-decode-image = 이미지를 디코딩하지 못했습니다
editor-preview-large-image = 이미지(미리보기에는 너무 큼)
editor-preview-binary = 바이너리
editor-preview-file = 파일

git-status-clean = 깨끗함
git-status-modified = 수정됨
git-status-staged = 스테이징됨
git-status-staged-modified = 스테이징됨*
git-status-untracked = 추적 안 됨
git-status-deleted = 삭제됨
git-status-conflict = 충돌
git-accept-all = ✓ 모두 수락
git-unstage = 스테이징 해제
git-confirm-deny-all = 모두 거부 확인
git-deny-all = ✗ 모두 거부
git-commit-message = 커밋 메시지
git-commit = 커밋({ $count })
git-push = ↑ 푸시
git-loading-diff = diff 로드 중…
git-no-changes = 표시할 변경 사항 없음
git-accept = ✓ 수락
git-deny = ✗ 거부
git-show-unchanged-lines = 변경 없는 줄 { $count }개 표시

terminal-loading = 로드 중…
terminal-runs-when-ready = 준비되면 실행 · Ctrl+C로 지우기 · Esc로 건너뛰기
terminal-booting = 부팅 중
terminal-type-command = 명령 입력 · 준비되면 실행 · Esc로 건너뛰기

setup-tagline-claude = Vmux에서 쓰는 Anthropic의 코딩 에이전트
setup-tagline-codex = Vmux에서 쓰는 OpenAI의 코딩 에이전트
setup-tagline-vibe = Vmux에서 쓰는 Mistral의 코딩 에이전트
setup-install-title = { $name } CLI 설치
setup-homebrew-required = { $command }을(를) 설치하려면 Homebrew가 필요하지만 아직 설정되어 있지 않습니다. Vmux가 먼저 Homebrew를 설치한 다음 { $name }을(를) 설치합니다.
setup-terminal-instructions = 터미널에서 Return을 눌러 시작한 뒤, 요청되면 Mac 암호를 입력하세요.
setup-command-missing = 로컬 { $command } 명령이 아직 설치되지 않아 Vmux가 이 페이지를 열었습니다. 아래 명령을 실행해 설치하세요.
setup-install-failed = 설치가 완료되지 않았습니다. 자세한 내용은 터미널에서 확인한 뒤 다시 시도하세요.
setup-installing = 설치 중…
setup-install-homebrew = Homebrew + { $name } 설치
setup-run-install = 설치 명령 실행
setup-auto-reload = Vmux가 터미널에서 실행하고 { $command }이(가) 준비되면 다시 로드합니다.

debug-title = 디버그
debug-auto-update = 자동 업데이트
debug-simulate-update = 업데이트 사용 가능 상태 시뮬레이션
debug-simulate-download = 다운로드 시뮬레이션
debug-clear-update = 업데이트 지우기
debug-trigger-restart = 재시작 트리거

command-manage-spaces = 스페이스 관리…
command-pane-stack-location = 패널 { $pane } / 스택 { $stack }
command-space-pane-stack-location = { $space } / 패널 { $pane } / 스택 { $stack }
command-terminal-path = 터미널({ $path })
command-group-interactive-mode = 대화형 모드
command-group-window = 창
command-group-tab = 탭
command-group-pane = 패널
command-group-stack = 스택
command-group-space = 스페이스
command-group-navigation = 탐색
command-group-open = 열기
command-group-view = 보기
command-group-bar = 막대

menu-close-vmux = Vmux 닫기

agents-terminal-coding-agent = 터미널 기반 코딩 에이전트

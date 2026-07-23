locale-name = português (Brasil)
common-open = Abrir
common-close = Fechar
common-install = Instalar
common-uninstall = Desinstalar
common-update = Atualizar
common-retry = Tentar de novo
common-refresh = Recarregar
common-remove = Remover
common-enable = Ativar
common-disable = Desativar
common-new = Novo
common-active = ativo
common-running = em execução
common-done = concluído
common-failed = Falhou
common-installed = Instalado
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } itens
}

tools-title = Ferramentas
tools-search = Pesquisar pacotes, agentes, MCP, ferramentas de linguagem e arquivos de configuração…
tools-open = Abrir Ferramentas
tools-fold = Recolher ferramentas
tools-unfold = Expandir ferramentas
tools-scanning = Verificando ferramentas locais…
tools-no-installed = Nenhuma ferramenta instalada
tools-empty = Nenhuma ferramenta correspondente
tools-empty-detail = Instale um pacote ou adicione um pacote de arquivos de configuração no estilo Stow.
tools-apply = Aplicar
tools-homebrew = Homebrew
tools-homebrew-sync = As fórmulas e os aplicativos instalados são sincronizados automaticamente.
tools-open-brewfile = Abrir Brewfile
tools-managed = gerenciado
tools-provider-homebrew-formulae = Fórmulas Homebrew
tools-provider-homebrew-casks = Aplicativos Homebrew
tools-provider-npm = Pacotes npm
tools-provider-acp-agents = Agentes ACP
tools-provider-language-tools = Ferramentas de linguagem
tools-provider-mcp-servers = Servidores MCP
tools-provider-dotfiles = Arquivos de configuração
tools-status-available = Disponível
tools-status-missing = Ausente
tools-status-conflict = Conflito
tools-forget = Esquecer
tools-manage = Gerenciar
tools-link = Vincular
tools-unlink = Desvincular
tools-import = Importar
tools-update-count = { $count ->
    [one] 1 atualização
   *[other] { $count } atualizações
}
tools-conflict-count = { $count ->
    [one] 1 conflito
   *[other] { $count } conflitos
}
tools-result-applied = Ferramentas aplicadas
tools-result-imported = Ferramentas importadas
tools-result-installed = { $name } instalado
tools-result-updated = { $name } atualizado
tools-result-uninstalled = { $name } desinstalado
tools-result-forgotten = { $name } esquecido
tools-result-managed = { $name } agora é gerenciado
tools-result-linked = { $name } vinculado
tools-result-unlinked = { $name } desvinculado
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Sincronize configurações, ferramentas, dotfiles e conhecimento com Git.
vault-sync = Sincronizar
vault-create = Criar
vault-connect = Conectar
vault-private = Repositório privado
vault-public-warning = Repositórios públicos expõem seu conhecimento e configuração.
vault-choose-repository = Escolha um repositório…
vault-empty = vazio
vault-clean = Atualizado
vault-not-connected = Não conectado
vault-change-count = Mudanças: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Início
start-tagline = Um prompt. Tudo pronto.

agents-title = Agentes
agents-search = Buscar agentes ACP e CLI…
agents-empty = Nenhum agente encontrado
agents-empty-detail = Tente um nome, runtime ou ACP/CLI.
agents-install-failed = Falha na instalação
agents-updating = Atualizando…
agents-retrying = Tentando de novo…
agents-preparing = Preparando…

extensions-title = Extensões
extensions-search = Buscar instaladas ou na Chrome Web Store…
extensions-relaunch = Reinicie para aplicar
extensions-empty = Nenhuma extensão instalada
extensions-no-match = Nenhuma extensão encontrada
extensions-empty-detail = Busque na Chrome Web Store acima e pressione Enter.
extensions-no-match-detail = Tente outro nome ou ID de extensão.
extensions-on = Ativada
extensions-off = Desativada
extensions-enable-confirm = Ativar { $name }?
extensions-enable-permissions = Ativar { $name } e permitir:

lsp-title = Servidores de linguagem
lsp-search = Buscar servidores de linguagem, linters, formatadores…
lsp-loading = Carregando catálogo…
lsp-empty = Nenhum servidor de linguagem encontrado
lsp-empty-detail = Tente outra linguagem, linter ou formatador.
lsp-needs = requer { $tool }
lsp-status-available = Disponível
lsp-status-on-path = No PATH
lsp-status-installing = Instalando…
lsp-status-installed = Instalado
lsp-status-outdated = Atualização disponível
lsp-status-running = Em execução
lsp-status-failed = Falhou

spaces-title = Espaços
spaces-new-placeholder = Nome do novo espaço
spaces-empty = Nenhum espaço
spaces-default-name = Espaço { $number }
spaces-tabs = { $count ->
    [one] 1 aba
   *[other] { $count } abas
}
spaces-delete = Excluir espaço

team-title = Equipe
team-just-you = Só você neste espaço
team-agents = { $count ->
    [one] Você e 1 agente
   *[other] Você e { $count } agentes
}
team-empty = Ainda não há ninguém aqui
team-you = Você
team-agent = Agente

services-title = Serviços em segundo plano
services-processes = { $count ->
    [one] 1 processo
   *[other] { $count } processos
}
services-kill-all = Forçar encerramento de todos
services-not-running = O serviço não está em execução
services-start-with = Iniciar com:
services-empty = Nenhum processo ativo
services-filter = Filtrar processos…
services-no-match = Nenhum processo encontrado
services-connected = Conectado
services-disconnected = Desconectado
services-attached = anexado
services-kill = Forçar encerramento
services-memory = Memória
services-size = Tamanho
services-shell = Shell

error-title = Erro

history-search = Buscar no histórico
history-clear-all = Limpar tudo
history-clear-confirm = Limpar todo o histórico?
history-clear-warning = Esta ação não pode ser desfeita.
history-cancel = Cancelar
history-today = Hoje
history-yesterday = Ontem
history-days-ago = há { $count } dias
history-day-offset = Dia -{ $count }

settings-title = Ajustes
settings-loading = Carregando ajustes…
settings-stored = Armazenado em ~/.vmux/settings.ron
settings-other = Outros
settings-software-update = Atualização de software
settings-check-updates = Buscar atualizações
settings-check-updates-hint = Verifica automaticamente ao iniciar e a cada hora quando a atualização automática está ativada.
settings-update-unavailable = Indisponível
settings-update-unavailable-hint = O atualizador não está incluído nesta build.
settings-update-checking = Verificando…
settings-update-checking-hint = Buscando atualizações…
settings-update-check-again = Verificar de novo
settings-update-current = O Vmux está atualizado.
settings-update-downloading = Baixando…
settings-update-downloading-hint = Baixando Vmux { $version }…
settings-update-installing = Instalando…
settings-update-installing-hint = Instalando Vmux { $version }…
settings-update-ready = Atualização pronta
settings-update-ready-hint = Vmux { $version } está pronto. Reinicie para aplicar.
settings-update-try-again = Tentar de novo
settings-update-failed = Não foi possível buscar atualizações.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pressione uma tecla…
settings-saved = Salvo
settings-record-key = Clique para gravar uma nova combinação de teclas

tray-open-window = Abrir janela
tray-close-window = Fechar janela
tray-pause-recording = Pausar gravação
tray-resume-recording = Retomar gravação
tray-finish-recording = Finalizar gravação
tray-quit = Sair do Vmux

composer-attach-files = Anexar arquivos (/upload)
composer-remove-attachment = Remover anexo

layout-back = Voltar
layout-forward = Avançar
layout-reload = Recarregar
layout-bookmark-page = Adicionar esta página aos favoritos
layout-remove-bookmark = Remover dos favoritos
layout-pin-page = Fixar esta página
layout-unpin-page = Desafixar esta página
layout-manage-extensions = Gerenciar extensões
layout-new-stack = Nova pilha
layout-close-tab = Fechar aba
layout-bookmark = Favorito
layout-pin = Fixar
layout-new-tab = Nova aba
layout-team = Equipe

command-switch-space = Trocar de espaço…
command-search-ask = Buscar ou perguntar…
command-new-tab-placeholder = Busque ou digite uma URL, ou selecione Terminal…
command-placeholder = Digite uma URL, busque abas ou use > para comandos…
command-composer-placeholder = Digite / para comandos ou @ para mídia
command-send = Enviar (Enter)
command-terminal = Terminal
command-open-terminal = Abrir no Terminal
command-stack = Pilha
command-tabs = { $count ->
    [one] 1 aba
   *[other] { $count } abas
}
command-prompt = Prompt
command-new-tab = Nova aba
command-search = Buscar
command-open-value = Abrir “{ $value }”
command-search-value = Buscar “{ $value }”

schema-appearance = Aparência
schema-general = Geral
schema-layout = Layout
schema-layout-detail = Janela, painéis, barra lateral e contorno de foco.
schema-agent = Agente
schema-agent-detail = Comportamento do agente e permissões de ferramentas.
schema-shortcuts = Atalhos
schema-shortcuts-detail = Visualização somente leitura. Edite settings.ron diretamente para alterar atalhos.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de cores para páginas web. Dispositivo segue o sistema.
schema-device = Dispositivo
schema-light = Claro
schema-dark = Escuro
schema-language = Idioma
schema-language-detail = Use o sistema, en-US, ja ou qualquer tag BCP 47 com um catálogo ~/.vmux/locales/<tag>.ftl correspondente.
schema-auto-update = Atualização automática
schema-auto-update-detail = Buscar e instalar atualizações ao iniciar e a cada hora.
schema-startup-url = URL inicial
schema-startup-url-detail = Vazio abre o prompt da barra de comandos.
schema-search-engine = Mecanismo de busca
schema-search-engine-detail = Usado para buscas na web a partir do Início e da barra de comandos.
schema-window = Janela
schema-pane = Painel
schema-side-sheet = Folha lateral
schema-focus-ring = Contorno de foco
schema-run-placement = Permitir sobrescrever posicionamento da execução
schema-run-placement-detail = Permite que agentes escolham o modo, a direção e a âncora do painel de execução.
schema-leader = Líder
schema-leader-detail = Tecla de prefixo para atalhos em acorde.
schema-chord-timeout = Tempo limite do acorde
schema-chord-timeout-detail = Milissegundos antes que um prefixo de acorde expire.
schema-bindings = Atalhos
schema-confirm-close = Confirmar fechamento
schema-confirm-close-detail = Perguntar antes de fechar um terminal com um processo em execução.
schema-default-theme = Tema padrão
schema-default-theme-detail = Nome do tema ativo na lista de temas.

settings-empty = (vazio)
settings-none = (nenhum)

schema-system = Sistema
schema-editor = Editor
schema-recording = Gravação
schema-radius = Raio
schema-padding = Preenchimento
schema-gap = Espaçamento
schema-width = Largura
schema-color = Cor
schema-red = Vermelho
schema-green = Verde
schema-blue = Azul
schema-follow-files = Seguir arquivos
schema-tidy-files = Organizar arquivos
schema-tidy-files-max = Limite de organização de arquivos
schema-tidy-files-auto = Organizar arquivos automaticamente
schema-app-providers = Provedores de apps
schema-provider = Provedor
schema-kind = Tipo
schema-models = Modelos
schema-acp = Agentes ACP
schema-id = ID
schema-name = Nome
schema-command = Comando
schema-arguments = Argumentos
schema-environment = Ambiente
schema-working-directory = Diretório de trabalho
schema-shell = Shell
schema-font-family = Família da fonte
schema-startup-directory = Diretório inicial
schema-themes = Temas
schema-color-scheme = Esquema de cores
schema-font-size = Tamanho da fonte
schema-line-height = Altura da linha
schema-cursor-style = Estilo do cursor
schema-cursor-blink = Cursor piscante
schema-custom-themes = Temas personalizados
schema-foreground = Primeiro plano
schema-background = Plano de fundo
schema-cursor = Cursor
schema-ansi-colors = Cores ANSI
schema-keymap = Mapa de teclas
schema-explorer = Explorador
schema-visible = Visível
schema-language-servers = Servidores de linguagem
schema-servers = Servidores
schema-language-id = ID de linguagem
schema-root-markers = Marcadores de raiz
schema-output-directory = Diretório de saída

menu-scene = Cena
menu-layout = Layout
menu-terminal = Terminal
menu-browser = Navegador
menu-service = Serviço
menu-bookmark = Favorito
menu-edit = Editar

layout-knowledge = Conhecimento
layout-open-knowledge = Abrir Conhecimento
layout-open-welcome-knowledge = Abrir Boas-vindas ao Conhecimento
layout-open-path = Abrir { $path }
layout-fold-knowledge = Recolher conhecimento
layout-unfold-knowledge = Expandir conhecimento
layout-bookmarks = Favoritos
layout-new-folder = Nova pasta
layout-add-to-bookmarks = Adicionar aos favoritos
layout-move-to-bookmarks = Mover para favoritos
layout-stack-number = Pilha { $number }
layout-fold-stack = Recolher pilha
layout-unfold-stack = Expandir pilha
layout-close-stack = Fechar pilha
layout-bookmark-in = Adicionar favorito em { $folder }

common-cancel = Cancelar
common-delete = Apagar
common-save = Salvar
common-rename = Renomear
common-expand = Expandir
common-collapse = Recolher
common-loading = Carregando…
common-error = Erro
common-output = Saída
common-pending = Pendente
common-current = atual
common-stop = Parar
services-command = serviço do Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }min { $seconds }s
services-uptime-hours = { $hours }h { $minutes }min
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Falha ao carregar a página
error-page-not-found = Página não encontrada
error-unknown-host = Host de app Vmux desconhecido: { $host }

history-title = Histórico

command-new-app-chat = Novo chat { $provider }/{ $model } (App)
command-interactive-mode-user = Cena > Modo interativo > Usuário
command-interactive-mode-player = Cena > Modo interativo > Player
command-minimize-window = Layout > Janela > Minimizar
command-toggle-layout = Layout > Layout > Alternar layout
command-close-tab = Layout > Aba > Fechar aba
command-new-task = Layout > Aba > Nova tarefa…
command-next-tab = Layout > Aba > Próxima aba
command-prev-tab = Layout > Aba > Aba anterior
command-rename-tab = Layout > Aba > Renomear aba
command-tab-select-1 = Layout > Aba > Selecionar aba 1
command-tab-select-2 = Layout > Aba > Selecionar aba 2
command-tab-select-3 = Layout > Aba > Selecionar aba 3
command-tab-select-4 = Layout > Aba > Selecionar aba 4
command-tab-select-5 = Layout > Aba > Selecionar aba 5
command-tab-select-6 = Layout > Aba > Selecionar aba 6
command-tab-select-7 = Layout > Aba > Selecionar aba 7
command-tab-select-8 = Layout > Aba > Selecionar aba 8
command-tab-select-last = Layout > Aba > Selecionar última aba
command-close-pane = Layout > Painel > Fechar painel
command-select-pane-left = Layout > Painel > Selecionar painel à esquerda
command-select-pane-right = Layout > Painel > Selecionar painel à direita
command-select-pane-up = Layout > Painel > Selecionar painel acima
command-select-pane-down = Layout > Painel > Selecionar painel abaixo
command-swap-pane-prev = Layout > Painel > Trocar com painel anterior
command-swap-pane-next = Layout > Painel > Trocar com próximo painel
command-equalize-pane-size = Layout > Painel > Igualar tamanho dos painéis
command-resize-pane-left = Layout > Painel > Redimensionar painel à esquerda
command-resize-pane-right = Layout > Painel > Redimensionar painel à direita
command-resize-pane-up = Layout > Painel > Redimensionar painel para cima
command-resize-pane-down = Layout > Painel > Redimensionar painel para baixo
command-stack-close = Layout > Pilha > Fechar pilha
command-stack-next = Layout > Pilha > Próxima pilha
command-stack-previous = Layout > Pilha > Pilha anterior
command-stack-reopen = Layout > Pilha > Reabrir página fechada
command-stack-swap-prev = Layout > Pilha > Mover pilha para a esquerda
command-stack-swap-next = Layout > Pilha > Mover pilha para a direita
command-space-open = Layout > Espaço > Espaços
command-terminal-close = Terminal > Fechar terminal
command-terminal-next = Terminal > Próximo terminal
command-terminal-prev = Terminal > Terminal anterior
command-terminal-clear = Terminal > Limpar terminal
command-browser-prev-page = Navegador > Navegação > Voltar
command-browser-next-page = Navegador > Navegação > Avançar
command-browser-reload = Navegador > Navegação > Recarregar
command-browser-hard-reload = Navegador > Navegação > Forçar recarregamento
command-open-in-place = Navegador > Abrir > Abrir aqui
command-open-in-new-stack = Navegador > Abrir > Abrir em nova pilha
command-open-in-pane-top = Navegador > Abrir > Abrir no painel acima
command-open-in-pane-right = Navegador > Abrir > Abrir no painel à direita
command-open-in-pane-bottom = Navegador > Abrir > Abrir no painel abaixo
command-open-in-pane-left = Navegador > Abrir > Abrir no painel à esquerda
command-open-in-new-tab = Navegador > Abrir > Abrir em nova aba
command-open-in-new-space = Navegador > Abrir > Abrir em novo espaço
command-browser-zoom-in = Navegador > Visualização > Ampliar
command-browser-zoom-out = Navegador > Visualização > Reduzir
command-browser-zoom-reset = Navegador > Visualização > Tamanho real
command-browser-dev-tools = Navegador > Visualização > Ferramentas de desenvolvedor
command-browser-open-command-bar = Navegador > Barra > Barra de comandos
command-browser-open-page-in-command-bar = Navegador > Barra > Editar página
command-browser-open-path-bar = Navegador > Barra > Navegador de caminho
command-browser-open-commands = Navegador > Barra > Comandos
command-browser-open-history = Navegador > Barra > Histórico
command-service-open = Serviço > Abrir monitor de serviços
command-bookmark-toggle-active = Favorito > Adicionar página aos favoritos
command-bookmark-pin-active = Favorito > Fixar página

layout-tab = Aba
layout-no-stacks = Nenhuma pilha
layout-loading = Carregando…
layout-no-markdown-files = Nenhum arquivo Markdown
layout-empty-folder = Pasta vazia
layout-worktree = worktree
layout-folder-name = Nome da pasta
layout-no-pins-bookmarks = Nenhum fixado ou favorito
layout-move-to = Mover para { $folder }
layout-bookmark-current-page = Adicionar página atual aos favoritos
layout-rename-folder = Renomear pasta
layout-remove-folder = Remover pasta
layout-update-downloading = Baixando atualização
layout-update-installing = Instalando atualização…
layout-update-ready = Nova versão disponível
layout-restart-update = Reiniciar para atualizar

agent-preparing = Preparando agente…
agent-send-all-queued = Enviar todos os prompts na fila agora (Esc)
agent-send = Enviar (Enter)
agent-ready = Pronto quando você estiver.
agent-loading-older = Carregando mensagens anteriores…
agent-load-older = Carregar mensagens anteriores
agent-continued-from = Continuado de { $source }
agent-older-context-omitted = contexto anterior omitido
agent-interrupted = interrompido
agent-allow-tool = Permitir { $tool }?
agent-deny = Negar
agent-allow-always = Permitir sempre
agent-allow = Permitir
agent-loading-sessions = Carregando sessões…
agent-no-resumable-sessions = Nenhuma sessão retomável encontrada
agent-no-matching-sessions = Nenhuma sessão correspondente
agent-no-matching-models = Nenhum modelo correspondente
agent-choice-help = ↑/↓ ou Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Escolha a pasta do repositório
agent-choose-repository-detail = Selecione o repositório Git local que o agente deve usar.
agent-choosing = Escolhendo…
agent-choose-folder = Escolher pasta
agent-queued = na fila
agent-attached = Anexado:
agent-cancel-queued = Cancelar prompt na fila
agent-resume-queued = Retomar prompts na fila
agent-clear-queue = Limpar fila
agent-send-all-now = enviar todos agora
agent-choose-option = Escolha uma opção acima
agent-loading-media = Carregando mídia…
agent-no-matching-media = Nenhuma mídia correspondente
agent-prompt-context = Contexto do prompt
agent-details = Detalhes
agent-path = Caminho
agent-tool = Ferramenta
agent-server = Servidor
agent-bytes = { $count } bytes
agent-worked-for = Trabalhou por { $duration }
agent-worked-for-steps = { $count ->
    [one] Trabalhou por { $duration } · 1 etapa
   *[other] Trabalhou por { $duration } · { $count } etapas
}
agent-tool-guardian-review = Revisão do Guardian
agent-tool-read-files = Leu arquivos
agent-tool-viewed-image = Visualizou imagem
agent-tool-used-browser = Usou navegador
agent-tool-searched-files = Pesquisou arquivos
agent-tool-ran-commands = Executou comandos
agent-thinking = Pensando
agent-subagent = Subagente
agent-prompt = Prompt
agent-thread = Thread
agent-parent = Pai
agent-children = Filhos
agent-call = Chamada
agent-raw-event = Evento bruto
agent-plan = Plano
agent-tasks = { $count ->
    [one] 1 tarefa
   *[other] { $count } tarefas
}
agent-edited = Editado
agent-reconnecting = Reconectando { $attempt }/{ $total }
agent-status-running = Em execução
agent-status-done = Concluído
agent-status-failed = Falhou
agent-status-pending = Pendente
agent-slash-attach-files = Anexar arquivos
agent-slash-resume-session = Retomar uma sessão anterior
agent-slash-select-model = Selecionar modelo
agent-slash-continue-cli = Continuar esta sessão na CLI
agent-session-just-now = agora mesmo
agent-session-minutes-ago = há { $count }min
agent-session-hours-ago = há { $count }h
agent-session-days-ago = há { $count }d
agent-working-working = Trabalhando
agent-working-thinking = Pensando
agent-working-pondering = Refletindo
agent-working-noodling = Rascunhando ideias
agent-working-percolating = Elaborando
agent-working-conjuring = Conjurando
agent-working-cooking = Cozinhando
agent-working-brewing = Preparando
agent-working-musing = Matutando
agent-working-ruminating = Ruminando
agent-working-scheming = Planejando
agent-working-synthesizing = Sintetizando
agent-working-tinkering = Ajustando
agent-working-churning = Processando
agent-working-vibing = Entrando no clima
agent-working-simmering = Apurando
agent-working-crafting = Criando
agent-working-divining = Decifrando
agent-working-mulling = Considerando
agent-working-spelunking = Explorando

editor-toggle-explorer = Alternar Explorer (Cmd+B)
editor-unsaved = não salvo
editor-rendered-markdown = Markdown renderizado com edição ao vivo
editor-note = Nota
editor-source-editor = Editor de código-fonte
editor-editor = Editor
editor-git-diff = Diff do Git
editor-diff = Diff
editor-tidy = Organizar
editor-always = Sempre
editor-unchanged-previews = { $count ->
    [one] ✦ 1 prévia sem alterações
   *[other] ✦ { $count } prévias sem alterações
}
editor-open-externally = Abrir externamente
editor-changed-line = Linha alterada
editor-go-to-definition = Ir para definição
editor-find-references = Localizar referências
editor-references = { $count ->
    [one] 1 referência
   *[other] { $count } referências
}
editor-lsp-starting = { $server } iniciando…
editor-lsp-not-installed = { $server } — não instalado
editor-explorer = Explorer
editor-open-editors = Editores abertos
editor-outline = Estrutura
editor-new-file = Novo arquivo
editor-new-folder = Nova pasta
editor-delete-confirm = Apagar “{ $name }”? Esta ação não pode ser desfeita.
editor-created-folder = Pasta { $name } criada
editor-created-file = Arquivo { $name } criado
editor-renamed-to = Renomeado para { $name }
editor-deleted = { $name } apagado
editor-failed-decode-image = Falha ao decodificar imagem
editor-preview-large-image = imagem (grande demais para pré-visualizar)
editor-preview-binary = binário
editor-preview-file = arquivo

git-status-clean = limpo
git-status-modified = modificado
git-status-staged = preparado
git-status-staged-modified = preparado*
git-status-untracked = não rastreado
git-status-deleted = apagado
git-status-conflict = conflito
git-accept-all = ✓ aceitar tudo
git-unstage = Remover do stage
git-confirm-deny-all = Confirmar negar tudo
git-deny-all = ✗ negar tudo
git-commit-message = mensagem de commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Carregando diff…
git-no-changes = Nenhuma alteração para mostrar
git-accept = ✓ aceitar
git-deny = ✗ negar
git-show-unchanged-lines = Mostrar { $count } linhas sem alterações

terminal-loading = Carregando…
terminal-runs-when-ready = executa quando estiver pronto · Ctrl+C limpa · Esc pula
terminal-booting = inicializando
terminal-type-command = digite um comando · executa quando estiver pronto · Esc pula

setup-tagline-claude = Agente de código da Anthropic, no Vmux
setup-tagline-codex = Agente de código da OpenAI, no Vmux
setup-tagline-vibe = Agente de código da Mistral, no Vmux
setup-install-title = Instalar CLI do { $name }
setup-homebrew-required = O Homebrew é necessário para instalar { $command } e ainda não está configurado. O Vmux instalará o Homebrew primeiro e depois { $name }.
setup-terminal-instructions = No terminal, pressione Return para começar e informe sua senha do Mac quando solicitado.
setup-command-missing = O Vmux abriu esta página porque o comando local { $command } ainda não está instalado. Execute o comando abaixo para obtê-lo.
setup-install-failed = A instalação não foi concluída. Confira os detalhes no terminal e tente novamente.
setup-installing = Instalando…
setup-install-homebrew = Instalar Homebrew + { $name }
setup-run-install = Executar comando de instalação
setup-auto-reload = O Vmux executa isso em um terminal e recarrega quando { $command } estiver pronto.

debug-title = Depuração
debug-auto-update = Atualização automática
debug-simulate-update = Simular atualização disponível
debug-simulate-download = Simular download
debug-clear-update = Limpar atualização
debug-trigger-restart = Acionar reinício

command-manage-spaces = Gerenciar espaços…
command-pane-stack-location = painel { $pane } / pilha { $stack }
command-space-pane-stack-location = { $space } / painel { $pane } / pilha { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Modo interativo
command-group-window = Janela
command-group-tab = Aba
command-group-pane = Painel
command-group-stack = Pilha
command-group-space = Espaço
command-group-navigation = Navegação
command-group-open = Abrir
command-group-view = Visualização
command-group-bar = Barra

menu-close-vmux = Fechar o Vmux

agents-terminal-coding-agent = Agente de programação no terminal

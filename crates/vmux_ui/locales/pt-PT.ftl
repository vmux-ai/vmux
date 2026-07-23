locale-name = português europeu
common-open = Abrir
common-close = Fechar
common-install = Instalar
common-uninstall = Desinstalar
common-update = Atualizar
common-retry = Tentar novamente
common-refresh = Atualizar
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
tools-search = Procurar pacotes, agentes, MCP, ferramentas de linguagem e ficheiros de configuração…
tools-open = Abrir Ferramentas
tools-fold = Recolher ferramentas
tools-unfold = Expandir ferramentas
tools-scanning = A analisar ferramentas locais…
tools-no-installed = Nenhuma ferramenta instalada
tools-empty = Nenhuma ferramenta correspondente
tools-empty-detail = Instale um pacote ou adicione um pacote de ficheiros de configuração ao estilo Stow.
tools-apply = Aplicar
tools-homebrew = Homebrew
tools-homebrew-sync = As fórmulas e aplicações instaladas são sincronizadas automaticamente.
tools-open-brewfile = Abrir Brewfile
tools-managed = gerido
tools-provider-homebrew-formulae = Fórmulas Homebrew
tools-provider-homebrew-casks = Aplicações Homebrew
tools-provider-npm = Pacotes npm
tools-provider-acp-agents = Agentes ACP
tools-provider-language-tools = Ferramentas de linguagem
tools-provider-mcp-servers = Servidores MCP
tools-provider-dotfiles = Ficheiros de configuração
tools-status-available = Disponível
tools-status-missing = Em falta
tools-status-conflict = Conflito
tools-forget = Esquecer
tools-manage = Gerir
tools-link = Associar
tools-unlink = Desassociar
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
tools-result-managed = { $name } é agora gerido
tools-result-linked = { $name } associado
tools-result-unlinked = { $name } desassociado

start-title = Início
start-tagline = Um prompt. Tudo tratado.

agents-title = Agentes
agents-search = Pesquisar agentes ACP e CLI…
agents-empty = Nenhum agente correspondente
agents-empty-detail = Experimente um nome, runtime ou ACP/CLI.
agents-install-failed = Falha na instalação
agents-updating = A atualizar…
agents-retrying = A tentar novamente…
agents-preparing = A preparar…

extensions-title = Extensões
extensions-search = Pesquisar instaladas ou na Chrome Web Store…
extensions-relaunch = Reiniciar para aplicar
extensions-empty = Nenhuma extensão instalada
extensions-no-match = Nenhuma extensão correspondente
extensions-empty-detail = Pesquise na Chrome Web Store acima e prima Enter.
extensions-no-match-detail = Experimente outro nome ou ID de extensão.
extensions-on = Ativado
extensions-off = Desativado
extensions-enable-confirm = Ativar { $name }?
extensions-enable-permissions = Ativar { $name } e permitir:

lsp-title = Servidores de linguagem
lsp-search = Pesquisar servidores de linguagem, linters, formatadores…
lsp-loading = A carregar catálogo…
lsp-empty = Nenhum servidor de linguagem correspondente
lsp-empty-detail = Experimente outra linguagem, linter ou formatador.
lsp-needs = precisa de { $tool }
lsp-status-available = Disponível
lsp-status-on-path = No PATH
lsp-status-installing = A instalar…
lsp-status-installed = Instalado
lsp-status-outdated = Atualização disponível
lsp-status-running = Em execução
lsp-status-failed = Falhou

spaces-title = Espaços
spaces-new-placeholder = Nome do novo espaço
spaces-empty = Sem espaços
spaces-default-name = Espaço { $number }
spaces-tabs = { $count ->
    [one] 1 separador
   *[other] { $count } separadores
}
spaces-delete = Eliminar espaço

team-title = Equipa
team-just-you = Só você neste espaço
team-agents = { $count ->
    [one] Você e 1 agente
   *[other] Você e { $count } agentes
}
team-empty = Ainda não está cá ninguém
team-you = Você
team-agent = Agente

services-title = Serviços em segundo plano
services-processes = { $count ->
    [one] 1 processo
   *[other] { $count } processos
}
services-kill-all = Terminar tudo
services-not-running = O serviço não está em execução
services-start-with = Iniciar com:
services-empty = Nenhum processo ativo
services-filter = Filtrar processos…
services-no-match = Nenhum processo correspondente
services-connected = Ligado
services-disconnected = Desligado
services-attached = anexado
services-kill = Terminar
services-memory = Memória
services-size = Tamanho
services-shell = Shell

error-title = Erro

history-search = Pesquisar histórico
history-clear-all = Limpar tudo
history-clear-confirm = Limpar todo o histórico?
history-clear-warning = Esta ação não pode ser anulada.
history-cancel = Cancelar
history-today = Hoje
history-yesterday = Ontem
history-days-ago = Há { $count } dias
history-day-offset = Dia -{ $count }

settings-title = Definições
settings-loading = A carregar definições…
settings-stored = Guardado em ~/.vmux/settings.ron
settings-other = Outros
settings-software-update = Atualização de software
settings-check-updates = Procurar atualizações
settings-check-updates-hint = Verifica automaticamente ao iniciar e de hora a hora quando a atualização automática está ativada.
settings-update-unavailable = Indisponível
settings-update-unavailable-hint = O atualizador não está incluído nesta compilação.
settings-update-checking = A verificar…
settings-update-checking-hint = A procurar atualizações…
settings-update-check-again = Verificar novamente
settings-update-current = O Vmux está atualizado.
settings-update-downloading = A descarregar…
settings-update-downloading-hint = A descarregar Vmux { $version }…
settings-update-installing = A instalar…
settings-update-installing-hint = A instalar Vmux { $version }…
settings-update-ready = Atualização pronta
settings-update-ready-hint = O Vmux { $version } está pronto. Reinicie para aplicar.
settings-update-try-again = Tentar novamente
settings-update-failed = Não foi possível procurar atualizações.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Prima uma tecla…
settings-saved = Guardado
settings-record-key = Clique para gravar uma nova combinação de teclas

tray-open-window = Abrir janela
tray-close-window = Fechar janela
tray-pause-recording = Pausar gravação
tray-resume-recording = Retomar gravação
tray-finish-recording = Terminar gravação
tray-quit = Sair do Vmux

composer-attach-files = Anexar ficheiros (/upload)
composer-remove-attachment = Remover anexo

layout-back = Recuar
layout-forward = Avançar
layout-reload = Recarregar
layout-bookmark-page = Adicionar esta página aos marcadores
layout-remove-bookmark = Remover marcador
layout-pin-page = Fixar esta página
layout-unpin-page = Desafixar esta página
layout-manage-extensions = Gerir extensões
layout-new-stack = Nova pilha
layout-close-tab = Fechar separador
layout-bookmark = Marcador
layout-pin = Fixar
layout-new-tab = Novo separador
layout-team = Equipa

command-switch-space = Mudar de espaço…
command-search-ask = Pesquisar ou perguntar…
command-new-tab-placeholder = Pesquise ou escreva um URL, ou selecione Terminal…
command-placeholder = Escreva um URL, pesquise separadores ou use > para comandos…
command-composer-placeholder = Escreva / para comandos ou @ para multimédia
command-send = Enviar (Enter)
command-terminal = Terminal
command-open-terminal = Abrir no Terminal
command-stack = Pilha
command-tabs = { $count ->
    [one] 1 separador
   *[other] { $count } separadores
}
command-prompt = Prompt
command-new-tab = Novo separador
command-search = Pesquisar
command-open-value = Abrir “{ $value }”
command-search-value = Pesquisar “{ $value }”

schema-appearance = Aparência
schema-general = Geral
schema-layout = Disposição
schema-layout-detail = Janela, painéis, barra lateral e anel de foco.
schema-agent = Agente
schema-agent-detail = Comportamento do agente e permissões das ferramentas.
schema-shortcuts = Atalhos
schema-shortcuts-detail = Vista só de leitura. Edite settings.ron diretamente para alterar atalhos.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de cores para páginas web. Dispositivo segue o sistema.
schema-device = Dispositivo
schema-light = Claro
schema-dark = Escuro
schema-language = Idioma
schema-language-detail = Use o sistema, en-US, ja ou qualquer etiqueta BCP 47 com um catálogo ~/.vmux/locales/<tag>.ftl correspondente.
schema-auto-update = Atualização automática
schema-auto-update-detail = Procurar e instalar atualizações ao iniciar e de hora a hora.
schema-startup-url = URL de arranque
schema-startup-url-detail = Se estiver vazio, abre o prompt da barra de comandos.
schema-search-engine = Motor de pesquisa
schema-search-engine-detail = Usado para pesquisas web a partir do Início e da barra de comandos.
schema-window = Janela
schema-pane = Painel
schema-side-sheet = Painel lateral
schema-focus-ring = Anel de foco
schema-run-placement = Permitir substituir o posicionamento da execução
schema-run-placement-detail = Permitir que os agentes escolham o modo, a direção e a âncora do painel de execução.
schema-leader = Líder
schema-leader-detail = Tecla de prefixo para atalhos em acorde.
schema-chord-timeout = Tempo limite do acorde
schema-chord-timeout-detail = Milissegundos até expirar um prefixo de acorde.
schema-bindings = Atalhos
schema-confirm-close = Confirmar fecho
schema-confirm-close-detail = Perguntar antes de fechar um terminal com um processo em execução.
schema-default-theme = Tema predefinido
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
schema-follow-files = Seguir ficheiros
schema-tidy-files = Arrumar ficheiros
schema-tidy-files-max = Limiar de arrumação de ficheiros
schema-tidy-files-auto = Arrumar ficheiros automaticamente
schema-app-providers = Fornecedores de apps
schema-provider = Fornecedor
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
schema-font-family = Família tipográfica
schema-startup-directory = Diretório de arranque
schema-themes = Temas
schema-color-scheme = Esquema de cores
schema-font-size = Tamanho da letra
schema-line-height = Altura da linha
schema-cursor-style = Estilo do cursor
schema-cursor-blink = Intermitência do cursor
schema-custom-themes = Temas personalizados
schema-foreground = Primeiro plano
schema-background = Fundo
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
menu-layout = Disposição
menu-terminal = Terminal
menu-browser = Navegador
menu-service = Serviço
menu-bookmark = Marcador
menu-edit = Edição

layout-knowledge = Conhecimento
layout-open-knowledge = Abrir Conhecimento
layout-open-welcome-knowledge = Abrir Boas-vindas ao Conhecimento
layout-open-path = Abrir { $path }
layout-fold-knowledge = Recolher conhecimento
layout-unfold-knowledge = Expandir conhecimento
layout-bookmarks = Marcadores
layout-new-folder = Nova pasta
layout-add-to-bookmarks = Adicionar aos Marcadores
layout-move-to-bookmarks = Mover para os Marcadores
layout-stack-number = Pilha { $number }
layout-fold-stack = Recolher pilha
layout-unfold-stack = Expandir pilha
layout-close-stack = Fechar pilha
layout-bookmark-in = Marcar em { $folder }

common-cancel = Cancelar
common-delete = Apagar
common-save = Guardar
common-rename = Mudar o nome
common-expand = Expandir
common-collapse = Recolher
common-loading = A carregar…
common-error = Erro
common-output = Saída
common-pending = Pendente
common-current = atual
common-stop = Parar
services-command = Serviço Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Não foi possível carregar a página
error-page-not-found = Página não encontrada
error-unknown-host = Host de app Vmux desconhecido: { $host }

history-title = Histórico

command-new-app-chat = Nova conversa { $provider }/{ $model } (App)
command-interactive-mode-user = Cena > Modo interativo > Utilizador
command-interactive-mode-player = Cena > Modo interativo > Player
command-minimize-window = Disposição > Janela > Minimizar
command-toggle-layout = Disposição > Disposição > Alternar disposição
command-close-tab = Disposição > Separador > Fechar separador
command-new-task = Disposição > Separador > Nova tarefa…
command-next-tab = Disposição > Separador > Separador seguinte
command-prev-tab = Disposição > Separador > Separador anterior
command-rename-tab = Disposição > Separador > Mudar nome do separador
command-tab-select-1 = Disposição > Separador > Selecionar separador 1
command-tab-select-2 = Disposição > Separador > Selecionar separador 2
command-tab-select-3 = Disposição > Separador > Selecionar separador 3
command-tab-select-4 = Disposição > Separador > Selecionar separador 4
command-tab-select-5 = Disposição > Separador > Selecionar separador 5
command-tab-select-6 = Disposição > Separador > Selecionar separador 6
command-tab-select-7 = Disposição > Separador > Selecionar separador 7
command-tab-select-8 = Disposição > Separador > Selecionar separador 8
command-tab-select-last = Disposição > Separador > Selecionar último separador
command-close-pane = Disposição > Painel > Fechar painel
command-select-pane-left = Disposição > Painel > Selecionar painel à esquerda
command-select-pane-right = Disposição > Painel > Selecionar painel à direita
command-select-pane-up = Disposição > Painel > Selecionar painel acima
command-select-pane-down = Disposição > Painel > Selecionar painel abaixo
command-swap-pane-prev = Disposição > Painel > Trocar com painel anterior
command-swap-pane-next = Disposição > Painel > Trocar com painel seguinte
command-equalize-pane-size = Disposição > Painel > Igualar tamanho dos painéis
command-resize-pane-left = Disposição > Painel > Redimensionar painel para a esquerda
command-resize-pane-right = Disposição > Painel > Redimensionar painel para a direita
command-resize-pane-up = Disposição > Painel > Redimensionar painel para cima
command-resize-pane-down = Disposição > Painel > Redimensionar painel para baixo
command-stack-close = Disposição > Pilha > Fechar pilha
command-stack-next = Disposição > Pilha > Pilha seguinte
command-stack-previous = Disposição > Pilha > Pilha anterior
command-stack-reopen = Disposição > Pilha > Reabrir página fechada
command-stack-swap-prev = Disposição > Pilha > Mover pilha para a esquerda
command-stack-swap-next = Disposição > Pilha > Mover pilha para a direita
command-space-open = Disposição > Espaço > Espaços
command-terminal-close = Terminal > Fechar terminal
command-terminal-next = Terminal > Terminal seguinte
command-terminal-prev = Terminal > Terminal anterior
command-terminal-clear = Terminal > Limpar terminal
command-browser-prev-page = Navegador > Navegação > Recuar
command-browser-next-page = Navegador > Navegação > Avançar
command-browser-reload = Navegador > Navegação > Recarregar
command-browser-hard-reload = Navegador > Navegação > Recarregamento forçado
command-open-in-place = Navegador > Abrir > Abrir aqui
command-open-in-new-stack = Navegador > Abrir > Abrir em nova pilha
command-open-in-pane-top = Navegador > Abrir > Abrir no painel acima
command-open-in-pane-right = Navegador > Abrir > Abrir no painel à direita
command-open-in-pane-bottom = Navegador > Abrir > Abrir no painel abaixo
command-open-in-pane-left = Navegador > Abrir > Abrir no painel à esquerda
command-open-in-new-tab = Navegador > Abrir > Abrir em novo separador
command-open-in-new-space = Navegador > Abrir > Abrir em novo espaço
command-browser-zoom-in = Navegador > Visualização > Ampliar
command-browser-zoom-out = Navegador > Visualização > Reduzir
command-browser-zoom-reset = Navegador > Visualização > Tamanho real
command-browser-dev-tools = Navegador > Visualização > Ferramentas de programador
command-browser-open-command-bar = Navegador > Barra > Barra de comandos
command-browser-open-page-in-command-bar = Navegador > Barra > Editar página
command-browser-open-path-bar = Navegador > Barra > Navegador de caminho
command-browser-open-commands = Navegador > Barra > Comandos
command-browser-open-history = Navegador > Barra > Histórico
command-service-open = Serviço > Abrir monitor de serviços
command-bookmark-toggle-active = Marcador > Marcar página
command-bookmark-pin-active = Marcador > Fixar página

layout-tab = Separador
layout-no-stacks = Sem pilhas
layout-loading = A carregar…
layout-no-markdown-files = Sem ficheiros Markdown
layout-empty-folder = Pasta vazia
layout-worktree = árvore de trabalho
layout-folder-name = Nome da pasta
layout-no-pins-bookmarks = Sem páginas fixadas nem marcadores
layout-move-to = Mover para { $folder }
layout-bookmark-current-page = Marcar página atual
layout-rename-folder = Mudar nome da pasta
layout-remove-folder = Remover pasta
layout-update-downloading = A descarregar atualização
layout-update-installing = A instalar atualização…
layout-update-ready = Nova versão disponível
layout-restart-update = Reiniciar para atualizar

agent-preparing = A preparar agente…
agent-send-all-queued = Enviar agora todos os pedidos em fila (Esc)
agent-send = Enviar (Enter)
agent-ready = Pronto quando estiver.
agent-loading-older = A carregar mensagens mais antigas…
agent-load-older = Carregar mensagens mais antigas
agent-continued-from = Continuação de { $source }
agent-older-context-omitted = contexto antigo omitido
agent-interrupted = interrompido
agent-allow-tool = Permitir { $tool }?
agent-deny = Recusar
agent-allow-always = Permitir sempre
agent-allow = Permitir
agent-loading-sessions = A carregar sessões…
agent-no-resumable-sessions = Não foram encontradas sessões retomáveis
agent-no-matching-sessions = Sem sessões correspondentes
agent-no-matching-models = Sem modelos correspondentes
agent-choice-help = ↑/↓ ou Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Escolher pasta do repositório
agent-choose-repository-detail = Selecione o repositório Git local que o agente deve usar.
agent-choosing = A escolher…
agent-choose-folder = Escolher pasta
agent-queued = em fila
agent-attached = Anexado:
agent-cancel-queued = Cancelar pedido em fila
agent-resume-queued = Retomar pedidos em fila
agent-clear-queue = Limpar fila
agent-send-all-now = enviar tudo agora
agent-choose-option = Escolha uma opção acima
agent-loading-media = A carregar multimédia…
agent-no-matching-media = Sem multimédia correspondente
agent-prompt-context = Contexto do pedido
agent-details = Detalhes
agent-path = Caminho
agent-tool = Ferramenta
agent-server = Servidor
agent-bytes = { $count } bytes
agent-worked-for = Trabalhou durante { $duration }
agent-worked-for-steps = { $count ->
    [one] Trabalhou durante { $duration } · 1 passo
   *[other] Trabalhou durante { $duration } · { $count } passos
}
agent-tool-guardian-review = Revisão do Guardian
agent-tool-read-files = Leu ficheiros
agent-tool-viewed-image = Viu imagem
agent-tool-used-browser = Usou navegador
agent-tool-searched-files = Pesquisou ficheiros
agent-tool-ran-commands = Executou comandos
agent-thinking = A pensar
agent-subagent = Subagente
agent-prompt = Pedido
agent-thread = Conversa
agent-parent = Principal
agent-children = Subordinados
agent-call = Chamada
agent-raw-event = Evento bruto
agent-plan = Plano
agent-tasks = { $count ->
    [one] 1 tarefa
   *[other] { $count } tarefas
}
agent-edited = Editado
agent-reconnecting = A religar { $attempt }/{ $total }
agent-status-running = Em execução
agent-status-done = Concluído
agent-status-failed = Falhou
agent-status-pending = Pendente
agent-slash-attach-files = Anexar ficheiros
agent-slash-resume-session = Retomar uma sessão anterior
agent-slash-select-model = Selecionar modelo
agent-slash-continue-cli = Continuar esta sessão na CLI
agent-session-just-now = agora mesmo
agent-session-minutes-ago = há { $count }m
agent-session-hours-ago = há { $count }h
agent-session-days-ago = há { $count }d
agent-working-working = A trabalhar
agent-working-thinking = A pensar
agent-working-pondering = A ponderar
agent-working-noodling = A explorar ideias
agent-working-percolating = A maturar ideias
agent-working-conjuring = A engendrar
agent-working-cooking = A cozinhar
agent-working-brewing = A preparar
agent-working-musing = A refletir
agent-working-ruminating = A ruminar
agent-working-scheming = A arquitetar
agent-working-synthesizing = A sintetizar
agent-working-tinkering = A ajustar
agent-working-churning = A processar
agent-working-vibing = A entrar no ritmo
agent-working-simmering = Em lume brando
agent-working-crafting = A criar
agent-working-divining = A adivinhar
agent-working-mulling = A matutar
agent-working-spelunking = A explorar a fundo

editor-toggle-explorer = Alternar Explorador (Cmd+B)
editor-unsaved = não guardado
editor-rendered-markdown = Markdown renderizado com edição em tempo real
editor-note = Nota
editor-source-editor = Editor de código-fonte
editor-editor = Editor
editor-git-diff = Diferenças Git
editor-diff = Diferenças
editor-tidy = Arrumar
editor-always = Sempre
editor-unchanged-previews = { $count ->
    [one] ✦ 1 pré-visualização sem alterações
   *[other] ✦ { $count } pré-visualizações sem alterações
}
editor-open-externally = Abrir externamente
editor-changed-line = Linha alterada
editor-go-to-definition = Ir para a definição
editor-find-references = Encontrar referências
editor-references = { $count ->
    [one] 1 referência
   *[other] { $count } referências
}
editor-lsp-starting = { $server } a iniciar…
editor-lsp-not-installed = { $server } — não instalado
editor-explorer = Explorador
editor-open-editors = Editores abertos
editor-outline = Estrutura
editor-new-file = Novo ficheiro
editor-new-folder = Nova pasta
editor-delete-confirm = Apagar “{ $name }”? Esta ação não pode ser anulada.
editor-created-folder = Pasta { $name } criada
editor-created-file = Ficheiro { $name } criado
editor-renamed-to = Nome alterado para { $name }
editor-deleted = { $name } apagado
editor-failed-decode-image = Não foi possível descodificar a imagem
editor-preview-large-image = imagem (demasiado grande para pré-visualizar)
editor-preview-binary = binário
editor-preview-file = ficheiro

git-status-clean = limpo
git-status-modified = modificado
git-status-staged = preparado
git-status-staged-modified = preparado*
git-status-untracked = não seguido
git-status-deleted = apagado
git-status-conflict = conflito
git-accept-all = ✓ aceitar tudo
git-unstage = Remover da preparação
git-confirm-deny-all = Confirmar recusar tudo
git-deny-all = ✗ recusar tudo
git-commit-message = mensagem de commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = A carregar diferenças…
git-no-changes = Sem alterações para mostrar
git-accept = ✓ aceitar
git-deny = ✗ recusar
git-show-unchanged-lines = Mostrar { $count } linhas sem alterações

terminal-loading = A carregar…
terminal-runs-when-ready = executa quando estiver pronto · Ctrl+C limpa · Esc ignora
terminal-booting = a arrancar
terminal-type-command = escreva um comando · executa quando estiver pronto · Esc ignora

setup-tagline-claude = O agente de programação da Anthropic, no Vmux
setup-tagline-codex = O agente de programação da OpenAI, no Vmux
setup-tagline-vibe = O agente de programação da Mistral, no Vmux
setup-install-title = Instalar CLI { $name }
setup-homebrew-required = O Homebrew é necessário para instalar { $command } e ainda não está configurado. O Vmux vai instalar primeiro o Homebrew e depois { $name }.
setup-terminal-instructions = No terminal, prima Return para começar e introduza a palavra-passe do Mac quando lhe for pedido.
setup-command-missing = O Vmux abriu esta página porque o comando local { $command } ainda não está instalado. Execute o comando abaixo para o obter.
setup-install-failed = A instalação não terminou. Consulte o terminal para ver os detalhes e tente novamente.
setup-installing = A instalar…
setup-install-homebrew = Instalar Homebrew + { $name }
setup-run-install = Executar comando de instalação
setup-auto-reload = O Vmux executa-o num terminal e recarrega quando { $command } estiver pronto.

debug-title = Depuração
debug-auto-update = Atualização automática
debug-simulate-update = Simular atualização disponível
debug-simulate-download = Simular descarga
debug-clear-update = Limpar atualização
debug-trigger-restart = Acionar reinício

command-manage-spaces = Gerir espaços…
command-pane-stack-location = painel { $pane } / pilha { $stack }
command-space-pane-stack-location = { $space } / painel { $pane } / pilha { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Modo interativo
command-group-window = Janela
command-group-tab = Separador
command-group-pane = Painel
command-group-stack = Pilha
command-group-space = Espaço
command-group-navigation = Navegação
command-group-open = Abrir
command-group-view = Ver
command-group-bar = Barra

menu-close-vmux = Fechar o Vmux

agents-terminal-coding-agent = Agente de programação baseado no Terminal

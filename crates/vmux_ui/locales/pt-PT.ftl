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

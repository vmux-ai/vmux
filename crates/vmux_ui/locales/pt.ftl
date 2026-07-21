common-open = Abrir
common-close = Fechar
common-install = Instalar
common-uninstall = Desinstalar
common-update = Atualizar
common-retry = Tentar novamente
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
start-title = Início
start-tagline = Um prompt. Tudo feito.

agents-title = Agentes
agents-search = Pesquise agentes ACP e CLI…
agents-empty = Nenhum agente encontrado
agents-empty-detail = Tente um nome, runtime ou ACP/CLI.
agents-install-failed = Falha na instalação
agents-updating = Atualizando…
agents-retrying = Tentando novamente…
agents-preparing = Preparando…

extensions-title = Extensões
extensions-search = Pesquise instaladas ou na Chrome Web Store…
extensions-relaunch = Reinicie para aplicar
extensions-empty = Nenhuma extensão instalada
extensions-no-match = Nenhuma extensão encontrada
extensions-empty-detail = Pesquise na Chrome Web Store acima e pressione Enter.
extensions-no-match-detail = Tente outro nome ou ID de extensão.
extensions-on = Ativada
extensions-off = Desativada
extensions-enable-confirm = Ativar { $name }?
extensions-enable-permissions = Ativar { $name } e permitir:

lsp-title = Servidores de linguagem
lsp-search = Pesquise servidores de linguagem, linters, formatadores…
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
team-empty = Ninguém aqui ainda
team-you = Você
team-agent = Agente

services-title = Serviços em segundo plano
services-processes = { $count ->
    [one] 1 processo
   *[other] { $count } processos
}
services-kill-all = Encerrar todos
services-not-running = O serviço não está em execução
services-start-with = Iniciar com:
services-empty = Nenhum processo ativo
services-filter = Filtrar processos…
services-no-match = Nenhum processo encontrado
services-connected = Conectado
services-disconnected = Desconectado
services-attached = anexado
services-kill = Encerrar
services-memory = Memória
services-size = Tamanho
services-shell = Shell

error-title = Erro

history-search = Pesquisar histórico
history-clear-all = Limpar tudo
history-clear-confirm = Limpar todo o histórico?
history-clear-warning = Esta ação não pode ser desfeita.
history-cancel = Cancelar
history-today = Hoje
history-yesterday = Ontem
history-days-ago = há { $count } dias
history-day-offset = Dia -{ $count }

settings-title = Definições
settings-loading = Carregando definições…
settings-stored = Armazenado em ~/.vmux/settings.ron
settings-other = Outros
settings-software-update = Atualização de software
settings-check-updates = Procurar atualizações
settings-check-updates-hint = Verifica automaticamente ao iniciar e a cada hora quando a atualização automática está ativada.
settings-update-unavailable = Indisponível
settings-update-unavailable-hint = O atualizador não está incluído nesta build.
settings-update-checking = Verificando…
settings-update-checking-hint = Verificando atualizações…
settings-update-check-again = Verificar novamente
settings-update-current = Vmux está atualizado.
settings-update-downloading = Baixando…
settings-update-downloading-hint = Baixando Vmux { $version }…
settings-update-installing = Instalando…
settings-update-installing-hint = Instalando Vmux { $version }…
settings-update-ready = Atualização pronta
settings-update-ready-hint = Vmux { $version } está pronto. Reinicie para aplicar.
settings-update-try-again = Tentar novamente
settings-update-failed = Não foi possível verificar atualizações.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pressione uma tecla…
settings-saved = Salvo
settings-record-key = Clique para gravar uma nova combinação de teclas

tray-open-window = Abrir janela
tray-close-window = Fechar janela
tray-pause-recording = Pausar gravação
tray-resume-recording = Retomar gravação
tray-finish-recording = Concluir gravação
tray-quit = Sair do Vmux

composer-attach-files = Anexar arquivos (/upload)
composer-remove-attachment = Remover anexo

layout-back = Voltar
layout-forward = Avançar
layout-reload = Recarregar
layout-bookmark-page = Adicionar esta página aos favoritos
layout-remove-bookmark = Remover favorito
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
command-search-ask = Pesquisar ou perguntar…
command-new-tab-placeholder = Pesquise, digite um URL ou selecione Terminal…
command-placeholder = Digite um URL, pesquise abas ou use > para comandos…
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
command-search = Pesquisar
command-open-value = Abrir “{ $value }”
command-search-value = Pesquisar “{ $value }”

schema-appearance = Aparência
schema-general = Geral
schema-layout = Layout
schema-layout-detail = Janela, painéis, barra lateral e anel de foco.
schema-agent = Agente
schema-agent-detail = Comportamento do agente e permissões de ferramentas.
schema-shortcuts = Atalhos
schema-shortcuts-detail = Visualização somente leitura. Edite settings.ron diretamente para alterar os atalhos.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de cores das páginas web. Dispositivo segue o sistema.
schema-device = Dispositivo
schema-light = Claro
schema-dark = Escuro
schema-language = Idioma
schema-language-detail = Use o sistema, en-US, ja ou qualquer tag BCP 47 com um catálogo ~/.vmux/locales/<tag>.ftl correspondente.
schema-auto-update = Atualização automática
schema-auto-update-detail = Procurar e instalar atualizações ao iniciar e a cada hora.
schema-startup-url = URL inicial
schema-startup-url-detail = Vazio abre o prompt da barra de comandos.
schema-search-engine = Mecanismo de pesquisa
schema-search-engine-detail = Usado para pesquisas na web a partir do Início e da barra de comandos.
schema-window = Janela
schema-pane = Painel
schema-side-sheet = Painel lateral
schema-focus-ring = Anel de foco
schema-run-placement = Permitir substituição do posicionamento da execução
schema-run-placement-detail = Permitir que agentes escolham o modo, a direção e a âncora do painel de execução.
schema-leader = Tecla líder
schema-leader-detail = Tecla de prefixo para atalhos em acorde.
schema-chord-timeout = Tempo limite do acorde
schema-chord-timeout-detail = Milissegundos até o prefixo de acorde expirar.
schema-bindings = Atalhos
schema-confirm-close = Confirmar fechamento
schema-confirm-close-detail = Perguntar antes de fechar um terminal com um processo em execução.
schema-default-theme = Tema padrão
schema-default-theme-detail = Nome do tema ativo na lista de temas.

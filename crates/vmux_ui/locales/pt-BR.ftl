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

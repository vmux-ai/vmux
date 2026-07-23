locale-name = français
common-open = Ouvrir
common-close = Fermer
common-install = Installer
common-uninstall = Désinstaller
common-update = Mettre à jour
common-retry = Réessayer
common-refresh = Actualiser
common-remove = Supprimer
common-enable = Activer
common-disable = Désactiver
common-new = Nouveau
common-active = actif
common-running = en cours
common-done = terminé
common-failed = Échec
common-installed = Installé
common-items = { $count ->
    [one] { $count } élément
   *[other] { $count } éléments
}

tools-title = Outils
tools-search = Rechercher des paquets, agents, MCP, outils linguistiques et fichiers de configuration…
tools-open = Ouvrir les outils
tools-fold = Replier les outils
tools-unfold = Déplier les outils
tools-scanning = Analyse des outils locaux…
tools-no-installed = Aucun outil installé
tools-empty = Aucun outil correspondant
tools-empty-detail = Installez un paquet ou ajoutez un paquet de fichiers de configuration de type Stow.
tools-apply = Appliquer
tools-homebrew = Homebrew
tools-homebrew-sync = Les formules et applications installées se synchronisent automatiquement.
tools-open-brewfile = Ouvrir le Brewfile
tools-managed = géré
tools-provider-homebrew-formulae = Formules Homebrew
tools-provider-homebrew-casks = Applications Homebrew
tools-provider-npm = Paquets npm
tools-provider-acp-agents = Agents ACP
tools-provider-language-tools = Outils linguistiques
tools-provider-mcp-servers = Serveurs MCP
tools-provider-dotfiles = Fichiers de configuration
tools-status-available = Disponible
tools-status-missing = Manquant
tools-status-conflict = Conflit
tools-forget = Oublier
tools-manage = Gérer
tools-link = Lier
tools-unlink = Dissocier
tools-import = Importer
tools-update-count = { $count ->
    [one] 1 mise à jour
   *[other] { $count } mises à jour
}
tools-conflict-count = { $count ->
    [one] 1 conflit
   *[other] { $count } conflits
}
tools-result-applied = Outils appliqués
tools-result-imported = Outils importés
tools-result-installed = { $name } installé
tools-result-updated = { $name } mis à jour
tools-result-uninstalled = { $name } désinstallé
tools-result-forgotten = { $name } oublié
tools-result-managed = { $name } est maintenant géré
tools-result-linked = { $name } lié
tools-result-unlinked = { $name } dissocié
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synchronisez les paramètres, les outils, les fichiers dot et les connaissances avec Git.
vault-sync = Synchroniser
vault-create = Créer
vault-connect = Connecter
vault-private = Dépôt privé
vault-public-warning = Les référentiels publics exposent vos connaissances et votre configuration.
vault-choose-repository = Choisissez un référentiel…
vault-empty = vide
vault-clean = À jour
vault-not-connected = Non connecté
vault-change-count = Changements: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Démarrer
start-tagline = Une consigne. Tout est fait.

agents-title = Agents
agents-search = Rechercher des agents ACP et CLI…
agents-empty = Aucun agent correspondant
agents-empty-detail = Essayez un nom, un environnement d’exécution ou ACP/CLI.
agents-install-failed = Échec de l’installation
agents-updating = Mise à jour…
agents-retrying = Nouvelle tentative…
agents-preparing = Préparation…

extensions-title = Extensions
extensions-search = Rechercher parmi les extensions installées ou dans le Chrome Web Store…
extensions-relaunch = Relancer pour appliquer
extensions-empty = Aucune extension installée
extensions-no-match = Aucune extension correspondante
extensions-empty-detail = Recherchez dans le Chrome Web Store ci-dessus, puis appuyez sur Entrée.
extensions-no-match-detail = Essayez un autre nom ou ID d’extension.
extensions-on = Activé
extensions-off = Désactivé
extensions-enable-confirm = Activer { $name } ?
extensions-enable-permissions = Activer { $name } et autoriser :

lsp-title = Language Servers
lsp-search = Rechercher des language servers, linters, formateurs…
lsp-loading = Chargement du catalogue…
lsp-empty = Aucun language server correspondant
lsp-empty-detail = Essayez un autre langage, linter ou formateur.
lsp-needs = nécessite { $tool }
lsp-status-available = Disponible
lsp-status-on-path = Sur le PATH
lsp-status-installing = Installation…
lsp-status-installed = Installé
lsp-status-outdated = Mise à jour disponible
lsp-status-running = En cours
lsp-status-failed = Échec

spaces-title = Espaces
spaces-new-placeholder = Nom du nouvel espace
spaces-empty = Aucun espace
spaces-default-name = Espace { $number }
spaces-tabs = { $count ->
    [one] 1 onglet
   *[other] { $count } onglets
}
spaces-delete = Supprimer l’espace

team-title = Équipe
team-just-you = Vous êtes seul dans cet espace
team-agents = { $count ->
    [one] Vous et 1 agent
   *[other] Vous et { $count } agents
}
team-empty = Personne ici pour l’instant
team-you = Vous
team-agent = Agent

services-title = Services en arrière-plan
services-processes = { $count ->
    [one] 1 processus
   *[other] { $count } processus
}
services-kill-all = Tout arrêter de force
services-not-running = Le service n’est pas en cours d’exécution
services-start-with = Démarrer avec :
services-empty = Aucun processus actif
services-filter = Filtrer les processus…
services-no-match = Aucun processus correspondant
services-connected = Connecté
services-disconnected = Déconnecté
services-attached = attaché
services-kill = Arrêter de force
services-memory = Mémoire
services-size = Taille
services-shell = Shell

error-title = Erreur

history-search = Rechercher dans l’historique
history-clear-all = Tout effacer
history-clear-confirm = Effacer tout l’historique ?
history-clear-warning = Cette action est irréversible.
history-cancel = Annuler
history-today = Aujourd’hui
history-yesterday = Hier
history-days-ago = Il y a { $count } jours
history-day-offset = Jour -{ $count }

settings-title = Réglages
settings-loading = Chargement des réglages…
settings-stored = Stocké dans ~/.vmux/settings.ron
settings-other = Autre
settings-software-update = Mise à jour logicielle
settings-check-updates = Rechercher des mises à jour
settings-check-updates-hint = Vérifie automatiquement au lancement, puis toutes les heures si la mise à jour automatique est activée.
settings-update-unavailable = Indisponible
settings-update-unavailable-hint = Le programme de mise à jour n’est pas inclus dans cette version.
settings-update-checking = Vérification…
settings-update-checking-hint = Recherche de mises à jour…
settings-update-check-again = Rechercher à nouveau
settings-update-current = Vmux est à jour.
settings-update-downloading = Téléchargement…
settings-update-downloading-hint = Téléchargement de Vmux { $version }…
settings-update-installing = Installation…
settings-update-installing-hint = Installation de Vmux { $version }…
settings-update-ready = Mise à jour prête
settings-update-ready-hint = Vmux { $version } est prêt. Redémarrez pour appliquer la mise à jour.
settings-update-try-again = Réessayer
settings-update-failed = Impossible de rechercher les mises à jour.
settings-item = Élément
settings-item-number = Élément { $number }
settings-press-key = Appuyez sur une touche…
settings-saved = Enregistré
settings-record-key = Cliquez pour enregistrer un nouveau raccourci

tray-open-window = Ouvrir la fenêtre
tray-close-window = Fermer la fenêtre
tray-pause-recording = Suspendre l’enregistrement
tray-resume-recording = Reprendre l’enregistrement
tray-finish-recording = Terminer l’enregistrement
tray-quit = Quitter Vmux

composer-attach-files = Joindre des fichiers (/upload)
composer-remove-attachment = Supprimer la pièce jointe

layout-back = Précédent
layout-forward = Suivant
layout-reload = Recharger
layout-bookmark-page = Ajouter cette page aux favoris
layout-remove-bookmark = Retirer le favori
layout-pin-page = Épingler cette page
layout-unpin-page = Détacher cette page
layout-manage-extensions = Gérer les extensions
layout-new-stack = Nouvelle pile
layout-close-tab = Fermer l’onglet
layout-bookmark = Favori
layout-pin = Épingler
layout-new-tab = Nouvel onglet
layout-team = Équipe

command-switch-space = Changer d’espace…
command-search-ask = Rechercher ou demander…
command-new-tab-placeholder = Recherchez, saisissez une URL ou sélectionnez Terminal…
command-placeholder = Saisissez une URL, recherchez des onglets ou tapez > pour les commandes…
command-composer-placeholder = Tapez / pour les commandes ou @ pour les médias
command-send = Envoyer (Entrée)
command-terminal = Terminal
command-open-terminal = Ouvrir dans Terminal
command-stack = Pile
command-tabs = { $count ->
    [one] 1 onglet
   *[other] { $count } onglets
}
command-prompt = Consigne
command-new-tab = Nouvel onglet
command-search = Rechercher
command-open-value = Ouvrir « { $value } »
command-search-value = Rechercher « { $value } »

schema-appearance = Apparence
schema-general = Général
schema-layout = Disposition
schema-layout-detail = Fenêtre, volets, barre latérale et anneau de focus.
schema-agent = Agent
schema-agent-detail = Comportement de l’agent et autorisations des outils.
schema-shortcuts = Raccourcis
schema-shortcuts-detail = Affichage en lecture seule. Modifiez directement settings.ron pour changer les raccourcis.
schema-terminal = Terminal
schema-browser = Navigateur
schema-mode = Mode
schema-mode-detail = Jeu de couleurs pour les pages web. Appareil suit votre système.
schema-device = Appareil
schema-light = Clair
schema-dark = Sombre
schema-language = Langue
schema-language-detail = Utilisez le système, en-US, ja ou toute balise BCP 47 avec un catalogue ~/.vmux/locales/<tag>.ftl correspondant.
schema-auto-update = Mise à jour automatique
schema-auto-update-detail = Rechercher et installer les mises à jour au lancement, puis toutes les heures.
schema-startup-url = URL de démarrage
schema-startup-url-detail = Si vide, ouvre l’invite de la barre de commandes.
schema-search-engine = Moteur de recherche
schema-search-engine-detail = Utilisé pour les recherches web depuis Démarrer et la barre de commandes.
schema-window = Fenêtre
schema-pane = Volet
schema-side-sheet = Panneau latéral
schema-focus-ring = Anneau de focus
schema-run-placement = Autoriser le remplacement du placement d’exécution
schema-run-placement-detail = Permettre aux agents de choisir le mode, la direction et l’ancrage du volet d’exécution.
schema-leader = Leader
schema-leader-detail = Touche de préfixe pour les raccourcis en accord.
schema-chord-timeout = Délai d’expiration de l’accord
schema-chord-timeout-detail = Millisecondes avant l’expiration d’un préfixe d’accord.
schema-bindings = Raccourcis
schema-confirm-close = Confirmer la fermeture
schema-confirm-close-detail = Demander confirmation avant de fermer un terminal avec un processus en cours.
schema-default-theme = Thème par défaut
schema-default-theme-detail = Nom du thème actif dans la liste des thèmes.

settings-empty = (vide)
settings-none = (aucun)

schema-system = Système
schema-editor = Éditeur
schema-recording = Enregistrement
schema-radius = Rayon
schema-padding = Marge intérieure
schema-gap = Espacement
schema-width = Largeur
schema-color = Couleur
schema-red = Rouge
schema-green = Vert
schema-blue = Bleu
schema-follow-files = Suivre les fichiers
schema-tidy-files = Nettoyer les fichiers
schema-tidy-files-max = Seuil de nettoyage des fichiers
schema-tidy-files-auto = Nettoyer les fichiers automatiquement
schema-app-providers = Fournisseurs d’applications
schema-provider = Fournisseur
schema-kind = Type
schema-models = Modèles
schema-acp = Agents ACP
schema-id = ID
schema-name = Nom
schema-command = Commande
schema-arguments = Arguments
schema-environment = Environnement
schema-working-directory = Répertoire de travail
schema-shell = Shell
schema-font-family = Famille de police
schema-startup-directory = Répertoire de démarrage
schema-themes = Thèmes
schema-color-scheme = Jeu de couleurs
schema-font-size = Taille de police
schema-line-height = Hauteur de ligne
schema-cursor-style = Style du curseur
schema-cursor-blink = Clignotement du curseur
schema-custom-themes = Thèmes personnalisés
schema-foreground = Premier plan
schema-background = Arrière-plan
schema-cursor = Curseur
schema-ansi-colors = Couleurs ANSI
schema-keymap = Raccourcis clavier
schema-explorer = Explorateur
schema-visible = Visible
schema-language-servers = Serveurs de langage
schema-servers = Serveurs
schema-language-id = ID de langage
schema-root-markers = Marqueurs de racine
schema-output-directory = Répertoire de sortie

menu-scene = Scène
menu-layout = Disposition
menu-terminal = Terminal
menu-browser = Navigateur
menu-service = Service
menu-bookmark = Signet
menu-edit = Édition

layout-knowledge = Connaissances
layout-open-knowledge = Ouvrir les connaissances
layout-open-welcome-knowledge = Ouvrir Bienvenue dans les connaissances
layout-open-path = Ouvrir { $path }
layout-fold-knowledge = Replier les connaissances
layout-unfold-knowledge = Déplier les connaissances
layout-bookmarks = Signets
layout-new-folder = Nouveau dossier
layout-add-to-bookmarks = Ajouter aux signets
layout-move-to-bookmarks = Déplacer vers les signets
layout-stack-number = Pile { $number }
layout-fold-stack = Replier la pile
layout-unfold-stack = Déplier la pile
layout-close-stack = Fermer la pile
layout-bookmark-in = Signet dans { $folder }

common-cancel = Annuler
common-delete = Supprimer
common-save = Enregistrer
common-rename = Renommer
common-expand = Développer
common-collapse = Réduire
common-loading = Chargement…
common-error = Erreur
common-output = Sortie
common-pending = En attente
common-current = actuel
common-stop = Arrêter
services-command = Service Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } j { $hours } h

error-page-failed-load = Échec du chargement de la page
error-page-not-found = Page introuvable
error-unknown-host = Hôte d’app Vmux inconnu : { $host }

history-title = Historique

command-new-app-chat = Nouveau chat { $provider }/{ $model } (app)
command-interactive-mode-user = Scène > Mode interactif > Utilisateur
command-interactive-mode-player = Scène > Mode interactif > Lecteur
command-minimize-window = Disposition > Fenêtre > Réduire
command-toggle-layout = Disposition > Disposition > Basculer la disposition
command-close-tab = Disposition > Onglet > Fermer l’onglet
command-new-task = Disposition > Onglet > Nouvelle tâche…
command-next-tab = Disposition > Onglet > Onglet suivant
command-prev-tab = Disposition > Onglet > Onglet précédent
command-rename-tab = Disposition > Onglet > Renommer l’onglet
command-tab-select-1 = Disposition > Onglet > Sélectionner l’onglet 1
command-tab-select-2 = Disposition > Onglet > Sélectionner l’onglet 2
command-tab-select-3 = Disposition > Onglet > Sélectionner l’onglet 3
command-tab-select-4 = Disposition > Onglet > Sélectionner l’onglet 4
command-tab-select-5 = Disposition > Onglet > Sélectionner l’onglet 5
command-tab-select-6 = Disposition > Onglet > Sélectionner l’onglet 6
command-tab-select-7 = Disposition > Onglet > Sélectionner l’onglet 7
command-tab-select-8 = Disposition > Onglet > Sélectionner l’onglet 8
command-tab-select-last = Disposition > Onglet > Sélectionner le dernier onglet
command-close-pane = Disposition > Volet > Fermer le volet
command-select-pane-left = Disposition > Volet > Sélectionner le volet de gauche
command-select-pane-right = Disposition > Volet > Sélectionner le volet de droite
command-select-pane-up = Disposition > Volet > Sélectionner le volet du haut
command-select-pane-down = Disposition > Volet > Sélectionner le volet du bas
command-swap-pane-prev = Disposition > Volet > Permuter avec le volet précédent
command-swap-pane-next = Disposition > Volet > Permuter avec le volet suivant
command-equalize-pane-size = Disposition > Volet > Égaliser la taille des volets
command-resize-pane-left = Disposition > Volet > Redimensionner le volet vers la gauche
command-resize-pane-right = Disposition > Volet > Redimensionner le volet vers la droite
command-resize-pane-up = Disposition > Volet > Redimensionner le volet vers le haut
command-resize-pane-down = Disposition > Volet > Redimensionner le volet vers le bas
command-stack-close = Disposition > Pile > Fermer la pile
command-stack-next = Disposition > Pile > Pile suivante
command-stack-previous = Disposition > Pile > Pile précédente
command-stack-reopen = Disposition > Pile > Rouvrir la page fermée
command-stack-swap-prev = Disposition > Pile > Déplacer la pile à gauche
command-stack-swap-next = Disposition > Pile > Déplacer la pile à droite
command-space-open = Disposition > Espace > Espaces
command-terminal-close = Terminal > Fermer le terminal
command-terminal-next = Terminal > Terminal suivant
command-terminal-prev = Terminal > Terminal précédent
command-terminal-clear = Terminal > Effacer le terminal
command-browser-prev-page = Navigateur > Navigation > Retour
command-browser-next-page = Navigateur > Navigation > Suivant
command-browser-reload = Navigateur > Navigation > Recharger
command-browser-hard-reload = Navigateur > Navigation > Rechargement complet
command-open-in-place = Navigateur > Ouvrir > Ouvrir ici
command-open-in-new-stack = Navigateur > Ouvrir > Ouvrir dans une nouvelle pile
command-open-in-pane-top = Navigateur > Ouvrir > Ouvrir dans le volet du haut
command-open-in-pane-right = Navigateur > Ouvrir > Ouvrir dans le volet de droite
command-open-in-pane-bottom = Navigateur > Ouvrir > Ouvrir dans le volet du bas
command-open-in-pane-left = Navigateur > Ouvrir > Ouvrir dans le volet de gauche
command-open-in-new-tab = Navigateur > Ouvrir > Ouvrir dans un nouvel onglet
command-open-in-new-space = Navigateur > Ouvrir > Ouvrir dans un nouvel espace
command-browser-zoom-in = Navigateur > Présentation > Zoom avant
command-browser-zoom-out = Navigateur > Présentation > Zoom arrière
command-browser-zoom-reset = Navigateur > Présentation > Taille réelle
command-browser-dev-tools = Navigateur > Présentation > Outils de développement
command-browser-open-command-bar = Navigateur > Barre > Barre de commandes
command-browser-open-page-in-command-bar = Navigateur > Barre > Modifier la page
command-browser-open-path-bar = Navigateur > Barre > Navigateur de chemin
command-browser-open-commands = Navigateur > Barre > Commandes
command-browser-open-history = Navigateur > Barre > Historique
command-service-open = Service > Ouvrir le moniteur de service
command-bookmark-toggle-active = Signet > Ajouter la page aux signets
command-bookmark-pin-active = Signet > Épingler la page

layout-tab = Onglet
layout-no-stacks = Aucune pile
layout-loading = Chargement…
layout-no-markdown-files = Aucun fichier Markdown
layout-empty-folder = Dossier vide
layout-worktree = copie de travail
layout-folder-name = Nom du dossier
layout-no-pins-bookmarks = Aucun élément épinglé ni signet
layout-move-to = Déplacer vers { $folder }
layout-bookmark-current-page = Ajouter la page actuelle aux signets
layout-rename-folder = Renommer le dossier
layout-remove-folder = Supprimer le dossier
layout-update-downloading = Téléchargement de la mise à jour
layout-update-installing = Installation de la mise à jour…
layout-update-ready = Nouvelle version disponible
layout-restart-update = Redémarrer pour mettre à jour

agent-preparing = Préparation de l’agent…
agent-send-all-queued = Envoyer maintenant toutes les invites en file (Échap)
agent-send = Envoyer (Entrée)
agent-ready = Prêt quand vous l’êtes.
agent-loading-older = Chargement des anciens messages…
agent-load-older = Charger les anciens messages
agent-continued-from = Suite de { $source }
agent-older-context-omitted = ancien contexte omis
agent-interrupted = interrompu
agent-allow-tool = Autoriser { $tool } ?
agent-deny = Refuser
agent-allow-always = Toujours autoriser
agent-allow = Autoriser
agent-loading-sessions = Chargement des sessions…
agent-no-resumable-sessions = Aucune session reprenable trouvée
agent-no-matching-sessions = Aucune session correspondante
agent-no-matching-models = Aucun modèle correspondant
agent-choice-help = ↑/↓ ou Ctrl+N/Ctrl+P · 1–9 · Entrée
agent-choose-repository = Choisir le dossier du dépôt
agent-choose-repository-detail = Sélectionnez le dépôt Git local à utiliser par l’agent.
agent-choosing = Sélection…
agent-choose-folder = Choisir un dossier
agent-queued = en file
agent-attached = Joint :
agent-cancel-queued = Annuler l’invite en file
agent-resume-queued = Reprendre les invites en file
agent-clear-queue = Vider la file
agent-send-all-now = tout envoyer maintenant
agent-choose-option = Choisissez une option ci-dessus
agent-loading-media = Chargement des médias…
agent-no-matching-media = Aucun média correspondant
agent-prompt-context = Contexte de l’invite
agent-details = Détails
agent-path = Chemin
agent-tool = Outil
agent-server = Serveur
agent-bytes = { $count } octets
agent-worked-for = A travaillé pendant { $duration }
agent-worked-for-steps = { $count ->
    [one] A travaillé pendant { $duration } · 1 étape
   *[other] A travaillé pendant { $duration } · { $count } étapes
}
agent-tool-guardian-review = Revue Guardian
agent-tool-read-files = Fichiers lus
agent-tool-viewed-image = Image consultée
agent-tool-used-browser = Navigateur utilisé
agent-tool-searched-files = Fichiers recherchés
agent-tool-ran-commands = Commandes exécutées
agent-thinking = Réflexion
agent-subagent = Sous-agent
agent-prompt = Invite
agent-thread = Fil
agent-parent = Parent
agent-children = Enfants
agent-call = Appel
agent-raw-event = Événement brut
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 tâche
   *[other] { $count } tâches
}
agent-edited = Modifié
agent-reconnecting = Reconnexion { $attempt }/{ $total }
agent-status-running = En cours
agent-status-done = Terminé
agent-status-failed = Échec
agent-status-pending = En attente
agent-slash-attach-files = Joindre des fichiers
agent-slash-resume-session = Reprendre une session passée
agent-slash-select-model = Sélectionner le modèle
agent-slash-continue-cli = Continuer cette session dans le CLI
agent-session-just-now = à l’instant
agent-session-minutes-ago = il y a { $count } min
agent-session-hours-ago = il y a { $count } h
agent-session-days-ago = il y a { $count } j
agent-working-working = Travail en cours
agent-working-thinking = Réflexion
agent-working-pondering = Réflexion approfondie
agent-working-noodling = Exploration
agent-working-percolating = Maturation
agent-working-conjuring = Conjuration
agent-working-cooking = Préparation
agent-working-brewing = Infusion
agent-working-musing = Méditation
agent-working-ruminating = Rumination
agent-working-scheming = Élaboration
agent-working-synthesizing = Synthèse
agent-working-tinkering = Bricolage
agent-working-churning = Traitement
agent-working-vibing = Inspiration
agent-working-simmering = Mijotage
agent-working-crafting = Conception
agent-working-divining = Divination
agent-working-mulling = Réflexion
agent-working-spelunking = Exploration

editor-toggle-explorer = Afficher/masquer l’explorateur (Cmd+B)
editor-unsaved = non enregistré
editor-rendered-markdown = Markdown rendu avec édition en direct
editor-note = Note
editor-source-editor = Éditeur de source
editor-editor = Éditeur
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Ranger
editor-always = Toujours
editor-unchanged-previews = { $count ->
    [one] ✦ 1 aperçu inchangé
   *[other] ✦ { $count } aperçus inchangés
}
editor-open-externally = Ouvrir avec une app externe
editor-changed-line = Ligne modifiée
editor-go-to-definition = Aller à la définition
editor-find-references = Rechercher les références
editor-references = { $count ->
    [one] 1 référence
   *[other] { $count } références
}
editor-lsp-starting = Démarrage de { $server }…
editor-lsp-not-installed = { $server } — non installé
editor-explorer = Explorateur
editor-open-editors = Éditeurs ouverts
editor-outline = Plan
editor-new-file = Nouveau fichier
editor-new-folder = Nouveau dossier
editor-delete-confirm = Supprimer « { $name } » ? Cette action est irréversible.
editor-created-folder = Dossier { $name } créé
editor-created-file = Fichier { $name } créé
editor-renamed-to = Renommé en { $name }
editor-deleted = { $name } supprimé
editor-failed-decode-image = Échec du décodage de l’image
editor-preview-large-image = image (trop volumineuse pour l’aperçu)
editor-preview-binary = binaire
editor-preview-file = fichier

git-status-clean = propre
git-status-modified = modifié
git-status-staged = indexé
git-status-staged-modified = indexé*
git-status-untracked = non suivi
git-status-deleted = supprimé
git-status-conflict = conflit
git-accept-all = ✓ tout accepter
git-unstage = Désindexer
git-confirm-deny-all = Confirmer le refus global
git-deny-all = ✗ tout refuser
git-commit-message = message de commit
git-commit = Commit ({ $count })
git-push = ↑ Pousser
git-loading-diff = Chargement du diff…
git-no-changes = Aucune modification à afficher
git-accept = ✓ accepter
git-deny = ✗ refuser
git-show-unchanged-lines = Afficher { $count } lignes inchangées

terminal-loading = Chargement…
terminal-runs-when-ready = s’exécute dès que prêt · Ctrl+C efface · Échap ignore
terminal-booting = démarrage
terminal-type-command = saisissez une commande · s’exécute dès que prêt · Échap ignore

setup-tagline-claude = L’agent de codage d’Anthropic, dans Vmux
setup-tagline-codex = L’agent de codage d’OpenAI, dans Vmux
setup-tagline-vibe = L’agent de codage de Mistral, dans Vmux
setup-install-title = Installer le CLI { $name }
setup-homebrew-required = Homebrew est requis pour installer { $command } et n’est pas encore configuré. Vmux installera d’abord Homebrew, puis { $name }.
setup-terminal-instructions = Dans le terminal, appuyez sur Retour pour démarrer, puis saisissez le mot de passe de votre Mac lorsque vous y êtes invité.
setup-command-missing = Vmux a ouvert cette page car la commande locale { $command } n’est pas encore installée. Exécutez la commande ci-dessous pour l’obtenir.
setup-install-failed = L’installation ne s’est pas terminée. Consultez le terminal pour les détails, puis réessayez.
setup-installing = Installation…
setup-install-homebrew = Installer Homebrew + { $name }
setup-run-install = Exécuter la commande d’installation
setup-auto-reload = Vmux l’exécute dans un terminal et recharge quand { $command } est prêt.

debug-title = Débogage
debug-auto-update = Mise à jour automatique
debug-simulate-update = Simuler une mise à jour disponible
debug-simulate-download = Simuler un téléchargement
debug-clear-update = Effacer la mise à jour
debug-trigger-restart = Déclencher le redémarrage

command-manage-spaces = Gérer les espaces…
command-pane-stack-location = volet { $pane } / pile { $stack }
command-space-pane-stack-location = { $space } / volet { $pane } / pile { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Mode interactif
command-group-window = Fenêtre
command-group-tab = Onglet
command-group-pane = Volet
command-group-stack = Pile
command-group-space = Espace
command-group-navigation = Navigation
command-group-open = Ouvrir
command-group-view = Affichage
command-group-bar = Barre

menu-close-vmux = Fermer Vmux

agents-terminal-coding-agent = Agent de code dans le terminal

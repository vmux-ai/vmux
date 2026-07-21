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

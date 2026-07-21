common-open = Ouvert
common-close = Fermer
common-install = Installer
common-uninstall = Désinstaller
common-update = Mise à jour
common-retry = Réessayer
common-refresh = Actualiser
common-remove = Supprimer
common-enable = Activer
common-disable = Désactiver
common-new = Nouveau
common-active = actif
common-running = courir
common-done = fait
common-failed = Échec
common-installed = Installé
common-items = { $count ->
    [one] { $count } élément
   *[other] { $count } éléments
}
start-title = Commencer
start-tagline = Une invite. N'importe quoi, c'est fait.

agents-title = Agents
agents-search = Rechercher les agents ACP et CLI…
agents-empty = Aucun agent correspondant
agents-empty-detail = Essayez un nom, un environnement d'exécution ou ACP/CLI.
agents-install-failed = L'installation a échoué
agents-updating = Mise à jour…
agents-retrying = Nouvelle tentative…
agents-preparing = Préparation…

extensions-title = Rallonges
extensions-search = Recherche installée ou Chrome Web Store…
extensions-relaunch = Relancer pour postuler
extensions-empty = Aucune extension installée
extensions-no-match = Aucune extension correspondante
extensions-empty-detail = Recherchez le Chrome Web Store ci-dessus et appuyez sur Return.
extensions-no-match-detail = Essayez un autre nom ou un autre ID d'extension.
extensions-on = Sur
extensions-off = Désactivé
extensions-enable-confirm = Activer { $name } ?
extensions-enable-permissions = Activez { $name } et autorisez :

lsp-title = Serveurs de langues
lsp-search = Rechercher des serveurs de langage, des linters, des formateurs…
lsp-loading = Chargement du catalogue…
lsp-empty = Aucun serveur de langue correspondant
lsp-empty-detail = Essayez une autre langue, un linter ou un formateur.
lsp-needs = a besoin de { $tool }
lsp-status-available = Disponible
lsp-status-on-path = Le PATH
lsp-status-installing = Installation…
lsp-status-installed = Installé
lsp-status-outdated = Mise à jour disponible
lsp-status-running = Courir
lsp-status-failed = Échec

spaces-title = Espaces
spaces-new-placeholder = Nouveau nom d'espace
spaces-empty = Aucun espace
spaces-default-name = Espace { $number }
spaces-tabs = { $count ->
    [one] 1 onglet
   *[other] Onglets { $count }
}
spaces-delete = Supprimer l'espace

team-title = Équipe
team-just-you = Juste toi dans cet espace
team-agents = { $count ->
    [one] Vous et 1 agent
   *[other] Vous et les agents { $count }
}
team-empty = Personne ici encore
team-you = Vous
team-agent = Agent

services-title = Services d'arrière-plan
services-processes = { $count ->
    [one] 1 processus
   *[other] { $count } processus
}
services-kill-all = Tuer tout
services-not-running = Le service ne fonctionne pas
services-start-with = Commencez par :
services-empty = Aucun processus actif
services-filter = Processus de filtrage…
services-no-match = Aucun processus de correspondance
services-connected = Connecté
services-disconnected = Déconnecté
services-attached = ci-joint
services-kill = Tuer
services-memory = Mémoire
services-size = Taille
services-shell = Coquille

error-title = Erreur

history-search = Historique de recherche
history-clear-all = Tout effacer
history-clear-confirm = Effacer tout l'historique ?
history-clear-warning = Cela ne peut pas être annulé.
history-cancel = Annuler
history-today = Aujourd'hui
history-yesterday = Hier
history-days-ago = Il y a { $count } jours
history-day-offset = Jour -{ $count }

settings-title = Paramètres
settings-loading = Chargement des paramètres…
settings-stored = Stocké dans ~/.vmux/settings.ron
settings-other = Autre
settings-software-update = Mise à jour du logiciel
settings-check-updates = Vérifier les mises à jour
settings-check-updates-hint = Vérifie automatiquement au lancement et toutes les heures lorsque la mise à jour automatique est activée.
settings-update-unavailable = Indisponible
settings-update-unavailable-hint = Le programme de mise à jour n'est pas inclus dans cette version.
settings-update-checking = Vérification…
settings-update-checking-hint = Vérification des mises à jour…
settings-update-check-again = Vérifiez à nouveau
settings-update-current = Vmux est à jour.
settings-update-downloading = Téléchargement…
settings-update-downloading-hint = Téléchargement de Vmux { $version }…
settings-update-installing = Installation…
settings-update-installing-hint = Installation de Vmux { $version }…
settings-update-ready = Mise à jour prête
settings-update-ready-hint = Vmux { $version } est prêt. Redémarrez pour l’appliquer.
settings-update-try-again = Réessayez
settings-update-failed = Impossible de vérifier les mises à jour.
settings-item = Article
settings-item-number = Article { $number }
settings-press-key = Appuyez sur une touche…
settings-saved = Enregistré
settings-record-key = Cliquez pour enregistrer une nouvelle combinaison de touches

tray-open-window = Ouvrir la fenêtre
tray-close-window = Fermer la fenêtre
tray-pause-recording = Suspendre l'enregistrement
tray-resume-recording = Reprendre l'enregistrement
tray-finish-recording = Terminer l'enregistrement
tray-quit = Quitter Vmux

composer-attach-files = Joindre des fichiers (/upload)
composer-remove-attachment = Supprimer la pièce jointe

layout-back = Retour
layout-forward = En avant
layout-reload = Recharger
layout-bookmark-page = Ajouter cette page à vos favoris
layout-remove-bookmark = Supprimer le favori
layout-pin-page = Épingler cette page
layout-unpin-page = Détacher cette page
layout-manage-extensions = Gérer les extensions
layout-new-stack = Nouvelle pile
layout-close-tab = Fermer l'onglet
layout-bookmark = Marque-page
layout-pin = Épingle
layout-new-tab = Nouvel onglet
layout-team = Équipe

command-switch-space = Changer d’espace…
command-search-ask = Recherchez ou demandez…
command-new-tab-placeholder = Recherchez ou tapez un URL, ou sélectionnez Terminal…
command-placeholder = Tapez un URL, recherchez des onglets ou > pour les commandes…
command-composer-placeholder = Tapez / pour les commandes ou @ pour les médias
command-send = Envoyer (Enter)
command-terminal = Borne
command-open-terminal = Ouvrir dans le terminal
command-stack = Pile
command-tabs = { $count ->
    [one] 1 onglet
   *[other] Onglets { $count }
}
command-prompt = Invite
command-new-tab = Nouvel onglet
command-search = Rechercher
command-open-value = Ouvrez « { $value } »
command-search-value = Recherchez « { $value } »

schema-appearance = Apparence
schema-general = Général
schema-layout = Disposition
schema-layout-detail = Fenêtre, volets, barre latérale et bague de mise au point.
schema-agent = Agent
schema-agent-detail = Comportement de l'agent et autorisations des outils.
schema-shortcuts = Raccourcis
schema-shortcuts-detail = Vue en lecture seule. Modifiez settings.ron directement pour modifier les liaisons.
schema-terminal = Borne
schema-browser = Navigateur
schema-mode = Mode
schema-mode-detail = Jeu de couleurs pour les pages Web. L'appareil suit votre système.
schema-device = Appareil
schema-light = Lumière
schema-dark = Sombre
schema-language = Langue
schema-language-detail = Utilisez system, en-US, ja ou toute balise BCP 47 avec un catalogue ~/.vmux/locales/<tag>.ftl correspondant.
schema-auto-update = Mise à jour automatique
schema-auto-update-detail = Recherchez et installez les mises à jour au lancement et toutes les heures.
schema-startup-url = Démarrage URL
schema-startup-url-detail = Vide ouvre l’invite de la barre de commandes.
schema-search-engine = Moteur de recherche
schema-search-engine-detail = Utilisé pour les recherches Web à partir de Démarrer et de la barre de commandes.
schema-window = Fenêtre
schema-pane = Volet
schema-side-sheet = Feuille latérale
schema-focus-ring = Bague de mise au point
schema-run-placement = Autoriser le remplacement du placement d'exécution
schema-run-placement-detail = Laissez les agents choisir le mode, la direction et l’ancrage du volet d’exécution.
schema-leader = Chef
schema-leader-detail = Touche de préfixe pour les raccourcis d'accords.
schema-chord-timeout = Délai d'expiration des accords
schema-chord-timeout-detail = Millisecondes avant l'expiration d'un préfixe d'accord.
schema-bindings = Reliures
schema-confirm-close = Confirmer la fermeture
schema-confirm-close-detail = Invite avant de fermer un terminal avec un processus en cours d'exécution.
schema-default-theme = Thème par défaut
schema-default-theme-detail = Nom du thème actif dans la liste des thèmes.

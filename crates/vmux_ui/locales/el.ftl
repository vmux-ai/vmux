locale-name = Ελληνικά
common-open = Άνοιγμα
common-close = Κλείσιμο
common-install = Εγκατάσταση
common-uninstall = Απεγκατάσταση
common-update = Ενημέρωση
common-retry = Δοκιμή ξανά
common-refresh = Ανανέωση
common-remove = Αφαίρεση
common-enable = Ενεργοποίηση
common-disable = Απενεργοποίηση
common-new = Νέο
common-active = ενεργό
common-running = εκτελείται
common-done = ολοκληρώθηκε
common-failed = Απέτυχε
common-installed = Εγκατεστημένο
common-items = { $count ->
    [one] { $count } στοιχείο
   *[other] { $count } στοιχεία
}

tools-title = Εργαλεία
tools-search = Αναζήτηση πακέτων, πρακτόρων, MCP, γλωσσικών εργαλείων και αρχείων ρυθμίσεων…
tools-open = Άνοιγμα εργαλείων
tools-fold = Σύμπτυξη εργαλείων
tools-unfold = Ανάπτυξη εργαλείων
tools-scanning = Σάρωση τοπικών εργαλείων…
tools-no-installed = Δεν υπάρχουν εγκατεστημένα εργαλεία
tools-empty = Δεν υπάρχουν εργαλεία που να ταιριάζουν
tools-empty-detail = Εγκαταστήστε ένα πακέτο ή προσθέστε ένα πακέτο αρχείων ρυθμίσεων τύπου Stow.
tools-apply = Εφαρμογή
tools-homebrew = Homebrew
tools-homebrew-sync = Οι εγκατεστημένες φόρμουλες και εφαρμογές συγχρονίζονται αυτόματα.
tools-open-brewfile = Άνοιγμα Brewfile
tools-managed = διαχειριζόμενο
tools-provider-homebrew-formulae = Φόρμουλες Homebrew
tools-provider-homebrew-casks = Εφαρμογές Homebrew
tools-provider-npm = Πακέτα npm
tools-provider-acp-agents = Πράκτορες ACP
tools-provider-language-tools = Γλωσσικά εργαλεία
tools-provider-mcp-servers = Διακομιστές MCP
tools-provider-dotfiles = Αρχεία ρυθμίσεων
tools-status-available = Διαθέσιμο
tools-status-missing = Λείπει
tools-status-conflict = Διένεξη
tools-forget = Παράβλεψη
tools-manage = Διαχείριση
tools-link = Σύνδεση
tools-unlink = Αποσύνδεση
tools-import = Εισαγωγή
tools-update-count = { $count ->
    [one] 1 ενημέρωση
   *[other] { $count } ενημερώσεις
}
tools-conflict-count = { $count ->
    [one] 1 διένεξη
   *[other] { $count } διενέξεις
}
tools-result-applied = Τα εργαλεία εφαρμόστηκαν
tools-result-imported = Τα εργαλεία εισήχθησαν
tools-result-installed = Το { $name } εγκαταστάθηκε
tools-result-updated = Το { $name } ενημερώθηκε
tools-result-uninstalled = Το { $name } απεγκαταστάθηκε
tools-result-forgotten = Το { $name } παραβλέφθηκε
tools-result-managed = Το { $name } είναι πλέον διαχειριζόμενο
tools-result-linked = Το { $name } συνδέθηκε
tools-result-unlinked = Το { $name } αποσυνδέθηκε
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Συγχρονίστε ρυθμίσεις, εργαλεία, dotfiles και Knowledge με το Git.
vault-sync = Συγχρονισμός
vault-create = Δημιουργώ
vault-connect = Συνδέω
vault-private = Ιδιωτικό αποθετήριο
vault-public-warning = Τα δημόσια αποθετήρια εκθέτουν τη Γνώση και τη διαμόρφωσή σας.
vault-choose-repository = Επιλέξτε ένα αποθετήριο…
vault-empty = αδειάζω
vault-clean = Σύγχρονος
vault-not-connected = Δεν είναι συνδεδεμένο
vault-change-count = Αλλαγές: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Έναρξη
start-tagline = Μία οδηγία. Όλα έτοιμα.

agents-title = Agents
agents-search = Αναζήτηση agent ACP και CLI…
agents-empty = Δεν βρέθηκαν agents
agents-empty-detail = Δοκιμάστε όνομα, runtime ή ACP/CLI.
agents-install-failed = Η εγκατάσταση απέτυχε
agents-updating = Γίνεται ενημέρωση…
agents-retrying = Γίνεται νέα προσπάθεια…
agents-preparing = Γίνεται προετοιμασία…

extensions-title = Επεκτάσεις
extensions-search = Αναζήτηση εγκατεστημένων ή στο Chrome Web Store…
extensions-relaunch = Επανεκκίνηση για εφαρμογή
extensions-empty = Δεν υπάρχουν εγκατεστημένες επεκτάσεις
extensions-no-match = Δεν βρέθηκαν επεκτάσεις
extensions-empty-detail = Αναζητήστε στο Chrome Web Store παραπάνω και πατήστε Enter.
extensions-no-match-detail = Δοκιμάστε άλλο όνομα ή ID επέκτασης.
extensions-on = Ενεργό
extensions-off = Ανενεργό
extensions-enable-confirm = Ενεργοποίηση του { $name };
extensions-enable-permissions = Ενεργοποίηση του { $name } και άδεια για:

lsp-title = Διακομιστές γλωσσών
lsp-search = Αναζήτηση διακομιστών γλωσσών, linters, formatters…
lsp-loading = Φόρτωση καταλόγου…
lsp-empty = Δεν βρέθηκαν διακομιστές γλωσσών
lsp-empty-detail = Δοκιμάστε άλλη γλώσσα, linter ή formatter.
lsp-needs = απαιτεί { $tool }
lsp-status-available = Διαθέσιμο
lsp-status-on-path = Στο PATH
lsp-status-installing = Γίνεται εγκατάσταση…
lsp-status-installed = Εγκατεστημένο
lsp-status-outdated = Διαθέσιμη ενημέρωση
lsp-status-running = Εκτελείται
lsp-status-failed = Απέτυχε

spaces-title = Χώροι
spaces-new-placeholder = Όνομα νέου χώρου
spaces-empty = Δεν υπάρχουν χώροι
spaces-default-name = Χώρος { $number }
spaces-tabs = { $count ->
    [one] 1 καρτέλα
   *[other] { $count } καρτέλες
}
spaces-delete = Διαγραφή χώρου

team-title = Ομάδα
team-just-you = Μόνο εσείς σε αυτόν τον χώρο
team-agents = { $count ->
    [one] Εσείς και 1 agent
   *[other] Εσείς και { $count } agents
}
team-empty = Δεν υπάρχει κανείς εδώ ακόμα
team-you = Εσείς
team-agent = Agent

services-title = Υπηρεσίες παρασκηνίου
services-processes = { $count ->
    [one] 1 διεργασία
   *[other] { $count } διεργασίες
}
services-kill-all = Τερματισμός όλων
services-not-running = Η υπηρεσία δεν εκτελείται
services-start-with = Εκκίνηση με:
services-empty = Δεν υπάρχουν ενεργές διεργασίες
services-filter = Φιλτράρισμα διεργασιών…
services-no-match = Δεν βρέθηκαν διεργασίες
services-connected = Συνδεδεμένο
services-disconnected = Αποσυνδεδεμένο
services-attached = συνδεδεμένο
services-kill = Τερματισμός
services-memory = Μνήμη
services-size = Μέγεθος
services-shell = Κέλυφος

error-title = Σφάλμα

history-search = Αναζήτηση ιστορικού
history-clear-all = Εκκαθάριση όλων
history-clear-confirm = Εκκαθάριση όλου του ιστορικού;
history-clear-warning = Αυτή η ενέργεια δεν μπορεί να αναιρεθεί.
history-cancel = Άκυρο
history-today = Σήμερα
history-yesterday = Χθες
history-days-ago = Πριν από { $count } ημέρες
history-day-offset = Ημέρα -{ $count }

settings-title = Ρυθμίσεις
settings-loading = Φόρτωση ρυθμίσεων…
settings-stored = Αποθηκεύεται στο ~/.vmux/settings.ron
settings-other = Άλλα
settings-software-update = Ενημέρωση λογισμικού
settings-check-updates = Έλεγχος για ενημερώσεις
settings-check-updates-hint = Ελέγχει αυτόματα κατά την εκκίνηση και κάθε ώρα όταν είναι ενεργή η αυτόματη ενημέρωση.
settings-update-unavailable = Μη διαθέσιμο
settings-update-unavailable-hint = Το πρόγραμμα ενημέρωσης δεν περιλαμβάνεται σε αυτήν την έκδοση.
settings-update-checking = Γίνεται έλεγχος…
settings-update-checking-hint = Έλεγχος για ενημερώσεις…
settings-update-check-again = Έλεγχος ξανά
settings-update-current = Το Vmux είναι ενημερωμένο.
settings-update-downloading = Γίνεται λήψη…
settings-update-downloading-hint = Γίνεται λήψη του Vmux { $version }…
settings-update-installing = Γίνεται εγκατάσταση…
settings-update-installing-hint = Γίνεται εγκατάσταση του Vmux { $version }…
settings-update-ready = Η ενημέρωση είναι έτοιμη
settings-update-ready-hint = Το Vmux { $version } είναι έτοιμο. Κάντε επανεκκίνηση για εφαρμογή.
settings-update-try-again = Δοκιμή ξανά
settings-update-failed = Δεν ήταν δυνατός ο έλεγχος για ενημερώσεις.
settings-item = Στοιχείο
settings-item-number = Στοιχείο { $number }
settings-press-key = Πατήστε ένα πλήκτρο…
settings-saved = Αποθηκεύτηκε
settings-record-key = Κάντε κλικ για καταγραφή νέου συνδυασμού πλήκτρων

tray-open-window = Άνοιγμα παραθύρου
tray-close-window = Κλείσιμο παραθύρου
tray-pause-recording = Παύση εγγραφής
tray-resume-recording = Συνέχιση εγγραφής
tray-finish-recording = Ολοκλήρωση εγγραφής
tray-quit = Έξοδος από το Vmux

composer-attach-files = Επισύναψη αρχείων (/upload)
composer-remove-attachment = Αφαίρεση συνημμένου

layout-back = Πίσω
layout-forward = Μπροστά
layout-reload = Επαναφόρτωση
layout-bookmark-page = Προσθήκη σελιδοδείκτη για αυτήν τη σελίδα
layout-remove-bookmark = Αφαίρεση σελιδοδείκτη
layout-pin-page = Καρφίτσωμα αυτής της σελίδας
layout-unpin-page = Ξεκαρφίτσωμα αυτής της σελίδας
layout-manage-extensions = Διαχείριση επεκτάσεων
layout-new-stack = Νέο stack
layout-close-tab = Κλείσιμο καρτέλας
layout-bookmark = Σελιδοδείκτης
layout-pin = Καρφίτσωμα
layout-new-tab = Νέα καρτέλα
layout-team = Ομάδα

command-switch-space = Αλλαγή χώρου…
command-search-ask = Αναζήτηση ή ερώτηση…
command-new-tab-placeholder = Αναζήτηση ή πληκτρολόγηση URL, ή επιλογή Terminal…
command-placeholder = Πληκτρολογήστε URL, αναζητήστε καρτέλες ή > για εντολές…
command-composer-placeholder = Πληκτρολογήστε / για εντολές ή @ για πολυμέσα
command-send = Αποστολή (Enter)
command-terminal = Τερματικό
command-open-terminal = Άνοιγμα στο Τερματικό
command-stack = Stack
command-tabs = { $count ->
    [one] 1 καρτέλα
   *[other] { $count } καρτέλες
}
command-prompt = Οδηγία
command-new-tab = Νέα καρτέλα
command-search = Αναζήτηση
command-open-value = Άνοιγμα «{ $value }»
command-search-value = Αναζήτηση «{ $value }»

schema-appearance = Εμφάνιση
schema-general = Γενικά
schema-layout = Διάταξη
schema-layout-detail = Παράθυρο, τμήματα, πλαϊνή γραμμή και περίγραμμα εστίασης.
schema-agent = Agent
schema-agent-detail = Συμπεριφορά agent και δικαιώματα εργαλείων.
schema-shortcuts = Συντομεύσεις
schema-shortcuts-detail = Προβολή μόνο για ανάγνωση. Επεξεργαστείτε απευθείας το settings.ron για να αλλάξετε συνδυασμούς.
schema-terminal = Τερματικό
schema-browser = Πρόγραμμα περιήγησης
schema-mode = Λειτουργία
schema-mode-detail = Χρωματικός συνδυασμός για ιστοσελίδες. Η συσκευή ακολουθεί το σύστημά σας.
schema-device = Συσκευή
schema-light = Ανοιχτό
schema-dark = Σκούρο
schema-language = Γλώσσα
schema-language-detail = Χρησιμοποιήστε το σύστημα, en-US, ja ή οποιαδήποτε ετικέτα BCP 47 με αντίστοιχο κατάλογο ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Αυτόματη ενημέρωση
schema-auto-update-detail = Έλεγχος και εγκατάσταση ενημερώσεων κατά την εκκίνηση και κάθε ώρα.
schema-startup-url = URL εκκίνησης
schema-startup-url-detail = Αν είναι κενό, ανοίγει η γραμμή εντολών.
schema-search-engine = Μηχανή αναζήτησης
schema-search-engine-detail = Χρησιμοποιείται για αναζητήσεις στον ιστό από την Έναρξη και τη γραμμή εντολών.
schema-window = Παράθυρο
schema-pane = Τμήμα
schema-side-sheet = Πλαϊνό φύλλο
schema-focus-ring = Περίγραμμα εστίασης
schema-run-placement = Να επιτρέπεται παράκαμψη τοποθέτησης εκτέλεσης
schema-run-placement-detail = Επιτρέπει στους agents να επιλέγουν λειτουργία τμήματος εκτέλεσης, κατεύθυνση και αγκύρωση.
schema-leader = Leader
schema-leader-detail = Πλήκτρο προθέματος για συντομεύσεις chord.
schema-chord-timeout = Χρονικό όριο chord
schema-chord-timeout-detail = Χιλιοστά του δευτερολέπτου πριν λήξει ένα πρόθεμα chord.
schema-bindings = Συνδυασμοί
schema-confirm-close = Επιβεβαίωση κλεισίματος
schema-confirm-close-detail = Ερώτηση πριν κλείσει τερματικό με διεργασία που εκτελείται.
schema-default-theme = Προεπιλεγμένο θέμα
schema-default-theme-detail = Όνομα του ενεργού θέματος από τη λίστα θεμάτων.

settings-empty = (κενό)
settings-none = (κανένα)

schema-system = Σύστημα
schema-editor = Επεξεργαστής
schema-recording = Εγγραφή
schema-radius = Ακτίνα
schema-padding = Εσωτερικό περιθώριο
schema-gap = Διάκενο
schema-width = Πλάτος
schema-color = Χρώμα
schema-red = Κόκκινο
schema-green = Πράσινο
schema-blue = Μπλε
schema-follow-files = Παρακολούθηση αρχείων
schema-tidy-files = Τακτοποίηση αρχείων
schema-tidy-files-max = Όριο τακτοποίησης αρχείων
schema-tidy-files-auto = Αυτόματη τακτοποίηση αρχείων
schema-app-providers = Πάροχοι εφαρμογών
schema-provider = Πάροχος
schema-kind = Τύπος
schema-models = Μοντέλα
schema-acp = Πράκτορες ACP
schema-id = ID
schema-name = Όνομα
schema-command = Εντολή
schema-arguments = Ορίσματα
schema-environment = Περιβάλλον
schema-working-directory = Κατάλογος εργασίας
schema-shell = Κέλυφος
schema-font-family = Οικογένεια γραμματοσειράς
schema-startup-directory = Αρχικός κατάλογος
schema-themes = Θέματα
schema-color-scheme = Συνδυασμός χρωμάτων
schema-font-size = Μέγεθος γραμματοσειράς
schema-line-height = Ύψος γραμμής
schema-cursor-style = Στυλ δρομέα
schema-cursor-blink = Αναβόσβημα δρομέα
schema-custom-themes = Προσαρμοσμένα θέματα
schema-foreground = Προσκήνιο
schema-background = Φόντο
schema-cursor = Δρομέας
schema-ansi-colors = Χρώματα ANSI
schema-keymap = Χαρτογράφηση πλήκτρων
schema-explorer = Εξερευνητής
schema-visible = Ορατό
schema-language-servers = Διακομιστές γλώσσας
schema-servers = Διακομιστές
schema-language-id = ID γλώσσας
schema-root-markers = Δείκτες ρίζας
schema-output-directory = Κατάλογος εξόδου

menu-scene = Σκηνή
menu-layout = Διάταξη
menu-terminal = Τερματικό
menu-browser = Περιηγητής
menu-service = Υπηρεσία
menu-bookmark = Σελιδοδείκτης
menu-edit = Επεξεργασία

layout-knowledge = Γνώση
layout-open-knowledge = Άνοιγμα Γνώσης
layout-open-welcome-knowledge = Άνοιγμα Καλωσορίσματος στη Γνώση
layout-open-path = Άνοιγμα { $path }
layout-fold-knowledge = Σύμπτυξη γνώσης
layout-unfold-knowledge = Ανάπτυξη γνώσης
layout-bookmarks = Σελιδοδείκτες
layout-new-folder = Νέος φάκελος
layout-add-to-bookmarks = Προσθήκη στους σελιδοδείκτες
layout-move-to-bookmarks = Μετακίνηση στους σελιδοδείκτες
layout-stack-number = Στοίβα { $number }
layout-fold-stack = Σύμπτυξη στοίβας
layout-unfold-stack = Ανάπτυξη στοίβας
layout-close-stack = Κλείσιμο στοίβας
layout-bookmark-in = Σελιδοδείκτης σε { $folder }

common-cancel = Άκυρο
common-delete = Διαγραφή
common-save = Αποθήκευση
common-rename = Μετονομασία
common-expand = Ανάπτυξη
common-collapse = Σύμπτυξη
common-loading = Φόρτωση…
common-error = Σφάλμα
common-output = Έξοδος
common-pending = Σε αναμονή
common-current = τρέχον
common-stop = Διακοπή
services-command = Υπηρεσία Vmux
services-uptime-seconds = { $seconds }δ
services-uptime-minutes = { $minutes }λ { $seconds }δ
services-uptime-hours = { $hours }ω { $minutes }λ
services-uptime-days = { $days }η { $hours }ω

error-page-failed-load = Η σελίδα δεν φορτώθηκε
error-page-not-found = Η σελίδα δεν βρέθηκε
error-unknown-host = Άγνωστος κεντρικός υπολογιστής εφαρμογής Vmux: { $host }

history-title = Ιστορικό

command-new-app-chat = Νέα συνομιλία { $provider }/{ $model } (Εφαρμογή)
command-interactive-mode-user = Σκηνή > Διαδραστική λειτουργία > Χρήστης
command-interactive-mode-player = Σκηνή > Διαδραστική λειτουργία > Παίκτης
command-minimize-window = Διάταξη > Παράθυρο > Ελαχιστοποίηση
command-toggle-layout = Διάταξη > Διάταξη > Εναλλαγή διάταξης
command-close-tab = Διάταξη > Καρτέλα > Κλείσιμο καρτέλας
command-new-task = Διάταξη > Καρτέλα > Νέα εργασία…
command-next-tab = Διάταξη > Καρτέλα > Επόμενη καρτέλα
command-prev-tab = Διάταξη > Καρτέλα > Προηγούμενη καρτέλα
command-rename-tab = Διάταξη > Καρτέλα > Μετονομασία καρτέλας
command-tab-select-1 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 1
command-tab-select-2 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 2
command-tab-select-3 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 3
command-tab-select-4 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 4
command-tab-select-5 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 5
command-tab-select-6 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 6
command-tab-select-7 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 7
command-tab-select-8 = Διάταξη > Καρτέλα > Επιλογή καρτέλας 8
command-tab-select-last = Διάταξη > Καρτέλα > Επιλογή τελευταίας καρτέλας
command-close-pane = Διάταξη > Τμήμα > Κλείσιμο τμήματος
command-select-pane-left = Διάταξη > Τμήμα > Επιλογή αριστερού τμήματος
command-select-pane-right = Διάταξη > Τμήμα > Επιλογή δεξιού τμήματος
command-select-pane-up = Διάταξη > Τμήμα > Επιλογή επάνω τμήματος
command-select-pane-down = Διάταξη > Τμήμα > Επιλογή κάτω τμήματος
command-swap-pane-prev = Διάταξη > Τμήμα > Εναλλαγή με προηγούμενο τμήμα
command-swap-pane-next = Διάταξη > Τμήμα > Εναλλαγή με επόμενο τμήμα
command-equalize-pane-size = Διάταξη > Τμήμα > Ίσο μέγεθος τμημάτων
command-resize-pane-left = Διάταξη > Τμήμα > Αλλαγή μεγέθους τμήματος αριστερά
command-resize-pane-right = Διάταξη > Τμήμα > Αλλαγή μεγέθους τμήματος δεξιά
command-resize-pane-up = Διάταξη > Τμήμα > Αλλαγή μεγέθους τμήματος επάνω
command-resize-pane-down = Διάταξη > Τμήμα > Αλλαγή μεγέθους τμήματος κάτω
command-stack-close = Διάταξη > Στοίβα > Κλείσιμο στοίβας
command-stack-next = Διάταξη > Στοίβα > Επόμενη στοίβα
command-stack-previous = Διάταξη > Στοίβα > Προηγούμενη στοίβα
command-stack-reopen = Διάταξη > Στοίβα > Άνοιγμα ξανά κλειστής σελίδας
command-stack-swap-prev = Διάταξη > Στοίβα > Μετακίνηση στοίβας αριστερά
command-stack-swap-next = Διάταξη > Στοίβα > Μετακίνηση στοίβας δεξιά
command-space-open = Διάταξη > Χώρος > Χώροι
command-terminal-close = Τερματικό > Κλείσιμο τερματικού
command-terminal-next = Τερματικό > Επόμενο τερματικό
command-terminal-prev = Τερματικό > Προηγούμενο τερματικό
command-terminal-clear = Τερματικό > Εκκαθάριση τερματικού
command-browser-prev-page = Πρόγραμμα περιήγησης > Πλοήγηση > Πίσω
command-browser-next-page = Πρόγραμμα περιήγησης > Πλοήγηση > Εμπρός
command-browser-reload = Πρόγραμμα περιήγησης > Πλοήγηση > Επαναφόρτωση
command-browser-hard-reload = Πρόγραμμα περιήγησης > Πλοήγηση > Πλήρης επαναφόρτωση
command-open-in-place = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα εδώ
command-open-in-new-stack = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε νέα στοίβα
command-open-in-pane-top = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε επάνω τμήμα
command-open-in-pane-right = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε δεξί τμήμα
command-open-in-pane-bottom = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε κάτω τμήμα
command-open-in-pane-left = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε αριστερό τμήμα
command-open-in-new-tab = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε νέα καρτέλα
command-open-in-new-space = Πρόγραμμα περιήγησης > Άνοιγμα > Άνοιγμα σε νέο χώρο
command-browser-zoom-in = Πρόγραμμα περιήγησης > Προβολή > Μεγέθυνση
command-browser-zoom-out = Πρόγραμμα περιήγησης > Προβολή > Σμίκρυνση
command-browser-zoom-reset = Πρόγραμμα περιήγησης > Προβολή > Πραγματικό μέγεθος
command-browser-dev-tools = Πρόγραμμα περιήγησης > Προβολή > Εργαλεία προγραμματιστή
command-browser-open-command-bar = Πρόγραμμα περιήγησης > Γραμμή > Γραμμή εντολών
command-browser-open-page-in-command-bar = Πρόγραμμα περιήγησης > Γραμμή > Επεξεργασία σελίδας
command-browser-open-path-bar = Πρόγραμμα περιήγησης > Γραμμή > Πλοηγός διαδρομής
command-browser-open-commands = Πρόγραμμα περιήγησης > Γραμμή > Εντολές
command-browser-open-history = Πρόγραμμα περιήγησης > Γραμμή > Ιστορικό
command-service-open = Υπηρεσία > Άνοιγμα παρακολούθησης υπηρεσίας
command-bookmark-toggle-active = Σελιδοδείκτης > Προσθήκη σελιδοδείκτη στη σελίδα
command-bookmark-pin-active = Σελιδοδείκτης > Καρφίτσωμα σελίδας

layout-tab = Καρτέλα
layout-no-stacks = Δεν υπάρχουν στοίβες
layout-loading = Φόρτωση…
layout-no-markdown-files = Δεν υπάρχουν αρχεία Markdown
layout-empty-folder = Κενός φάκελος
layout-worktree = worktree
layout-folder-name = Όνομα φακέλου
layout-no-pins-bookmarks = Δεν υπάρχουν καρφιτσώματα ή σελιδοδείκτες
layout-move-to = Μετακίνηση σε { $folder }
layout-bookmark-current-page = Προσθήκη τρέχουσας σελίδας στους σελιδοδείκτες
layout-rename-folder = Μετονομασία φακέλου
layout-remove-folder = Αφαίρεση φακέλου
layout-update-downloading = Λήψη ενημέρωσης
layout-update-installing = Εγκατάσταση ενημέρωσης…
layout-update-ready = Διαθέσιμη νέα έκδοση
layout-restart-update = Επανεκκίνηση για ενημέρωση

agent-preparing = Προετοιμασία agent…
agent-send-all-queued = Αποστολή όλων των μηνυμάτων στην ουρά τώρα (Esc)
agent-send = Αποστολή (Enter)
agent-ready = Έτοιμο όταν είστε.
agent-loading-older = Φόρτωση παλαιότερων μηνυμάτων…
agent-load-older = Φόρτωση παλαιότερων μηνυμάτων
agent-continued-from = Συνέχεια από { $source }
agent-older-context-omitted = παραλείφθηκε παλαιότερο περιεχόμενο
agent-interrupted = διακόπηκε
agent-allow-tool = Να επιτραπεί το { $tool };
agent-deny = Απόρριψη
agent-allow-always = Πάντα να επιτρέπεται
agent-allow = Να επιτραπεί
agent-loading-sessions = Φόρτωση συνεδριών…
agent-no-resumable-sessions = Δεν βρέθηκαν συνεδρίες για συνέχιση
agent-no-matching-sessions = Δεν υπάρχουν αντίστοιχες συνεδρίες
agent-no-matching-models = Δεν υπάρχουν αντίστοιχα μοντέλα
agent-choice-help = ↑/↓ ή Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Επιλέξτε φάκελο αποθετηρίου
agent-choose-repository-detail = Επιλέξτε το τοπικό αποθετήριο Git που θα χρησιμοποιεί ο agent.
agent-choosing = Επιλογή…
agent-choose-folder = Επιλέξτε φάκελο
agent-queued = στην ουρά
agent-attached = Συνημμένα:
agent-cancel-queued = Ακύρωση μηνύματος στην ουρά
agent-resume-queued = Συνέχιση μηνυμάτων στην ουρά
agent-clear-queue = Εκκαθάριση ουράς
agent-send-all-now = αποστολή όλων τώρα
agent-choose-option = Επιλέξτε μια επιλογή παραπάνω
agent-loading-media = Φόρτωση πολυμέσων…
agent-no-matching-media = Δεν υπάρχουν αντίστοιχα πολυμέσα
agent-prompt-context = Περιεχόμενο προτροπής
agent-details = Λεπτομέρειες
agent-path = Διαδρομή
agent-tool = Εργαλείο
agent-server = Διακομιστής
agent-bytes = { $count } byte
agent-worked-for = Εργάστηκε για { $duration }
agent-worked-for-steps = { $count ->
    [one] Εργάστηκε για { $duration } · 1 βήμα
   *[other] Εργάστηκε για { $duration } · { $count } βήματα
}
agent-tool-guardian-review = Έλεγχος Guardian
agent-tool-read-files = Ανάγνωση αρχείων
agent-tool-viewed-image = Προβολή εικόνας
agent-tool-used-browser = Χρήση προγράμματος περιήγησης
agent-tool-searched-files = Αναζήτηση σε αρχεία
agent-tool-ran-commands = Εκτέλεση εντολών
agent-thinking = Σκέφτεται
agent-subagent = Υπο-agent
agent-prompt = Προτροπή
agent-thread = Νήμα
agent-parent = Γονικό
agent-children = Θυγατρικά
agent-call = Κλήση
agent-raw-event = Ανεπεξέργαστο συμβάν
agent-plan = Σχέδιο
agent-tasks = { $count ->
    [one] 1 εργασία
   *[other] { $count } εργασίες
}
agent-edited = Τροποποιήθηκε
agent-reconnecting = Επανασύνδεση { $attempt }/{ $total }
agent-status-running = Εκτελείται
agent-status-done = Ολοκληρώθηκε
agent-status-failed = Απέτυχε
agent-status-pending = Σε αναμονή
agent-slash-attach-files = Επισύναψη αρχείων
agent-slash-resume-session = Συνέχιση προηγούμενης συνεδρίας
agent-slash-select-model = Επιλογή μοντέλου
agent-slash-continue-cli = Συνέχιση αυτής της συνεδρίας στο CLI
agent-session-just-now = μόλις τώρα
agent-session-minutes-ago = πριν από { $count }λ
agent-session-hours-ago = πριν από { $count }ω
agent-session-days-ago = πριν από { $count }η
agent-working-working = Εργάζεται
agent-working-thinking = Σκέφτεται
agent-working-pondering = Συλλογίζεται
agent-working-noodling = Πειραματίζεται
agent-working-percolating = Ωριμάζει ιδέες
agent-working-conjuring = Σκαρφίζεται λύσεις
agent-working-cooking = Μαγειρεύει λύση
agent-working-brewing = Ετοιμάζει κάτι
agent-working-musing = Αναλογίζεται
agent-working-ruminating = Το επεξεργάζεται
agent-working-scheming = Καταστρώνει σχέδιο
agent-working-synthesizing = Συνθέτει
agent-working-tinkering = Μαστορεύει
agent-working-churning = Επεξεργάζεται
agent-working-vibing = Πιάνει τον ρυθμό
agent-working-simmering = Σιγοβράζει ιδέες
agent-working-crafting = Δημιουργεί
agent-working-divining = Ανιχνεύει λύση
agent-working-mulling = Το σκέφτεται
agent-working-spelunking = Εξερευνά σε βάθος

editor-toggle-explorer = Εναλλαγή Explorer (Cmd+B)
editor-unsaved = μη αποθηκευμένο
editor-rendered-markdown = Αποδομένο Markdown με ζωντανή επεξεργασία
editor-note = Σημείωση
editor-source-editor = Επεξεργαστής πηγαίου κώδικα
editor-editor = Επεξεργαστής
editor-git-diff = Διαφορές Git
editor-diff = Διαφορές
editor-tidy = Τακτοποίηση
editor-always = Πάντα
editor-unchanged-previews = { $count ->
    [one] ✦ 1 αμετάβλητη προεπισκόπηση
   *[other] ✦ { $count } αμετάβλητες προεπισκοπήσεις
}
editor-open-externally = Άνοιγμα εξωτερικά
editor-changed-line = Αλλαγμένη γραμμή
editor-go-to-definition = Μετάβαση στον ορισμό
editor-find-references = Εύρεση αναφορών
editor-references = { $count ->
    [one] 1 αναφορά
   *[other] { $count } αναφορές
}
editor-lsp-starting = Εκκίνηση { $server }…
editor-lsp-not-installed = { $server } — δεν είναι εγκατεστημένο
editor-explorer = Explorer
editor-open-editors = Ανοιχτοί επεξεργαστές
editor-outline = Περίγραμμα
editor-new-file = Νέο αρχείο
editor-new-folder = Νέος φάκελος
editor-delete-confirm = Διαγραφή του «{ $name }»; Δεν είναι δυνατή η αναίρεση.
editor-created-folder = Δημιουργήθηκε ο φάκελος { $name }
editor-created-file = Δημιουργήθηκε το αρχείο { $name }
editor-renamed-to = Μετονομάστηκε σε { $name }
editor-deleted = Διαγράφηκε το { $name }
editor-failed-decode-image = Η αποκωδικοποίηση της εικόνας απέτυχε
editor-preview-large-image = εικόνα (πολύ μεγάλη για προεπισκόπηση)
editor-preview-binary = δυαδικό
editor-preview-file = αρχείο

git-status-clean = καθαρό
git-status-modified = τροποποιημένο
git-status-staged = στο stage
git-status-staged-modified = στο stage*
git-status-untracked = μη παρακολουθούμενο
git-status-deleted = διαγραμμένο
git-status-conflict = διένεξη
git-accept-all = ✓ αποδοχή όλων
git-unstage = Αφαίρεση από stage
git-confirm-deny-all = Επιβεβαίωση απόρριψης όλων
git-deny-all = ✗ απόρριψη όλων
git-commit-message = μήνυμα commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Φόρτωση διαφορών…
git-no-changes = Δεν υπάρχουν αλλαγές για προβολή
git-accept = ✓ αποδοχή
git-deny = ✗ απόρριψη
git-show-unchanged-lines = Εμφάνιση { $count } αμετάβλητων γραμμών

terminal-loading = Φόρτωση…
terminal-runs-when-ready = εκτελείται όταν είναι έτοιμο · Ctrl+C καθαρίζει · Esc παραλείπει
terminal-booting = εκκίνηση
terminal-type-command = πληκτρολογήστε εντολή · εκτελείται όταν είναι έτοιμο · Esc παραλείπει

setup-tagline-claude = Ο agent προγραμματισμού της Anthropic, στο Vmux
setup-tagline-codex = Ο agent προγραμματισμού της OpenAI, στο Vmux
setup-tagline-vibe = Ο agent προγραμματισμού της Mistral, στο Vmux
setup-install-title = Εγκατάσταση { $name } CLI
setup-homebrew-required = Το Homebrew απαιτείται για την εγκατάσταση του { $command } και δεν έχει ρυθμιστεί ακόμα. Το Vmux θα εγκαταστήσει πρώτα το Homebrew και μετά το { $name }.
setup-terminal-instructions = Στο τερματικό, πατήστε Return για έναρξη και μετά πληκτρολογήστε τον κωδικό του Mac σας όταν ζητηθεί.
setup-command-missing = Το Vmux άνοιξε αυτή τη σελίδα επειδή η τοπική εντολή { $command } δεν είναι εγκατεστημένη ακόμα. Εκτελέστε την παρακάτω εντολή για να την αποκτήσετε.
setup-install-failed = Η εγκατάσταση δεν ολοκληρώθηκε. Ελέγξτε το τερματικό για λεπτομέρειες και δοκιμάστε ξανά.
setup-installing = Εγκατάσταση…
setup-install-homebrew = Εγκατάσταση Homebrew + { $name }
setup-run-install = Εκτέλεση εντολής εγκατάστασης
setup-auto-reload = Το Vmux την εκτελεί σε τερματικό και επαναφορτώνει όταν το { $command } είναι έτοιμο.

debug-title = Αποσφαλμάτωση
debug-auto-update = Αυτόματη ενημέρωση
debug-simulate-update = Προσομοίωση διαθέσιμης ενημέρωσης
debug-simulate-download = Προσομοίωση λήψης
debug-clear-update = Εκκαθάριση ενημέρωσης
debug-trigger-restart = Ενεργοποίηση επανεκκίνησης

command-manage-spaces = Διαχείριση χώρων…
command-pane-stack-location = τμήμα { $pane } / στοίβα { $stack }
command-space-pane-stack-location = { $space } / τμήμα { $pane } / στοίβα { $stack }
command-terminal-path = Τερματικό ({ $path })
command-group-interactive-mode = Διαδραστική λειτουργία
command-group-window = Παράθυρο
command-group-tab = Καρτέλα
command-group-pane = Τμήμα
command-group-stack = Στοίβα
command-group-space = Χώρος
command-group-navigation = Πλοήγηση
command-group-open = Άνοιγμα
command-group-view = Προβολή
command-group-bar = Γραμμή

menu-close-vmux = Κλείσιμο Vmux

agents-terminal-coding-agent = Πράκτορας προγραμματισμού σε τερματικό

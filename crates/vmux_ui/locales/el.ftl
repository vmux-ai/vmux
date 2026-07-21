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

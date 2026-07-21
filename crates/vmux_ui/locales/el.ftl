common-open = Ανοίξτε
common-close = Κλείσιμο
common-install = Εγκατάσταση
common-uninstall = Απεγκατάσταση
common-update = Ενημέρωση
common-retry = Επανάληψη
common-refresh = Ανανέωση
common-remove = Αφαίρεση
common-enable = Ενεργοποίηση
common-disable = Απενεργοποίηση
common-new = Νέο
common-active = ενεργός
common-running = τρέξιμο
common-done = έγινε
common-failed = Απέτυχε
common-installed = Εγκατεστημένο
common-items = { $count ->
    [one] { $count } στοιχείο
   *[other] { $count } στοιχεία
}
start-title = Έναρξη
start-tagline = Μια προτροπή. Οτιδήποτε, έγινε.

agents-title = Πράκτορες
agents-search = Αναζήτηση πρακτόρων ACP και CLI…
agents-empty = Δεν υπάρχουν αντιστοιχισμένοι παράγοντες
agents-empty-detail = Δοκιμάστε ένα όνομα, χρόνο εκτέλεσης ή ACP/CLI.
agents-install-failed = Η εγκατάσταση απέτυχε
agents-updating = Ενημέρωση…
agents-retrying = Επανάληψη…
agents-preparing = Προετοιμασία…

extensions-title = Επεκτάσεις
extensions-search = Εγκαταστάθηκε η αναζήτηση ή Chrome Web Store…
extensions-relaunch = Επανεκκινήστε για να κάνετε αίτηση
extensions-empty = Δεν έχουν εγκατασταθεί επεκτάσεις
extensions-no-match = Δεν υπάρχουν αντίστοιχες επεκτάσεις
extensions-empty-detail = Αναζητήστε το Chrome Web Store παραπάνω και πατήστε Return.
extensions-no-match-detail = Δοκιμάστε άλλο όνομα ή αναγνωριστικό επέκτασης.
extensions-on = Ενεργό
extensions-off = Απενεργοποίηση
extensions-enable-confirm = Ενεργοποίηση { $name };
extensions-enable-permissions = Ενεργοποιήστε το { $name } και επιτρέψτε:

lsp-title = Διακομιστές Γλωσσών
lsp-search = Αναζήτηση διακομιστών γλώσσας, γραμμών, μορφοποιητών…
lsp-loading = Φόρτωση καταλόγου…
lsp-empty = Δεν υπάρχουν διακομιστές αντιστοίχισης γλώσσας
lsp-empty-detail = Δοκιμάστε μια άλλη γλώσσα, linter ή μορφοποιητή.
lsp-needs = χρειάζεται { $tool }
lsp-status-available = Διαθέσιμο
lsp-status-on-path = Στις PATH
lsp-status-installing = Εγκατάσταση…
lsp-status-installed = Εγκατεστημένο
lsp-status-outdated = Διαθέσιμη ενημέρωση
lsp-status-running = Τρέξιμο
lsp-status-failed = Απέτυχε

spaces-title = Χώροι
spaces-new-placeholder = Νέο όνομα χώρου
spaces-empty = Χωρίς κενά
spaces-default-name = Χώρος { $number }
spaces-tabs = { $count ->
    [one] 1 καρτέλα
   *[other] { $count } καρτέλες
}
spaces-delete = Διαγραφή χώρου

team-title = Ομάδα
team-just-you = Μόνο εσύ σε αυτόν τον χώρο
team-agents = { $count ->
    [one] Εσείς και 1 πράκτορας
   *[other] Εσείς και οι πράκτορες { $count }
}
team-empty = Κανείς εδώ ακόμα
team-you = Εσύ
team-agent = Πράκτορας

services-title = Υπηρεσίες Ιστορικού
services-processes = { $count ->
    [one] 1 διαδικασία
   *[other] { $count } διεργασίες
}
services-kill-all = Σκοτώστε Όλους
services-not-running = Η υπηρεσία δεν λειτουργεί
services-start-with = Ξεκινήστε με:
services-empty = Χωρίς ενεργές διαδικασίες
services-filter = Φιλτράρισμα διαδικασιών…
services-no-match = Δεν υπάρχουν διαδικασίες αντιστοίχισης
services-connected = Συνδεδεμένος
services-disconnected = Αποσυνδέθηκε
services-attached = επισυνάπτεται
services-kill = Σκότωσε
services-memory = Μνήμη
services-size = Μέγεθος
services-shell = Shell

error-title = Σφάλμα

history-search = Ιστορικό αναζήτησης
history-clear-all = Καθαρίστε όλα
history-clear-confirm = Διαγραφή όλου του ιστορικού;
history-clear-warning = Αυτό δεν μπορεί να αναιρεθεί.
history-cancel = Ακύρωση
history-today = Σήμερα
history-yesterday = Χθες
history-days-ago = πριν από { $count } ημέρες
history-day-offset = Ημέρα -{ $count }

settings-title = Ρυθμίσεις
settings-loading = Φόρτωση ρυθμίσεων…
settings-stored = Αποθηκευμένο σε ~/.vmux/settings.ron
settings-other = Άλλο
settings-software-update = Ενημέρωση λογισμικού
settings-check-updates = Ελέγξτε για Ενημερώσεις
settings-check-updates-hint = Ελέγχει αυτόματα κατά την εκκίνηση και κάθε ώρα όταν είναι ενεργοποιημένη η Αυτόματη ενημέρωση.
settings-update-unavailable = Μη διαθέσιμο
settings-update-unavailable-hint = Το Updater δεν περιλαμβάνεται σε αυτήν την έκδοση.
settings-update-checking = Έλεγχος…
settings-update-checking-hint = Έλεγχος για ενημερώσεις…
settings-update-check-again = Ελέγξτε ξανά
settings-update-current = Το Vmux είναι ενημερωμένο.
settings-update-downloading = Λήψη…
settings-update-downloading-hint = Λήψη Vmux { $version }…
settings-update-installing = Εγκατάσταση…
settings-update-installing-hint = Εγκατάσταση Vmux { $version }…
settings-update-ready = Έτοιμη ενημέρωση
settings-update-ready-hint = Vmux { $version } είναι έτοιμο. Κάντε επανεκκίνηση για να το εφαρμόσετε.
settings-update-try-again = Δοκιμάστε ξανά
settings-update-failed = Δεν είναι δυνατός ο έλεγχος για ενημερώσεις.
settings-item = Στοιχείο
settings-item-number = Στοιχείο { $number }
settings-press-key = Πατήστε ένα πλήκτρο…
settings-saved = Αποθηκεύτηκε
settings-record-key = Κάντε κλικ για εγγραφή νέου συνδυασμού πλήκτρων

tray-open-window = Άνοιγμα παραθύρου
tray-close-window = Κλείσιμο παραθύρου
tray-pause-recording = Παύση εγγραφής
tray-resume-recording = Συνέχιση εγγραφής
tray-finish-recording = Ολοκλήρωση εγγραφής
tray-quit = Κλείστε Vmux

composer-attach-files = Επισύναψη αρχείων (/upload)
composer-remove-attachment = Αφαιρέστε το συνημμένο

layout-back = Πίσω
layout-forward = Εμπρός
layout-reload = Επαναφόρτωση
layout-bookmark-page = Προσθέστε σελιδοδείκτη αυτή τη σελίδα
layout-remove-bookmark = Αφαίρεση σελιδοδείκτη
layout-pin-page = Καρφιτσώστε αυτήν τη σελίδα
layout-unpin-page = Ξεκαρφιτσώστε αυτήν τη σελίδα
layout-manage-extensions = Διαχείριση επεκτάσεων
layout-new-stack = Νέα Στοίβα
layout-close-tab = Κλείσιμο καρτέλας
layout-bookmark = Σελιδοδείκτης
layout-pin = Καρφίτσα
layout-new-tab = Νέα καρτέλα
layout-team = Ομάδα

command-switch-space = Εναλλαγή χώρου…
command-search-ask = Αναζητήστε ή ρωτήστε…
command-new-tab-placeholder = Αναζητήστε ή πληκτρολογήστε URL ή επιλέξτε Τερματικό…
command-placeholder = Πληκτρολογήστε URL, καρτέλες αναζήτησης ή > για εντολές…
command-composer-placeholder = Πληκτρολογήστε / για εντολές ή @ για μέσα
command-send = Αποστολή (Enter)
command-terminal = Τερματικό
command-open-terminal = Άνοιγμα στο τερματικό
command-stack = Στοίβα
command-tabs = { $count ->
    [one] 1 καρτέλα
   *[other] { $count } καρτέλες
}
command-prompt = Προτροπή
command-new-tab = Νέα καρτέλα
command-search = Αναζήτηση
command-open-value = Άνοιγμα "{ $value }"
command-search-value = Αναζήτηση "{ $value }"

schema-appearance = Εμφάνιση
schema-general = Στρατηγός
schema-layout = Διάταξη
schema-layout-detail = Παράθυρο, παράθυρα, πλαϊνή γραμμή και δακτύλιος εστίασης.
schema-agent = Πράκτορας
schema-agent-detail = Συμπεριφορά αντιπροσώπου και δικαιώματα εργαλείου.
schema-shortcuts = Συντομεύσεις
schema-shortcuts-detail = Προβολή μόνο για ανάγνωση. Επεξεργαστείτε το settings.ron απευθείας για να αλλάξετε τις δεσμεύσεις.
schema-terminal = Τερματικό
schema-browser = Πρόγραμμα περιήγησης
schema-mode = Λειτουργία
schema-mode-detail = Χρωματικός συνδυασμός για ιστοσελίδες. Η συσκευή ακολουθεί το σύστημά σας.
schema-device = Συσκευή
schema-light = Φως
schema-dark = Σκοτεινό
schema-language = Γλώσσα
schema-language-detail = Χρησιμοποιήστε σύστημα, en-US, ja ή οποιαδήποτε ετικέτα BCP 47 με αντίστοιχο κατάλογο ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Αυτόματη ενημέρωση
schema-auto-update-detail = Ελέγξτε και εγκαταστήστε ενημερώσεις κατά την εκκίνηση και κάθε ώρα.
schema-startup-url = Εκκίνηση URL
schema-startup-url-detail = Το Empty ανοίγει τη γραμμή εντολών.
schema-search-engine = μηχανή αναζήτησης
schema-search-engine-detail = Χρησιμοποιείται για αναζητήσεις ιστού από το Start και τη γραμμή εντολών.
schema-window = Παράθυρο
schema-pane = Παράθυρο
schema-side-sheet = Πλαϊνό φύλλο
schema-focus-ring = Δαχτυλίδι εστίασης
schema-run-placement = Να επιτρέπεται η παράκαμψη της τοποθέτησης εκτέλεσης
schema-run-placement-detail = Αφήστε τους πράκτορες να επιλέξουν τη λειτουργία παραθύρου εκτέλεσης, την κατεύθυνση και την αγκύρωση.
schema-leader = Ηγέτης
schema-leader-detail = Πλήκτρο προθέματος για συντομεύσεις συγχορδιών.
schema-chord-timeout = Χρονικό όριο συγχορδίας
schema-chord-timeout-detail = Χιλιοστά του δευτερολέπτου πριν από τη λήξη ενός προθέματος συγχορδίας.
schema-bindings = Δεσίματα
schema-confirm-close = Επιβεβαιώστε το κλείσιμο
schema-confirm-close-detail = Ερώτηση πριν κλείσετε ένα τερματικό με μια διαδικασία που εκτελείται.
schema-default-theme = Προεπιλεγμένο θέμα
schema-default-theme-detail = Όνομα του ενεργού θέματος από τη λίστα θεμάτων.

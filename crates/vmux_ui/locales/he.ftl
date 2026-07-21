common-open = פתוח
common-close = סגור
common-install = התקן
common-uninstall = הסר את ההתקנה
common-update = עדכון
common-retry = נסה שוב
common-refresh = רענן
common-remove = הסר
common-enable = הפעל
common-disable = השבת
common-new = חדש
common-active = פעיל
common-running = ריצה
common-done = נעשה
common-failed = נכשל
common-installed = מותקן
common-items = { $count ->
    [one] { $count } פריט
   *[other] { $count } פריטים
}
start-title = התחל
start-tagline = הנחיה אחת. הכל, נעשה.

agents-title = סוכנים
agents-search = חפש סוכנים ACP וCLI...
agents-empty = אין סוכנים תואמים
agents-empty-detail = נסה שם, זמן ריצה או ACP/CLI.
agents-install-failed = ההתקנה נכשלה
agents-updating = מעדכן...
agents-retrying = מנסה שוב...
agents-preparing = מתכונן...

extensions-title = הרחבות
extensions-search = חיפוש מותקן או Chrome Web Store...
extensions-relaunch = הפעל מחדש כדי להגיש בקשה
extensions-empty = לא הותקנו הרחבות
extensions-no-match = אין תוספים תואמים
extensions-empty-detail = חפש ב-Chrome Web Store למעלה ולחץ על Return.
extensions-no-match-detail = נסה שם אחר או מזהה תוסף אחר.
extensions-on = פועל
extensions-off = כבוי
extensions-enable-confirm = להפעיל את { $name }?
extensions-enable-permissions = הפעל את { $name } ואפשר:

lsp-title = שרתי שפה
lsp-search = חיפוש שרתי שפות, linters, פורמטורים...
lsp-loading = טוען קטלוג...
lsp-empty = אין שרתי שפה תואמים
lsp-empty-detail = נסה שפה אחרת, linter או פורמט.
lsp-needs = צריך { $tool }
lsp-status-available = זמין
lsp-status-on-path = ב-PATH
lsp-status-installing = מתקין...
lsp-status-installed = מותקן
lsp-status-outdated = עדכון זמין
lsp-status-running = ריצה
lsp-status-failed = נכשל

spaces-title = רווחים
spaces-new-placeholder = שם חלל חדש
spaces-empty = אין רווחים
spaces-default-name = שטח { $number }
spaces-tabs = { $count ->
    [one] כרטיסייה אחת
   *[other] { $count } כרטיסיות
}
spaces-delete = מחק שטח

team-title = צוות
team-just-you = רק אתה במרחב הזה
team-agents = { $count ->
    [one] אתה וסוכן אחד
   *[other] אתה וסוכנים { $count }
}
team-empty = אין פה אף אחד עדיין
team-you = אתה
team-agent = סוכן

services-title = שירותי רקע
services-processes = { $count ->
    [one] תהליך 1
   *[other] { $count } תהליכים
}
services-kill-all = הרוג הכל
services-not-running = השירות לא פועל
services-start-with = התחל עם:
services-empty = אין תהליכים פעילים
services-filter = סינון תהליכים...
services-no-match = אין תהליכי התאמה
services-connected = מחובר
services-disconnected = מנותק
services-attached = מצורף
services-kill = להרוג
services-memory = זיכרון
services-size = גודל
services-shell = מעטפת

error-title = שגיאה

history-search = היסטוריית חיפושים
history-clear-all = נקה הכל
history-clear-confirm = לנקות את כל ההיסטוריה?
history-clear-warning = לא ניתן לבטל זאת.
history-cancel = בטל
history-today = היום
history-yesterday = אתמול
history-days-ago = לפני { $count } ימים
history-day-offset = יום -{ $count }

settings-title = הגדרות
settings-loading = טוען הגדרות...
settings-stored = מאוחסן ב-~/.vmux/settings.ron
settings-other = אחר
settings-software-update = עדכון תוכנה
settings-check-updates = בדוק אם קיימים עדכונים
settings-check-updates-hint = בודק אוטומטית בעת ההשקה ובכל שעה כאשר העדכון האוטומטי מופעל.
settings-update-unavailable = לא זמין
settings-update-unavailable-hint = עדכון אינו כלול בגירסה זו.
settings-update-checking = בודק...
settings-update-checking-hint = מחפש עדכונים...
settings-update-check-again = בדוק שוב
settings-update-current = Vmux מעודכן.
settings-update-downloading = מוריד...
settings-update-downloading-hint = מוריד את Vmux { $version }...
settings-update-installing = מתקין...
settings-update-installing-hint = מתקין את Vmux { $version }...
settings-update-ready = עדכון מוכן
settings-update-ready-hint = Vmux { $version } מוכן. הפעל מחדש כדי ליישם אותו.
settings-update-try-again = נסה שוב
settings-update-failed = לא ניתן לחפש עדכונים.
settings-item = פריט
settings-item-number = פריט { $number }
settings-press-key = הקש על מקש…
settings-saved = נשמר
settings-record-key = לחץ כדי להקליט שילוב מקשים חדש

tray-open-window = פתח חלון
tray-close-window = סגור חלון
tray-pause-recording = השהה את ההקלטה
tray-resume-recording = המשך הקלטה
tray-finish-recording = סיים את ההקלטה
tray-quit = צא מVmux

composer-attach-files = צרף קבצים (/upload)
composer-remove-attachment = הסר את הקובץ המצורף

layout-back = חזרה
layout-forward = קדימה
layout-reload = טען מחדש
layout-bookmark-page = הוסף דף זה לסימניות
layout-remove-bookmark = הסר את הסימניה
layout-pin-page = הצמד את הדף הזה
layout-unpin-page = בטל את ההצמדה של דף זה
layout-manage-extensions = ניהול הרחבות
layout-new-stack = מחסנית חדשה
layout-close-tab = סגור כרטיסייה
layout-bookmark = סימניה
layout-pin = סיכה
layout-new-tab = כרטיסייה חדשה
layout-team = צוות

command-switch-space = החלף מקום...
command-search-ask = חפש או שאל...
command-new-tab-placeholder = חפש או הקלד URL, או בחר מסוף...
command-placeholder = הקלד URL, כרטיסיות חיפוש או > עבור פקודות...
command-composer-placeholder = הקלד / עבור פקודות או @ עבור מדיה
command-send = שלח (Enter)
command-terminal = טרמינל
command-open-terminal = פתח בטרמינל
command-stack = מחסנית
command-tabs = { $count ->
    [one] כרטיסייה אחת
   *[other] { $count } כרטיסיות
}
command-prompt = הנחה
command-new-tab = כרטיסייה חדשה
command-search = חפש
command-open-value = פתח את "{ $value }"
command-search-value = חפש "{ $value }"

schema-appearance = מראה
schema-general = כללי
schema-layout = פריסה
schema-layout-detail = חלון, חלוניות, סרגל צד וטבעת מיקוד.
schema-agent = סוכן
schema-agent-detail = התנהגות סוכן והרשאות כלי.
schema-shortcuts = קיצורי דרך
schema-shortcuts-detail = תצוגה לקריאה בלבד. ערוך את settings.ron ישירות כדי לשנות כריכות.
schema-terminal = טרמינל
schema-browser = דפדפן
schema-mode = מצב
schema-mode-detail = ערכת צבעים עבור דפי אינטרנט. המכשיר עוקב אחר המערכת שלך.
schema-device = מכשיר
schema-light = אור
schema-dark = כהה
schema-language = שפה
schema-language-detail = השתמש במערכת, en-US, ja, או כל תג BCP 47 עם קטלוג ~/.vmux/locales/<tag>.ftl תואם.
schema-auto-update = עדכון אוטומטי
schema-auto-update-detail = בדוק והתקן עדכונים בעת ההשקה ובכל שעה.
schema-startup-url = הפעלה URL
schema-startup-url-detail = ריק פותח את שורת הפקודה.
schema-search-engine = מנוע חיפוש
schema-search-engine-detail = משמש לחיפושי אינטרנט מ-Start ומשורת הפקודות.
schema-window = חלון
schema-pane = חלונית
schema-side-sheet = גיליון צד
schema-focus-ring = טבעת פוקוס
schema-run-placement = אפשר לעקוף את מיקום הריצה
schema-run-placement-detail = אפשר לסוכנים לבחור מצב חלונית ריצה, כיוון ועוגן.
schema-leader = מנהיג
schema-leader-detail = מקש קידומת לקיצורי אקורדים.
schema-chord-timeout = פסק זמן לאקורד
schema-chord-timeout-detail = אלפיות שניות לפני שפג תוקפו של קידומת אקורד.
schema-bindings = כריכות
schema-confirm-close = אשר סגירה
schema-confirm-close-detail = הנח לפני סגירת מסוף עם תהליך פועל.
schema-default-theme = ערכת נושא המוגדרת כברירת מחדל
schema-default-theme-detail = שם הנושא הפעיל מרשימת הנושאים.

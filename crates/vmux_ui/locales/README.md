# Locale catalogs

`en-US.ftl` is the source catalog. `ja.ftl` is the bundled Japanese catalog.

Additional languages do not require code changes. Copy the English catalog to `~/.vmux/locales/<BCP-47-tag>.ftl`, translate values without changing message IDs or variables, then set:

```ron
(
    appearance: (
        locale: "fr-FR",
    ),
)
```

Missing messages fall back to English. Region tags fall back to their base-language file.

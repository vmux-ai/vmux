# Locale catalogs

`en-US.ftl` is the source catalog. Vmux bundles 115 locale tags, including Japanese, regional Chinese and Portuguese variants, and broad ISO 639-1 coverage.

Every bundled catalog has the same message IDs and variables as English. All non-English catalogs, including Japanese, are context-aware LLM localizations rather than literal translations. Native corrections are welcome.

Each catalog's `locale-name` is its autonym and is shown unchanged in the language picker.

Custom languages and overrides do not require code changes. Copy the English catalog to `~/.vmux/locales/<BCP-47-tag>.ftl`, translate values without changing message IDs or variables, then set:

```ron
(
    appearance: (
        locale: "fr-FR",
    ),
)
```

External catalogs take precedence over bundled catalogs. Missing messages fall back to English. Region tags fall back to their base-language file.

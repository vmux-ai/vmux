# Feature flags

Open `vmux://features/` to inspect feature flags. The page uses the same lists, detail panels and controls as the other Vmux managers.

Choose an environment and search the complete list of published and declared flags. Select a flag to see its published value, available bundled default and proposed draft value. An undeclared default is shown explicitly rather than guessed. If loading a different environment fails, its error is shown without values from the previous environment.

## Review and publish

Editing a draft does not change published values. Save the draft, open Changes and inspect the complete before/after set. Search filters do not hide changes from this review. Mark the exact revision as reviewed, then choose Publish on the Changes page.

The server checks the operator's permission, the draft revision and the current published revision. Another change, an unsaved edit or an unavailable connection prevents publication. Refreshing preserves unsaved proposed values. A publication with an uncertain result keeps its identity for a retry; the page does not treat a missing response as success. When refresh confirms that the reviewed draft has been replaced or changed, editing and review become available again.

History records the published revision, operator and time. Prepare rollback stages historical values as a new draft; save, review and publish it through the same workflow. Published revision numbers continue forward and history remains intact.

The first implementation manages boolean flags. Richer targeting and experiment analysis are not available in this page yet. Production inspection is read-only; local administration requires the explicitly configured development operator. The server, rather than page visibility, owns authorization.

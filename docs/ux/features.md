# Feature flags

Open `vmux://features/` to inspect declarations and effective decisions. Select an environment and search flags in the existing manager page. The inspector shows type, class, value, source, reason, variant, permitted context, snapshot age, expiry, lifecycle, development override and durable retention status.

Inspection of bundled declarations and local decisions works offline. Development overrides are available only when the application permits them. An override cannot clear a retained operational disable. A newer authenticated cloud-on must be durably accepted before Live reopens; declaration expiry, retirement, cache, defaults and conditional refresh cannot clear it.

Authorized administration uses the selected environment and the existing Account credential broker. The server derives current permissions. Edit boolean values directly or edit a typed configuration containing its value, ordered targeting rules, rollout percentage and experiment generation. Review the complete changes, save the draft, review its saved revision and publish explicitly. A conflicting or failed publication preserves the editable draft and current published configuration. Retrying a lost publication response retains the operation identity.

Audit history identifies the actor and publication revision. Use “Load older history” to read earlier pages without changing the published configuration or editable draft. Authorized rollback publishes historical values at a newer revision and preserves its operation identity when retrying a lost response. Legacy local operators stage historical values for the same save, review and publish flow. Changing environments or credentials cannot adopt a completion from the previous selection.

Sticky experiment assignments are saved before behavior can activate. Inspection shows assignment capacity separately from physical storage failures. Explicitly retire an inactive experiment generation to reclaim capacity; active generations cannot be removed. Exposure is recorded when behavior activates, not when an inspector reads a decision.

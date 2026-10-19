# BYOK setup: Terax comparison and delivery checklist

Source inspected on 2026-10-19: Terax commit
`5502f826a6476556182ddbe1466ec2ba94c0fd47`. This is a source review, not a
live test of Terax or its providers. No upstream application code was copied.
skillissue keeps its native Rust/GPUI architecture.

## What the reference actually implements

- [ProviderKeyCard](https://github.com/crynta/terax-ai/blob/5502f826a6476556182ddbe1466ec2ba94c0fd47/src/settings/components/ProviderKeyCard.tsx)
  offers provider names, Get key links, masked entry/reveal, replace/remove,
  trimmed keys, pending state and save errors. Its Connected badge means a key
  exists; it does not establish successful inference. Prefix checks can help
  with hints but should not reject new key formats in skillissue.
- [ModelsSection](https://github.com/crynta/terax-ai/blob/5502f826a6476556182ddbe1466ec2ba94c0fd47/src/settings/sections/ModelsSection.tsx)
  separates provider selection, custom endpoints and speech-to-text selection.
  Local/custom endpoint Test calls `lm_ping` and treats a positive HTTP status
  as reachable. Its status correctly says "Reachable — server responded."
  This is not evidence of valid credentials, model access or audio support.
- [Agent construction](https://github.com/crynta/terax-ai/blob/5502f826a6476556182ddbe1466ec2ba94c0fd47/src/modules/ai/lib/agent.ts)
  uses distinct SDK adapters for OpenAI, Anthropic, Google and other protocols.
  A provider URL preset alone cannot add equivalent support to skillissue.
- [Key storage](https://github.com/crynta/terax-ai/blob/5502f826a6476556182ddbe1466ec2ba94c0fd47/src/modules/ai/lib/keyring.ts)
  separates named-provider keys and custom-endpoint IDs; its
  [native backend](https://github.com/crynta/terax-ai/blob/5502f826a6476556182ddbe1466ec2ba94c0fd47/src-tauri/src/modules/secrets.rs)
  uses the system keyring on Windows. skillissue already scopes credentials to
  the profile and endpoint and does not return saved secrets to the UI.

## Compact setup update

The form now has Text, Audio and Images rows with provider dropdowns and key
fields. Common presets fill URLs and starter models. Advanced holds manual
settings and checks. Separate image routing, scoped credentials and atomic saves
are implemented. See [setup](providers.md#byok-setup) for the current flow.

## Implemented in skillissue

- [x] Separate optional transcription URL and credential scope, with the existing
  transcription model field. Empty URL retains the previous shared connection.
  Existing settings migrate through defaults without moving keys.
- [x] Chat-only mode: disable BYOK audio and recording returns a setup message
  before opening devices. Chat remains available.
- [x] Text, Audio and Images credentials and settings participate in save rollback. Changes wait
  for active recording, answers, notes and queued transcription. Queued audio
  retains a snapshot of its own endpoint, model and key.
- [x] Explicit, cancellable checks of saved chat/audio connections. Requests are
  authenticated GET `/models`, bounded to 1 MiB and 4096 entries with a 15-second
  request timeout. Redirects are disabled. Edits, cancellation and provider
  revision changes invalidate results. No inference or audio is sent by checks.
- [x] Successful checks report model-list access, not verified inference or media
  capabilities. Unsupported listing leaves manual IDs available. Authentication,
  quota, network and compatibility errors provide corrective guidance.
- [x] Filterable model picker (up to 128 matches displayed), manual model IDs,
  and the [full provider picker](byok-compatibility.md). Switching presets preserves
  unfinished keys within their role and endpoint. Common hosted presets supply
  starter models; local and custom servers use manual model IDs.
- [x] Masked key entry, accessible show/hide for entered keys only, explicit key
  removal, draft preservation after failed saves, key format validation and
  endpoint-scoped Windows credentials. Saved secrets are not returned to the UI.
- [x] Trim pasted model IDs, reject full operation URLs in base URL fields and
  explain image/Smart requirements in the settings form.

Preset URLs were checked against the official
[OpenAI model API](https://developers.openai.com/api/reference/resources/models/methods/list),
[Ollama compatibility guide](https://docs.ollama.com/api/openai-compatibility) and
[LM Studio compatibility guide](https://lmstudio.ai/docs/developer/openai-compat).
These are endpoint presets, not a guarantee of support for every app operation.

## Verification

The workspace test suite passed after the compact setup changes (170 core,
107 desktop, 7 domain and 15 platform tests; one interactive platform test
intentionally ignored). An additional image-model discovery regression passed,
bringing core coverage to 171 tests. Clippy with warnings denied also passed.

New coverage includes audio credential isolation, restart, endpoint switching,
removal, failed-publication rollback of both keys, busy/queued work protection,
model-list deduplication and key-echo filtering, cancellation, stale UI results,
audio draft preservation and chat-only capture rejection before device access.
No real provider key or paid inference request was used for these checks.

## Remaining acceptance and extensions

1. **Live acceptance:** new profile → save → check → ask → transcribe → restart,
   with a chosen real provider and physical Windows audio devices. Exercise key
   replacement, expired credentials, credit exhaustion and unsupported audio.
   Verify keyboard navigation and narrow layouts interactively. Automated UI
   rendering tests do not replace this check.
2. **Capability metadata:** model listing cannot establish streaming, vision,
   audio or Responses web-search support. The UI keeps those capabilities
   unverified and explains requirements. Add trustworthy provider-specific
   metadata before automatic capability-based selection or gating.
3. **Optional inference probe:** a separate explicit action could send synthetic
   content and disclose possible charges. Do not infer permission from Save or
   Check connection. Do not replay accepted POSTs automatically.
4. **Additional native protocols:** Anthropic and Gemini now have presets for
   their official Chat Completions compatibility layers. Full native thinking,
   caching, grounding, workspace headers and agent tools remain separate work.

The native onboarding foundation is implemented; live-provider and physical-device
acceptance remain open.

# Provider connections

The application has one local session engine and two provider modes. Selecting a
provider never uploads previous sessions by itself.

New BYOK profiles select DeepSeek `deepseek-flash` for Text and Images and
Deepgram `nova-3` for Audio. Existing settings, including legacy files with
omitted fields, keep their previous defaults. Provider keys are still required.

For Deepgram, select **Settings > AI provider > BYOK > Audio > Deepgram**,
enter the key, and Save. Test sends half a second of synthetic silence and may
consume credits. The base URL is `https://api.deepgram.com/v1`; the app uses
`wss://api.deepgram.com/v1/listen` with Token authentication. Model discovery
reads Deepgram's `stt` catalog and offers supported streaming Nova-3 IDs.
An empty language means English. Use a supported language code such as `id`
or `zh`, or `multi` for Deepgram's supported multilingual set.

Deepgram capture sends continuous mono 16 kHz PCM16 in 100 ms frames, with a
separate WebSocket for each enabled audio source. The transcript drawer shows
interim text; only final results enter saved history and answer context. Final
records retain word timings, confidence and provider speaker numbers when supplied.
Speaker numbers identify voices within one connection, not verified people.
Capture sends silence too, so leaving both sources enabled can increase usage.
No ffmpeg, websocat, or jq installation is required.

Ask and Stop send `CloseStream` and wait for final results and completion metadata.
After Ask, newly captured audio waits in a bounded queue for a new connection.
Speaker numbering can restart at that boundary. A disconnect or full queue stops
capture and records a visible gap; accepted audio is never replayed automatically.
The Audio Test button still uses a short, separate synthetic stream.

Protocol references: Deepgram [CloseStream](https://developers.deepgram.com/docs/close-stream),
[KeepAlive](https://developers.deepgram.com/docs/audio-keep-alive), and
[speaker diarization](https://developers.deepgram.com/docs/diarization).

Before answering, the app sends pending captured speech for transcription and
keeps the question pending until those segments finish. The waiting message shows
the pending count and elapsed time; Cancel stops the question. Speech captured
after the flush does not extend this wait. The app sends the answer request once,
without requiring you to resend the question. Reference search has a separate
ten-second timeout. The transcript
drawer shows each audio segment's progress. Failed audio requests leave a visible
gap and require user action; the app never retries them automatically.
Transcription requests may consume provider credits.

BYOK also offers **Palepale account** independently for Text, Audio, and Images.
Connect Palepale once, return to BYOK, then select it for the roles you want.
For example, keep DeepSeek for Text and use Palepale for Audio. Selecting Palepale
loads its role-compatible models automatically and fills an empty model selection
from the account's saved model when compatible, otherwise from the available list.
Save to apply the selection. Existing model choices are preserved.
Palepale rows share the paired account credential and use its
published model capabilities; other rows keep their own provider keys.

See [BYOK setup roadmap](byok-roadmap.md) for the Terax source comparison,
completed input improvements and prioritized onboarding work.

| Operation | BYOK | Palepale |
| --- | --- | --- |
| Chat and notes | Configured Chat Completions endpoint | Fixed hosted Chat Completions endpoint with metered SSE |
| Models | Manual IDs or filtered results from explicit model-list checks | Public catalog with published capability checks |
| Authentication | Endpoint-scoped provider key | Browser-approved device pairing |
| Transcription | Deepgram live WebSocket, multipart `/audio/transcriptions`, or supported audio chat | Independent published audio model |
| Screenshots | Provider must accept image input | Published image capability required |
| Smart effort | Low/High reasoning on supported Text and Images models | Published reasoning efforts required |

Palepale pairing exposes only a short public approval code to the browser. The
private device code and released credential remain in the native process. Approved
credentials are saved in Windows Credential Manager; interrupted activation uses a
separate pending scope and never becomes active automatically.

Settings reports a hosted connection as ready only after a usable committed model
and an authenticated, read-only account availability check. Public catalog data is
not proof of entitlement, credit, or successful inference.

Connection, model, and credential changes wait for active recording, accepted
requests, notes, and pending transcription to finish. Appearance and other
unrelated preferences can still be saved during an answer. Requests retain the
provider and session snapshot they were prepared with.

### Palepale account and usage

AI Providers projects the server's authenticated `GET /api/me` response into
wallet availability, reservations, lifetime wallet spending limit, balance fallback,
and active passes. Pass bars show remaining allowance after reservations, with
expiry and full-refill timestamps as supplied by the server. Pass allowances and
wallet balances remain separate; a pass is not presented as an auto-renewing subscription.

`GET /api/me/overview?days=0` supplies all-time, account-wide request counts,
charged microdollars, unresolved requests, input/output tokens, cached input,
and reasoning tokens. The chart compares input and output on the same scale;
cached and reasoning tokens are subsets, not additional totals. Amounts retain
six decimal places. Missing data is unavailable, never fabricated as zero.
The compact panel uses GPUI bar charts for tokens and progress indicators for
pass allowance and wallet limits. Details expands exact token counts, pass dates,
reservations, scope, and wallet fallback.

Refresh status reloads both snapshots. The main settings window also refreshes
about once a minute while AI Providers is visible. Results sync to the overlay,
and connection changes discard stale results. Usage failures do not erase a
successful account check. Account IDs, key hints, and pass IDs are not retained
in the UI projection. The panel reports the last completed refresh, not a live
billing stream. This contract was checked against the local palepale-server
controllers, account service, and usage repository; deployed authenticated
responses still require live acceptance.

Failed chat POSTs are not automatically replayed, even when no answer text has
arrived: the provider may already have accepted and billed the request. Automatic
recovery needs a provider-supported idempotency or stream-resume contract.

No provider key is written to settings JSON, history, catalog requests, UI events,
or logs. The default profile may use `SKILLISSUE_API_KEY` as a BYOK compatibility
fallback; non-default profiles and Palepale never use that environment variable.

Connecting Palepale, refreshing status, or reopening a connected profile can fetch
its public catalog and authenticated account availability. Recording sends audio
continuously with Deepgram, or in segments with other adapters; Ask and notes send
the selected context to the configured provider.
Smart changes reasoning effort for the selected model. Provider
retention, billing, and privacy terms apply to these requests; review them before
connecting or enabling capture. There is no analytics or automatic update service.

## BYOK setup

Choose a provider and paste its key in each row, then Save:

- **Text:** answers and notes.
- **Audio:** microphone and system audio transcription. Choose Deepgram, OpenAI, Groq,
  Gemini, Mistral, OpenRouter, a custom speech service, Use text provider, or Off.
- **Images:** screenshot questions. Choose OpenAI, Anthropic, Gemini, DeepSeek,
  Groq, Mistral, OpenRouter, a custom image-capable service, Use text provider, or Off.

Common cloud presets fill the URL and a starter model. Advanced contains URLs,
model overrides, connection checks, key removal and the system prompt. Each row
also has a Model dropdown and Test button. Saving BYOK automatically loads
model lists for enabled roles; Refresh models retries discovery. Lists stay scoped to their role and endpoint; they are
filtered by input/output modalities and provider capability fields. Missing
metadata falls back to exact reviewed preset IDs present in the response;
unknown models stay out of role dropdowns. Custom IDs remain in Advanced.
OpenRouter Audio fetches its transcription catalog. Audio chat models are only
offered where the app has an audio-chat adapter. Large lists have a filter. Shared Images
uses the Text model, so changing its dropdown also changes Text. Local
servers and providers without a starter model need a model ID in Advanced.
Preset models may require account access or a model change.

Test sends a small request to the saved provider and selected model. Text uses
a fixed prompt, Images adds a generated test PNG, and Audio uses half a second
of synthetic silence. No saved conversation or captured media is sent. Tests
may use API credits. Success means the sample request was accepted; it does not
measure transcription accuracy or image understanding. Changes and cancellation
discard stale results. Requests time out after 30 seconds.

A separate Images provider receives the screenshot and question context and
answers that request. Text-only questions and notes keep using Text. Images
means screenshot understanding, not image generation. Smart stays visible in the
composer. Enable it and click Low effort / High effort to change the shared setting
for Text and Images. Off leaves the provider default unchanged. Notes and audio
transcription do not use this control. Smart no longer starts web research.

OpenAI reasoning models and Gemini thinking models use `reasoning_effort`.
DeepSeek Flash/V4 also enables thinking. Reviewed Claude 4.5/4.6 models use a
2,048/8,192-token thinking budget, with 16,384 maximum output tokens; their
compatibility endpoint ignores `reasoning_effort`. OpenRouter requests reasoning
and requires parameter support. Custom compatible endpoints receive
`reasoning_effort` and must validate it. Unsupported preset/model combinations
fail before sending; model names never silently change to enable Smart.
Palepale checks published efforts and skips automatic model routing while Smart
is enabled. Higher effort can increase latency and token use.

Parameter references: [OpenAI](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create),
[Gemini](https://ai.google.dev/gemini-api/docs/openai),
[DeepSeek](https://api-docs.deepseek.com/guides/thinking_mode/),
[Claude](https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk),
[OpenRouter](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens).

Blank keys keep saved keys. A masked saved-key label remains visible after saving;
replacement keys and pending removal have separate labels. Saved secrets are
never copied into input values. Use text provider shares the Text key without copying
it into another field. Separate Audio and Images keys have independent scopes,
even if their URLs match Text. Switching providers keeps unfinished keys with
their original endpoint while the form is open. Keys never sync into other windows.

Saved configuration updates the dashboard, overlay and settings tabs. Unsaved
form fields survive tab switches and background updates. Saves wait for active
requests and recording; failed saves preserve drafts and roll back key changes.

Advanced connection checks send only a model-list request. A successful response
does not prove model access, credit, image or audio support. Checks never send
meeting content or paid inference. Editing or cancelling invalidates results.

See the [provider compatibility matrix](byok-compatibility.md) for provider-specific
limits. The default profile can use `SKILLISSUE_API_KEY` for Text; separate Audio
and Images connections never use that environment fallback.

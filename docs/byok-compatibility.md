# BYOK provider compatibility

Reviewed against provider documentation on 2026-10-19. These are protocol and
setup support statements, not live-account certifications. The application stays
Rust/GPUI; no Tauri or JavaScript runtime is required.

## Setup flow

The default form has Text, Audio and Images rows. Choose providers, paste keys,
and Save. Common cloud presets supply a starter model and URL. Audio and Images
can share Text or be turned off. Advanced contains manual URLs/models, model-list
checks and key removal. Local/custom servers and presets without a starter model
need an explicit model choice there. Account access is never assumed.

Smart effort is shared by Text and Images and persists when providers change.
Unsupported reasoning combinations report an error before sending. Shared audio is
turned off when no speech preset is available; shared screenshots are turned off
unless the preset has a starter vision model. Separate connections are preserved.
Saved configuration syncs across windows; unfinished keys stay in the editing
window and are never transferred to another endpoint.

Images routes screenshot questions and their conversation context to that
provider. Text-only questions and notes continue through Text. Images is screenshot
understanding, not image generation. OpenAI, Anthropic, Gemini, DeepSeek, Groq,
Mistral and OpenRouter have starter image models. Groq uses a separate vision
default because its text default is text-only. Other compatible image models
can be configured in Advanced.

## Provider matrix

Chat presets use the provider's Chat Completions-compatible interface for
streaming answers and notes. Deepgram is an Audio-only preset.

| Provider | Chat setup | Screenshots | Transcription setup | Smart / native provider tools |
| --- | --- | --- | --- | --- |
| [OpenAI](https://developers.openai.com/api/reference/resources/chat/subresources/completions) | OpenAI key; choose chat model | Vision model required | Shared speech model or separate connection | Smart requires a supported reasoning model; no general tool execution |
| [Anthropic / Claude](https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk) | Official compatibility endpoint; workspace-scoped key | Compatible Claude image input | Separate speech connection | Smart uses thinking budgets on reviewed models; caching, workspace selection headers and native tools not integrated |
| [Google Gemini](https://ai.google.dev/gemini-api/docs/openai) | Google AI Studio key; compatibility endpoint | Gemini Flash preset | Gemini Flash via audio chat; same key can serve all three roles | Gemini grounding and native tools not integrated |
| [DeepSeek](https://api-docs.deepseek.com/guides/vision/) | DeepSeek key; defaults to `deepseek-flash` | Flash supports image and screenshot input | Separate speech connection | Native Search/Fetch not integrated |
| [Deepgram](https://developers.deepgram.com/docs/getting-started-with-live-streaming-audio) | Not offered for Text | Not offered for Images | Nova-3 continuous WebSocket; separate key; interim text and final word/speaker metadata | Speaker IDs are local to each connection |
| [Groq](https://console.groq.com/docs/vision) | Groq key; current chat model | `qwen/qwen3.8-27b` image preset | Shared Whisper speech model or separate connection | Groq built-in tools not integrated |
| [xAI / Grok](https://docs.x.ai/developers/model-capabilities/text/generate-text) | xAI key; supported chat model | Model-specific | Separate speech connection | Native xAI search/tools not integrated |
| [Cerebras](https://inference-docs.cerebras.ai/resources/openai) | Cerebras key; model or dedicated endpoint ID | Model/account-specific; omit unsupported image detail field | Separate speech connection | Native tools not integrated |
| [Mistral](https://docs.mistral.ai/api/endpoint/audio/transcriptions) | Mistral key; compatible chat model | Mistral Small preset | `voxtral-mini-latest`; multipart upload, same key | Diarization and native agents/tools not integrated |
| [OpenRouter](https://openrouter.ai/docs/guides/overview/multimodal/stt) | OpenRouter key; full provider/model ID | `openai/gpt-4.1-mini` preset | `openai/whisper-large-v3`; compatible multipart upload | Routing/plugins are not equivalent to app tool execution |
| [Ollama](https://docs.ollama.com/api/openai-compatibility) | Start local server and install model; key optional | Loaded vision model required | Separate speech connection | Local model tool execution not implemented |
| [LM Studio](https://lmstudio.ai/docs/developer/openai-compat) | Enable server, load model; key if server requires one | Loaded model-specific | Separate speech connection | Local model tool execution not implemented |
| [MLX](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/SERVER.md) | Run server on Apple silicon; use HTTPS or a localhost tunnel from Windows | Server/model-specific; unverified | Separate speech connection | No Windows MLX runtime is installed by the preset |
| Custom OpenAI-compatible | User-supplied base URL, key and model | Server/model-specific | Compatible multipart service required | Requires the exact app protocol; not arbitrary vendor API compatibility |

## Audio research and limits

[Artificial Analysis speech-to-text](https://artificialanalysis.ai/speech-to-text/non-streaming)
compares recognition accuracy, latency and cost. Its rankings help shortlist
models; they do not establish API compatibility or account access. Deepgram uses
continuous audio streaming. Other adapters use bounded audio segments.

The Audio picker includes Deepgram, OpenAI, Groq, Gemini, Mistral and OpenRouter.
Deepgram uses its native WebSocket protocol. Gemini uses documented `input_audio`
chat messages; the others use multipart transcription.
Mistral omits the unsupported `response_format` field. OpenRouter may route to
additional speech vendors with an appropriate model ID in Advanced, but direct
ElevenLabs, AssemblyAI, Azure and Google Cloud Speech keys are not
supported by those presets. Dedicated integrations remain separate work.

Multimodal is model-specific: text support does not imply image or audio support.
These additions have local protocol tests, not live provider certification.

## What a check proves

Check connection sends only GET `/models`. Anthropic listing uses the native
Models endpoint with its version/key headers; chat uses its documented Bearer
compatibility route. Model lists may be public (notably OpenRouter), so a
successful list does not establish valid credentials, credit or inference access.
No paid inference, screenshot or audio is sent by this action.

Saving BYOK now loads model lists automatically for enabled roles. One failed
role does not discard another role's list. Refresh models retries discovery.
The separate Test button sends synthetic sample data to the saved model and
may consume credits. It reports request acceptance or an error, times out after
30 seconds, and never reads session history or captured media.

The app does not execute model-requested tools, run an agent loop or expose MCP
servers to models. A provider's advertised function calling support does not add
those capabilities. Such a response returns an explicit unsupported-tool error.
Terax has a broader agent tool layer; this change covers provider setup and chat
compatibility, not all of Terax's agent functionality.

## Validation and limits

Regression coverage checks exact endpoint recognition (including hostile
lookalikes), provider request shapes, preserving separate audio and unsaved
provider drafts, and the incomplete-setup guard. Existing coverage checks bounded
listing, stale results, cancellation, secret isolation and audio routing.
Live inference, account permissions, model availability and device capture still
require acceptance with the user's selected services. Models are discovered
rather than hardcoded as universally available defaults.

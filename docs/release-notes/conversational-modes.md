# Shorter, more natural answers in every mode

All modes now share KISS and no-ai-slop writing instructions. Answers default to
plain, conversational language and the smallest complete response. Formal wording
is reserved for requests and documents that need it.

- General, existing custom modes, screen answers, saved-session questions and
  generated notes all receive the shared policy.
- Meeting notes, Interview practice and Technical review templates add specific
  guidance for decisions, feedback and technical conclusions.
- Conclusions state the supported outcome and next action without repeating the
  response or adding a motivational closing line.
- Notes keep their sections and citations. Detailed requests and complete code
  can exceed the default length. Saved prompts and transcripts are unchanged.
- Request-size checks now include the shared writing instructions.

This release also includes continuous Deepgram audio and the word-timing fix
from the recent previews. A malformed optional word timestamp no longer discards
recognized text or stops capture. Shutdown preserves the original provider error.

Download the Windows x64 setup executable to install. The standalone executable
requires the Visual C++ x64 runtime. Source, notices and SHA-256 checksums are
included with the downloads.

Published on the stable update channel. The binaries are unsigned. Physical
device, clean-machine and long-session acceptance and complete dependency license
coverage remain open. Request tests verify the writing policy is sent; actual
wording still depends on the selected model.

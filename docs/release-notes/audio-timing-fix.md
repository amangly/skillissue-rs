# Fix microphone capture stopping on word timing

Deepgram word timestamps could trigger an overly strict validation check and
stop capture. A later closed-queue error then hid the original cause.

The app now accepts word alignment across result boundaries and omits malformed
optional timing without discarding recognized text. Required result timing,
response limits and credential redaction remain validated. Shutdown preserves
the original provider error; failed audio is never replayed automatically.

Regression tests cover boundary alignment, malformed optional timing, transcript
persistence before drain completion and late callbacks after a disconnect.
A live 15-second Nova-3 sample completed with 12 interim and five final results.
Physical microphone and long-session acceptance remain open.

Download the Windows x64 setup executable to install. The standalone executable
requires the Visual C++ x64 runtime. Source, notices and SHA-256 checksums
accompany the downloads.

This is an unsigned development preview. Complete dependency license coverage
remains unresolved.

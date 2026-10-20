# Continuous audio and cited meeting notes

Deepgram Nova-3 now streams microphone and system audio continuously. The
transcript drawer shows provisional text while speech is being recognized;
saved history and answers use final results.

- Questions wait for pending transcription and send once it finishes. The
  previous four-second cutoff no longer makes you resend the question.
- Final transcripts retain word timings, confidence and speaker numbers when
  supplied. Speaker numbers apply within one connection and can restart after Ask.
- Generated notes request transcript citations and action items with owners,
  due dates and evidence. Citation buttons open the source excerpt. Markdown
  exports retain matching transcript numbers.
- A full queue or failed audio connection stops capture and reports a gap.
  Accepted audio is never replayed automatically.

Download the Windows x64 setup executable for installation. The standalone
executable requires the Visual C++ x64 runtime. Source, notices and SHA-256
checksums accompany the downloads.

Local validation passed 349 tests with two ignored, formatting, Clippy, the
release build and a startup smoke check. One 15-second live Nova-3 sample
returned its first interim text after 2.03 seconds. Accuracy was not scored.

This is an unsigned development preview. Real-device and long-session acceptance,
broader accuracy benchmarks, full-meeting retrieval and complete dependency
license coverage remain open. Continuous capture sends silence too; enabling
both audio sources can increase provider usage.

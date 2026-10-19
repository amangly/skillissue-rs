# skillissue core

The UI-independent application engine owns sessions, transactional preferences, provider requests, durable notes/history and document context. It depends on `skillissue-domain`; it has no dependency on the native renderer or desktop binary.

`engine/` separates lifecycle, configuration, session operations, capture and assistance. `provider/` separates transport policy, streaming/framing, transcription, notes and web citations. Explicit audio/screenshot/startup and storage modules contain the I/O adapters. See [architecture](../../docs/architecture.md) for ownership and lock contracts.

Run `cargo test -p skillissue-core --locked`. Tests use isolated temporary profiles and loopback provider fixtures, never production credentials or registry startup changes.

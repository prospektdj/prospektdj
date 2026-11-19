# Prospekt DJ 2.4 PRE-BETA — implementation in progress

This 2.4 PRE-BETA publishes the implemented development checkpoint, not a completed implementation of the attached 11-point specification. The application remains Rust/egui/eframe. Current README/release pages use the six genuine 2.4 screenshots; the old 2.3 screenshots remain historical assets.

| Point | Implemented in this checkpoint | Still required before accepting the complete specification |
|---|---|---|
| 1 | SQLite WAL Cue banks, source-PCM SHA-256 identity, automatic ordered writes from engine commands, cross-deck synchronization, zero-time/deletion/restart tests, shutdown drain, future schema rejection. | Explicit path-repair/conflict UI and broader migration/crash-recovery testing. PCM identity is decoder-dependent; it is not a cross-decoder acoustic fingerprint. |
| 2 | Persistent ratings/comments, exact rating filters, independent favorites, local MASTER-time history, recent/frequent views, prior-session date, real cached waveform previews and Cue markers, four read-only import adapters and preview/report UI. | Full historical-event import, earliest-known aggregation across sources, full Serato/native and Mixxx Cue/history support, explicit matching/conflict resolution. Imported counters and original date strings are retained separately, not summed into a fabricated total. |
| 3 | A/C bend slider between jog and mixer; temporary bend behavior retained. | Native visual/regression checks at all requested sizes/DPI. |
| 4 | Compact Auto Loop length cells and Manual Loop controls, shared sampler bank. | Manual IN state and saved-loop re-entry after EXIT are implemented and covered by an engine regression test. Additional native interaction checks remain. |
| 5 | Larger PLAY/SYNC, hover/state animation, brief one-shot SYNC indication, reduced-motion setting. | Further accessibility and touch testing. |
| 6 | System RAM and measured frame-window FPS; driver-unavailable VRAM shown as N/D; existing background CPU/disk worker retained. | Native dedicated/shared GPU memory adapters. No physical GPU reading has been verified. |
| 7 | Reference topology: mirrored vertical EQ columns, green HIGH/yellow MID/red LOW, shared FX filter, independent pre-EQ smoothed GAIN, faders/PFL/meters, shared crossfader, #1A1A1A mixer, global header MASTER/PHONES/REC. Legacy MIDI gain mapping remains the channel fader; new `pre_gain` action controls the independent stage. | Final visual comparison and physical controller/headphone checks. |
| 8 | Category sidebar/search, working refresh limit, saved UI scale, statistics visibility/reduced motion, waveform presets/custom palette, import category, retained audio/MIDI/recording controls. | Complete categorized search/reset, backup/restore UI, all requested density/columns/backend controls. |
| 9 | Small shared knobs, deck-local 2×2 sampler, required four colors, load/trigger/clear/capture/level, shared playing state, removal of former large Samples tab. | Narrow-window placement and additional interaction testing. |
| 10 | Existing ten languages preserved. | Full 24-EU-language translation and complete new-flow/error/tooltip localization are **not implemented**. No English-filled catalog is being presented as a finished translation. |
| 11 | Consulted OpenFadeR history and XML adapter documentation plus Mixxx schema as reference. | No OpenFadeR application code was copied. Additional source-format verification remains necessary. |

## Import adapters and limits

- **rekordbox**: DJ_PLAYLISTS collection XML; TrackID, URI/path, title/artist/album/genre/year, BPM/duration, rating 0..255 in steps of 51, comments, DateAdded/LastPlayed/PlayCount, Type=0 and Num=0..7 Hot Cue, playlist track references. Memory Cue Num=-1 is excluded. This is not a full-session database reader.
- **VirtualDJ**: VirtualDJ_Database/Song XML, Tags/Infos/Scan/Comment/Poi. BPM seconds-per-beat is converted to beats/minute; Cue Num=1..8 maps to slots 0..7. Separate playlist/history files are not yet imported.
- **Mixxx**: read-only SQLite transaction, including committed WAL contents; library.location joined to track_locations.id; optional columns detected; ratings/comments/aggregate play fields plus crates/playlists. Tested fixture uses this actual schema family. Native Cue positions and history-event timestamps are not yet imported.
- **Serato**: UTF-8 comma-separated history CSV **with an explicit Location/File Path/Filename/File column**. Quoting and Unicode are tested. Title-only exports are rejected rather than guessed. Native crates/databases and complete marker/history variants remain pending.

Import writes only the Prospekt database; existing manual fields/occupied Cue slots win. Source payloads are upserted by source file and source record ID, retaining provenance and date precision. Completed per-track transactions survive cancellation; repeating does not multiply those records. `last-import-report.json` records the source, recognized format, applied count and limitations/conflicts/errors. This is not yet a complete import-conflict manager.

## Data and realtime boundaries

- `tracks.sqlite`: Cue banks, user metadata, waveform cache, local play events, source import payloads.
- The existing catalog JSON/crates/favorites are retained. Rating/history fields in catalog entries are filter indexes mirrored from SQLite; rescanning preserves them.
- Source identity is calculated during background decoding, excluding tags and paths. Exact duplicate audio shares its bank. Different audio does not merge just because artist/title match.
- SQLite writes, waveform decoding, metadata reading and OS statistics do not run in the render callback. Engine event order associates Cue changes with the actual loaded Track ID; UI polling is not the source of Cue writes.
- Local history counts real rendered MASTER time, default 30 s (or half of a short track), once per load/performance; pause/seek/Cue/loop do not generate repeated events. PFL-only playback is excluded. Short noise/false-positive protection and cross-import event deduplication still need broader verification.

New third-party crates use their own upstream licenses in `licenses/dependencies`. Application code remains AGPL-3.0-or-later. SQLite is a bundled external C dependency; the native application logic remains Rust.

## Verified development checkpoint and publication

- 74 Rust tests passed with the CPAL ALSA-null test and optional FFmpeg codec tests enabled. The importer fixtures include rekordbox URI/Unicode/rating/Memory Cue, VirtualDJ tempo units, Mixxx committed-WAL consistency and source immutability, Serato CSV quoting/path requirements, and repeated-import/manual-data preservation.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and optimized Linux `cargo build --locked --release` passed.
- Additional regressions cover Cue zero/deletion/restart/cross-deck sharing, late-import deletion protection, source identity, independent gain/fader mute, MASTER-time history excluding PFL, exact rating filters/rescan, future database versions, Manual Loop IN/EXIT/RE, and waveform preset/Cue palette separation.
- Native UI screenshots are captured from the actual optimized 2.4 executable at 1920×1080, 1280×720, and 150% DPI. Two- and four-deck layouts were inspected. At 1280×720 four-deck controls require vertical scrolling; this is an explicit remaining layout limitation. The right TRACK INFO panel also scrolls independently.
- Native UI interaction restored Cue slots 1/3/8 after relaunch, including exactly `0.0` seconds, and loading the same audio into A/B displayed the same bank.
- Physical controller, independent physical headphone outputs, physical GPU VRAM telemetry, Windows compilation and hardware latency were not tested in this checkpoint. ALSA-null tests are not hardware/latency certification.
- Initial Cue-bank hydration currently happens before stream startup; making that bootstrap fully asynchronous is still required for the specification's strict no-database-I/O-on-UI requirement.
- Initial checkpoint `a525fa6` was local only. The user subsequently authorized publication of this implemented state as **2.4.0-prebeta.1**, including source, updated bilingual README and five Linux/Windows packages. See the release workflow and `docs/RELEASE-2.4.md`; unfinished requirements remain explicitly listed above.
- Screenshot build metadata retains its original capture-time publication flag and base commit. It records the genuine local build, not a claim that local and CI executables are byte-identical.

### Native screenshots

[Two decks](screenshots/v24-main-1920.png) · [Four decks](screenshots/v24-four-1920.png) · [1280×720](screenshots/v24-small-1280.png) · [Four decks at 1280×720](screenshots/v24-four-small.png) · [150% DPI](screenshots/v24-dpi150.png) · [Settings](screenshots/v24-settings.png)

Capture and executable hashes are recorded in [build-info.json](screenshots/build-info.json). These screenshots use original generated test covers/audio and are not screenshots of another DJ application.

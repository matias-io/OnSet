# OnSet

Real-time visuals for DJs who play on rekordbox. OnSet runs on the same laptop as rekordbox 7,
renders to a second display, and knows the structure of the track that is playing: beat grid,
phrases, hot cues. Animation is driven from a beat phase that does not lag, and a build-up can
start before the drop because rekordbox already analysed where the drop is.

It also shows what is playing (title, artists, album, year, key, BPM, artwork), themes the
visuals from the artwork's colours, and displays synced lyrics when they exist.

## Status

Pre-alpha. Nothing renders yet. The current milestone is the data layer and the musical time
engine, exercised through a developer CLI:

- `onset-cli library` lists the rekordbox collection (title, artist, BPM, key, artwork).
- `onset-cli anlz <title>` prints a track's beat grid summary, phrases and cues.
- `onset-cli sim <title>` plays a track through the built-in simulator and prints beat, bar,
  phrase and drop-countdown state live.
- `onset-cli listen` captures the master mix and shows a 24-band meter.

## How it reads rekordbox

OnSet does not talk to rekordbox through an API, because rekordbox has none. On the user's own
machine it reads:

- the rekordbox library database (`master.db`) and analysis files (beat grids, phrases, cues,
  waveforms) that rekordbox writes to the user's profile;
- rekordbox's process memory, to learn which deck is master and where the playhead is
  (planned; the pointer offsets are specific to each rekordbox version and are calibrated
  locally);
- the master mix, via rekordbox's PC MASTER OUT setting and a WASAPI loopback capture.

Nothing is written to rekordbox's files or memory.

## Requirements

- Windows 11 x64, rekordbox 7 (tested with 7.2.18), a controller in Performance mode.
- A second display for the output window.
- To build: Rust stable (`rustup`), Visual Studio Build Tools with the C++ workload.

## Build

```bash
cargo build --release
```

## License

OnSet's original code and visuals
are available under the [OnSet Personal Use License 1.0](LICENSE).

- Free personal, noncommercial use and modification.
- Shared copies must retain the attribution required by LICENSE, include the same
  license, and provide corresponding source when distributing executables.
- Commercial use and competing products or services require separate written
  permission from the copyright holder.

This is source-available software with use restrictions. Earlier license grants
remain valid for recipients who already received them. Read LICENSE for the
complete terms. Contributions to OnSet must be offered under these same terms.

Dependencies retain their own licenses. Music, artwork and lyrics from your
library or external services are not covered by OnSet's license.
